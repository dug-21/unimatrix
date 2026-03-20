# Agent Report: nan-007-agent-11-rework-runner

**Task**: Gate 3b rework — fix two failing checks in eval/runner.rs
**Feature**: nan-007
**Date**: 2026-03-20

---

## What Was Done

### Issue 1: Embed Model Wait Loop

Added the 30 × 100 ms embed model readiness poll loop to `run_eval_async` in `runner/mod.rs`, per pseudocode `eval-runner.md` lines 148–158.

The loop is implemented in `eval/runner/layer.rs` as `wait_for_embed_model()`. It polls `embed_handle.get_adapter().await` up to 30 times with 100 ms between each attempt. On success it returns `Ok(())`; on exhaustion it returns a descriptive error including the profile name.

**Embed handle method used**: `EmbedServiceHandle::get_adapter()` (line 116 of `infra/embed_handle.rs`). This returns `Result<Arc<EmbedAdapter>, ServerError>` — `EmbedNotReady` when still loading, `EmbedFailed` when permanently failed.

Two bugs were found and fixed in the pre-existing `profile/` submodule tree:
1. `profile/layer.rs` line 162: `Arc::new(EmbedServiceHandle::new())` was double-wrapping since `EmbedServiceHandle::new()` already returns `Arc<Self>`. Fixed to `EmbedServiceHandle::new()`.
2. `profile/mod.rs` line 25: `pub use validation::parse_profile_toml` re-exported a `pub(crate)` function as `pub`. Fixed to `pub(crate) use`.

### Issue 2: runner.rs Split (500-line limit)

Replaced `eval/runner.rs` (1084 lines) with a submodule tree at `eval/runner/`:

| File | Lines | Responsibility |
|------|-------|---------------|
| `mod.rs` | 149 | Public API (`run_eval`), `run_eval_async`, module declarations |
| `layer.rs` | 63 | `wait_for_embed_model()` — embed readiness poll loop |
| `replay.rs` | 169 | Scenario loading, replay loop, `run_single_profile` |
| `metrics.rs` | 215 | P@K, MRR, Kendall tau, rank changes, ground truth resolution |
| `output.rs` | 90 | Result structs, `write_scenario_result` |
| `tests.rs` | 195 | Runner I/O + entry point tests |
| `tests_metrics.rs` | 276 | Metric function tests |

All files are under 500 lines. The `eval/mod.rs` `pub mod runner;` declaration required no change.

---

## Files Created/Modified

**New files (runner/ submodule tree)**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/mod.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/layer.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/replay.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/metrics.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/output.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/tests.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner/tests_metrics.rs`

**Modified files (profile/ bug fixes)**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/profile/layer.rs` — removed double-Arc wrapping
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/profile/mod.rs` — fixed `pub use` → `pub(crate) use`

**Deleted**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner.rs` (replaced by runner/ directory)

All changes were incorporated into commit `4b3fb3c` by agent-12's concurrent rework commit.

---

## Tests

`cargo test -p unimatrix-server --lib 2>&1 | tail -5`:
`test result: ok. 1588 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`

No new failures introduced.

---

## Issues / Blockers

None. The `scenarios.rs` deletion (unstaged, not my scope) remains for another agent or the coordinator to commit.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "unimatrix-server eval runner module split submodule" — no direct matches; result #299 (Sequential Module Migration Pattern) is tangentially related but not specific to eval runner.
- Stored: nothing novel to store — the runner submodule split follows the existing `eval/report/` pattern already established in this feature. The `EmbedServiceHandle::new()` double-Arc gotcha is worth storing as a pattern trap.

**Notable runtime gotcha found**: `EmbedServiceHandle::new()` returns `Arc<Self>` — wrapping with `Arc::new(...)` creates `Arc<Arc<EmbedServiceHandle>>`, which compiles but causes a type mismatch at the struct field assignment. This is invisible until you reach the struct construction site, not at the `Arc::new(...)` call itself.
