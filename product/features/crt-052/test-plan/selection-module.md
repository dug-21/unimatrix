# Test Plan — C3 Candidate Selection Module

**Component**: `unimatrix-observe/src/distill/` — `jsonl.rs`, `markers.rs`,
`select.rs` (`select_candidates(bytes, session_id, base_offset, session_cap) -> Vec<TranscriptCandidate>`),
`mod.rs`. **ADRs**: ADR-003, ADR-002 (byte_offset). **Wave**: A. Pure: no I/O, no locks, no tracing.
**Tests live in**: `#[cfg(test)] mod tests` in each module (synthesis.rs style) over committed
fixtures. **Merge gates**: AC-V-FUZZ, content-leak (no parse under lock — N/A here, pure). Carries
AC-03 fixture-recall and the fuzz corpus.

## Unit Test Expectations — jsonl.rs (parse, untrusted-input-hardened)
- `test_jsonl_keeps_user_assistant_text_blocks` — parse Claude Code JSONL; assert only user/assistant
  TEXT blocks survive; `tool_use`, `tool_result`, `thinking`, command-noise are dropped.
- `test_jsonl_unknown_record_type_skip_with_count` — unknown record type → dropped, skip-count
  incremented, never `Err`.
- `test_jsonl_tolerates_truncated_final_line` — a truncated final JSON line (ring-tail/hole boundary)
  → skip-with-count, prior lines parsed; no panic.
- `test_jsonl_operates_on_bytes` — entry point takes `&[u8]` (not `&str`); non-UTF-8 inside a line is
  tolerated via skip-with-count, not a `from_utf8` panic.

### AC-V-FUZZ corpus (**merge gate**, R-10) — `jsonl.rs` + `select.rs` level
- `test_jsonl_truncated_json_no_panic`
- `test_jsonl_non_utf8_bytes_no_panic`
- `test_jsonl_oversized_single_line_bounded` — gigantic single line → bounded handling, no resource
  exhaustion, skip-with-count.
- `test_jsonl_unknown_record_type_no_panic`
- `test_jsonl_embedded_nul_no_panic`
- `test_jsonl_deeply_nested_json_bounded` — billion-laughs-style nesting → bounded, no stack overflow.
- `test_select_candidates_fully_corrupt_input_returns_empty` — every line corrupt → returns empty Vec
  (or only the parseable subset), skip-count == line-count, never `Err`/panic.
  All assert: **skip-with-count, no `Err`, no panic.** Drive from a committed `corpus/malformed/`
  fixture set so the corpus is reviewable and extensible.

## Unit Test Expectations — markers.rs (four families)
- `test_markers_four_families_match` — the four families (Decision, Rework, Lesson, PhaseGate; ~50
  patterns ported from ass-070 `extractor.py`) each match representative blocks → yield the correct
  advisory `FamilyHint`.
- `test_markers_built_once_oncelock` — regex-class set built once via `OnceLock`; no per-call compile.
- `test_markers_hint_is_advisory_only` — a matched block produces `Vec<FamilyHint>` that is advisory
  and non-empty; assert no semantic classification beyond the family label (rules select, agent
  extracts — Constraint 6). No `FamilyHint` carries extracted meaning.
- `test_markers_no_heavyweight_runtime_dep` — regex-class crate only (AC-13); structural/dep check.

## Unit Test Expectations — select.rs (the pipeline, AC-02)
- `test_select_keeps_matched_blocks_whole` — matched user/assistant blocks kept WHOLE, no windowing
  (ass-070 ablation: windowing loses multi-paragraph context). Assert returned `text` is the full
  block.
- `test_select_drops_unmatched_blocks` — blocks matching no family are dropped.
- `test_select_dedup` — duplicate matched blocks deduped.
- `test_select_per_session_cap` — candidates exceeding `session_cap` (default 24 KB) truncated at the
  per-session cap; assert the cap is honored and the truncation surfaces (drives AC-08 dropped-count).
- `test_select_orders_chronologically` — output ordered by `(ts, session_id, byte_offset)`; stable.
- `test_select_populates_fields` — each `TranscriptCandidate` has `session_id`, logical `byte_offset`,
  `ts` (Option), non-empty `family_hints`, `text`.

### byte_offset logical semantics (R-12, ADR-002)
- `test_byte_offset_logical_under_overflow` — `base_offset > 0` (overflowed snapshot) → each candidate
  `byte_offset == base_offset + in_snapshot_offset`.
- `test_byte_offset_equals_in_snapshot_when_no_overflow` — `base_offset == 0` → `byte_offset` equals
  the in-snapshot offset.
- `test_candidate_ordering_stable_across_elision` — the `(ts, session_id, byte_offset)` key is stable
  and meaningful across an elision event.

## AC-03 — Independent fixture recall/volume (**review gate**, R-20)
- **Fixture**: committed labeled corpus at `crates/unimatrix-observe/src/distill/corpus/` (or
  `fixtures/`) with a **provenance header** asserting independence mode — `anchors-before-port` OR
  `different-author` — plus authoring order/author. This header is a Stage-3a review gate; Stage-3c
  verifies it is present and asserts one of the two modes (`test_corpus_provenance_header_present`).
- `test_independent_corpus_recall_ge_090` — block-level recall of in-block labeled items ≥ 0.90
  against the independent labels.
- `test_selected_volume_le_10pct` — selected candidate volume ≤ 10% of raw fixture bytes.
- **Anti-circularity note for delivery**: the fixture MUST NOT be authored from the regex set it
  validates, or ≥0.90 is self-fulfilling (R-20). The header + a different author/order is the only
  enforcement; reviewer-policed.

## AC-12 — throughput
- `test_select_4mib_under_50ms` (or bench) — full rule pass over a 4 MiB fixture < 50 ms (pure Rust
  over in-memory bytes; ass-070 <10ms estimate gives margin). Off-lock by construction (pure module).

## Wave-boundary (R-11)
- `test_distill_module_no_transcript_hold_reference` — source/dep assertion: `distill/*` has zero
  compile-time reference to `transcript_hold.rs`. (Also asserted in distill-handler.md as the
  dependency-direction gate.)

## Assertions Summary (concrete)
- `select_candidates(bytes, sid, base, cap)` is pure, total (never panics, never `Err`), and returns
  only `Vec<TranscriptCandidate>` — no `Result` carrying content.
- Every malformed-corpus case returns parseable candidates + a skip-count, never panics.
- Recall ≥0.90 and volume ≤10% asserted against an independently-authored, provenance-headed corpus.
