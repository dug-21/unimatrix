# Gate 3c Report: crt-034

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-30
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 risks have named passing tests; RISK-COVERAGE-REPORT maps each risk to evidence |
| Test coverage completeness | PASS | All mandatory AC items covered; all 6 edge cases covered; integration smoke + lifecycle suites passed |
| Specification compliance | PASS | All 17 FRs and 7 NFRs implemented and tested; all 15 ACs verified |
| Architecture compliance | PASS | Tick ordering, write-pool path, constants location, config field pattern all match ARCHITECTURE.md |
| Knowledge stewardship compliance | PASS | Tester report includes Queried and Stored entries with reasoning |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 13 risks to named test functions. Verified against actual test execution:

- R-01 (Critical — write failure silent absorption): `test_write_failure_mid_batch_warn_and_continue`, `test_write_failure_info_log_always_fires` — both pass. The continuation-on-error scenario is covered by pre-seeding pair (1,2) to produce an INSERT no-op, then asserting pairs (1,3) and (1,4) are still processed.
- R-02 (High — division by zero on empty table): `test_empty_co_access_table_noop_late_tick`, `test_all_below_threshold_noop_late_tick` — both pass. `CoAccessBatchRow.max_count: Option<i64>` confirmed in source; early return before per-pair loop eliminates the division-by-zero risk.
- R-03 (High — MAX subquery correctness): `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` — both pass. Second scenario seeds counts [3,4,5,80,100] with cap=3; asserts e5.weight == 0.05 (5/100), confirming global MAX of 100 was used.
- R-04 (High — INSERT OR IGNORE no-op detection): `test_existing_edge_stale_weight_updated`, `test_existing_edge_current_weight_no_update`, `test_double_tick_idempotent` — all pass.
- R-05 (High — tick ordering): ORDERING INVARIANT anchor comment verified at lines 550–556 of background.rs; `run_co_access_promotion_tick` call at line 556 precedes `TypedGraphState::rebuild()` at line 570.
- R-06 (High — early-tick warn window): `test_early_tick_warn_when_qualifying_count_zero` (tick=0, empty → warn fires), `test_late_tick_no_warn_empty_table` (tick=5, empty → no warn), `test_fully_promoted_table_no_warn` (tick=0, qualifying_count>0 → no warn) — all pass. All four quadrants of (qualifying=0/positive) × (tick<5/tick>=5) covered.
- R-07 (High — config merge): `test_merge_configs_project_overrides_global_co_access_cap` (50 wins over 200), `test_merge_configs_global_only_co_access_cap` (300 preserved when project is default) — both pass. Confirmed `merge_configs()` stanza at config.rs lines 2110–2116.
- R-08 (Med — constant divergence): `test_co_access_graph_min_count_value` (asserts == 3i64), `test_co_access_constants_colocated_with_nli` — both pass. Code comment in read.rs explicitly documents that migration.rs has its own file-private copy; single authoritative value is `CO_ACCESS_GRAPH_MIN_COUNT`.
- R-09 (Med — near-threshold oscillation): `test_double_tick_idempotent` (exactly 1 row after 2 ticks), `test_sub_threshold_pair_not_gc` — both pass.
- R-10 (Med — one-directional edge): `test_inserted_edge_is_one_directional` (asserts no reverse edge), `test_basic_promotion_new_qualifying_pair` (inline reverse-edge assertion) — both pass.
- R-11 (High — ORDER BY count DESC): `test_cap_selects_highest_count_pairs` (counts [3,3,3,3,3,10,20,50,80,100], cap=3 → only [100,80,50] promoted) — passes.
- R-12 (Low — file size): `co_access_promotion_tick.rs` is 288 lines. PASS.
- R-13 (High — metadata fields): `test_inserted_edge_metadata_all_four_fields` asserts `bootstrap_only=0`, `source="co_access"`, `created_by="tick"`, `relation_type="CoAccess"` — passes.

Total workspace test run: **4141 passed, 0 failed, 0 ignored** (confirmed via `cargo test --workspace`).

---

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**: All mandatory tests from RISK-TEST-STRATEGY.md Coverage Summary are present and passing.

crt-034 test breakdown (33 total):
- `services::co_access_promotion_tick` (tick module): 23 tests
- `infra::config` (co_access fields): 6 tests (`test_max_co_access_promotion_per_tick_default`, `_validation_zero`, `_validation_over_limit`, `_validation_boundary_values`, `test_merge_configs_project_overrides_global_co_access_cap`, `test_merge_configs_global_only_co_access_cap`)
- `background::tests` (constant): 1 test (`test_promotion_early_run_warn_ticks_constant_value`)
- `unimatrix-store::read::tests` (constants): 3 tests (`test_edge_source_co_access_value`, `test_co_access_graph_min_count_value`, `test_co_access_constants_colocated_with_nli`)

Edge cases E-01 through E-06 all covered:
- E-01: `test_single_qualifying_pair_weight_one`
- E-02: `test_tied_counts_secondary_sort_stable`
- E-03: `test_cap_equals_qualifying_count`
- E-04: `test_cap_one_selects_highest_count`
- E-05: `test_weight_delta_exactly_at_boundary_no_update`
- E-06: `test_self_loop_pair_no_panic`

**Integration tests:**

Smoke gate: 22 passed, 0 failed — PASS.

Lifecycle suite: 41 passed, 2 xfailed, 1 xpassed, 0 failed — PASS.

xfail markers verified:
- `test_auto_quarantine_after_consecutive_bad_ticks`: marked `@pytest.mark.xfail(reason="Pre-existing: GH#291...")` — confirmed pre-existing, GH#291 is tick-interval-not-drivable, unrelated to crt-034.
- `test_dead_knowledge_entries_deprecated_by_tick`: same pre-existing tick-interval limitation, GH#291.
- `test_search_multihop_injects_terminal_active` (XPASS): marked `@pytest.mark.xfail(reason="Pre-existing: GH#406...")` — confirmed pre-existing, multi-hop topology traversal. The XPASS means the test now passes despite the xfail marker. This is not caused by crt-034 (crt-034 does not touch search.rs or multi-hop injection logic). Since `xfail_strict` is not set in pytest.ini, XPASS is a warning-level outcome, not a failure. No action required from this feature.

No integration tests were deleted, commented out, or newly marked xfail by crt-034.

**RISK-COVERAGE-REPORT includes integration test counts**: smoke 22, lifecycle 41 — confirmed.

---

### 3. Specification Compliance

**Status**: PASS

**Evidence**: All 17 functional requirements verified:

- FR-01 (promote qualifying pairs): implemented — batch SELECT with `WHERE count >= CO_ACCESS_GRAPH_MIN_COUNT`
- FR-02 (ORDER BY count DESC): confirmed in SQL query at line 93 of co_access_promotion_tick.rs
- FR-03 (conditional UPDATE with delta guard): `if delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA { continue; }` at line 241 — strict `<=` means delta=0.1 exactly is NOT updated (AC-03, E-05)
- FR-04 (global MAX normalization): embedded scalar subquery `(SELECT MAX(count) FROM co_access WHERE count >= ?1)` — same WHERE predicate as outer query, ensuring global scope
- FR-05 (edge metadata): `relation_type='CoAccess'`, `source=EDGE_SOURCE_CO_ACCESS`, `created_by='tick'`, `bootstrap_only=0` — verified by test and code
- FR-06 (positioning): after orphaned-edge compaction, before TypedGraphState::rebuild — ORDERING INVARIANT anchor confirmed
- FR-07 (unconditional): no `nli_enabled` guard — confirmed by code review of background.rs
- FR-08 (SR-05 warn): `qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS` — implemented at line 120
- FR-09 (infallible): function signature `async fn run_co_access_promotion_tick(...) -> ()` — confirmed; all error paths use `warn!` + `continue`/early return
- FR-10 (summary info! log): emitted at Phase 4 (line 274), always fires including on fetch error path (line 105)
- FR-11 (InferenceConfig field): `max_co_access_promotion_per_tick: usize`, serde default 200, validate [1,10000], merge_configs — all confirmed
- FR-12 (CO_ACCESS_GRAPH_MIN_COUNT public const): exported from unimatrix-store, value 3i64 — confirmed
- FR-13 (EDGE_SOURCE_CO_ACCESS public const): exported alongside EDGE_SOURCE_NLI in read.rs — confirmed
- FR-14 (CO_ACCESS_WEIGHT_UPDATE_DELTA as f64): `const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1` at line 33 — correct type, correct value
- FR-15 (write_pool_server() direct path): all sqlx queries use `store.write_pool_server()` — confirmed
- FR-16 (module size <= 500 lines): 288 lines — PASS
- FR-17 (no GC of sub-threshold edges): confirmed — tick only INSERT OR IGNORE and UPDATE, no DELETE

Non-functional requirements:
- NFR-01 (tick latency): batch size bounded by cap (default 200), pure SQL on ~0.34 MB table
- NFR-02 (write pool contention): infallible contract absorbs timeouts
- NFR-03 (cap behavior): ORDER BY count DESC LIMIT applied
- NFR-04 (idempotency): INSERT OR IGNORE + delta guard — confirmed by `test_double_tick_idempotent`
- NFR-05 (observability): tracing::info! and warn! confirmed
- NFR-06 (module size): 288 lines, well under 500
- NFR-07 (no schema migration): no migration files changed — confirmed

All 15 acceptance criteria verified: AC-01 through AC-15 map to passing named tests (see RISK-COVERAGE-REPORT Acceptance Criteria Verification table).

**Note**: ACCEPTANCE-MAP.md shows all AC statuses as "PENDING". This is a cosmetic issue — the map was not updated after testing completed. The RISK-COVERAGE-REPORT provides the authoritative verified state. This is a WARN only.

---

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- **Component boundaries**: `services/co_access_promotion_tick.rs` is the single new module, registered via `services/mod.rs` — matches ARCHITECTURE.md §Component Breakdown.
- **Constants location**: `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` defined in `unimatrix-store/src/read.rs` alongside `EDGE_SOURCE_NLI` — matches ADR-002.
- **CO_ACCESS_WEIGHT_UPDATE_DELTA**: module-private `const f64` in `co_access_promotion_tick.rs` — matches ADR-003. Type is `f64` not `f32` as required by FR-14/ADR-003.
- **InferenceConfig field**: serde default fn `default_max_co_access_promotion_per_tick() -> usize { 200 }`, validate() range check, merge_configs stanza — matches ADR-004 and ARCHITECTURE.md §infra/config.rs.
- **Tick insertion**: ORDERING INVARIANT anchor comment present, call positioned after orphaned-edge compaction (line 556) and before TypedGraphState::rebuild() (line 570) — matches ADR-005.
- **write_pool_server() path**: all writes use direct pool, not analytics drain — matches ARCHITECTURE.md §Why write_pool_server().
- **No rayon pool**: confirmed — pure SQL path.
- **No schema migration**: no migration files changed.
- **One-directional edges v1**: source_id=entry_id_a (lower ID), target_id=entry_id_b — matches ADR-006 and known limitation documentation.
- **PROMOTION_EARLY_RUN_WARN_TICKS = 5**: defined in tick module (not background.rs), as resolved by Gate 3a OQ-4.

No architectural drift detected.

---

### 5. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md (tester report) contains a `## Knowledge Stewardship` section with:
- `Queried: mcp__unimatrix__context_briefing` — returned entries #3822, #3826, #3821, #2800, #3621. Applied to verify test coverage against known patterns.
- `Stored: nothing novel to store — the crt-034 test suite follows established patterns from nli_detection_tick.rs and the infra-001 USAGE-PROTOCOL. No new fixture patterns or test infrastructure was invented.`

Both `Queried:` and `Stored:` entries are present with adequate reasoning.

---

## File Size Note

The test extract file `co_access_promotion_tick_tests.rs` is 636 lines. The workspace 500-line convention targets source modules; extracted `*_tests.rs` files (a pattern used to keep the primary module under 500 lines, per NFR-06/FR-16) are test-only files and are conventionally excluded from this limit. The primary module at 288 lines is compliant. This is consistent with prior Unimatrix precedent for test extraction (visible in `co_access_promotion_tick.rs` line 287: `#[path = "co_access_promotion_tick_tests.rs"] mod tests;`).

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — no new recurring gate failure patterns were identified across this validation. All risks were fully mitigated and all coverage was complete on first pass. The ACCEPTANCE-MAP "PENDING" status cosmetic issue is feature-specific and does not represent a systemic pattern.
