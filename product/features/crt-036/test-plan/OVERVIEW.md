# crt-036 Test Plan Overview

## Feature Summary

crt-036 replaces the two 60-day wall-clock observation DELETE sites with a
cycle-aligned GC policy. It adds `RetentionConfig`, four new store methods
(`list_purgeable_cycles`, `gc_cycle_activity`, `gc_unattributed_activity`,
`gc_audit_log`), rewrites `run_maintenance()` step 4, and adds a PhaseFreqTable
alignment guard. No schema migration. No MCP tool interface changes.

---

## Test Strategy

### Unit Tests (unimatrix-server crate, `cargo test -p unimatrix-server`)

All 19 ACs except AC-01a and AC-01b are covered by named unit tests in
`unimatrix-server`. The tests live alongside the components they test:
- `RetentionConfig` tests: `infra/config.rs` test module
- GC pass tests: `services/status.rs` test module (exercises store methods through
  the actual store layer — these are integration-style unit tests using an in-memory
  SQLite database created by the test harness)
- PhaseFreqTable guard tests: `services/status.rs` test module

### Grep Assertions (AC-01a, AC-01b)

Two independent file-scope grep assertions confirm both legacy DELETE sites are
absent. These are structural checks, not runtime tests.

### Integration Harness (infra-001)

Smoke gate is mandatory. No new integration tests are required (see section below
for reasoning). Suites that exercise session/observation data paths will passively
validate that existing behavior is not regressed by the GC.

---

## Risk-to-AC Mapping

| Risk ID | Priority | AC(s) | Test Function(s) |
|---------|----------|-------|-----------------|
| R-01 | Critical | AC-01a, AC-01b | grep assertions (two independent) |
| R-02 | Critical | AC-08 | `test_gc_cascade_delete_order` (with order-inversion mutation) |
| R-04 | Critical | AC-16 | `test_gc_max_cycles_per_tick_cap` (concurrent write sub-assertion) |
| R-03 | High | AC-05 | `test_gc_raw_signals_flag_and_summary_json_preserved` |
| R-05 | High | AC-04, AC-15 | `test_gc_gate_no_review_row`, `test_gc_tracing_output` |
| R-07 | High | AC-06 | `test_gc_unattributed_active_guard` |
| R-08 | High | AC-16 | `test_gc_max_cycles_per_tick_cap` (multi-tick count assertion) |
| R-09 | High | NFR-03 | `test_gc_query_plan_uses_index` (EXPLAIN QUERY PLAN assertions) |
| R-10 | High | AC-11, AC-12, AC-12b | `test_retention_config_validate_*` |
| R-12 | High | AC-09 | `test_gc_audit_log_retention_boundary` |
| R-14 | High | AC-03, AC-14 | `test_gc_protected_tables_regression`, `test_gc_protected_tables_row_level` |
| R-06 | Medium | (idempotency) | `test_gc_max_cycles_per_tick_cap` second-run assertion |
| R-11 | Medium | AC-17 | `test_gc_phase_freq_table_mismatch_warning` |
| R-15 | Medium | AC-10 | `test_retention_config_defaults_and_override` |
| R-16 | Medium | AC-17 | `test_gc_phase_freq_table_mismatch_warning` (boundary sub-case) |
| R-13 | Low | — | Accepted; documented in RISK-TEST-STRATEGY; no dedicated test |

**Non-negotiable Gate 3c blockers (from RISK-TEST-STRATEGY):**
1. AC-01a + AC-01b: two independent grep assertions
2. AC-08: order-inversion mutation in `test_gc_cascade_delete_order`
3. AC-05: `summary_json` preservation check in `test_gc_raw_signals_flag_and_summary_json_preserved`
4. NFR-03: EXPLAIN QUERY PLAN assertions in `test_gc_query_plan_uses_index`
5. AC-16: multi-tick drain in `test_gc_max_cycles_per_tick_cap`
6. AC-06: Active-session guard in `test_gc_unattributed_active_guard`
7. AC-10/AC-11/AC-12/AC-12b: validate() boundary tests
8. AC-09: both sides of audit_log retention boundary

---

## Cross-Component Test Dependencies

```
test_gc_raw_signals_flag_and_summary_json_preserved
  → requires: gc_cycle_activity (cycle-gc-pass) + store_cycle_review (cycle_review_index)
  → validates: run_maintenance GC block step ordering (step 3c runs after 3b commits)

test_gc_max_cycles_per_tick_cap
  → requires: list_purgeable_cycles + gc_cycle_activity (cycle-gc-pass)
  → requires: RetentionConfig.max_cycles_per_tick (retention-config)
  → validates: run_maintenance GC block loop capping (run-maintenance-gc-block)

test_gc_phase_freq_table_mismatch_warning
  → requires: RetentionConfig.activity_detail_retention_cycles (retention-config)
  → validates: phase-freq-table-guard (component 5)

test_gc_cascade_delete_order
  → requires: gc_cycle_activity internals (cycle-gc-pass)
  → validates: run_maintenance GC block step 3b ordering
```

---

## Full AC Coverage Table

| AC-ID | Test Name | Component |
|-------|-----------|-----------|
| AC-01a | grep: `DELETE FROM observations WHERE ts_millis` absent from `status.rs` | legacy-delete-removal |
| AC-01b | grep: `DELETE FROM observations WHERE ts_millis` absent from `tools.rs` | legacy-delete-removal |
| AC-02 | `test_gc_cycle_based_pruning_correctness` | cycle-gc-pass |
| AC-03 | `test_gc_protected_tables_regression` | cycle-gc-pass |
| AC-04 | `test_gc_gate_no_review_row` | run-maintenance-gc-block |
| AC-05 | `test_gc_raw_signals_flag_and_summary_json_preserved` | run-maintenance-gc-block |
| AC-06 | `test_gc_unattributed_active_guard` | cycle-gc-pass |
| AC-07 | `test_gc_query_log_pruned_with_cycle` | cycle-gc-pass |
| AC-08 | `test_gc_cascade_delete_order` | cycle-gc-pass |
| AC-09 | `test_gc_audit_log_retention_boundary` | cycle-gc-pass |
| AC-10 | `test_retention_config_defaults_and_override` | retention-config |
| AC-11 | `test_retention_config_validate_rejects_zero_retention_cycles` | retention-config |
| AC-12 | `test_retention_config_validate_rejects_zero_audit_days` | retention-config |
| AC-12b | `test_retention_config_validate_rejects_invalid_max_cycles` | retention-config |
| AC-13 | Manual PR review: `///` doc comment on `activity_detail_retention_cycles` | retention-config |
| AC-14 | `test_gc_protected_tables_row_level` | cycle-gc-pass |
| AC-15 | `test_gc_tracing_output` | run-maintenance-gc-block |
| AC-16 | `test_gc_max_cycles_per_tick_cap` | run-maintenance-gc-block |
| AC-17 | `test_gc_phase_freq_table_mismatch_warning` | phase-freq-table-guard |

---

## Integration Harness Plan (infra-001)

### Suite Selection Analysis

crt-036 makes no MCP tool interface changes and no schema changes. Its changes are:
- New store methods (no MCP surface)
- Rewritten `run_maintenance()` step 4 (background tick, no MCP surface during tests)
- Removed DELETE blocks (only affects background tick behavior)
- New `RetentionConfig` (server startup config; no new MCP tool parameter)

The GC pass runs in the background tick, which infra-001 tests do not trigger
explicitly (except the `availability` suite, which is pre-release only).

**Suites to run:**

| Suite | Run? | Reason |
|-------|------|--------|
| smoke | YES (mandatory gate) | Always required; validates server starts with new RetentionConfig in config |
| tools | YES | context_cycle_review tool behavior unchanged but tool tests exercise session/observation data paths |
| lifecycle | YES | Tests seed sessions and observations, exercise context_cycle — verifies GC code does not corrupt in-flight session state at startup |
| protocol | No | Protocol compliance unaffected |
| volume | No | GC is not MCP-facing; no volume-level observation behavior changes |
| security | No | No security boundary changes |
| confidence | No | No scoring changes |
| contradiction | No | No contradiction logic changes |
| edge_cases | No | No edge-case-facing interface changes |

**Minimum gate:** `pytest -m smoke` must pass before Gate 3c.

**Recommended run:** `smoke` + `tools` + `lifecycle` suites to verify session/observation
table integrity is not disturbed by the new retention.rs module being compiled into the binary.

### New Integration Tests Required

**None.** The GC pass is not invocable through the MCP interface — it runs only during
the background maintenance tick. The infra-001 harness does not trigger maintenance ticks
(except the `availability` suite). The correctness of the GC logic is fully validatable
through unit/integration-style tests within `cargo test -p unimatrix-server`.

The one scenario that could theoretically be tested through infra-001 (confirming that
a server started with `RetentionConfig` defaults still responds to `context_status`) is
already covered by the smoke suite's `test_status_report_works` path.

No new test files or test functions need to be added to `suites/`.

### Smoke Gate Command

```bash
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

### Recommended Suite Commands

```bash
cd product/test/infra-001
python -m pytest suites/test_tools.py suites/test_lifecycle.py -v --timeout=60
```
