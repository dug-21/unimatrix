# Agent Report — crt-055 Component 6: Activity-fold landing

**Agent**: crt-055-agent-3-activity_fold_landing
**Component**: 6 — Activity-fold landing (read-before-purge, width conversion, JSON)
**Wave**: 3

## Summary

Implemented the self-contained activity-fold landing helper. Given a
`feature_cycle`, it calls crt-054's `activity_snapshots_for_feature(fc)` to read
each held/registered session's `ActivitySnapshot`, sums across the cycle's
sessions in producer widths (saturating), width-converts `u64`/`u32` → `i64`
(checked/saturating, never wraps — AC-14/R-09), and lands `transcript_bytes_total`,
`transcript_delta_count`, `transcript_error_count` (`class_counts[0]`),
`transcript_refusal_count` (`class_counts[1]`), and `signal_class_counts_json`
(full `class_name → count` map via serde_json). Content-free (integers + count
map only) — structural leak gate intact (NFR-01).

Placed in a new module `mcp/activity_fold_handler.rs`, mirroring the existing
`distill_handler.rs` read-before-purge pattern. The module performs the READ
only; Component 9 (later wave, same `tools.rs` pipeline) places the
`land_activity_fold(...)` call STRICTLY BEFORE `purge_cycle_transcripts`. A
module-header comment + a doc note on `land_activity_fold` mark this integration
contract and flag the `#![allow(dead_code)]` to remove once Component 9 wires it.

## Design notes / decisions

- **Pure-fold split**: `land_activity_fold` (registry read) delegates to a pure
  `fold_snapshots(&[(String, ActivitySnapshot)], &[String])`. The near-`u64::MAX`
  saturation (AC-14) is unreachable through the real producer, so it is unit-tested
  by hand-building `ActivitySnapshot` literals and calling the pure helper.
- **Class names source**: `SessionRegistry` holds the compiled `SignatureScanner`,
  NOT the class names. Added `TranscriptSignalsConfig::enabled_class_names()`
  (parallel to `enabled_patterns()`, same enabled-set + config order) so Component 9
  supplies the names as a `&[String]` param. Resolves the pseudocode 6b OPEN-Q:
  names come from the startup-validated `[transcript_signals]` config, read by index.
- **JSON**: built with `serde_json::Map` (serde_json has `preserve_order` enabled),
  so config order is stable and JSON-special class names are escaped — never
  string concatenation. Empty catalog → `"{}"`.

## Files modified

- `crates/unimatrix-server/src/mcp/activity_fold_handler.rs` (new — landing fn, FoldLanding, width conversion, JSON builder)
- `crates/unimatrix-server/src/mcp/activity_fold_handler_tests.rs` (new — 16 unit tests)
- `crates/unimatrix-server/src/mcp/mod.rs` (registered `activity_fold_handler` module)
- `crates/unimatrix-server/src/infra/config.rs` (added `enabled_class_names()` accessor + parallel test coverage exists via existing config tests)

## Tests

- `cargo test -p unimatrix-server --lib activity_fold_handler`: **16 passed, 0 failed**.
- `cargo test -p unimatrix-server --lib transcript_signals` (regression after config accessor): **21 passed, 0 failed**.
- `cargo build -p unimatrix-server`: clean.
- `cargo clippy -p unimatrix-server --lib`: no new warnings from this code.
- `cargo fmt -p unimatrix-server`: applied.

Unit coverage:
- AC-07: `test_fold_lands_bytes_and_delta`, `test_fold_lands_class_counts_by_pinned_index`
  (R-12 fixed-index), `test_fold_sums_across_held_sessions`,
  `test_signal_class_counts_json_matches_catalog`,
  `test_signal_json_forward_compatible_beyond_error_refusal` (NFR-06),
  `test_signal_json_empty_catalog_is_empty_object`,
  `test_signal_json_class_name_with_special_chars_is_escaped`.
- AC-14/R-09: `test_fold_width_conversion_saturates`, `test_fold_summation_saturates_at_i64_max`.
- AC-19/R-11: `test_consumed_surface_is_metadata_only`.
- R-04 availability: `test_empty_cycle_is_unavailable_not_zero`,
  `test_present_session_with_zero_buffer_is_available`,
  `test_land_activity_fold_undeclared_cycle_is_unavailable`,
  `test_land_activity_fold_undeclared_session_does_not_zero_valid`.
- Registry end-to-end: `test_land_activity_fold_reads_registered_session`.

## Deferred to Stage 3c / Component 9 (not unit-testable here)

- **AC-08 (read-before-purge ordering + inversion)** — requires the real pipeline
  call order; the inversion test (purge first → zeroed columns) must manipulate
  call order in the integration layer.
- **AC-09 (held-route silent-zero harness guard)** — full review through the binary
  with held activity → `transcript_*` columns non-zero end-to-end.
- The `land_activity_fold(...)` call must be PLACED before `purge_cycle_transcripts`
  by Component 9; remove the module's `#![allow(dead_code)]` then.

## Issues / blockers

None. All scope kept to the self-contained landing function + module; no broader
pipeline reordering (that is Component 9). No git operations performed (Delivery
Leader owns git).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern + decision) + `context_get` on
  ADR-007 (#5042) and ADR-008 (#5043) — confirmed read-before-purge, fixed catalog
  indices (`[0]=error`/`[1]=refusal`), checked/saturating width conversion, and the
  forward-compatible JSON map contract. No prior pattern covered this fold-landing.
- Stored: entry #5064 "Split registry-reading fold from pure summation to unit-test
  u64::MAX saturation" via `/uni-store-pattern` (topic `unimatrix-server`).
