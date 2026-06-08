//! C5 — Reconstruction fallback (crt-052, Wave A).
//!
//! When a session's transcript snapshot is empty or hole-ridden past threshold,
//! distillation input is rebuilt from that session's already-loaded
//! [`ObservationRecord`]s (tool, input, response_snippet ≤500 chars). This is a
//! **fidelity FLOOR** (0.81 ceiling, DEC-weakest — ass-070 Q6), NOT parity. The
//! degraded path is made discriminable by `provenance: Reconstructed`, assigned
//! per-session by the C6 handler via `SessionLossInfo` (ADR-006/ADR-007).
//!
//! Pure: no I/O, no lock, no `tracing`. Wave A invariant (R-11): zero
//! compile-time reference to `transcript_hold.rs`.
//!
//! ## Hard invariants (AC-07)
//! - NEVER writes the transcript byte buffer.
//! - NEVER produces or inserts `ObservationRecord` rows — read-only over the
//!   already-loaded `obs` slice.
//! - Output is distillation-INPUT only.
//! - `topic_source` is an ORDER-only stable-sort preference, NEVER a filter
//!   (SR-06 / R-14): every feature-matched observation contributes; no session
//!   is excluded by `topic_source`.
//!
//! ## Fallback trigger contract (ADR-006 — documented here, INVOKED by C6)
//! The decision of *whether* a session falls back lives in the C6 handler, keyed
//! to the `TranscriptSnapshot` metadata and the `transcript_fallback_hole_fraction`
//! config knob. A session falls back (whole-session, OQ-2) when, from its snapshot:
//! 1. `bytes` is empty after JSONL filtering yields no user/assistant blocks, OR
//! 2. `elided_bytes > 0` (ring-tail clipping present), OR
//! 3. `holes` cover more than the configured fraction of
//!    `high_water - base_offset`.
//!
//! The SAME predicate result drives BOTH the path choice AND the `provenance`
//! label in `SessionLossInfo` (no re-computation). C6 owns the predicate and the
//! snapshot read; this module is the reconstruction logic the trigger selects.
//! See `pseudocode/distill-handler.md` (C6) for the trigger implementation.
//!
//! ## `topic_source` contract gap (FLAGGED — read before changing the rank)
//! ADR-006/the C5 pseudocode read a per-row `topic_source` to order
//! `declared`/`registry-fill` ahead of `vote`/`extracted`/NULL. vnc-030 ADR-005
//! (#4817) shipped `topic_source` as a SQL column on `observations`, written at
//! insert time — but it is NOT a field on the in-memory
//! [`ObservationRecord`] struct, and the cycle-review observation-load query does
//! not project it. The pinned ARCH §4 signature is
//! `obs: &[ObservationRecord]`, so the column is not reachable here today.
//!
//! Resolution: [`topic_source_rank`] reads an `Option<&str>` source, surfaced via
//! [`observation_topic_source`]. Today that accessor returns `None` (the record
//! carries no column), so all rows rank equally and the stable sort is a no-op —
//! which PRESERVES the load-bearing SR-06 invariant exactly (no row dropped, no
//! session excluded). When the record/query gains `topic_source`, point
//! `observation_topic_source` at the field and ordering activates with no other
//! change. This is the only faithful implementation that honors the pinned
//! signature without inventing a `unimatrix-core` field outside this task's scope.

use crate::types::{FamilyHint, ObservationRecord, TranscriptCandidate};

/// Max chars of `response_snippet` folded into a reconstructed block. The
/// ingest boundary already caps `response_snippet` at 500 chars; this is a
/// defensive second bound so an over-long snippet cannot inflate a candidate.
const MAX_SNIPPET_CHARS: usize = 500;

/// Rebuild degraded distillation input for one session from its already-loaded
/// observations.
///
/// `obs` is the cycle's full observation set; rows are filtered to `session_id`
/// here (the caller passes the whole cycle slice). Returns candidates ordered
/// chronologically and bounded by `session_cap` bytes (keep-earliest, consistent
/// with C3). `provenance: Reconstructed` is assigned per-session by C6 — a
/// session with zero matching observations returns an empty `Vec` and C6 still
/// emits a `SessionLossInfo` row so the loss stays visible (ADR-007).
///
/// Pure: no I/O, no lock, no buffer write, no observation-row insert (AC-07).
pub fn reconstruct_from_observations(
    session_id: &str,
    obs: &[ObservationRecord],
    session_cap: usize,
) -> Vec<TranscriptCandidate> {
    // (1) Scope to this session's observations (read-only borrow; never mutates).
    let mut rows: Vec<&ObservationRecord> =
        obs.iter().filter(|o| o.session_id == session_id).collect();

    // (2) SOFT topic_source preference (SR-06 / R-14): STABLE-sort declared/
    //     registry-fill ahead of vote/extracted/NULL. NEVER a filter — no row is
    //     dropped and no session is excluded. `sort_by_key` on a Vec is a stable
    //     sort, so rows with equal rank keep their original (chronological-ish)
    //     order until the explicit chronological sort in step (4).
    rows.sort_by_key(|o| topic_source_rank(observation_topic_source(o)));

    // (3) Build a synthetic block per observation from tool / input /
    //     response_snippet. Advisory family hints are inferred over the
    //     reconstructed text; `family_hints` is guaranteed non-empty (C4
    //     invariant) via a coarse event-type fallback.
    let mut candidates: Vec<TranscriptCandidate> = rows
        .iter()
        .map(|o| {
            let text = compose_reconstructed_text(o);
            let mut hints = infer_family_hints(&text);
            if hints.is_empty() {
                hints = vec![default_family_hint_for(o)];
            }
            TranscriptCandidate {
                session_id: session_id.to_string(),
                // No buffer stream position for reconstructed input; the ordering
                // key is `ts`. Documented so consumers never treat this as a
                // stream offset.
                byte_offset: 0,
                ts: Some(format_ts(o.ts)),
                family_hints: hints,
                text,
            }
        })
        .collect();

    // (4) Order chronologically (ts, byte_offset). `ts` is a zero-padded
    //     fixed-width millis string (see `format_ts`) so lexical order equals
    //     numeric order within this session.
    candidates.sort_by(|a, b| (&a.ts, a.byte_offset).cmp(&(&b.ts, b.byte_offset)));

    // (5) Per-session cap (keep-earliest), consistent with C3 `select_candidates`.
    keep_earliest_within(candidates, session_cap)
}

/// Read the `topic_source` provenance for one observation, if available.
///
/// See the module-level "topic_source contract gap" note: the in-memory
/// [`ObservationRecord`] carries no `topic_source` column today, so this returns
/// `None` and the soft-ordering sort is a no-op (SR-06 invariant preserved). When
/// the record gains the field, return it here and ordering activates.
#[inline]
fn observation_topic_source(_o: &ObservationRecord) -> Option<&str> {
    None
}

/// Stable-sort rank for `topic_source` (ordering only, NEVER a filter, R-14).
///
/// declared = 0, registry-fill = 1, extracted = 2, vote = 3, NULL/unknown = 4.
/// Lower sorts earlier (preferred). Every value maps to a rank, so no row is ever
/// excluded — the rank only reorders.
fn topic_source_rank(topic_source: Option<&str>) -> u8 {
    match topic_source {
        Some("declared") => 0,
        Some("registry-fill") => 1,
        Some("extracted") => 2,
        Some("vote") => 3,
        // NULL, empty, or any unrecognized value — ordered last, never dropped.
        _ => 4,
    }
}

/// Compose a degraded distillation block from one observation's fields.
///
/// Deterministic, content-only text: `event_type`, `tool`, a compact `input`
/// rendering, and the (re-bounded) `response_snippet`. No buffer bytes, no
/// fabricated narrative — this is exactly the lower-fidelity floor (ass-070 Q6).
fn compose_reconstructed_text(o: &ObservationRecord) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);
    parts.push(format!("event: {}", o.event_type));

    if let Some(tool) = &o.tool {
        parts.push(format!("tool: {tool}"));
    }

    if let Some(input) = &o.input {
        let rendered = match input {
            // A prompt snippet arrives as a JSON string; render it raw.
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !rendered.is_empty() {
            parts.push(format!(
                "input: {}",
                truncate_chars(&rendered, MAX_SNIPPET_CHARS)
            ));
        }
    }

    if let Some(snippet) = &o.response_snippet
        && !snippet.is_empty()
    {
        parts.push(format!(
            "response: {}",
            truncate_chars(snippet, MAX_SNIPPET_CHARS)
        ));
    }

    parts.join("\n")
}

/// Coarse, advisory family-hint inference over reconstructed text.
///
/// Dependency-free keyword matching (no `regex`): the heavyweight rule set lives
/// in C3 `markers.rs`; the reconstruction floor stays self-sufficient and
/// severable. Hints are ADVISORY only — the server is never authoritative over
/// family classification (Constraint 6 / Non-Goal). Returns possibly-empty; the
/// caller guarantees non-empty via [`default_family_hint_for`].
fn infer_family_hints(text: &str) -> Vec<FamilyHint> {
    let lower = text.to_lowercase();
    let mut hints = Vec::new();

    const DECISION_KW: [&str; 6] = [
        "decided",
        "decision",
        "chose",
        "we will",
        "adr",
        "trade-off",
    ];
    const REWORK_KW: [&str; 6] = ["rework", "revert", "redo", "broke", "regression", "fix the"];
    const LESSON_KW: [&str; 5] = ["lesson", "learned", "gotcha", "pitfall", "next time"];
    const PHASEGATE_KW: [&str; 6] = [
        "phase",
        "gate",
        "cycle_",
        "milestone",
        "checkpoint",
        "pass/fail",
    ];

    if DECISION_KW.iter().any(|k| lower.contains(k)) {
        hints.push(FamilyHint::Decision);
    }
    if REWORK_KW.iter().any(|k| lower.contains(k)) {
        hints.push(FamilyHint::Rework);
    }
    if LESSON_KW.iter().any(|k| lower.contains(k)) {
        hints.push(FamilyHint::Lesson);
    }
    if PHASEGATE_KW.iter().any(|k| lower.contains(k)) {
        hints.push(FamilyHint::PhaseGate);
    }

    hints
}

/// Coarse advisory fallback so `family_hints` is never empty (C4 invariant).
///
/// Infers a single family from `event_type`. A `cycle_*` event is a phase/gate
/// signal; everything else defaults to `Decision` (the broadest advisory bucket
/// for narrative observations). Advisory only.
fn default_family_hint_for(o: &ObservationRecord) -> FamilyHint {
    if o.event_type.starts_with("cycle_") {
        FamilyHint::PhaseGate
    } else {
        FamilyHint::Decision
    }
}

/// Per-session keep-earliest cap: include candidates in chronological order until
/// adding the next would exceed `session_cap` bytes (by `text` length), then stop.
/// Deterministic and repeatable (R-15), consistent with C3.
fn keep_earliest_within(
    candidates: Vec<TranscriptCandidate>,
    session_cap: usize,
) -> Vec<TranscriptCandidate> {
    let mut kept = Vec::with_capacity(candidates.len());
    let mut used: usize = 0;
    for c in candidates {
        let cost = c.text.len();
        // Saturating guard so a pathological text length cannot wrap the counter.
        let next = used.saturating_add(cost);
        if next > session_cap {
            break;
        }
        used = next;
        kept.push(c);
    }
    kept
}

/// Render an epoch-millis timestamp as a fixed-width, zero-padded string so the
/// chronological sort orders lexically the same as numerically within a session.
fn format_ts(ts: u64) -> String {
    // 20 digits covers u64::MAX; zero-padding keeps lexical == numeric order.
    format!("{ts:020}")
}

/// Truncate a string to at most `max` chars on a char boundary (UTF-8 safe).
fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
#[path = "reconstruct_tests.rs"]
mod tests;
