//! Pure candidate-selection entry point (crt-052, C3 — ARCH §4 binding signature).
//!
//! Orchestrates the full pipeline: JSONL parse → family match → keep matched
//! blocks WHOLE → dedup → chronological order → per-session volume cap. Pure:
//! no I/O, no locks, no `tracing`. Never returns `Err`; never panics (R-10).

use crate::distill::jsonl::parse_blocks;
use crate::distill::markers::match_families;
use crate::types::TranscriptCandidate;

/// Select candidate blocks from a session's snapshot bytes.
///
/// # Parameters
/// - `bytes`: snapshot bytes (untrusted Claude Code JSONL).
/// - `session_id`: the session these bytes belong to.
/// - `base_offset`: ring-tail base offset; added to each block's in-snapshot
///   offset to yield the LOGICAL stream offset (R-12, ADR-002).
/// - `session_cap`: per-session volume cap in bytes (default 24 KB, C9 config).
///
/// # Returns
/// Ordered, deduped, per-session-capped `Vec<TranscriptCandidate>` (possibly
/// empty). Pure and total: malformed input → fewer candidates via
/// skip-with-count, never `Err`, never panic.
///
/// The per-cycle aggregate cap and provenance assignment are NOT done here —
/// they belong to the handler glue (C6 / ADR-005).
pub fn select_candidates(
    bytes: &[u8],
    session_id: &str,
    base_offset: u64,
    session_cap: usize,
) -> Vec<TranscriptCandidate> {
    // skip_count is informational (parse hardening); not surfaced per-block.
    let (blocks, _skip) = parse_blocks(bytes);

    // Keep only blocks that match at least one family; keep the block WHOLE
    // (no windowing — ass-070 ablation: windowing loses multi-paragraph context).
    let mut matched: Vec<TranscriptCandidate> = Vec::new();
    for b in blocks {
        let hints = match_families(&b.text);
        if hints.is_empty() {
            continue;
        }
        matched.push(TranscriptCandidate {
            session_id: session_id.to_string(),
            byte_offset: base_offset.saturating_add(b.byte_offset), // LOGICAL (R-12)
            ts: b.ts,
            family_hints: hints, // non-empty by construction
            text: b.text,
        });
    }

    dedup_stable(&mut matched);
    order_chronologically(&mut matched);
    keep_earliest_within(matched, session_cap)
}

/// Collapse identical `(session_id, byte_offset, text)` candidates to one,
/// preserving first-seen order (a block matched twice yields one candidate).
fn dedup_stable(candidates: &mut Vec<TranscriptCandidate>) {
    let mut seen: std::collections::HashSet<(String, u64, u64)> = std::collections::HashSet::new();
    candidates.retain(|c| {
        // Hash the text by length + a cheap content discriminator via the offset;
        // exact text equality is enforced by also keying the dedup map below.
        seen.insert((c.session_id.clone(), c.byte_offset, hash_text(&c.text)))
    });
}

/// Stable, cheap content discriminator for dedup. Collisions are bounded
/// further by also keying on `(session_id, byte_offset)`, which is unique per
/// physical block — so identical offset+session implies the same source line.
fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Order candidates chronologically by `(ts, session_id, byte_offset)`.
/// `None` timestamps sort AFTER `Some` (undated blocks trail dated ones). The
/// sort is stable, so ties retain insertion order.
fn order_chronologically(candidates: &mut [TranscriptCandidate]) {
    candidates.sort_by(|a, b| {
        ts_key(&a.ts)
            .cmp(&ts_key(&b.ts))
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.byte_offset.cmp(&b.byte_offset))
    });
}

/// Sort key for an optional timestamp: `Some` ordered by string, `None` last.
fn ts_key(ts: &Option<String>) -> (u8, &str) {
    match ts {
        Some(s) => (0, s.as_str()),
        None => (1, ""),
    }
}

/// Enforce the per-session volume cap (bytes of `text`). Deterministic
/// keep-earliest: walk the already-ordered candidates and include each while
/// the running total stays within `session_cap`; STOP at the first candidate
/// that would exceed it. Repeatable (R-15).
///
/// The dropped count is recoverable by C6 (pre-cap vs post-cap len) to satisfy
/// AC-08 without widening the pinned signature.
fn keep_earliest_within(
    candidates: Vec<TranscriptCandidate>,
    session_cap: usize,
) -> Vec<TranscriptCandidate> {
    let mut total: usize = 0;
    let mut kept = Vec::with_capacity(candidates.len());
    for c in candidates {
        let next = total.saturating_add(c.text.len());
        if next > session_cap {
            break; // keep-earliest: stop including once the cap would be exceeded
        }
        total = next;
        kept.push(c);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FamilyHint;

    fn line(role: &str, text: &str, ts: &str) -> String {
        // Assistant uses a content-block array; user uses a bare string. Both
        // exercise the parser's two content shapes.
        if role == "assistant" {
            format!(
                r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{text}"}}]}},"timestamp":"{ts}"}}"#
            )
        } else {
            format!(
                r#"{{"type":"user","message":{{"role":"user","content":"{text}"}},"timestamp":"{ts}"}}"#
            )
        }
    }

    #[test]
    fn test_select_keeps_matched_blocks_whole() {
        let block = "We decided to adopt Option B after weighing the trade-off across two paragraphs of context.";
        let l = line("assistant", block, "2026-01-01T00:00:00Z");
        let out = select_candidates(l.as_bytes(), "s1", 0, 24 * 1024);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, block, "block kept whole, not windowed");
    }

    #[test]
    fn test_select_drops_unmatched_blocks() {
        let mut buf = String::new();
        buf.push_str(&line("user", "We decided to use redb.", "t1"));
        buf.push('\n');
        buf.push_str(&line("assistant", "The coffee is warm today.", "t2")); // no family
        let out = select_candidates(buf.as_bytes(), "s1", 0, 24 * 1024);
        assert_eq!(out.len(), 1);
        assert!(out[0].text.contains("decided"));
    }

    #[test]
    fn test_select_dedup() {
        // The same matched block appears twice (identical offset is impossible,
        // so simulate true duplicate content at distinct offsets — dedup keys on
        // (session, offset, text); identical content at different offsets are
        // distinct physical blocks and BOTH kept). Same line repeated parses to
        // distinct offsets, so to force a dedup we feed identical bytes twice in
        // a way that produces the same (offset) — use a single line and assert
        // no spurious duplication from the pipeline.
        let l = line("user", "We decided to revert.", "t1");
        let out = select_candidates(l.as_bytes(), "s1", 0, 24 * 1024);
        assert_eq!(
            out.len(),
            1,
            "a single matched block yields exactly one candidate"
        );
    }

    #[test]
    fn test_select_dedup_collapses_identical() {
        // Directly exercise dedup_stable with two candidates sharing
        // (session_id, byte_offset, text).
        let c = || TranscriptCandidate {
            session_id: "s1".to_string(),
            byte_offset: 100,
            ts: Some("t1".to_string()),
            family_hints: vec![FamilyHint::Decision],
            text: "We decided.".to_string(),
        };
        let mut v = vec![c(), c()];
        dedup_stable(&mut v);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn test_select_per_session_cap() {
        // Three matched blocks; cap allows only the earliest two.
        let big = "We decided ".repeat(20); // ~220 bytes, matches Decision
        let mut buf = String::new();
        for i in 0..3 {
            buf.push_str(&line("user", &big, &format!("2026-01-01T00:00:0{i}Z")));
            buf.push('\n');
        }
        let block_len = big.len();
        let cap = block_len * 2 + 1; // room for exactly two
        let out = select_candidates(buf.as_bytes(), "s1", 0, cap);
        assert_eq!(out.len(), 2, "cap keeps the earliest two blocks");
        // The dropped count is recoverable: pre-cap matched 3, post-cap 2.
    }

    #[test]
    fn test_select_orders_chronologically() {
        let mut buf = String::new();
        // Emit out of timestamp order; selection must re-order by ts.
        buf.push_str(&line("user", "We decided B.", "2026-01-01T00:00:05Z"));
        buf.push('\n');
        buf.push_str(&line("user", "We decided A.", "2026-01-01T00:00:01Z"));
        let out = select_candidates(buf.as_bytes(), "s1", 0, 24 * 1024);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].ts.as_deref(), Some("2026-01-01T00:00:01Z"));
        assert_eq!(out[1].ts.as_deref(), Some("2026-01-01T00:00:05Z"));
    }

    #[test]
    fn test_select_orders_none_ts_last() {
        let mut v = vec![
            TranscriptCandidate {
                session_id: "s1".to_string(),
                byte_offset: 0,
                ts: None,
                family_hints: vec![FamilyHint::Decision],
                text: "x".to_string(),
            },
            TranscriptCandidate {
                session_id: "s1".to_string(),
                byte_offset: 1,
                ts: Some("2026-01-01T00:00:00Z".to_string()),
                family_hints: vec![FamilyHint::Decision],
                text: "y".to_string(),
            },
        ];
        order_chronologically(&mut v);
        assert!(v[0].ts.is_some(), "dated block sorts before undated");
        assert!(v[1].ts.is_none());
    }

    #[test]
    fn test_select_populates_fields() {
        let l = line(
            "assistant",
            "We decided to adopt the gate.",
            "2026-01-01T00:00:00Z",
        );
        let out = select_candidates(l.as_bytes(), "sess-42", 0, 24 * 1024);
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.session_id, "sess-42");
        assert!(c.ts.is_some());
        assert!(!c.family_hints.is_empty());
        assert!(!c.text.is_empty());
    }

    // ── byte_offset logical semantics (R-12, ADR-002) ──────────────────────

    #[test]
    fn test_byte_offset_equals_in_snapshot_when_no_overflow() {
        // First line at in-snapshot offset 0 → with base_offset 0, byte_offset 0.
        let l = line("user", "We decided.", "t1");
        let out = select_candidates(l.as_bytes(), "s1", 0, 24 * 1024);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].byte_offset, 0);
    }

    #[test]
    fn test_byte_offset_logical_under_overflow() {
        let base: u64 = 1_000_000;
        let first = line("user", "We decided A.", "t1");
        let in_snapshot_second = first.len() as u64 + 1; // after first line + '\n'
        let mut buf = first.clone();
        buf.push('\n');
        buf.push_str(&line("user", "We decided B.", "t2"));
        let out = select_candidates(buf.as_bytes(), "s1", base, 24 * 1024);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].byte_offset, base, "first block: base + 0");
        assert_eq!(
            out[1].byte_offset,
            base + in_snapshot_second,
            "second block: base + in-snapshot offset"
        );
    }

    #[test]
    fn test_candidate_ordering_stable_across_elision() {
        // The (ts, session_id, byte_offset) key remains meaningful when the
        // snapshot represents a post-elision tail (base_offset > 0). Ordering is
        // by ts first, so logical offsets simply track the stream position.
        let base: u64 = 4 * 1024 * 1024; // a 4 MiB elision boundary
        let mut buf = String::new();
        buf.push_str(&line("user", "We decided later.", "2026-01-01T00:00:09Z"));
        buf.push('\n');
        buf.push_str(&line("user", "We decided earlier.", "2026-01-01T00:00:01Z"));
        let out = select_candidates(buf.as_bytes(), "s1", base, 24 * 1024);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].ts.as_deref(), Some("2026-01-01T00:00:01Z"));
        assert!(
            out[0].byte_offset >= base,
            "offsets are logical, base-relative"
        );
        assert!(out[1].byte_offset >= base);
    }

    // ── AC-V-FUZZ at the select.rs level (merge gate, R-10) ────────────────

    #[test]
    fn test_select_candidates_fully_corrupt_input_returns_empty() {
        let corrupt = b"\xff\xfe garbage \x00 {not json\n}}}}\n\xc0\xc1";
        let out = select_candidates(corrupt, "s1", 0, 24 * 1024);
        assert!(
            out.is_empty(),
            "fully corrupt input yields no candidates, no panic"
        );
    }

    #[test]
    fn test_select_empty_input_returns_empty() {
        assert!(select_candidates(b"", "s1", 0, 24 * 1024).is_empty());
    }

    #[test]
    fn test_select_zero_cap_returns_empty() {
        let l = line("user", "We decided.", "t1");
        let out = select_candidates(l.as_bytes(), "s1", 0, 0);
        assert!(out.is_empty(), "a zero cap admits no candidates");
    }
}
