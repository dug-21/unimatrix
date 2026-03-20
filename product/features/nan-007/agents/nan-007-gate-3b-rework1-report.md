# Agent Report: nan-007-gate-3b-rework1

> Gate: 3b (Code Review) — Rework Iteration 1
> Agent ID: nan-007-gate-3b-rework1
> Date: 2026-03-20
> Result: PASS

## Gate Summary

All four rework items from the previous REWORKABLE FAIL are resolved. Gate passes with two WARNs (one pre-existing, one architectural).

## Rework Items — Resolution Status

| Rework Item | Resolution |
|-------------|-----------|
| FR-24/C-02: `SqlxStore::open()` on snapshot | RESOLVED — `SqlxStore::open_readonly()` added; no migration, no drain task, analytics receiver immediately dropped |
| Embed model wait loop missing | RESOLVED — `runner/layer.rs::wait_for_embed_model()` with 30 × 100 ms loop; called in `runner/mod.rs` line 129 |
| File line limits (profile.rs 1031, runner.rs 1084, scenarios.rs 900) | RESOLVED — all three split into submodule trees; all files now under 500 lines |
| AC-14: `HookPayloadTooLargeError` not a `ValueError` subclass | RESOLVED — now `class HookPayloadTooLargeError(HookClientError, ValueError)`; two test assertions lock the contract |

## Key Findings

**ADR-002 Option A/B deviation (WARN)**: The rework chose Option A (`SqlxStore::open_readonly()`) which ADR-002 explicitly rejected in favor of Option B (raw pool). The rework was forced by an API constraint: `VectorIndex::new()` requires `Arc<SqlxStore>`, not a raw `SqlitePool`. ADR-002's behavioral intent (no migration, no drain task, analytics suppressed) is fully satisfied by `open_readonly()`. The spec's "NOT in scope" clause that stated "`SqlxStore::open_readonly()` is not added" is violated, but the violation was necessary and technically sound.

**`report/tests.rs` at 531 lines (WARN)**: Pre-existing from original delivery commit `886c566`. Not a rework regression. Not directed in the rework scope. Prior gate report incorrectly stated all report/ files were under 500 lines.

## Build and Test Results

- `cargo build --workspace`: clean (6 pre-existing warnings, zero errors)
- `cargo test -p unimatrix-server --lib`: 1588 passed, 0 failed

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "gate 3b rework spec deviation SqlxStore eval snapshot" — found entries #1203, #2618, #723
- Queried: `/uni-query-patterns` for "spec NOT in scope clause violated rework architectural deviation" — found entry #723
- Stored: nothing novel to store — the `open_readonly()` pattern was stored by agent-10 (the rework implementer). The pre-existing `report/tests.rs` oversight is covered by entry #1203. No new cross-feature patterns.
