//! Ephemeral diagnostics store (#41). Client error reports (and the file that
//! triggered them) are written under a per-report directory so a failure that
//! only reproduces on someone's phone can be pulled and replayed here. The store
//! lives OFF the `/data` volume (a tmpfs in production) so it is wiped on every
//! container restart and never accumulates. Pure filesystem + JSON; the HTTP
//! layer (handlers) does the decoding and enrichment.

use std::path::{Path, PathBuf};

use crate::models::{DiagFileMeta, DiagReport};

/// Fixed on-disk name for a report's attached bytes. Fixed (not derived from the
/// user's filename) so there's no path to traverse and nothing to persist.
const BLOB: &str = "blob";

/// A report id is a uuid we mint; reject anything else so a crafted `GET
/// /diag/:id` path can't traverse out of the store.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn report_json(dir: &Path, id: &str) -> PathBuf {
    dir.join(id).join("report.json")
}

/// Persist a report (and its optional file bytes) under `<dir>/<id>/`.
pub fn store(dir: &Path, report: &DiagReport, bytes: Option<&[u8]>) -> std::io::Result<()> {
    let rdir = dir.join(&report.id);
    std::fs::create_dir_all(&rdir)?;
    if let (Some(_meta), Some(b)) = (&report.file, bytes) {
        std::fs::write(rdir.join(BLOB), b)?;
    }
    let json = serde_json::to_vec_pretty(report).map_err(std::io::Error::other)?;
    std::fs::write(rdir.join("report.json"), json)
}

/// All reports, newest first. Skips any dir whose `report.json` is missing or
/// unreadable (a half-written report never breaks the list).
pub fn list(dir: &Path) -> Vec<DiagReport> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Some(id) = e.file_name().to_str() {
                if let Some(r) = load(dir, id) {
                    out.push(r);
                }
            }
        }
    }
    // received_at is an ISO `YYYY-MM-DD HH:MM:SS` string, so lexical sort == time
    // sort. Ties break by id for a stable order.
    out.sort_by(|a, b| b.received_at.cmp(&a.received_at).then(b.id.cmp(&a.id)));
    out
}

pub fn load(dir: &Path, id: &str) -> Option<DiagReport> {
    if !valid_id(id) {
        return None;
    }
    let bytes = std::fs::read(report_json(dir, id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The report's file metadata + bytes, if it attached one.
pub fn load_file(dir: &Path, id: &str) -> Option<(DiagFileMeta, Vec<u8>)> {
    let report = load(dir, id)?;
    let meta = report.file?;
    let bytes = std::fs::read(dir.join(id).join(BLOB)).ok()?;
    Some((meta, bytes))
}

/// Keep only the newest `keep` reports; delete the rest. Bounds the store so a
/// burst of reports (or big attached files) can't fill the tmpfs.
pub fn prune(dir: &Path, keep: usize) {
    let reports = list(dir); // newest first
    for r in reports.into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(dir.join(&r.id));
    }
}

/// Drop every report (used to reset the store between debugging sessions).
pub fn clear(dir: &Path) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}
