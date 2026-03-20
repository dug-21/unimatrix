# Agent Report: nan-007-agent-6-eval-runner

**Feature**: nan-007 — Evaluation Harness
**Component**: D3 — In-Process A/B Scenario Replay Engine (`eval/runner.rs`)
**Commit**: `030dd11`

## Deliverables

### Files Created

- `/workspaces/unimatrix/crates/unimatrix-server/src/eval/runner.rs` (1084 lines, 27 unit tests)

### Files Modified

None — `eval/mod.rs` and `Cargo.toml` were already updated by the Wave 2 sibling agents (eval/scenarios + eval/report) before this agent ran.

## Implementation Summary

Implemented `run_eval()` — the synchronous CLI entry point for D3 scenario replay. The runner:

1. Validates `k > 0` (`EvalError::InvalidK`) and detects profile name collisions (`EvalError::ProfileNameCollision`) before any I/O.
2. Canonicalizes the DB path via `std::fs::canonicalize()` to reject non-existent paths (C-13, FR-44).
3. Loads scenarios from JSONL via `load_scenarios()` (blank-line tolerant).
4. Constructs one `EvalServiceLayer` per profile config via `EvalServiceLayer::from_profile()`, which enforces `AnalyticsMode::Suppressed` (ADR-002).
5. Replays each scenario through all profiles concurrently via `tokio::spawn`, timing latency with `tokio::time::Instant`.
6. Computes per-scenario metrics: P@K (dual-mode: `expected` hard labels > `baseline.entry_ids` soft GT, AC-07), MRR, and Kendall tau (intersection-safe).
7. Writes one JSON result file per scenario to the output directory, sanitizing scenario IDs for safe filenames.
8. Computes `ComparisonMetrics` (tau delta, latency delta, P@K delta, rank changes) for each non-baseline profile.

### Key Design Decisions

- **`compute_tau_safe()`**: Kendall tau is computed over the intersection of baseline and candidate entry ID lists. `kendall_tau()` from `unimatrix_engine::test_scenarios` asserts element identity; intersection prevents panic when profiles return different recall sets.
- **Dual-mode ground truth**: `determine_ground_truth()` returns `record.expected` if non-empty, otherwise falls back to `record.baseline.entry_ids` (soft GT from the original query_log result).
- **Baseline selection**: The first profile (sorted by config path string, deterministic) is designated baseline when computing `ComparisonMetrics`. An explicit `baseline_name` parameter is wired for future use.
- **Inline tests**: Test plan specifies `#[cfg(test)]` inline tests to access private functions (`compute_p_at_k`, `compute_mrr`, `determine_ground_truth`, etc.). This causes runner.rs to exceed the 500-line guideline (525 lines of test code + 559 lines of production code = 1084 total). The inline tests are kept per test plan specification; the 500-line limit is a guideline, not a hard constraint when test coverage requires private-function access.

## Test Results

```
test eval::runner::tests::... (27 tests) ... ok
running 1588 tests (unimatrix-server lib)
test result: ok. 1588 passed; 0 failed
```

All 27 unit tests pass. No new failures introduced.

### Test Coverage (per component test plan)

| AC/R | Coverage | Test(s) |
|------|----------|---------|
| AC-01 | P@K dual-mode ground truth | `test_p_at_k_*`, `test_ground_truth_*` |
| AC-02 | MRR reciprocal rank | `test_mrr_*` |
| AC-03 | Kendall tau safe intersection | `test_compute_tau_safe_*` |
| AC-04 | Latency timing present | `test_replay_scenario_timing` |
| AC-05 | Rank change detection | `test_compute_rank_changes_*` |
| AC-06 | JSON result file written | `test_write_scenario_result_*` |
| AC-07 | expected > baseline soft GT priority | `test_ground_truth_expected_takes_priority` |
| C-13  | Live-DB path guard | `test_run_eval_live_db_guard` |
| R-01  | InvalidK(0) | `test_run_eval_invalid_k` |
| R-02  | ProfileNameCollision | `test_profile_name_collision` |
| R-03  | Empty scenario list no panic | `test_run_eval_empty_scenarios` |
| R-04  | All-miss MRR = 0.0 | `test_mrr_no_hit_returns_zero` |
| R-05  | P@K empty GT = 0.0 | `test_p_at_k_empty_gt` |
| R-06  | tau_safe disjoint sets = 0.0 | `test_compute_tau_safe_disjoint` |

## Issues / Deviations

- **Embed model wait loop removed**: Pseudocode referenced `layer.inner.embed_handle().get_adapter()` to wait for the embedding model. `ServiceLayer` and `EvalServiceLayer` do not expose an embed handle. Removed; search handles embed-not-ready internally via its own timeout path.
- **Pre-existing doctest failure**: `crates/unimatrix-server/src/infra/config.rs` has a pre-existing doctest failure (tilde path `~/.unimatrix/config.toml` parsed as Rust syntax). Confirmed pre-existing via `git stash` + reproduce. Not introduced by this agent.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found pattern #749 (Calibration Scenario Builder) and #2610 (HashMap profile iteration determinism, from sibling agent). Neither covered the kendall_tau intersection trap.
- Stored: entry #2612 "Compute intersection before calling kendall_tau() on diverging eval result sets" via `/uni-store-pattern`
