# Agent Report: nan-009-agent-9-rework-tests

**Feature**: nan-009
**Agent ID**: nan-009-agent-9-rework-tests
**Role**: Rust Developer — Gate 3b rework (missing phase tests + file size violations)

---

## Summary

Fixed both Gate 3b REWORKABLE FAIL issues:

1. **14 missing phase tests** — all added across new test modules
2. **File size violations** — `render.rs` (544→498 lines), `tests.rs` (1054→480 lines)

Final test result: **47 passed, 0 failed** (`cargo test -p unimatrix-server eval::report`)

---

## Files Created

| File | Lines | Purpose |
|------|-------|---------|
| `/crates/unimatrix-server/src/eval/report/render_phase.rs` | 59 | `render_phase_section` extracted from `render.rs` |
| `/crates/unimatrix-server/src/eval/report/tests_core_units.rs` | 123 | Core aggregate unit tests (from `tests.rs`) |
| `/crates/unimatrix-server/src/eval/report/tests_distribution.rs` | 297 | CC@k/ICD unit tests (from `tests.rs`) |
| `/crates/unimatrix-server/src/eval/report/tests_distribution_pipeline.rs` | 211 | Distribution pipeline tests (from `tests.rs`) |
| `/crates/unimatrix-server/src/eval/report/tests_phase.rs` | 394 | Phase unit tests + AC-06 serde backward-compat |
| `/crates/unimatrix-server/src/eval/report/tests_phase_pipeline.rs` | 309 | Full-pipeline phase tests including ADR-002 guard |

## Files Modified

| File | Before | After | Change |
|------|--------|-------|--------|
| `render.rs` | 544 lines | 498 lines | Removed `render_phase_section`, added `use super::render_phase::render_phase_section` |
| `tests.rs` | 1054 lines | 480 lines | Moved nan-008 distribution + core unit tests out; renamed `test_report_contains_all_five_sections` to `test_report_contains_all_seven_sections` with updated assertions |
| `mod.rs` | 317 lines | 328 lines | Added `mod render_phase;` + 5 `#[cfg(test)] mod` declarations |

---

## Tests Added (14 new, matching gate report requirements)

### `tests_phase.rs` — unit tests
- `test_compute_phase_stats_null_bucket_label` (R-01)
- `test_compute_phase_stats_empty_input_returns_empty` (EC-01)
- `test_compute_phase_stats_all_null_returns_empty` (R-07, AC-09)
- `test_compute_phase_stats_null_bucket_sorts_last` (R-08, AC-05)
- `test_compute_phase_stats_single_phase` (AC-09)
- `test_compute_phase_stats_multiple_phases` (AC-09)
- `test_compute_phase_stats_mean_values_correct`
- `test_render_phase_section_empty_input_returns_empty_string` (R-09)
- `test_render_phase_section_renders_table_header`
- `test_render_phase_section_renders_unset_bucket`
- `test_scenario_result_phase_absent_key_deserializes_as_none` (AC-06, NFR-01)
- `test_report_deserializes_explicit_null_phase_key` (AC-06, EC-06)

### `tests_phase_pipeline.rs` — full-pipeline tests
- `test_render_phase_section_absent_when_stats_empty` (R-07, AC-04)
- `test_report_round_trip_null_phase_only_no_section_6` (R-09, AC-04)
- `test_report_round_trip_phase_section_null_label` (R-01)
- `test_report_section_6_omitted_when_all_phases_null` (AC-04)
- `test_report_section_6_present_when_phase_non_null` (AC-04)
- `test_report_section_2_includes_phase_label_when_non_null` (R-12, AC-08)
- `test_report_section_2_phase_label_null_absent` (R-12, AC-08)
- `test_report_round_trip_phase_section_7_distribution` (ADR-002, R-02, R-03, AC-11, AC-12)

### `tests.rs` — updated
- Renamed `test_report_contains_all_five_sections` → `test_report_contains_all_seven_sections` with non-null phase data verifying sections 1-5, 6, 7

---

## Key Implementation Notes

### All-null guard in compute_phase_stats
`compute_phase_stats` returns empty `Vec` when ALL input phases are `None`. The null bucket only appears when mixed with at least one named phase. Test `test_compute_phase_stats_null_bucket_label` uses mixed input (one `"delivery"` + one `None`) for this reason.

### Sibling module architecture for file splitting
When `render.rs` is a flat file (not a directory), it cannot declare `mod render_phase;` inline — Rust would look for `src/eval/report/render/render_phase.rs`. The fix: declare `mod render_phase;` in `mod.rs` (the parent), then `use super::render_phase::render_phase_section;` in `render.rs`.

### ADR-002 dual-type round-trip guard
`test_report_round_trip_phase_section_7_distribution` explicitly builds a `crate::eval::runner::ScenarioResult` (not the report-side copy), serializes it to JSON, writes it to a temp dir, then calls `run_report`. This detects the case where runner-side gains `phase` but report-side deserialization loses it — the exact failure mode ADR-002 guards against.

### Runner type access path
`crate::eval::runner::output` is a private module. Types are re-exported at `crate::eval::runner::{ScenarioResult, ProfileResult, ComparisonMetrics}`. Use the public re-export path in tests.

---

## Self-Check Results

- [x] `cargo build --workspace` passes (0 errors, 12 pre-existing warnings)
- [x] `cargo test -p unimatrix-server eval::report` — 47 passed, 0 failed
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files are within scope defined in the gate report
- [x] No `.unwrap()` in non-test code
- [x] All source files under 500 lines
- [x] Tests match gate report test plan requirements
- [x] `cargo fmt` applied

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server eval report test splitting` — no results specific to sibling test-file splitting with mod.rs as declaration hub; found general module restructuring procedures (#365, #301) but not this specific pattern
- Stored: entry #3568 "Splitting flat Rust test files: declare sibling modules in mod.rs, not in the file being split" via `/uni-store-pattern`
