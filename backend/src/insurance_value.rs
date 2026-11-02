//! Insurance value (#74): what each policy term cost against what it paid back,
//! and what it would take to come out even. Reads only what is already recorded
//! (policies from #8/#29, payouts as `medical_records.covered_cents` from #70);
//! never writes, and never invents a number the data doesn't hold — a missing
//! input is reported by name in `missing` instead.
//!
//! The arithmetic is pure ([`pet_value`]) so it is unit-tested with literal
//! expected values; the handlers only load rows and pick the date.

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::Serialize;
use sqlx::types::Json as SqlxJson;
use sqlx::{FromRow, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::handlers::{iso_date, load_pet};
use crate::import::{today, CLAIM_KEY_PREFIX, CLAIM_POLICY_SEP};
use crate::models::{InsurancePolicy, LabeledDetail};

/// One term of one policy, valued. A renewal is its own policy row (import
/// records a new policy per term and ends the old one), so policy == term.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PolicyValue {
    pub policy_id: String,
    pub pet_id: String,
    pub kind: String,
    pub provider: String,
    pub plan_name: Option<String>,
    pub policy_number: Option<String>,
    pub status: String,
    pub start_date: Option<String>,
    /// The end the numbers use: the recorded end, or start + 1 year for an
    /// active policy with none recorded (then `assumed` says so).
    pub end_date: Option<String>,
    /// `upcoming` | `current` | `ended` | `unknown` (no start date), on `as_of`.
    pub phase: &'static str,
    /// The premium as a yearly figure (`premium_cents` × 12 for a monthly one).
    pub annual_premium_cents: Option<i64>,
    /// What the whole term costs, prorated by day to its real end — a policy
    /// replaced early costs only the part it ran.
    pub term_premium_cents: Option<i64>,
    /// Paid in up to and including `as_of` (the full term once it has ended).
    pub paid_in_cents: Option<i64>,
    pub paid_back_cents: i64,
    /// `paid_back_cents - paid_in_cents`; negative = more paid in than back.
    pub net_cents: Option<i64>,
    /// The visits whose payouts make up `paid_back_cents`.
    pub payouts: Vec<Payout>,
    /// Accident & illness only: the one eligible bill that returns the term's
    /// premium. `None` for a wellness plan, or when an input is missing.
    pub break_even: Option<BreakEven>,
    /// Preventative / wellness sub-limits found in the coverages, if any.
    pub preventative: Option<Preventative>,
    /// Inputs a number needed but the policy doesn't record ("premium",
    /// "start date", "end date", "premium interval", "reimbursement %",
    /// "deductible"). Shown to the person rather than guessed.
    pub missing: Vec<&'static str>,
    /// Defaults the numbers lean on ("end date" = start + 1 year, "premium per
    /// year" when no interval is recorded).
    pub assumed: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Payout {
    pub record_id: i64,
    pub occurred_on: String,
    pub title: String,
    pub amount_cents: i64,
    /// `claim` = the visit's "Insurance claim …" note names this policy;
    /// `term` = the visit's date falls inside this policy's term.
    pub matched_by: &'static str,
}

/// `bill_cents = deductible + premium / reimbursement rate`, rounded UP to the
/// cent (the smallest bill that returns at least the premium). A standalone
/// yardstick: ONE bill on its own, as if it were the term's only claim, so it
/// ignores payouts already made (those are in `paid_back_cents`). The inputs
/// ride along so the UI can show the formula, not just the answer.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BreakEven {
    /// `None` when no single bill can return the premium (`reachable` false):
    /// 0% reimbursement, or an annual limit below the premium.
    pub bill_cents: Option<i64>,
    pub premium_cents: i64,
    pub deductible_cents: i64,
    pub reimburse_percent: i64,
    pub annual_limit_cents: Option<i64>,
    pub reachable: bool,
}

/// Preventative sub-limits ("Vaccines: up to $105/yr"). FurFiles records a
/// payout per VISIT, not per benefit line, so which sub-limit a payout used is
/// unknown; what is exact (going by recorded payouts) is the lower bound
/// `allowance - paid back`, and — when nothing has been paid back this term —
/// every sub-limit in full (`unused_cents`). A wellness plan's benefits are
/// used at the counter without a payout, so for one usage can't be seen at
/// all: `unused_at_least_cents` is 0 and every `unused_cents` is null.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Preventative {
    pub allowance_cents: i64,
    pub unused_at_least_cents: i64,
    pub limits: Vec<SubLimit>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SubLimit {
    pub label: String,
    pub limit_cents: i64,
    /// Known only for insurance while nothing has been paid back this term
    /// (then = limit).
    pub unused_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PetInsuranceValue {
    pub pet_id: String,
    pub name: String,
    pub as_of: String,
    /// Current term first, then upcoming, then the most recent start.
    pub policies: Vec<PolicyValue>,
    /// Sums over the INSURANCE terms that have the number (see
    /// `paid_in_incomplete`). A wellness plan pays at the counter, so its fee
    /// has no payouts to set against it and would only drag the net down; it
    /// is summed apart in `wellness_paid_in_cents` (and valued per term).
    pub paid_in_cents: i64,
    pub paid_back_cents: i64,
    pub net_cents: i64,
    /// True when some insurance term is missing what paid-in needs, so the
    /// sum is short.
    pub paid_in_incomplete: bool,
    /// Wellness-plan fees paid in to date.
    pub wellness_paid_in_cents: i64,
    /// Payouts on this pet's visits that no policy's term or claim note covers.
    pub unmatched_paid_back_cents: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InsuranceValueSummary {
    pub as_of: String,
    /// Pets with at least one policy, by name.
    pub pets: Vec<PetInsuranceValue>,
}

/// A visit that paid something back: the input to attribution.
#[derive(Debug, Clone, FromRow)]
pub struct PaidVisit {
    pub id: i64,
    pub pet_id: String,
    pub occurred_on: String,
    pub title: String,
    pub covered_cents: i64,
    pub extra: Option<SqlxJson<BTreeMap<String, String>>>,
}

// ---------- the arithmetic (pure) ----------

fn date(s: Option<&str>) -> Option<NaiveDate> {
    s.and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
}

fn add_years(d: NaiveDate, n: u32) -> NaiveDate {
    // Feb 29 + 1 year clamps to Feb 28 (chrono's month arithmetic). Past
    // chrono's last year it stays put, which `prorate` guards against.
    d.checked_add_months(Months::new(n.saturating_mul(12)))
        .unwrap_or(d)
}

/// i128 cents back to i64, saturating: inputs are validated non-negative on
/// write, but a stored absurd value must not wrap or panic here.
fn clamp(v: i128) -> i64 {
    v.clamp(0, i64::MAX as i128) as i64
}

/// Saturating sum of cents (stored amounts are unbounded i64).
fn total(xs: impl Iterator<Item = i64>) -> i64 {
    xs.fold(0, i64::saturating_add)
}

/// Integer division rounding half up (inputs are non-negative cents × days).
fn div_round(a: i128, b: i128) -> i128 {
    (2 * a + b) / (2 * b)
}

/// `annual` prorated over `[from, to)`: whole calendar years at the full
/// premium, the remainder by day against the length of the year it falls in
/// (365 or 366). A one-year term is exactly the annual premium, leap or not.
pub fn prorate(annual: i64, from: NaiveDate, to: NaiveDate) -> i64 {
    if to <= from {
        return 0;
    }
    let mut years = (to.year() - from.year()).max(0) as u32;
    while years > 0 && add_years(from, years) > to {
        years -= 1;
    }
    let anchor = add_years(from, years);
    let rem = (to - anchor).num_days() as i128;
    let len = (add_years(from, years + 1) - anchor).num_days() as i128;
    let whole = annual as i128 * years as i128;
    if len <= 0 {
        // Past chrono's last representable year; only a typo'd date gets here.
        return clamp(whole);
    }
    clamp(whole + div_round(annual as i128 * rem, len))
}

/// First `$` amount in `s` as cents: "$1,050.50" → 105050, "$70." → 7000.
pub fn parse_money(s: &str) -> Option<i64> {
    let rest = &s[s.find('$')? + 1..];
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .filter(|c| *c != ',')
        .collect();
    let (dollars, cents) = num.split_once('.').unwrap_or((&num, ""));
    let dollars: i64 = dollars.parse().ok()?;
    let cents = match cents.len() {
        0 => 0,
        1 => cents.parse::<i64>().ok()? * 10,
        _ => cents[..2].parse::<i64>().ok()?,
    };
    dollars.checked_mul(100)?.checked_add(cents)
}

/// Words that mark a coverage line as preventative / wellness care.
const PREVENTATIVE_WORDS: &[&str] = &[
    "wellness",
    "preventative",
    "preventive",
    "routine",
    "vaccin",
    "exam",
    "dental",
    "teeth",
    "cleaning",
    "test",
    "screen",
    "fecal",
    "heartworm",
    "flea",
    "tick",
    "parasite",
    "deworm",
    "microchip",
    "spay",
    "neuter",
    "check",
];

/// A coverage line that is a preventative sub-limit, as cents. It must read as
/// preventative care (a [`PREVENTATIVE_WORDS`] word in its label or detail),
/// state an "up to $N" (or "$N/yr") cap, and must not be the accident & illness
/// cover, a deductible, a premium, or a copay — those carry dollar figures too
/// ("up to $50,000/yr, $500 deductible"; "Hospitalization: up to $5,000/yr").
pub fn sub_limit(c: &LabeledDetail) -> Option<i64> {
    let label = c.label.to_ascii_lowercase();
    let detail = c.detail.as_deref().unwrap_or("");
    let lower = detail.to_ascii_lowercase();
    let not_preventative = ["accident", "illness", "deductible", "premium"];
    if not_preventative.iter().any(|w| label.contains(w))
        || lower.contains("copay")
        || lower.contains("co-pay")
        || !PREVENTATIVE_WORDS
            .iter()
            .any(|w| label.contains(w) || lower.contains(w))
    {
        return None;
    }
    // ASCII lowercasing keeps byte offsets, so an index in `lower` is valid in
    // `detail`.
    if let Some(i) = lower.find("up to $") {
        return parse_money(&detail[i..]);
    }
    let i = lower.find('$')?;
    let amount = parse_money(&detail[i..])?;
    let after =
        lower[i + 1..].trim_start_matches(|c: char| c.is_ascii_digit() || c == ',' || c == '.');
    ["/yr", "/year", " per year", " a year", " annually"]
        .iter()
        .any(|s| after.starts_with(s))
        .then_some(amount)
}

/// A visit's claim notes as (amount, policy number), read back from what
/// import writes ([`crate::import::claim_key`] → [`crate::import::claim_value`]:
/// "$177.12 paid · policy
/// DEMO-POL-0401"). An entry without a readable amount or policy is skipped
/// (its payout then falls to the term dates).
pub fn claim_notes(extra: &BTreeMap<String, String>) -> Vec<(i64, String)> {
    extra
        .iter()
        .filter(|(k, _)| k.starts_with(CLAIM_KEY_PREFIX))
        .filter_map(|(_, v)| {
            let amount = parse_money(v)?;
            let (_, policy) = v.rsplit_once(CLAIM_POLICY_SEP)?;
            let policy = policy.trim().to_ascii_uppercase();
            (!policy.is_empty()).then_some((amount, policy))
        })
        .collect()
}

/// The term window and its premium facts, before payouts are attributed.
struct Term {
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    annual: Option<i64>,
    missing: Vec<&'static str>,
    assumed: Vec<&'static str>,
}

fn term_of(p: &InsurancePolicy) -> Term {
    let mut missing = Vec::new();
    let mut assumed = Vec::new();
    let start = date(p.start_date.as_deref());
    if start.is_none() {
        missing.push("start date");
    }
    let mut end = date(p.end_date.as_deref());
    if end.is_none() {
        match (start, p.status.as_str()) {
            // Policies renew yearly; an active one with no end runs a year.
            (Some(s), "active") => {
                end = Some(add_years(s, 1));
                assumed.push("end date");
            }
            // An ended policy with no end date: when it stopped is unknown.
            (Some(_), _) => missing.push("end date"),
            (None, _) => {}
        }
    }
    let annual = match (p.premium_cents, p.premium_interval.as_deref()) {
        (None, _) => {
            missing.push("premium");
            None
        }
        // A negative premium is a typo, not a number to add up.
        (Some(c), _) if c < 0 => {
            missing.push("premium");
            None
        }
        (Some(c), Some("month")) => Some(c.saturating_mul(12)),
        (Some(c), Some("year")) => Some(c),
        (Some(c), None) => {
            assumed.push("premium per year");
            Some(c)
        }
        (Some(_), Some(_)) => {
            missing.push("premium interval");
            None
        }
    };
    Term {
        start,
        end,
        annual,
        missing,
        assumed,
    }
}

fn in_term(t: &Term, d: NaiveDate) -> bool {
    // [start, end): on a renewal day the visit belongs to the new term.
    t.start.is_some_and(|s| d >= s) && t.end.is_none_or(|e| d < e)
}

/// The policy a claim note's number means for a visit on `on`. Many insurers
/// keep one number across renewals, so several rows can share it: the term
/// holding the visit wins, else the latest one started by then, else the first.
fn pick_numbered(rows: &[usize], terms: &[Term], on: Option<NaiveDate>) -> usize {
    let holds = |&&i: &&usize| on.is_some_and(|d| in_term(&terms[i], d));
    let started = |&&i: &&usize| on.is_some_and(|d| terms[i].start.is_some_and(|s| s <= d));
    rows.iter()
        .find(holds)
        .or_else(|| rows.iter().filter(started).max_by_key(|&&i| terms[i].start))
        .copied()
        .unwrap_or(rows[0])
}

/// Value every policy of one pet on `as_of`. Each payout is attributed ONCE:
/// first to the policy its claim note names (a note naming a policy this pet
/// doesn't have is left unmatched rather than credited to another insurer),
/// then whatever is left of the visit's `covered_cents` to the insurance
/// policy whose term holds the visit date (the latest-starting one if terms
/// overlap). A wellness plan pays at the counter, so it only collects payouts
/// a claim note names. Anything left over is `unmatched_paid_back_cents`.
/// Visits after `as_of` haven't happened yet on that day and are ignored.
pub fn pet_value(
    pet_id: &str,
    name: &str,
    policies: &[InsurancePolicy],
    visits: &[PaidVisit],
    as_of: NaiveDate,
) -> PetInsuranceValue {
    let terms: Vec<Term> = policies.iter().map(term_of).collect();
    let mut payouts: Vec<Vec<Payout>> = vec![Vec::new(); policies.len()];
    let mut unmatched = 0;
    let mut by_number: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in policies.iter().enumerate() {
        if let Some(n) = p
            .policy_number
            .as_deref()
            .map(|n| n.trim().to_ascii_uppercase())
        {
            by_number.entry(n).or_default().push(i);
        }
    }

    for v in visits.iter().filter(|v| v.covered_cents > 0) {
        let on = date(Some(&v.occurred_on));
        if on.is_some_and(|d| d > as_of) {
            continue;
        }
        let payout = |amount_cents, matched_by| Payout {
            record_id: v.id,
            occurred_on: v.occurred_on.clone(),
            title: v.title.clone(),
            amount_cents,
            matched_by,
        };
        let mut left = v.covered_cents;
        let notes = v
            .extra
            .as_ref()
            .map(|e| claim_notes(&e.0))
            .unwrap_or_default();
        for (amount, number) in notes {
            let amount = amount.min(left);
            if amount <= 0 {
                continue;
            }
            left -= amount;
            match by_number.get(&number) {
                Some(rows) => {
                    payouts[pick_numbered(rows, &terms, on)].push(payout(amount, "claim"))
                }
                None => unmatched += amount,
            }
        }
        if left <= 0 {
            continue;
        }
        let holder = on.and_then(|d| {
            (0..policies.len())
                .filter(|&i| policies[i].kind == "insurance" && in_term(&terms[i], d))
                .max_by_key(|&i| terms[i].start)
        });
        match holder {
            Some(i) => payouts[i].push(payout(left, "term")),
            None => unmatched += left,
        }
    }

    let mut values: Vec<PolicyValue> = policies
        .iter()
        .zip(terms)
        .zip(payouts)
        .map(|((p, t), pays)| policy_value(p, t, pays, as_of))
        .collect();
    let rank = |v: &PolicyValue| match v.phase {
        "current" => 0,
        "upcoming" => 1,
        _ => 2,
    };
    values.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.start_date.cmp(&a.start_date)));

    let (cover, wellness): (Vec<&PolicyValue>, Vec<&PolicyValue>) =
        values.iter().partition(|v| v.kind == "insurance");
    let paid_in = total(cover.iter().filter_map(|v| v.paid_in_cents));
    let paid_back = total(cover.iter().map(|v| v.paid_back_cents));
    PetInsuranceValue {
        pet_id: pet_id.to_string(),
        name: name.to_string(),
        as_of: as_of.to_string(),
        paid_in_incomplete: cover.iter().any(|v| v.paid_in_cents.is_none()),
        wellness_paid_in_cents: total(wellness.iter().filter_map(|v| v.paid_in_cents)),
        policies: values,
        paid_in_cents: paid_in,
        paid_back_cents: paid_back,
        net_cents: paid_back.saturating_sub(paid_in),
        unmatched_paid_back_cents: unmatched,
    }
}

fn policy_value(
    p: &InsurancePolicy,
    t: Term,
    payouts: Vec<Payout>,
    as_of: NaiveDate,
) -> PolicyValue {
    let phase = match (t.start, t.end) {
        (None, _) => "unknown",
        (Some(s), _) if as_of < s => "upcoming",
        (_, Some(e)) if as_of >= e => "ended",
        _ => "current",
    };
    let term_premium = match (t.annual, t.start, t.end) {
        (Some(a), Some(s), Some(e)) => Some(prorate(a, s, e)),
        _ => None,
    };
    // Up to and including today; never past the term's end.
    let paid_in = match (t.annual, t.start, t.end) {
        (Some(a), Some(s), Some(e)) => Some(prorate(a, s, (as_of + Duration::days(1)).min(e))),
        _ => None,
    };
    let paid_back = total(payouts.iter().map(|x| x.amount_cents));
    let mut missing = t.missing;

    let break_even = if p.kind == "insurance" {
        if p.reimburse_percent.is_none() {
            missing.push("reimbursement %");
        }
        if p.deductible_cents.is_none() {
            missing.push("deductible");
        }
        match (term_premium, p.reimburse_percent, p.deductible_cents) {
            (Some(premium), Some(pct), Some(deductible)) => {
                let reachable = pct > 0 && p.annual_limit_cents.is_none_or(|l| l >= premium);
                Some(BreakEven {
                    bill_cents: reachable.then(|| {
                        // ceil(premium * 100 / pct): the smallest bill whose
                        // reimbursement covers the whole premium.
                        let (premium, pct) = (premium as i128, pct as i128);
                        clamp(deductible as i128 + (premium * 100 + pct - 1) / pct)
                    }),
                    premium_cents: premium,
                    deductible_cents: deductible,
                    reimburse_percent: pct,
                    annual_limit_cents: p.annual_limit_cents,
                    reachable,
                })
            }
            _ => None,
        }
    } else {
        None
    };

    let limits: Vec<(String, i64)> = p
        .coverages
        .as_ref()
        .map(|c| {
            c.0.iter()
                .filter_map(|c| Some((c.label.trim().to_string(), sub_limit(c)?)))
                .collect()
        })
        .unwrap_or_default();
    // Usage is only visible where benefits are paid out (insurance), not
    // where they're used at the counter (a wellness plan).
    let visible = p.kind == "insurance";
    let preventative = (!limits.is_empty()).then(|| {
        let allowance = total(limits.iter().map(|(_, c)| *c));
        Preventative {
            allowance_cents: allowance,
            unused_at_least_cents: if visible {
                allowance.saturating_sub(paid_back).max(0)
            } else {
                0
            },
            limits: limits
                .into_iter()
                .map(|(label, limit_cents)| SubLimit {
                    label,
                    limit_cents,
                    unused_cents: (visible && paid_back == 0).then_some(limit_cents),
                })
                .collect(),
        }
    });

    PolicyValue {
        policy_id: p.id.clone(),
        pet_id: p.pet_id.clone(),
        kind: p.kind.clone(),
        provider: p.provider.clone(),
        plan_name: p.plan_name.clone(),
        policy_number: p.policy_number.clone(),
        status: p.status.clone(),
        start_date: t.start.map(|d| d.to_string()),
        end_date: t.end.map(|d| d.to_string()),
        phase,
        annual_premium_cents: t.annual,
        term_premium_cents: term_premium,
        paid_in_cents: paid_in,
        paid_back_cents: paid_back,
        net_cents: paid_in.map(|i| paid_back.saturating_sub(i)),
        payouts,
        break_even,
        preventative,
        missing,
        assumed: t.assumed,
    }
}

// ---------- handlers ----------

/// `?as_of=YYYY-MM-DD` values the terms on that day instead of today (for
/// scripts, tests, and "what did this look like at renewal").
fn as_of(params: &HashMap<String, String>) -> AppResult<NaiveDate> {
    match iso_date("as_of", params.get("as_of").cloned())? {
        Some(s) => date(Some(&s)).ok_or_else(|| AppError::BadRequest(format!("bad as_of '{s}'"))),
        None => date(Some(&today())).ok_or_else(|| AppError::Internal("no date for today".into())),
    }
}

async fn load_value(
    pool: &SqlitePool,
    pet_id: Option<&str>,
    on: NaiveDate,
) -> AppResult<Vec<PetInsuranceValue>> {
    let policies = sqlx::query_as::<_, InsurancePolicy>(
        "SELECT * FROM insurance_policies WHERE (?1 IS NULL OR pet_id = ?1)",
    )
    .bind(pet_id)
    .fetch_all(pool)
    .await?;
    let visits = sqlx::query_as::<_, PaidVisit>(
        "SELECT id, pet_id, occurred_on, title, covered_cents, extra FROM medical_records \
         WHERE covered_cents > 0 AND (?1 IS NULL OR pet_id = ?1) ORDER BY occurred_on, id",
    )
    .bind(pet_id)
    .fetch_all(pool)
    .await?;
    let pets = sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM pets WHERE (?1 IS NULL OR id = ?1) ORDER BY name COLLATE NOCASE",
    )
    .bind(pet_id)
    .fetch_all(pool)
    .await?;
    Ok(pets
        .into_iter()
        .filter_map(|(id, name)| {
            let mine: Vec<InsurancePolicy> = policies
                .iter()
                .filter(|p| p.pet_id == id)
                .cloned()
                .collect();
            if mine.is_empty() && pet_id.is_none() {
                return None;
            }
            let paid: Vec<PaidVisit> = visits.iter().filter(|v| v.pet_id == id).cloned().collect();
            Some(pet_value(&id, &name, &mine, &paid, on))
        })
        .collect())
}

/// Every pet with a policy (the Costs page card).
pub async fn household_value(
    State(pool): State<SqlitePool>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<InsuranceValueSummary>> {
    let on = as_of(&params)?;
    Ok(Json(InsuranceValueSummary {
        as_of: on.to_string(),
        pets: load_value(&pool, None, on).await?,
    }))
}

/// One pet, every term (empty `policies` for a pet with none).
pub async fn pet_insurance_value(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<PetInsuranceValue>> {
    load_pet(&pool, &id).await?;
    let on = as_of(&params)?;
    let mut all = load_value(&pool, Some(&id), on).await?;
    all.pop()
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("no pet '{id}'")))
}

/// One policy term (the policy sheet). Valued among its pet's other policies so
/// a payout is never counted for two of them.
pub async fn policy_insurance_value(
    State(pool): State<SqlitePool>,
    Path(pid): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<PolicyValue>> {
    let pet_id: String = sqlx::query_scalar("SELECT pet_id FROM insurance_policies WHERE id = ?")
        .bind(&pid)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("no insurance policy '{pid}'")))?;
    let on = as_of(&params)?;
    load_value(&pool, Some(&pet_id), on)
        .await?
        .into_iter()
        .flat_map(|p| p.policies)
        .find(|v| v.policy_id == pid)
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("no insurance policy '{pid}'")))
}
