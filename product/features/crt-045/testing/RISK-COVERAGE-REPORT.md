# Risk Coverage Report: crt-045
# Eval Harness — Wire TypedGraphState Rebuild into EvalServiceLayer

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Post-construction write-back does not propagate to SearchService — Arc clone assumption | `test_from_profile_typed_graph_rebuilt_after_construction` (layer 1 + layer 3) | PASS | Full |
| R-02 | Wired-but-unused: handle holds rebuilt state but search.rs reads from a stale clone | `test_from_profile_typed_graph_rebuilt_after_construction` (layer 2 + layer 3) | PASS | Full |
| R-03 | Test graph seeded with Quarantined entries produces vacuous `use_fallback=false` | `test_from_profile_typed_graph_rebuilt_after_construction` (Active entries + CoAccess edge fixture, C-09) | PASS | Full |
| R-04 | Rebuild error (cycle or I/O) causes `from_profile()` to return `Err`, blocking all metric collection | `test_from_profile_returns_ok_on_cycle_error` | PASS | Full |
| R-05 | TOML parse failure at profile load time prevents eval run before graph code is reached | `test_parse_no_distribution_change_flag` (unit, `eval::profile::tests`) | PASS | Full |
| R-06 | Baseline regression: rebuild call introduces latency or behavioral change on non-graph profiles | All 38 `eval::profile` tests pass unchanged | PASS | Full |
| R-07 | Rebuild hangs on corrupted GRAPH_EDGES with no timeout guard | Not tested — accepted residual risk per SPECIFICATION.md; sqlx query timeout is implicit guard | N/A | None (accepted) |
| R-08 | `typed_graph_handle()` accessor promoted to `pub` — external callers can write graph state | Code review: `pub(crate)` confirmed at `layer.rs:452` | PASS | Full (compile-time) |
| R-09 | `mrr_floor=0.2651` baseline threshold has drifted since crt-042 | Manual pre-merge verification required — not automatable | DEFERRED | Manual |
| R-10 | Step 13b write-back occurs before ServiceLayer fully initialises SearchService — race | Covered incidentally by `test_from_profile_typed_graph_rebuilt_after_construction`; `from_profile()` is sequential async | PASS | Full (incidental) |

---

## Test Results

### Unit Tests (cargo test --workspace)

- **Total passed**: 4,426
- **Total failed**: 0
- **Ignored**: 28

All test binaries returned `test result: ok`. No new failures introduced. The workspace
total is consistent with the pre-feature baseline plus the 2 new tests in `layer_graph_tests`.

### Eval Profile Integration Tests (cargo test -p unimatrix-server -- "eval::profile")

- **Total**: 38
- **Passed**: 38
- **Failed**: 0

Breakdown:
- `eval::profile::layer_graph_tests` (NEW, crt-045): 2 passed
  - `test_from_profile_typed_graph_rebuilt_after_construction`
  - `test_from_profile_returns_ok_on_cycle_error`
- `eval::profile::layer_tests` (pre-existing, regression guard): 9 passed
  - `test_from_profile_analytics_mode_is_suppressed`
  - `test_from_profile_returns_live_db_path_error_for_same_path`
  - `test_from_profile_snapshot_does_not_exist_returns_io_error`
  - `test_from_profile_invalid_weights_returns_config_invariant`
  - `test_from_profile_loads_vector_index_from_snapshot_dir`
  - `test_from_profile_nli_disabled_no_nli_handle`
  - `test_from_profile_nli_enabled_has_nli_handle`
  - `test_from_profile_invalid_nli_model_name_returns_config_invariant`
  - `test_from_profile_valid_weights_passes_validation`
- `eval::profile::tests` (pre-existing, unit): 27 passed

### Integration Smoke Gate (infra-001)

Command: `python -m pytest suites/ -v -m smoke --timeout=60`

- **Total collected**: 22 (of 259 in full suite, 237 deselected)
- **Passed**: 22
- **Failed**: 0
- **Run time**: 191.48s (3:11)

All smoke tests passed. crt-045 changes are orthogonal to all MCP-layer behaviors tested
by infra-001: no MCP tools were added or changed, no schema changes, no store behavior changes.

Suites represented in smoke run: `adaptation`, `confidence`, `contradiction`, `edge_cases`,
`lifecycle`, `protocol`, `security`, `tools`, `volume`.

No additional infra-001 suites were selected (per test-plan/OVERVIEW.md gap analysis:
crt-045 behavior is not observable through the MCP JSON-RPC interface; eval path is CLI-only).

---

## Non-Negotiable Scenarios — Gate Verification

Per RISK-TEST-STRATEGY.md Coverage Summary (entry #2758: every non-negotiable test function
name must be confirmed before PASS claims):

| Scenario | Test Function | Status |
|----------|--------------|--------|
| `use_fallback == false` AND `typed_graph` non-empty after `from_profile()` with Active-entry + edge snapshot | `test_from_profile_typed_graph_rebuilt_after_construction` | PASS |
| Live `search()` call returns `Ok(_)` (or `EmbeddingFailed`) on graph-enabled layer | `test_from_profile_typed_graph_rebuilt_after_construction` layer 3 | PASS |
| `Ok(layer)` returned on cycle-detected rebuild error with `use_fallback == true` | `test_from_profile_returns_ok_on_cycle_error` | PASS |
| All existing `layer_tests.rs` and `eval/profile/tests.rs` tests pass unchanged | 9 pre-existing layer_tests + 27 profile unit tests | PASS |

All four gate-blocking scenarios confirmed present and passing.

---

## Three-Layer Assertion Compliance (ADR-003)

`test_from_profile_typed_graph_rebuilt_after_construction` implements all three layers:

| Layer | Assertion | Status |
|-------|-----------|--------|
| Layer 1a | `assert!(!guard.use_fallback)` | PASS |
| Layer 1b | `assert!(guard.all_entries.len() >= 2)` | PASS |
| Layer 2 | `assert_eq!(find_terminal_active(id_a, ...), Some(id_a))` | PASS |
| Layer 3 | `search(params, ...).await` returns `Ok(_)` or `EmbeddingFailed` | PASS |

---

## Gaps

### R-07: Rebuild Hang (No Timeout Guard)
Accepted residual risk per SPECIFICATION.md. The sqlx query execution provides an implicit
bound; an explicit `tokio::time::timeout` wrapper was explicitly deferred. A follow-up issue
should add the timeout if sqlx query timeout is not configured in production. This risk is
documented in the architecture (ADR-002) and does not block the feature.

### R-09: mrr_floor Drift Since crt-042
Manual pre-merge verification required. The delivery agent must run
`unimatrix eval run --profile baseline.toml` against a post-crt-021 snapshot and confirm
the reported MRR matches or exceeds 0.2651 before merge. Not automatable without a live
populated snapshot. This is AC-02 and AC-04 (manual ACs).

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_from_profile_typed_graph_rebuilt_after_construction` layer 1: `assert!(!guard.use_fallback)` and `assert!(guard.all_entries.len() >= 2)` |
| AC-02 | DEFERRED (manual) | Requires live populated snapshot + eval harness run. Not automatable in CI. Per test-plan/OVERVIEW.md, this is a manual pre-merge gate only. |
| AC-03 | PASS | `test_parse_no_distribution_change_flag` asserts `parse_profile_toml()` returns `Ok` with `distribution_change=false`. TOML file confirmed at `ppr-expander-enabled.toml` with correct `distribution_change = false`, `mrr_floor = 0.2651`, `p_at_5_min = 0.1083`. |
| AC-04 | DEFERRED (manual) | Baseline regression confirmation requires before/after eval harness run with a live snapshot. Pre-existing 9 `layer_tests.rs` tests serve as automated regression proxy (all pass). |
| AC-05 | PASS | `test_from_profile_returns_ok_on_cycle_error`: `result.expect("must not abort")` and `assert!(guard.use_fallback)` — both pass |
| AC-06 | PASS | `test_from_profile_typed_graph_rebuilt_after_construction`: all three ADR-003 layers pass (handle state, `find_terminal_active` graph connectivity, live `search()` call) |
| AC-07 | PASS | `cargo test --workspace`: 4,426 passed, 0 failed |
| AC-08 | PASS | All 9 pre-existing `layer_tests.rs` tests pass unchanged; all 27 pre-existing `eval::profile::tests` pass unchanged |

---

## Constraints Verification

| Constraint | Status | Evidence |
|-----------|--------|---------|
| C-01: `rebuild()` called with `.await` — no `spawn_blocking` | PASS | `layer.rs:188`: `TypedGraphState::rebuild(&*store_arc).await` |
| C-02: Rebuild error path: `warn!` + `None` + `Ok(layer)` | PASS | `layer.rs:199–215`: match arm sets `None`; `Ok(EvalServiceLayer {...})` returned at line 397 |
| C-03: `with_rate_config()` signature unchanged | PASS | No signature change; confirmed by compilation and all existing tests passing |
| C-04: `typed_graph_handle()` declared `pub(crate)` | PASS | `layer.rs:452`: `pub(crate) fn typed_graph_handle(...)` |
| C-06: TOML gate values correct | PASS | `ppr-expander-enabled.toml`: `distribution_change = false`, `mrr_floor = 0.2651`, `p_at_5_min = 0.1083` |
| C-08: `typed_graph_handle()` delegates to `self.inner.typed_graph_handle()` | PASS | `layer.rs:453`: `self.inner.typed_graph_handle()` |
| C-09: Test fixture uses Active entries + S1/S2/S8 (CoAccess) edge | PASS | `layer_graph_tests.rs:73–83`: two Active entries + `CoAccess` edge, `bootstrap_only=0` |
| C-10: No `#[cfg(test)]` guard on `typed_graph_handle()` | PASS | `layer.rs:452`: no cfg guard; also available to `runner.rs` |

---

## GH Issues Filed

None. No integration test failures were encountered. No pre-existing failures were discovered
during this test execution pass.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 18 entries including entry #2758
  (non-negotiable test name grep before PASS claims), entry #4085 (eval harness snapshot timing),
  entry #3806 (gate 3b reworkable fail pattern). All applied: non-negotiable test names verified
  by direct `--list` output before reporting PASS; snapshot timing risk noted in AC-02/AC-04
  deferral.
- Stored: nothing novel to store — test execution followed the established eval layer pattern
  (in-process integration tests, `seed_graph_snapshot()` helper pattern, three-layer ADR-003
  assertion). Pattern entry #4096 and #4100 already capture the cold-start anti-pattern and
  the three-layer test requirement for eval layers. No new procedure or pattern emerged that
  would add value beyond those existing entries.
