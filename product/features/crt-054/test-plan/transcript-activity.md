# Test Plan — `transcript_activity` module + `SignatureScanner`

**Component**: new `infra/transcript_activity.rs` — the fold logic host + `SignatureScanner` (compiles `[transcript_signals]` into one shared `RegexSet`; one byte scan per delta).
**Pseudocode**: `pseudocode/transcript-activity.md` · **Layer**: unit.
**Anchor ACs**: AC-09 (one shared scan per delta, multi-class). **Risks**: R-10 (silent fallback — partial), security (ReDoS bound).

## Unit Test Expectations

`crates/unimatrix-server/src/infra/transcript_activity_tests.rs`.

1. `test_scanner_single_scan_increments_matched_classes` — Arrange: `SignatureScanner` built from the v1 catalog (`error`→0, `refusal`→1). Act: fold a delta whose bytes match BOTH the error and refusal patterns. Assert: `class_counts[0] >= 1` AND `class_counts[1] >= 1` from a SINGLE scan pass. (FR-B3/B5, AC-09.)
2. `test_scanner_one_pass_not_per_pattern` — assert the scan is invoked once per delta (one `RegexSet::matches`/Aho-Corasick pass), not one regex pass per class. Verify via a `RegexSet` (returns a `SetMatches` from one call) — structural assertion that the scanner holds one `RegexSet`, not `Vec<Regex>`.
3. `test_scanner_no_match_leaves_class_counts_zero` — a delta matching no pattern increments neither class; `delta_count`/`bytes_total` still advance (those are the accumulator's, not the scanner's).
4. `test_scanner_match_count_is_per_delta_not_per_occurrence` — a delta containing the `error` signature twice increments `class_counts[0]` by 1 (per-delta-per-class match, FR-B3), not by occurrence count. (Confirm the FR semantics: "+= per delta from one shared `RegexSet` pass" — one increment per matched class per delta.)
5. `test_scanner_built_once_at_load` — the `RegexSet` is compiled once (at config load / scanner construction), not per delta. Structural: the scanner owns the compiled set; `fold`/scan borrows it.

## Security / bound

6. `test_scanner_regexset_is_linear_time` — documented assertion: classes compile into Rust `regex` `RegexSet` (linear-time, no catastrophic backtracking). A pathological-but-valid operator pattern is bounded by `MAX_SIGNAL_CLASSES` + linear matching. (Security risk: ReDoS structurally bounded; not externally exploitable — config is operator-trusted.)

## Edge cases

- Delta matching multiple classes → multiple `class_counts` from one pass (test 1).
- Empty delta → no spurious class match (covered with activity-counters test 3).

## Notes
- Catalog default-set correctness (`error`/`refusal` only, indices) and `validate()` rejection live in `transcript-signals-config.md`; this file tests the scanner's SCAN behavior given a valid compiled catalog.
- Invalid-regex rejection is `validate()`'s job (config plan), so the scanner is only ever built from a validated catalog — assert the scanner constructor takes already-validated patterns.
