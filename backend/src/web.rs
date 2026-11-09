//! Fetch a facility's web page and reduce it to text for the LLM parser (#42).
//!
//! SECURITY (SSRF): this endpoint fetches an ARBITRARY user-supplied URL from the
//! server. FurFiles runs on a home server that neighbors other containers and the
//! host, so an unguarded fetcher could be steered at internal services (cloud
//! metadata endpoints, `localhost`, private LAN hosts). Guards, in order:
//!
//! - scheme must be http/https;
//! - the host must not be a private/loopback/link-local IP literal or a `localhost`/metadata name;
//! - the host is DNS-resolved and rejected if ANY resolved address is private;
//! - redirects are followed BY HAND (the client follows none), and every hop gets
//!   the same resolved check, as does every crawled same-origin link.
//!
//! Redirects are capped; response size and time are bounded.
//! Residual gap: a hostname that answers public to our lookup and private to the
//! connection a moment later (DNS rebinding between check and connect) isn't
//! caught, because reqwest does its own resolution. Acceptable for a
//! single-household self-hosted app, and noted rather than silently ignored.

use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use reqwest::Url;

/// Cap on the fetched HTML we read + hand to the model. Big enough for a policy
/// page, small enough to bound tokens and memory.
pub const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
/// Cap on the extracted text sent to the model (post-strip), in chars.
pub const MAX_TEXT_CHARS: usize = 40_000;
const FETCH_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_REDIRECTS: usize = 5;

/// True only for globally-routable addresses. Everything private, loopback,
/// link-local, CGNAT, multicast, documentation, or unspecified is rejected.
pub fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || o[0] == 0
                // CGNAT / shared address space 100.64.0.0/10 (is_shared is unstable)
                || (o[0] == 100 && (o[1] & 0xC0) == 64))
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_ip(&IpAddr::V4(mapped));
            }
            let seg0 = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // unique-local fc00::/7
                || (seg0 & 0xfe00) == 0xfc00
                // link-local fe80::/10
                || (seg0 & 0xffc0) == 0xfe80)
        }
    }
}

/// Host names that must never be fetched regardless of resolution.
fn is_blocked_host_name(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h == "localhost"
        || h.ends_with(".localhost")
        || h == "metadata"
        || h == "metadata.google.internal"
}

/// Validate a URL's scheme + host WITHOUT resolving DNS: reject non-http(s), a
/// missing host, a blocked name, or a private IP literal. The first half of
/// [`validate_url_resolved`].
pub fn validate_url_shallow(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        s => return Err(format!("unsupported URL scheme '{s}' (use http or https)")),
    }
    let host = url
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    if is_blocked_host_name(host) {
        return Err("that host is not allowed".to_string());
    }
    if let Some(ip) = host_as_ip(host) {
        if !is_public_ip(&ip) {
            return Err("that address is not allowed".to_string());
        }
    }
    Ok(())
}

/// Parse a `host_str()` as an IP address, stripping the brackets `host_str()`
/// keeps on an IPv6 literal (`[::1]` → `::1`). Returns None for a domain name.
fn host_as_ip(host: &str) -> Option<IpAddr> {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok()
}

/// Full validation including DNS resolution: every resolved address must be
/// public. Call for the initial host before fetching.
/// [`validate_url_resolved`] off the async runtime: the DNS lookup it does is
/// blocking (`to_socket_addrs`), and running it inline stalls a worker thread
/// for as long as the resolver takes.
async fn resolved_ok(url: Url) -> Result<(), String> {
    tokio::task::spawn_blocking(move || validate_url_resolved(&url))
        .await
        .map_err(|e| format!("could not check that address: {e}"))?
}

fn validate_url_resolved(url: &Url) -> Result<(), String> {
    validate_url_shallow(url)?;
    // Only hostnames need resolving; IP literals were checked in the shallow pass.
    let host = url.host_str().unwrap_or("");
    if host_as_ip(host).is_some() {
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|_| format!("could not resolve host '{host}'"))?;
    let mut any = false;
    for a in addrs {
        any = true;
        if !is_public_ip(&a.ip()) {
            return Err("that host resolves to a non-public address".to_string());
        }
    }
    if !any {
        return Err(format!("could not resolve host '{host}'"));
    }
    Ok(())
}

/// Remove `<tag>...</tag>` blocks (and their content) case-insensitively.
fn strip_block(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while let Some(rel) = lower[i..].find(&open) {
        let start = i + rel;
        out.push_str(&html[i..start]);
        // find the end of this block; if unclosed, drop the rest.
        match lower[start..].find(&close) {
            Some(erel) => i = start + erel + close.len(),
            None => {
                i = html.len();
                break;
            }
        }
    }
    out.push_str(&html[i..]);
    out
}

/// Decode the handful of HTML entities that show up in extracted text.
fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
}

/// The JSON of every `<script type="application/ld+json">` block. This is where
/// site builders (Squarespace, Wix, WordPress SEO) emit the business's name /
/// phone / address / hours as schema.org data — far more reliable than the
/// rendered body, so we KEEP it and hand it to the model rather than stripping it.
/// (Over-stripping this + the `<head>` is what made a homepage parse return
/// nothing useful; the fix is to feed the model the parts that carry the answer.)
fn json_ld_blocks(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = lower[i..].find("<script") {
        let start = i + rel;
        let gt = match lower[start..].find('>') {
            Some(e) => start + e + 1,
            None => break,
        };
        let open_tag = &lower[start..gt];
        let close_rel = lower[gt..].find("</script>");
        let inner_end = close_rel.map(|e| gt + e).unwrap_or(lower.len());
        if open_tag.contains("ld+json") {
            let inner = html[gt..inner_end].trim();
            if !inner.is_empty() {
                out.push(inner.split_whitespace().collect::<Vec<_>>().join(" "));
            }
        }
        i = close_rel
            .map(|e| gt + e + "</script>".len())
            .unwrap_or(lower.len());
    }
    out
}

/// Reduce HTML to the text worth reading: KEEP the `<title>` and the JSON-LD
/// structured data, drop only the noise (`<script>` JS, `<style>`, `<svg>`),
/// strip the remaining tags, decode entities, collapse whitespace. The model
/// does the field extraction — this just stops throwing away the parts of the
/// page where the answer lives. `<head>` is deliberately kept (its script/style
/// children are still removed) so the title survives.
pub fn html_to_text(html: &str) -> String {
    let json_ld = json_ld_blocks(html);
    let mut s = html.to_string();
    for tag in ["script", "style", "noscript", "svg", "template"] {
        s = strip_block(&s, tag);
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let mut combined = decode_entities(&out)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !json_ld.is_empty() {
        if !combined.is_empty() {
            combined.push_str("  ");
        }
        combined.push_str("Structured data (JSON-LD): ");
        combined.push_str(&json_ld.join(" "));
    }
    if combined.chars().count() > MAX_TEXT_CHARS {
        combined.chars().take(MAX_TEXT_CHARS).collect()
    } else {
        combined
    }
}

/// Read an attribute's value out of a single tag string (quoted or unquoted).
fn read_attr_value(v: &str) -> String {
    match v.as_bytes().first() {
        Some(&q) if q == b'"' || q == b'\'' => {
            let qc = q as char;
            match v[1..].find(qc) {
                Some(e) => v[1..1 + e].to_string(),
                None => v[1..].to_string(),
            }
        }
        _ => {
            let e = v
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(v.len());
            v[..e].to_string()
        }
    }
}

/// Extract `attr`'s value from a tag (e.g. `href` from an `<a>`), matching the
/// name at a boundary so `href` doesn't match inside `data-href`.
pub fn tag_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let bytes = tag.as_bytes();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(attr) {
        let at = from + rel;
        let before_ok = at == 0 || bytes[at - 1].is_ascii_whitespace();
        let after = at + attr.len();
        let rest = tag[after..].trim_start();
        if before_ok && rest.starts_with('=') {
            return Some(read_attr_value(rest[1..].trim_start()));
        }
        from = after;
    }
    None
}

/// A host+path key for de-duping crawl targets (ignores query/fragment/trailing /).
fn canon(u: &Url) -> String {
    format!(
        "{}{}",
        u.host_str().unwrap_or(""),
        u.path().trim_end_matches('/')
    )
}

/// Link substrings that mark a page likely to hold facility requirements. The
/// homepage rarely states "dogs must be current on rabies…"; that lives on a
/// Policies / FAQ / New-client page, so we follow those (same-origin only).
pub const CRAWL_KEYWORDS: &[&str] = &[
    "polic",
    "faq",
    "require",
    "boarding",
    "daycare",
    "rules",
    "vaccin",
    "register",
    "new-client",
    "new-guest",
    "new-customer",
    "what-to-bring",
    "reservation",
    "enroll",
    "policies",
];

/// Link substrings that mark a page likely to NAME someone: the owner, the
/// manager, the vets. A facility's homepage rarely does (a daycare website puts its
/// people on "Meet the Team"), and the contact person is a field the parser
/// fills. Crawled separately from the requirement pages so neither crowds the
/// other out of its cap.
const PEOPLE_KEYWORDS: &[&str] = &["team", "staff", "about", "meet", "owner", "who-we-are", "contact"];

/// How many people pages to follow, on top of the requirement pages.
const MAX_PEOPLE_PAGES: usize = 2;

/// Same-origin links from `html` whose URL or link text contains one of
/// `keywords`, capped at `max`, skipping the base page and anything in `skip`.
pub fn crawl_links(base: &Url, html: &str, keywords: &[&str], max: usize, skip: &[Url]) -> Vec<Url> {
    let lower = html.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    seen.insert(canon(base));
    for u in skip {
        seen.insert(canon(u));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while out.len() < max {
        let rel = match lower[i..].find("<a ") {
            Some(r) => r,
            None => break,
        };
        let start = i + rel;
        let gt = match lower[start..].find('>') {
            Some(e) => start + e + 1,
            None => break,
        };
        let a_tag = &html[start..gt];
        let text_l = lower[gt..]
            .find("</a>")
            .map(|e| lower[gt..gt + e].to_string())
            .unwrap_or_default();
        i = gt;
        let href = match tag_attr(a_tag, "href") {
            Some(h) => h,
            None => continue,
        };
        let abs = match base.join(href.trim()) {
            Ok(u) => u,
            Err(_) => continue,
        };
        // Same ORIGIN (scheme + host + port), not just the same host: from
        // https://site/ a link to http://site:8080/ is a different service.
        if abs.origin() != base.origin() || !matches!(abs.scheme(), "http" | "https") {
            continue;
        }
        let hl = abs.as_str().to_ascii_lowercase();
        if !keywords
            .iter()
            .any(|k| hl.contains(k) || text_l.contains(k))
        {
            continue;
        }
        if seen.insert(canon(&abs)) {
            out.push(abs);
        }
    }
    out
}

/// How many same-origin sub-pages to follow for requirements beyond the given URL.
const MAX_CRAWL_PAGES: usize = 4;

/// Read at most `cap` bytes of a response body, chunk by chunk, and stop
/// there. DO NOT replace this with `resp.bytes()` and a slice: that buffers the
/// WHOLE body first, so a hostile or broken site answering with gigabytes
/// exhausts the container's memory long before the 12 s timeout fires.
pub async fn read_capped(mut resp: reqwest::Response, cap: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    while out.len() < cap {
        match resp
            .chunk()
            .await
            .map_err(|e| format!("could not read that page: {e}"))?
        {
            Some(chunk) => {
                let room = cap - out.len();
                out.extend_from_slice(&chunk[..chunk.len().min(room)]);
            }
            None => break,
        }
    }
    Ok(out)
}

/// GET a URL and return (final URL, HTML), size-capped, lossy UTF-8. Redirects
/// are followed here, not by the client, so every hop is DNS-resolved and
/// checked like the first URL: a public page answering `302 Location:
/// http://router.lan/` must not reach the LAN. Browser-ish Accept header, since
/// some site builders serve a stub to unknown Accept values.
async fn get_html(client: &reqwest::Client, url: &Url) -> Result<(Url, String), String> {
    get_html_guarded(client, url, resolved_ok).await
}

/// [`get_html`] with the per-hop address check passed in. Production ALWAYS
/// goes through `get_html`, which passes the SSRF guard [`resolved_ok`]; this
/// exists so a test can drive the real fetch-and-read path against a local
/// server the guard would (correctly) refuse. DO NOT call it from app code.
#[doc(hidden)]
pub async fn get_html_guarded<F, Fut>(
    client: &reqwest::Client,
    url: &Url,
    guard: F,
) -> Result<(Url, String), String>
where
    F: Fn(Url) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let mut url = url.clone();
    for _ in 0..=MAX_REDIRECTS {
        guard(url.clone()).await?;
        let resp = client
            .get(url.clone())
            .header(
                "accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("accept-language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e| format!("could not fetch that page: {e}"))?;
        if resp.status().is_redirection() {
            let next = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|loc| url.join(loc).ok())
                .ok_or_else(|| "the site redirected nowhere".to_string())?;
            url = next;
            continue;
        }
        if !resp.status().is_success() {
            return Err(format!("the site returned {}", resp.status().as_u16()));
        }
        let bytes = read_capped(resp, MAX_PAGE_BYTES).await?;
        return Ok((url, String::from_utf8_lossy(&bytes).into_owned()));
    }
    Err("too many redirects".to_string())
}

/// Fetch a facility page and reduce it to text for the parser, applying every
/// SSRF guard. Beyond the given URL it shallow-crawls a few same-origin
/// Policies/FAQ pages (concurrently) so requirements that live off the homepage
/// are still captured.
pub async fn fetch_page_text(url_str: &str) -> Result<String, String> {
    let url = Url::parse(url_str.trim()).map_err(|_| "that doesn't look like a URL".to_string())?;
    // DO NOT let the client follow redirects: get_html follows them itself so
    // every hop is resolved and checked.
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("FurFiles/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Jacob-Lasky/furfiles; pet-records)"))
        .build()
        .map_err(|e| format!("could not build HTTP client: {e}"))?;

    // Crawled links resolve against where the page actually ended up.
    let (url, main_html) = get_html(&client, &url).await?;
    let mut combined = html_to_text(&main_html);

    // Follow same-origin Policies/FAQ (and people) links.
    // People pages first: they are short, and the requirement pages are long
    // enough to be the ones the MAX_TEXT_CHARS cap trims.
    let req_links = crawl_links(&url, &main_html, CRAWL_KEYWORDS, MAX_CRAWL_PAGES, &[]);
    let mut links = crawl_links(&url, &main_html, PEOPLE_KEYWORDS, MAX_PEOPLE_PAGES, &req_links);
    links.extend(req_links);
    // get_html re-resolves and checks each crawled URL and its redirects: the
    // host passed once, but its DNS answer can change between lookups.
    if !links.is_empty() {
        let mut handles = Vec::new();
        for l in links {
            let c = client.clone();
            handles.push(tokio::spawn(async move {
                get_html(&c, &l).await.ok()
            }));
        }
        for h in handles {
            if let Ok(Some((l, html))) = h.await {
                let t = html_to_text(&html);
                if !t.trim().is_empty() {
                    combined.push_str("\n\n--- from ");
                    combined.push_str(l.as_str());
                    combined.push_str(" ---\n");
                    combined.push_str(&t);
                }
            }
        }
    }

    if combined.chars().count() > MAX_TEXT_CHARS {
        combined = combined.chars().take(MAX_TEXT_CHARS).collect();
    }
    if combined.trim().is_empty() {
        return Err("that page had no readable text".to_string());
    }
    Ok(combined)
}
