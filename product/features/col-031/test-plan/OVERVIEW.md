# col-031 Test Plan Overview
# Phase-Conditioned Frequency Table

## Overall Test Strategy

col-031 is primarily a wiring and data-pipeline feature: one new in-memory struct
(`PhaseFreqTable`) built from `query_log`, threaded through `ServiceLayer` and the
background tick, and consumed in the `search.rs` scoring loop. The dominant failure
mode is *silent bypass* — the handle accepted but never wired through, leaving the
feature perpetually inert. The second dominant failure mode is a vacuous eval gate
caused by `replay.rs` never forwarding `current_phase`.

Testing is structured at three levels:

1. **Unit** — pure logic tests in `#[cfg(test)]` blocks: cold-start invariants,
   rank normalization formula, config validation, handle mechanics.
2. **Store integration** — tests using `TestDb` (real SQLite in-memory): SQL
   correctness, `CAST(je.value AS INTEGER)` validity, deserialization types.
3. **Feature-level integration** — tick cycle end-to-end, wiring verification,
   score-identity checks.

The infra-001 integration harness exercises the compiled MCP server binary. See the
section below for which suites apply to col-031.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Owning Component | Named Test(s) | AC |
|---------|----------|-----------------|---------------|----|
| R-01 | Critical | background_tick / service_layer | `test_run_single_tick_propagates_phase_freq_handle` (integration); grep audit of all `SearchService::new` sites | AC-05 |
| R-02 | Critical | replay_fix | `test_replay_forwards_current_phase_to_service_search_params`; eval output inspection | AC-16 |
| R-03 | High | search_scoring | `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`; `test_scoring_score_identity_cold_start` | AC-06, AC-11 |
| R-04 | High | phase_freq_table | `test_phase_affinity_score_use_fallback_returns_one`; `test_phase_affinity_score_absent_phase_returns_one`; `test_phase_affinity_score_absent_entry_returns_one` | AC-07, AC-11 |
| R-05 | High | query_log_store_method | `test_query_phase_freq_table_returns_correct_entry_id` (TestDb) | AC-08 |
| R-06 | High | search_scoring | `test_scoring_lock_released_before_scoring_loop` (concurrency); code review | AC-06 |
| R-07 | High | phase_freq_table | `test_phase_affinity_score_single_entry_bucket_returns_one`; `test_rebuild_normalization_three_entry_bucket_exact_scores` | AC-13, AC-14 |
| R-08 | Medium | inference_config | `test_validate_lookback_days_zero_is_error`; `test_validate_lookback_days_3651_is_error`; `test_validate_lookback_days_boundary_values` | — |
| R-09 | Medium | background_tick | `test_run_single_tick_retains_state_on_rebuild_error` | AC-04 |
| R-10 | Medium | phase_freq_table | `test_phase_affinity_score_unknown_phase_returns_one` | — |
| R-11 | Medium | replay_fix + eval gate | AC-12 eval run with AC-16 applied; sensitivity comparison at w=0.0 vs w=0.05 | AC-12 |
| R-12 | Low | background_tick | Code comment present at lock sequence site; static grep order audit | — |
| R-13 | Medium | query_log_store_method | `test_query_phase_freq_table_returns_correct_entry_id` (also validates freq as i64) | AC-08 |
| R-14 | High | service_layer + background_tick | `cargo build --workspace` clean; grep audit of all 7 sites | AC-05, R-14 |

---

## Cross-Component Test Dependencies

```
AC-16 (replay_fix) ──► AC-12 (eval gate)  [hard prerequisite — NFR-05]

AC-08 (TestDb SQL) ──► AC-14 (normalization unit test)
                        [same dataset, AC-14 can use the same store seed helper]

AC-01 (cold-start new()) ──► AC-03 (new_handle() wraps cold-start)
                              [AC-03 depends on AC-01 semantics being correct]

AC-11 Test 2 (use_fallback guard) ──► AC-06 (fused scoring guard)
                                       [AC-06 is the scoring-level proof]
```

The delivery wave containing AC-12 **must** include AC-16. Gate 3b must reject any
AC-12 PASS submission that does not include a scenario output file with at least one
non-null `current_phase` row.

---

## Integration Harness Plan (infra-001)

### Which Existing Suites Apply

col-031 modifies the **search scoring pipeline** (`services/search.rs`) and the
**background tick** (`background.rs`). It does not touch MCP tool signatures, the
confidence formula, contradiction detection, or security scanning.

| Suite | Applies? | Reason |
|-------|----------|--------|
| `smoke` | YES — mandatory gate | Always required. Verifies search basic path still works after scoring changes. |
| `tools` | YES | `context_search` tool is the primary scoring surface; any regression in search output format or ranking is detectable here. |
| `lifecycle` | YES | Multi-step store→search flows exercise the tick rebuild → search scoring chain. |
| `confidence` | NO | col-031 does not modify the 6-factor composite formula. |
| `contradiction` | NO | Unaffected. |
| `security` | NO | No new external input surface. `current_phase` comes from internal search params. |
| `edge_cases` | YES | `current_phase = None` is an edge case that must not break existing search paths. |
| `protocol` | NO | No MCP protocol changes. |
| `volume` | NO | Scale behavior is not the focus; frequency table is in-memory O(1) lookup. |
| `adaptation` | NO | Unaffected. |

**Minimum gate**: `pytest -m smoke` must pass.

**Recommended**: `tools`, `lifecycle`, `edge_cases` in addition to smoke.

### Gap Analysis: New Integration Tests Needed

The existing infra-001 suites do **not** exercise `current_phase` propagation through
the MCP `context_search` tool call. The search tool currently has no test that passes
`current_phase` and verifies the scoring path behaves differently.

Two new integration tests are planned for Stage 3c addition to
`suites/test_lifecycle.py`:

#### New Test 1: `test_search_phase_affinity_influences_ranking`

Validates that a populated `PhaseFreqTable` (after a tick) produces phase-biased
ranking — i.e., entries with high access frequency during `phase="delivery"` score
higher when `current_phase="delivery"` than when `current_phase=None`.

```python
# Fixture: shared_server (state accumulates: store + tick + search)
# 1. Store two entries: entry A (category="decision"), entry B (category="lesson-learned")
# 2. Insert query_log rows via direct DB seed: entry A heavily accessed in phase="delivery"
# 3. Wait for tick OR directly trigger rebuild via test hook (if available)
# 4. Search with current_phase="delivery" — assert A ranks above B
# 5. Search with current_phase=None — assert relative order is different (or equal)
```

**Note**: If the tick cannot be triggered synchronously, this test may require the
`shared_server` fixture with a longer timeout. If direct tick triggering is not
available through the MCP interface, this test should be planned as a unit-level
integration test (using `run_single_tick` directly) rather than an MCP-level test.
Assess at Stage 3c.

#### New Test 2: `test_search_cold_start_phase_score_identity`

Validates that `context_search` with `current_phase="delivery"` on a fresh (cold-start)
server produces scores identical to `current_phase=None`.

```python
# Fixture: server (fresh DB, cold-start — use_fallback=true guaranteed)
# 1. Store one entry.
# 2. Search with current_phase="delivery". Record score.
# 3. Search with current_phase=None. Record score.
# 4. Assert both scores are equal (cold-start score identity, NFR-04).
```

This test is MCP-accessible (no tick needed) and straightforward to implement.

### Decision: AC-08 Does Not Need a New infra-001 Test

AC-08 (`query_phase_freq_table` SQL correctness) is verified at the store-unit level
using `TestDb`. The infra-001 harness does not have a direct path to call
`query_phase_freq_table` — this method is internal. AC-08's correctness is surfaced
indirectly through the search ranking tests above. No new infra-001 test for AC-08 is
needed.

### Suite Commands for Stage 3c

```bash
# Mandatory minimum gate
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# Recommended additional suites
python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_edge_cases.py -v --timeout=60
```

---

## Per-Component Test File Map

| Test Plan File | Component |
|----------------|-----------|
| `phase_freq_table.md` | `services/phase_freq_table.rs` (new) |
| `query_log_store_method.md` | `unimatrix-store/src/query_log.rs` (store method) |
| `search_scoring.md` | `services/search.rs` (scoring wire-up) |
| `background_tick.md` | `background.rs` (tick integration) |
| `service_layer.md` | `services/mod.rs` (ServiceLayer wiring) |
| `inference_config.md` | `infra/config.rs` (InferenceConfig changes) |
| `replay_fix.md` | `eval/scenarios/replay.rs` (AC-16 fix) |
