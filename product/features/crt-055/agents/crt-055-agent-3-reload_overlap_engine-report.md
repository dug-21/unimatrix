# Agent Report — crt-055 Component 4: Reload Overlap Engine

**Agent**: crt-055-agent-3-reload_overlap_engine
**Component**: 4 — Reload overlap engine (`context_reload` + `compaction_reread`, one engine)
**Crate**: unimatrix-observe | **Wave**: 3

## Summary

Implemented the ONE generic file-set-intersection overlap primitive parameterized by
`ReloadWindow` (`CrossSession` | `PostCompaction { boundary_secs }`), with the
`CrossSession` caller wired for `context_reload`. Produced `context_reload_pct` as an
`i64` basis-points value via `round(fraction × 10000)` clamped to `0..=10000` (ADR-005).
The `PostCompaction` window is exposed (primitive + enum variant) for Component 5 to
drive; the compaction caller is NOT implemented here per scope.

### Design decision: new focused module

`session_metrics.rs` was already 1013 lines (over the 500-line limit). Rather than bloat
it further, I created `reload_overlap.rs` (focused, ~480 lines incl. tests) to host the
engine. `compute_context_reload_pct` was refactored to a thin `CrossSession` caller of the
shared `overlap_count` primitive — the cross-session intersection body now lives once in
the new module (R-07 "one engine", no duplicated intersection logic). `extract_file_path`
in `session_metrics.rs` was made `pub(crate)` for reuse.

### Encoding (load-bearing)

The live `compute_context_reload_pct` returns a FRACTION in `[0.0, 1.0]` (confirmed at
`session_metrics.rs:47`), NOT a 0–100 percentage. Per OVERVIEW Open-Q, basis-points
encoding is `round(fraction × 10000)` (0.375 → 3750), honoring the ADR worked example.
No `f64` reaches any column; there is no REAL column and no `is_finite()` guard — the
#4529/#4533 push_bind(f64) non-finite footgun is designed out by integer storage.

### Two columns, never collapsed (AC-13 / R-07)

The two windows are distinct call sites; `test_neither_window_derived_from_other` pins
that varying the compaction boundary leaves the cross-session output invariant and vice
versa. `CycleAggregates.context_reload_pct` (already present, set by Component 2 wiring)
receives `reckon_context_reload_bps`; `compaction_reread_count` / `compaction_count` are
Component 5's to populate via the exposed primitive.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-observe/src/reload_overlap.rs` (NEW) — `ReloadWindow` enum, `OverlapCounts`, `overlap_count` primitive, `fraction_to_basis_points`, `reckon_context_reload_bps`, 13 unit tests
- `/workspaces/unimatrix/crates/unimatrix-observe/src/session_metrics.rs` — `compute_context_reload_pct` refactored to call the shared primitive; `extract_file_path` → `pub(crate)`; dropped now-unused `HashSet` import
- `/workspaces/unimatrix/crates/unimatrix-observe/src/lib.rs` — declared `pub mod reload_overlap`; re-exported `OverlapCounts`, `ReloadWindow`, `fraction_to_basis_points`, `overlap_count`, `reckon_context_reload_bps`

## Tests

- New `reload_overlap` unit tests: **13 passed, 0 failed**
  - AC-20 basis-points: encode (37.5%→3750, 0.0→0, 1.0→10000), round-to-nearest (0.00005→1, 0.99995→10000, 2/3→6667), out-of-range clamp (1.5/2.0→10000, -0.5→0), no-float-column structural
  - AC-14 basis-points range round-trip (3750 ↔ 37.5%, always-in-range)
  - AC-13 / R-07 dual-not-collapsed (context_reload side): pure-window-input primitive, cross-session window not gated on compacted_at, compaction window gated on boundary, neither derived from the other
- Full observe lib suite: **564 passed, 0 failed** (no regression from the `compute_context_reload_pct` refactor)
- `cargo build -p unimatrix-observe`: clean
- `cargo clippy -p unimatrix-observe --lib`: 0 warnings in `reload_overlap.rs` (53 pre-existing warnings in other modules, untouched)
- `cargo fmt -p unimatrix-observe`: applied
- Did NOT run workspace-wide cargo or pytest integration tests per scope.

## Issues / Blockers

None.

- Note for Component 5: drive `overlap_count(records, ReloadWindow::PostCompaction { boundary_secs }, &[])` per session with `MIN(compacted_at)` as `boundary_secs`. The `summaries` arg is unused for the `PostCompaction` window (pass an empty slice). The primitive counts each distinct prior-set file re-read after the boundary once per session; the per-session `MIN(compacted_at)` selection and `compaction_events` read accessor remain Component 5's to own.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern "reload overlap engine cross-session window basis points") + (decision topic crt-055) + context_get #5047 (ADR-005). Findings: ADR-005 confirms two-columns/two-gates/one-engine + basis-points integer storage to dodge #4529/#4533; lessons #4529/#4533 are designed out here (no REAL column, no f64 bind). ADR text phrases source as "percentage × 100" but live fn returns a fraction → used `× 10000` per OVERVIEW Open-Q (worked example 37.5%→3750 holds either way).
- Stored: nothing novel to store — the basis-points-from-fraction encoding and the one-engine/two-window split are already captured in ADR-005 (#5047) and the pseudocode Open-Q; no gotcha emerged beyond what is already recorded. The module-split decision (avoid bloating the 1013-line session_metrics.rs) is a routine 500-line-rule application, not a reusable pattern.
