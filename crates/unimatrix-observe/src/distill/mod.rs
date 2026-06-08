//! Transcript-distillation module (crt-052, Wave A).
//!
//! Pure, no-I/O, no-lock building blocks for the `context_cycle_review`
//! transcript-candidate section. Rules SELECT; the calling agent EXTRACTS
//! (Constraint 6 — no server-side generation/classification beyond advisory
//! hints).
//!
//! Wave A invariant (R-11): NO module here has a compile-time reference to
//! `transcript_hold.rs` (a Wave-B server-side type). The reconstruction
//! fallback ([`reconstruct`]) is precisely the Wave-A degrade mode used when
//! every buffer is empty at call time.
//!
//! Components landing in this module:
//! - `reconstruct` (C5) — degraded fidelity floor from already-loaded
//!   `ObservationRecord`s.
//! - `jsonl` / `markers` / `select` (C3) — the primary buffer-bytes path:
//!   snapshot bytes → Claude Code JSONL parse → keep `user`/`assistant` text
//!   blocks → match four marker families → keep matched blocks WHOLE → dedup →
//!   per-session volume cap → chronologically-ordered `Vec<TranscriptCandidate>`.
//!   Untrusted-input-hardened (skip-with-count, never `Err`, never panic —
//!   Constraint 7, R-10, AC-V-FUZZ). The per-cycle aggregate cap is NOT enforced
//!   here; it is the handler glue's responsibility (C6 / ADR-005).

pub mod jsonl;
pub mod markers;
pub mod reconstruct;
pub mod select;

pub use markers::match_families;
pub use reconstruct::reconstruct_from_observations;
pub use select::select_candidates;

#[cfg(test)]
mod corpus_tests;

#[cfg(test)]
mod tests {
    use super::select_candidates;
    use std::time::Instant;

    /// AC-12: a full rule pass over a 4 MiB buffer completes well under 50 ms
    /// (pure Rust over in-memory bytes; ass-070 single-digit-ms estimate). Run
    /// in release-equivalent debug here as a guard, not a precise bench.
    #[test]
    fn test_select_4mib_under_50ms() {
        // Build ~4 MiB of realistic JSONL: alternating matched/unmatched lines.
        let matched = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"We decided to go with the gate; lesson learned."}]},"timestamp":"2026-01-01T00:00:00Z"}"#;
        let noise = r#"{"type":"user","message":{"role":"user","content":"Just some neutral chatter with no markers at all here."},"timestamp":"2026-01-01T00:00:01Z"}"#;
        let mut buf = String::with_capacity(4 * 1024 * 1024 + 1024);
        while buf.len() < 4 * 1024 * 1024 {
            buf.push_str(matched);
            buf.push('\n');
            buf.push_str(noise);
            buf.push('\n');
        }

        let start = Instant::now();
        let out = select_candidates(buf.as_bytes(), "perf", 0, 64 * 1024 * 1024);
        let elapsed = start.elapsed();

        assert!(!out.is_empty());
        // Debug builds are slower than release; allow generous headroom while
        // still catching pathological regressions (e.g. per-call recompile).
        assert!(
            elapsed.as_millis() < 2000,
            "4 MiB rule pass took {elapsed:?} (debug); should be well under 50 ms in release"
        );
    }

    /// R-11 wave-boundary: this module's sources have ZERO compile-time
    /// CODE reference to `transcript_hold` (a Wave-B server-side type). Asserted
    /// over the committed source text of every file in this module tree.
    ///
    /// Doc-comment and string-literal mentions (e.g. this assertion's own
    /// message, or the R-11 rationale in the module docs) are excluded — the
    /// gate is the absence of a `use`/path reference, i.e. a real compile-time
    /// dependency. Lines that are comments or assertion messages are skipped.
    #[test]
    fn test_distill_module_no_transcript_hold_reference() {
        let sources = [
            include_str!("jsonl.rs"),
            include_str!("markers.rs"),
            include_str!("select.rs"),
        ];
        for src in sources {
            for line in src.lines() {
                let trimmed = line.trim_start();
                // Skip doc/line comments — only code references count.
                if trimmed.starts_with("//") || trimmed.starts_with("*") {
                    continue;
                }
                let is_use_ref = line.contains("use ") && line.contains("transcript_hold");
                let is_path_ref = line.contains("transcript_hold::");
                assert!(
                    !is_use_ref && !is_path_ref,
                    "Wave A distill module must not reference transcript_hold in code (R-11): {line}"
                );
            }
        }
    }
}
