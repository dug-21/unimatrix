# Gate 3c Report: crt-036

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 16 risks covered; R-13 accepted as low-priority per strategy |
| Test coverage completeness | PASS | All 8 non-negotiable blockers verified; all 16 risks mapped to tests |
| Specification compliance | PASS | All 19 ACs verified; all FRs and NFRs implemented |
| Architecture compliance | PASS | Component boundaries, ADR decisions, and integration surface match architecture |
| Knowledge stewardship | PASS | Tester agent report has Queried: and Stored: entries |

---

## Detailed Findings

### Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 16 risks from RISK-TEST-STRATEGY.md to passing tests. The 8 non-negotiable Gate 3c blockers are all verified:

1. R-01 (legacy 60-day DELETE): Two independent grep assertions confirmed — `DELETE FROM observations WHERE ts_millis` absent from both `status.rs` and `tools.rs`. Verified by direct grep during this review: pattern returns no matches in either file.
2. R-02 (cascade delete order): `retention::tests::test_gc_cascade_delete_order` passes, including the order-inversion mutation assertion.
3. R-03 (summary_json preservation): `test_gc_raw_signals_flag_and_summary_json_preserved` asserts byte-for-byte `summary_json` identity post-GC alongside `raw_signals_available = 0`. The implementation correctly uses `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` with the record retained from the gate check.
4. R-09 (index scan): `test_gc_query_plan_uses_index` runs `EXPLAIN QUERY PLAN` and asserts `idx_observations_session` is used.
5. R-08 (max_cycles_per_tick cap): `test_gc_max_cycles_per_tick_cap` verifies cap enforcement and oldest-first ordering.
6. R-07 (active session guard): `test_gc_unattributed_active_guard` tests both Active (survives) and Closed (deleted) paths.
7. R-10 (validate() bounds): All three fields validated; `test_retention_config_validate_rejects_*` tests cover both lower and upper bounds.
8. R-12 (audit_log timestamp): `test_gc_audit_log_retention_boundary` tests both sides of 180-day retention boundary; `test_gc_audit_log_epoch_row_deleted` covers epoch edge case.

R-13 (low priority — raw_signals_available stays 1 after crash) is accepted without a dedicated test, per the risk strategy documentation. This is consistent with ADR-001 consequences and documented in the risk strategy.

---

### Test Coverage Completeness

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md enumerates tests for all 16 risks. Test counts confirmed by direct test run:

- `unimatrix-store` retention module: 14 tests, all pass (`retention::tests::*`)
- `unimatrix-server` crt-036 GC block tests: 9 tests, all pass (`crt_036_gc_block_tests::*` + `crt_036_phase_freq_table_guard_tests::*`)
- `unimatrix-server` config tests: 6 tests, all pass (`infra::config::tests::test_retention_*`)

Total crt-036-specific tests: 29 (14 store + 9 server + 6 config). Total workspace: 4191 passed, 0 failed, 28 ignored.

**Integration tests**: Smoke suite 22/22 pass. Tools + lifecycle: 139 passed, 5 xfailed. The 5 xfailed tests are pre-existing (GH#291 — tick not drivable externally, GH#406 — multi-hop traversal not implemented). No new xfail markers added by crt-036. Both GH issues exist and are unrelated to crt-036.

**WARN — AC-15 gate-skip path**: The `test_gc_tracing_gate_skip_warn_format` test verifies the `Ok(None)` gate-skip warn log format by direct emit rather than by triggering the production code path. The RISK-COVERAGE-REPORT acknowledges this: the `list_purgeable_cycles` SQL self-gates so only cycles with review rows appear in the purgeable set, making the `Ok(None)` branch unreachable through normal data setup. The format test is a reasonable pragmatic approach, but it does not exercise the production branch. This is a minor coverage gap — the warn format is verified but the conditional logic in the gate check is not exercised end-to-end for the `Ok(None)` case. Not blocking given: (a) the condition is a defense-in-depth branch, (b) the `Ok(Some(_))` path is thoroughly tested, (c) the `Ok(Err(_))` path is documented as having the same skip-and-warn behavior.

---

### Specification Compliance

**Status**: PASS

**Evidence**:

- **FR-01 (RetentionConfig)**: Struct implemented in `infra/config.rs` with correct fields (activity_detail_retention_cycles: u32 default 50, audit_log_retention_days: u32 default 180, max_cycles_per_tick: u32 default 10), `#[serde(default)]`, and `validate()` method. Wired into `UnimatrixConfig` under key `retention`.
- **FR-02 (K-cycle resolution)**: `list_purgeable_cycles(k, max_per_tick)` uses the NOT IN subquery with ORDER BY computed_at DESC LIMIT k, returns oldest-first (ORDER BY computed_at ASC LIMIT max_per_tick). Also returns `Option<i64>` for the PhaseFreqTable alignment check (K-th oldest retained).
- **FR-03 (per-cycle transaction)**: `gc_cycle_activity()` uses `pool.begin()` / `txn.commit()` per ADR-001 and entry #2159. Delete order is observations → query_log → injection_log → sessions. Connection released on return.
- **FR-04 (crt-033 gate)**: Gate check via `get_cycle_review()` before every delete. `Ok(None)` and `Err(_)` both skip the cycle with `tracing::warn!`.
- **FR-05 (unattributed cleanup)**: `gc_unattributed_activity()` deletes orphaned observations and query_log, plus unattributed non-active sessions and their injection_log rows. Active sessions guarded (status = 0 excluded).
- **FR-06 (raw_signals_available)**: After transaction commit, `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` with record retained from gate check. Outside the transaction.
- **FR-07 (audit_log GC)**: `gc_audit_log(retention_days)` uses `strftime('%s', 'now') - ?1 * 86400` (seconds, not milliseconds). Runs as step 4f.
- **FR-08 (remove 60-day DELETE sites)**: Both sites removed. Verified by grep — no matches for `DELETE FROM observations WHERE ts_millis` in either `status.rs` or `tools.rs`.
- **FR-09 (structured tracing)**: All required events emitted at correct levels with required fields (`purgeable_count`, `capped_to`, `cycle_id`, `observations_deleted`, `cycles_pruned`, etc.).
- **FR-10 (PhaseFreqTable guard)**: `run_phase_freq_table_alignment_check()` implemented; emits `tracing::warn!` when `oldest_retained_computed_at` falls before the lookback cutoff. Skipped when fewer than K cycles exist (None returned).
- **FR-11 (config.toml block)**: `config.toml` has `[retention]` section with all three fields and documentation comments. `activity_detail_retention_cycles` comment includes "governing ceiling for PhaseFreqTable lookback and future GNN training window."
- **FR-12 (step ordering)**: Cycle-based GC at step 4, audit_log at step 4f, session GC at step 6. Step ordering confirmed in `run_maintenance()`.
- **NFR-01 (hot path)**: GC runs only in `run_maintenance()` background tick. No MCP request handler involvement.
- **NFR-02 (connection release)**: Per-cycle transaction releases connection via `pool.begin()` → `txn.commit()` inside `gc_cycle_activity()` before returning.
- **NFR-03 (index usage)**: EXPLAIN QUERY PLAN test confirms index usage.
- **NFR-04 (idempotency)**: `test_gc_cycle_activity_idempotent` confirms zero rows affected on second run.
- **NFR-05 (no schema migration)**: No new schema migrations introduced. Schema remains at v19.
- **NFR-06 (config loaded once)**: `retention_config` is an `Arc<RetentionConfig>` created once at startup and passed by value into `run_maintenance()`. No per-tick re-read.
- **AC-13 (doc comment)**: Triple-slash comment on `activity_detail_retention_cycles` contains both "PhaseFreqTable lookback" and "GNN training window" — confirmed in config.rs lines 1066–1067.

All 19 acceptance criteria from ACCEPTANCE-MAP.md are PASS.

---

### Architecture Compliance

**Status**: PASS

**Evidence**:

- **Component boundaries**: `CycleGcPass` methods (`list_purgeable_cycles`, `gc_cycle_activity`, `gc_unattributed_activity`, `gc_audit_log`) live in `unimatrix-store/src/retention.rs` as specified. `RetentionConfig` and `validate()` in `unimatrix-server/src/infra/config.rs`. GC block in `services/status.rs` `run_maintenance()`. Background threading in `background.rs`.
- **ADR-001 (per-cycle transactions)**: Confirmed. `pool.begin()` inside the cycle loop, not outside. Each call to `gc_cycle_activity()` acquires, operates, and releases the write pool connection.
- **ADR-002 (max_cycles_per_tick in RetentionConfig)**: Confirmed. `max_cycles_per_tick` field is in `RetentionConfig`, not `InferenceConfig`.
- **ADR-003 (PhaseFreqTable alignment as tracing::warn!)**: Confirmed. `run_phase_freq_table_alignment_check()` emits advisory warn only; does not block GC.
- **SR-05 (struct update pattern)**: Confirmed. `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` where `record` is the `CycleReviewRecord` fetched in the gate check, retained in scope throughout the per-cycle iteration.
- **SR-06 (active session guard)**: `status != 0` in `gc_unattributed_activity()` excludes active sessions.
- **Integration surface**: All new methods match the documented signatures in the architecture. `run_maintenance()` and `run_single_tick()` both accept `retention_config` as specified. `list_purgeable_cycles` returns `(Vec<String>, Option<i64>)` to support the PhaseFreqTable guard.
- **Legacy DELETE removal**: Both sites removed entirely (not guarded or conditionalized), matching the architecture requirement.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: Tester agent report (`crt-036-agent-7-tester-report.md`) contains a `## Knowledge Stewardship` section with:
- `Queried: mcp__unimatrix__context_briefing — found entry #3930 (list_purgeable_cycles behavior); applied to confirm test scope`
- `Stored: nothing novel to store — tracing_test pattern and defense-in-depth format testing are not novel relative to existing codebase patterns`

---

## Rework Required

None.

---

## Integration Test Mandate — Verified

| Requirement | Status | Notes |
|-------------|--------|-------|
| `pytest -m smoke` passed | PASS | 22/22 |
| Relevant integration suites run | PASS | tools (73/73) + lifecycle (20/20 + 5 xfailed) |
| xfail markers have GH Issues | PASS | GH#291 (tick not drivable) and GH#406 (multi-hop traversal) — pre-existing, not crt-036 |
| No integration tests deleted/commented out | PASS | No removals in test files |
| RISK-COVERAGE-REPORT includes integration counts | PASS | Smoke 22, tools+lifecycle 139+5xfail |
| xfail failures unrelated to feature | PASS | Both failures are pre-existing infrastructure and algorithm gaps, not data retention issues |

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the gate-3c validation pattern (direct-emit format test for unreachable defense-in-depth branches) is feature-specific and present in the tester report; not yet recurrent across 2+ features.

