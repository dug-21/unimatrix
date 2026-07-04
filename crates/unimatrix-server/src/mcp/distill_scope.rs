//! crt-057 — Scoped transcript retrieval: filter compilation, cross-plane clock
//! normalization, and the response-transient honesty projection.
//!
//! This module owns the read-only `transcript{ phase?, anchor?, match?, window? }`
//! filtering machinery layered ON TOP of the existing candidate pipeline
//! (`retrieve_scoped_candidates`, `distill_handler.rs`). It introduces NO new
//! buffer reader (CON-3/#4848) — it only decides which already-selected
//! `TranscriptCandidate`s survive a scope.
//!
//! Two cross-plane facts drive the design (ADR-006 / R-05):
//! - Plane A (`EvidenceRecord.ts`, `cycle_events`) is `u64` epoch-millis.
//! - Plane B (`TranscriptCandidate.ts`) is an optional ISO-8601 JSONL string.
//!
//! Every A↔B comparison routes through ONE named boundary helper
//! ([`candidate_epoch_ms`]) so the unit mismatch cannot silently recur. The join
//! is ALWAYS windowed, never exact (`±millis` for ts-bearing candidates, a
//! `±blocks` `byte_offset` fallback for `ts:None`), so a realistic clock skew
//! never produces a silent false negative — the feature's raison d'être.
//!
//! Nothing here is persisted: [`SessionSearchStatus`] / [`ResolvedBounds`] are
//! response-transient projections, NEVER a field on `RetrospectiveReport` (CON-4).

use std::collections::BTreeSet;

use regex::{Regex, RegexBuilder};
use unimatrix_observe::{
    CandidateProvenance, ResolvedBounds, SessionSearchStatus, TranscriptCandidate,
    TranscriptCandidatesSection, TranscriptScope, Window,
};

/// Compiled-program size ceiling for a caller `match` regex (R-09 / security).
///
/// The `regex` crate has no catastrophic backtracking, but a pathological
/// pattern can still compile to a memory-heavy program. `size_limit` /
/// `dfa_size_limit` bound that surface; an oversized pattern is rejected as an
/// invalid `match` (→ `ERROR_INVALID_PARAMS`) rather than allowed to allocate.
const MATCH_REGEX_SIZE_LIMIT: usize = 1 << 20; // 1 MiB

/// Compile a caller `match` pattern with a bounded compiled-program size.
///
/// Total: returns `Err` on an invalid or oversized pattern, never panics.
pub(crate) fn compile_bounded_regex(pattern: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern)
        .size_limit(MATCH_REGEX_SIZE_LIMIT)
        .dfa_size_limit(MATCH_REGEX_SIZE_LIMIT)
        .build()
}

/// Validate a scope's `match` regex UP FRONT (in the handler) so
/// `retrieve_scoped_candidates` stays infallible (`-> Option<...>`).
///
/// No-op when the scope or its `match` is absent. An invalid/oversized pattern
/// surfaces `ERROR_INVALID_PARAMS` — reserved for MALFORMED input, never for a
/// merely non-matching id (ADR-006 §Consequences / transcript-scope.md).
pub(crate) fn validate_scope_regex(
    scope: Option<&TranscriptScope>,
) -> Result<(), rmcp::model::ErrorData> {
    if let Some(pattern) = scope.and_then(|s| s.r#match.as_deref()) {
        compile_bounded_regex(pattern).map_err(|e| {
            rmcp::model::ErrorData::new(
                crate::error::ERROR_INVALID_PARAMS,
                format!("Invalid 'match' regex: {e}"),
                None,
            )
        })?;
    }
    Ok(())
}

/// Resolved scope context, built ONCE per retrieval and reused per candidate.
///
/// `anchor_bounds` / `phase_bounds` are the resolved Plane-A `[lo, hi]`
/// epoch-millis spans (when the scope references a resolvable id). The
/// architecture-fixed `retrieve_scoped_candidates` signature threads only
/// `observations`; a finding-`anchor` (needs the report `hotspots`) or a
/// `phase`-id (needs `cycle_events`) that cannot be resolved from the data
/// available at this seam yields `None` bounds ⇒ an empty (absent) section
/// (FR-7), never an error. The windowed-join and regex machinery below is fully
/// exercised whenever bounds ARE present.
pub(crate) struct ScopeCtx {
    /// Compiled `match` regex (validated up front by the handler), if any.
    pub compiled: Option<Regex>,
    /// Resolved anchor span (honors `window`).
    pub anchor_bounds: Option<(u64, u64)>,
    /// Resolved phase span (self-bounding: IGNORES `window`).
    pub phase_bounds: Option<(u64, u64)>,
    /// Window radii source; ignored by `phase`.
    pub window: Option<Window>,
}

/// Named boundary conversion (R-05, #3385/#3372): Plane-B ISO-8601 candidate
/// `ts` → canonical Plane-A epoch-millis.
///
/// Total: `None` / a malformed timestamp ⇒ `None` (the candidate is routed to
/// the `byte_offset` fallback, never dropped, never a panic).
pub(crate) fn candidate_epoch_ms(ts: &Option<String>) -> Option<u64> {
    parse_iso8601_to_epoch_ms(ts.as_deref()?)
}

/// Dependency-free ISO-8601 → epoch-millis parser for the common JSONL shapes:
/// `YYYY-MM-DDThh:mm:ss[.fff][Z|±hh:mm]` (the `T` may be a space). Returns `None`
/// on any deviation — deliberately conservative so a malformed `ts` degrades to
/// the `byte_offset` fallback rather than a wrong epoch.
fn parse_iso8601_to_epoch_ms(s: &str) -> Option<u64> {
    let bytes = s.as_bytes();
    // Minimum: "YYYY-MM-DDThh:mm:ss" = 19 chars.
    if bytes.len() < 19 {
        return None;
    }
    let digit = |b: u8| (b as char).to_digit(10);
    let two = |i: usize| -> Option<u32> { Some(digit(bytes[i])? * 10 + digit(bytes[i + 1])?) };
    let four = |i: usize| -> Option<u32> {
        Some(
            digit(bytes[i])? * 1000
                + digit(bytes[i + 1])? * 100
                + digit(bytes[i + 2])? * 10
                + digit(bytes[i + 3])?,
        )
    };
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if !(bytes[10] == b'T' || bytes[10] == b't' || bytes[10] == b' ') {
        return None;
    }
    let year = four(0)? as i64;
    let month = two(5)?;
    let day = two(8)?;
    let hour = two(11)?;
    let min = two(14)?;
    let sec = two(17)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return None;
    }

    // Optional fractional seconds and timezone tail after position 19.
    let mut idx = 19usize;
    let mut millis: i64 = 0;
    if idx < bytes.len() && bytes[idx] == b'.' {
        idx += 1;
        let mut frac_digits = 0u32;
        let mut frac_val = 0i64;
        while idx < bytes.len() && (bytes[idx] as char).is_ascii_digit() {
            if frac_digits < 3 {
                frac_val = frac_val * 10 + digit(bytes[idx])? as i64;
                frac_digits += 1;
            }
            idx += 1;
        }
        // Right-pad to milliseconds (e.g. ".5" → 500ms).
        while frac_digits < 3 {
            frac_val *= 10;
            frac_digits += 1;
        }
        millis = frac_val;
    }

    // Timezone offset (default UTC when absent). Subtract the offset to reach UTC.
    let mut tz_offset_secs: i64 = 0;
    if idx < bytes.len() {
        match bytes[idx] {
            b'Z' | b'z' => {}
            b'+' | b'-' => {
                let sign = if bytes[idx] == b'-' { -1 } else { 1 };
                // Expect ±hh:mm or ±hhmm.
                if idx + 3 > bytes.len() {
                    return None;
                }
                let oh = (digit(bytes[idx + 1])? * 10 + digit(bytes[idx + 2])?) as i64;
                let om = if idx + 5 <= bytes.len() {
                    let mstart = if bytes.get(idx + 3) == Some(&b':') {
                        idx + 4
                    } else {
                        idx + 3
                    };
                    if mstart + 2 <= bytes.len() {
                        (digit(bytes[mstart])? * 10 + digit(bytes[mstart + 1])?) as i64
                    } else {
                        0
                    }
                } else {
                    0
                };
                tz_offset_secs = sign * (oh * 3600 + om * 60);
            }
            _ => return None,
        }
    }

    let days = days_from_civil(year, month, day);
    let secs =
        days * 86_400 + (hour as i64) * 3600 + (min as i64) * 60 + (sec as i64) - tz_offset_secs;
    let epoch_ms = secs * 1000 + millis;
    if epoch_ms < 0 {
        None
    } else {
        Some(epoch_ms as u64)
    }
}

/// Days since 1970-01-01 for a proleptic-Gregorian civil date (Howard Hinnant's
/// `days_from_civil`). Pure integer arithmetic; valid for all realistic dates.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719_468
}

/// Windowed containment for a ts-bearing candidate epoch (NEVER exact, R-05):
/// `c_ms ∈ [lo − millis, hi + millis]` with saturating subtraction.
pub(crate) fn ts_within_window(bounds: (u64, u64), millis: u64, c_ms: u64) -> bool {
    let lo = bounds.0.saturating_sub(millis);
    let hi = bounds.1.saturating_add(millis);
    lo <= c_ms && c_ms <= hi
}

/// Phase containment for a ts-bearing candidate epoch — self-bounding, NO window
/// padding (`phase` ignores `window`, R-09 sc.4): `c_ms ∈ [start, end]`.
pub(crate) fn phase_contains_ts(bounds: (u64, u64), c_ms: u64) -> bool {
    bounds.0 <= c_ms && c_ms <= bounds.1
}

/// Whether a `ts:None` candidate's block index falls within `±blocks` of the
/// `[lo_idx, hi_idx]` block range covered by the in-window ts-bearing candidates
/// of the SAME session (AC-07 byte_offset fallback). Pure index arithmetic.
pub(crate) fn block_within(idx: usize, lo_idx: usize, hi_idx: usize, blocks: u32) -> bool {
    let b = blocks as usize;
    let lo = lo_idx.saturating_sub(b);
    let hi = hi_idx.saturating_add(b);
    lo <= idx && idx <= hi
}

/// Per-candidate scope predicate (AND-composition — each present filter NARROWS,
/// never unions; R-09). `ts:None` candidates under a time-bounded (`anchor` /
/// `phase`) scope are NOT decided here — they are resolved by the session-level
/// `byte_offset` fallback in `retrieve_scoped_candidates` and passed through with
/// `include_ts_none = true`.
pub(crate) fn scope_predicate(
    candidate: &TranscriptCandidate,
    ctx: &ScopeCtx,
    include_ts_none: bool,
) -> bool {
    let c_ms = candidate_epoch_ms(&candidate.ts);

    if let Some(bounds) = ctx.phase_bounds {
        match c_ms {
            Some(ms) => {
                if !phase_contains_ts(bounds, ms) {
                    return false;
                }
            }
            None => {
                if !include_ts_none {
                    return false;
                }
            }
        }
    }

    if let Some(bounds) = ctx.anchor_bounds {
        let (millis, _blocks) = Window::effective(ctx.window.as_ref());
        match c_ms {
            Some(ms) => {
                if !ts_within_window(bounds, millis, ms) {
                    return false;
                }
            }
            None => {
                if !include_ts_none {
                    return false;
                }
            }
        }
    }

    if ctx
        .compiled
        .as_ref()
        .is_some_and(|re| !re.is_match(&candidate.text))
    {
        return false;
    }

    true
}

/// Response-transient loss/honesty projection (FR-14/15/16, R-01 — the central
/// silent-false-negative guard). Derived AFTER retrieval so
/// `TranscriptCandidatesSection` / `SessionLossInfo` stay byte-unchanged.
///
/// Key property (R-01): search_complete == false iff a SessionLossInfo row
/// exists for the session, and a loss row exists iff any of elided_bytes>0,
/// has_holes, Reconstructed, or dropped>0 (the UNCHANGED push_loss_if_any
/// predicate). So a match no-match over a lossy session is INDETERMINATE
/// (matched Some(false) + search_complete false), never a bare false; every
/// returned session in a match result carries its loss row.
pub(crate) fn derive_search_status(
    section: Option<&TranscriptCandidatesSection>,
    scope: Option<&TranscriptScope>,
) -> (Vec<SessionSearchStatus>, Option<ResolvedBounds>) {
    let (Some(section), Some(scope)) = (section, scope) else {
        return (Vec::new(), None);
    };

    let has_match = scope.r#match.is_some();
    let matched_sessions: BTreeSet<&str> = section
        .candidates
        .iter()
        .map(|c| c.session_id.as_str())
        .collect();

    // Returned sessions = union of candidate-bearing and loss-bearing sessions,
    // deduplicated and ordered deterministically.
    let mut sessions: BTreeSet<&str> = matched_sessions.clone();
    for l in &section.loss {
        sessions.insert(l.session_id.as_str());
    }

    let rows = sessions
        .into_iter()
        .map(|sid| {
            let lossrow = section.loss.iter().find(|l| l.session_id == sid);
            SessionSearchStatus {
                session_id: sid.to_string(),
                matched: if has_match {
                    Some(matched_sessions.contains(sid))
                } else {
                    None
                },
                // Any loss row present ⇒ one of the four conditions true ⇒
                // search_complete false (INDETERMINATE for a no-match). No loss
                // row + candidates ⇒ clean ⇒ true (trustworthy negative).
                search_complete: lossrow.is_none(),
                elided_bytes: lossrow.map(|l| l.elided_bytes).unwrap_or(0),
                provenance: lossrow
                    .map(|l| l.provenance)
                    .unwrap_or(CandidateProvenance::Primary),
            }
        })
        .collect();

    (rows, None)
}

#[cfg(test)]
#[path = "distill_scope_tests.rs"]
mod tests;
