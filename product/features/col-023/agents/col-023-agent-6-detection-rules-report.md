# Agent Report: col-023-agent-6-detection-rules

**Feature:** col-023 — Observation Pipeline Generalization
**Wave:** 3 — Detection Rules & Extraction Modules
**Agent ID:** col-023-agent-6-detection-rules
**Issue:** #331

## Task

Rewrite all 21 detection rules and 5 extraction modules in `unimatrix-observe` to use `event_type: String` + `source_domain: String` instead of the removed `HookType` enum. Apply mandatory ADR-005 `source_domain == "claude-code"` guard as the absolute first operation in every `detect()` and `evaluate()` implementation.

## Files Modified

- `crates/unimatrix-observe/src/detection/agent.rs` — 7 rules rewritten
- `crates/unimatrix-observe/src/detection/friction.rs` — 4 rules rewritten
- `crates/unimatrix-observe/src/detection/session.rs` — 5 rules rewritten
- `crates/unimatrix-observe/src/detection/scope.rs` — 5 rules rewritten
- `crates/unimatrix-observe/src/detection/mod.rs` — `find_completion_boundary` signature changed, test helpers updated
- `crates/unimatrix-observe/src/extraction/knowledge_gap.rs`
- `crates/unimatrix-observe/src/extraction/implicit_convention.rs`
- `crates/unimatrix-observe/src/extraction/file_dependency.rs`
- `crates/unimatrix-observe/src/extraction/dead_knowledge.rs`
- `crates/unimatrix-observe/src/extraction/recurring_friction.rs`
- `crates/unimatrix-observe/src/session_metrics.rs`
- `crates/unimatrix-observe/src/attribution.rs`
- `crates/unimatrix-observe/src/report.rs`
- `crates/unimatrix-observe/src/types.rs`
- `crates/unimatrix-observe/src/baseline.rs` — pre-existing `domain_metrics` missing field fixed
- `crates/unimatrix-observe/tests/extraction_pipeline.rs`

## Test Results

- `cargo test -p unimatrix-observe`: **401 passed, 0 failed** (357 lib + 44 integration)
- `cargo fmt`: applied
- `cargo clippy`: 52 pre-existing warnings; one new warning (`iter().copied().collect()`) fixed before final run

## Implementation Notes

### ADR-005 Source Domain Guard Pattern

Every `detect()` and `evaluate()` method starts with:

```rust
let records: Vec<&ObservationRecord> = records
    .iter()
    .filter(|r| r.source_domain == "claude-code")
    .collect();
```

This re-binds `records` as a filtered `Vec<&ObservationRecord>`. All subsequent logic uses the filtered slice. For extraction modules that iterate `observations`, the same pattern rebinds via `filtered` or re-uses `observations` variable.

### `find_completion_boundary` Signature Change

Changed from `&[ObservationRecord]` to `&[&ObservationRecord]` to accept pre-filtered slices. Callers in `session.rs` and `scope.rs` pass `&records` directly (where `records` is already the filtered `Vec<&ObservationRecord>`).

### `hook_type` Constants Module Path

The `hook_type` constants module is at `unimatrix_core::observation::hook_type`, **not** re-exported at the crate root. Using `use unimatrix_core::hook_type` fails to compile. The correct import is `use unimatrix_core::observation::hook_type`.

### Pre-existing Blocking Issue Fixed

`baseline.rs` had two `MetricVector` struct literals missing the `domain_metrics` field added by Wave 2. These blocked compilation of the entire `unimatrix-observe` test suite. Fixed by adding `domain_metrics: Default::default()` to both test helpers (`make_mv` and `make_mv_with_phases`).

### `recurring_friction` Test Logic

The original test helper constructed only `PostToolUse` records. `PermissionRetriesRule` fires on the differential between `PreToolUse` count and `PostToolUse` count (retries = pre - post). Rewrote the test helper to produce 5 `PreToolUse` + 2 `PostToolUse` = 3 retries, which exceeds the threshold of 2.

### Workspace Build Note

`unimatrix-server` imports `HookType` from `unimatrix_observe::types`. This is a pre-existing Wave 1 issue (out of scope for this agent). `unimatrix-observe` itself compiles and tests cleanly.

## Issues Encountered

- `git stash pop` failed at session start (Wave 2 store changes conflicted). All edits were re-applied from scratch by reading current file state and making targeted edits.
- `unimatrix_core::hook_type` import path incorrect — resolved by using full path `unimatrix_core::observation::hook_type`.
- Pre-existing `domain_metrics` compile errors in `baseline.rs` blocked tests — fixed as collateral.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for unimatrix-observe detection rules ObservationRecord — found ADR-005 (#2907), ADR-001 (#2903), and blast radius pattern (#2843). All relevant ADRs already stored by previous agents.
- Stored: pattern via `/uni-store-pattern` — "source_domain guard preamble in unimatrix-observe detect()" documenting the exact code pattern, the `find_completion_boundary` signature change, and the `hook_type` module path trap.
