# Gate 3a Report: crt-034

> Gate: 3a (Design Review)
> Date: 2026-03-30
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 4 components match architecture decomposition; interface contracts correct |
| Specification coverage | PASS | All 17 FRs and 15 ACs addressed in pseudocode; f64 type correction applied per ADR-003 |
| Risk coverage | PASS | All 13 risks mapped to named test scenarios; no risk without coverage |
| Interface consistency | PASS | Shared types and function signature consistent across OVERVIEW.md and component files |
| Knowledge stewardship | PASS | Both agent reports contain `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

All four components defined in ARCHITECTURE.md §Component Breakdown map 1:1 to pseudocode files:

| Architecture Component | Pseudocode File | Status |
|----------------------|-----------------|--------|
| `services/co_access_promotion_tick.rs` (new) | `pseudocode/co_access_promotion_tick.md` | Matches |
| `unimatrix-store` constants | `pseudocode/store_constants.md` | Matches |
| `infra/config.rs` InferenceConfig extension | `pseudocode/config_extension.md` | Matches |
| `background.rs` tick insertion | `pseudocode/background_tick_insertion.md` | Matches |

**ADR compliance verified**:

- ADR-001 (single batch fetch with embedded scalar subquery): `co_access_promotion_tick.md` SQL shows `(SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count` embedded in the main SELECT — one round-trip, correct.
- ADR-002 (constants in read.rs alongside EDGE_SOURCE_NLI): `store_constants.md` places both constants immediately after `EDGE_SOURCE_NLI` at line ~1630, re-exported via `lib.rs`. Correct.
- ADR-003 (CO_ACCESS_WEIGHT_UPDATE_DELTA as f64 module-private constant): `co_access_promotion_tick.md` declares `const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1`. Correct. The architecture integration surface table lists `f32` but this is explicitly superseded by ADR-003 and documented in `pseudocode/OVERVIEW.md` as a known deviation.
- ADR-004 (InferenceConfig field follows max_graph_inference_per_tick pattern): `config_extension.md` shows all 5 required modifications (struct field, serde default fn, Default impl, validate() range check, merge_configs() stanza). Correct.
- ADR-005 (tick insertion point with ORDERING INVARIANT anchor comment; SR-05 warn inside tick function): `background_tick_insertion.md` shows the full ORDERING INVARIANT comment block and unconditional call site. `co_access_promotion_tick.md` shows the SR-05 warn condition `IF qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS`. Correct.
- ADR-006 (one-directional edges, source_id=entry_id_a, target_id=entry_id_b): INSERT in `co_access_promotion_tick.md` binds `row.entry_id_a` to `?1` (source_id) and `row.entry_id_b` to `?2` (target_id). Correct.

**Technology consistency**: No new crates introduced. All writes use `write_pool_server()` per architecture requirement. No analytics drain path used. No rayon pool.

---

### Specification Coverage

**Status**: PASS

**Evidence**: All functional requirements verified against pseudocode:

| FR | Requirement | Pseudocode Evidence | Status |
|----|-------------|---------------------|--------|
| FR-01 | Promote qualifying pairs (count >= CO_ACCESS_GRAPH_MIN_COUNT) | WHERE count >= CO_ACCESS_GRAPH_MIN_COUNT in batch SELECT | PASS |
| FR-02 | ORDER BY count DESC | ORDER BY count DESC in batch SELECT | PASS |
| FR-03 | Conditional weight update when delta > CO_ACCESS_WEIGHT_UPDATE_DELTA | `IF delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA: CONTINUE` (strict >, per E-05) | PASS |
| FR-04 | Global MAX(count) normalization | Scalar subquery over all qualifying pairs, not batch subset | PASS |
| FR-05 | INSERT metadata (relation_type='CoAccess', source='co_access', created_by='tick', bootstrap_only=0) | All four values in INSERT OR IGNORE statement | PASS |
| FR-06 | Position after step 2, before step 3 | ORDERING INVARIANT comment in background_tick_insertion.md | PASS |
| FR-07 | Unconditional execution | No nli_enabled guard at call site | PASS |
| FR-08 | SR-05 warn on qualifying_count==0 AND tick < PROMOTION_EARLY_RUN_WARN_TICKS | Condition present; outside window is clean no-op | PASS |
| FR-09 | Infallible (async fn -> ()) | All error paths use warn! + continue; no Result propagation | PASS |
| FR-10 | info! log with inserted/updated counts | Phase 4 summary always fires | PASS |
| FR-11 | InferenceConfig field (default 200, range [1,10000], merge_configs) | config_extension.md Modifications 1-5 | PASS |
| FR-12 | CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3 | store_constants.md declares i64 = 3 | PASS |
| FR-13 | EDGE_SOURCE_CO_ACCESS: &str = "co_access" | store_constants.md declares &str = "co_access" | PASS |
| FR-14 | CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1 | Pseudocode declares f64 (not f32); documented deviation from architecture table | PASS |
| FR-15 | write_pool_server() for all writes | All sqlx::query calls use store.write_pool_server() | PASS |
| FR-16 | < 500 lines | Estimated 350 non-test + 200-250 test lines; file size guidance included | PASS |
| FR-17 | No GC of sub-threshold edges | AC-15 test covers; tick has no DELETE logic | PASS |

**NFR coverage**: NFR-04 (idempotency via INSERT OR IGNORE + delta guard), NFR-05 (observability via warn!/info!), NFR-06 (< 500 lines), NFR-07 (no schema changes) — all addressed in pseudocode.

**No scope additions**: Pseudocode implements only what the specification requires. No extra MCP tools, no schema changes, no GC logic, no bidirectional edges.

---

### Risk Coverage

**Status**: PASS

**Evidence**: All 13 risks from RISK-TEST-STRATEGY.md have named test scenarios in the test plans.

| Risk ID | Priority | Test Plan Coverage | Status |
|---------|----------|--------------------|--------|
| R-01 (write failure absorption) | Critical | `test_write_failure_mid_batch_warn_and_continue`, `test_write_failure_info_log_always_fires` | PASS |
| R-02 (division by zero max_count=0) | High | `test_empty_co_access_table_noop_late_tick`, `test_all_below_threshold_noop_late_tick`; max_count is Option<i64> with early-return guard | PASS |
| R-03 (subquery global MAX correctness) | High | `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` | PASS |
| R-04 (rows_affected no-op detection) | High | `test_existing_edge_stale_weight_updated`, `test_existing_edge_current_weight_no_update`, `test_double_tick_idempotent` | PASS |
| R-05 (tick ordering violation) | High | Code review AC-05; ORDERING INVARIANT anchor comment in background_tick_insertion.md | PASS |
| R-06 (SR-05 warn window reopens on restart) | High | 4-quadrant tests: early+zero, late+zero, early+qualifying, late+qualifying | PASS |
| R-07 (merge_configs omission) | High | `test_merge_configs_project_overrides_global_co_access_cap` directly targets merge omission | PASS |
| R-08 (threshold constant divergence) | Med | `test_co_access_graph_min_count_value` asserts value=3; code-review note for migration alignment | PASS |
| R-09 (near-threshold oscillation) | Med | `test_double_tick_idempotent`, `test_sub_threshold_pair_not_gcd` | PASS |
| R-10 (one-directional contract violated) | Med | `test_inserted_edge_is_one_directional` asserts no reverse row | PASS |
| R-11 (ORDER BY count DESC omitted) | High | `test_cap_selects_highest_count_pairs` asserts WHICH pairs selected (not just count) | PASS |
| R-12 (file size > 500 lines) | Low | Gate 3c static check; file size guidance in pseudocode | PASS |
| R-13 (inserted edge missing metadata) | High | `test_inserted_edge_metadata_all_four_fields` asserts all 4 fields | PASS |

Integration risks I-01 (GH #409 race), I-02 (analytics drain not used), I-03 (TypedGraphState inclusion) — addressed in test-plan OVERVIEW.md integration section.

Edge cases E-01 through E-06 all have named test functions in `co_access_promotion_tick.md` test plan.

**Delta boundary at exactly 0.1**: `test_weight_delta_exactly_at_boundary_no_update` explicitly tests `delta == 0.1` is NOT updated (strict `>`). This matches FR-03 and E-05 from the risk strategy. Test uses count=6/max=10 → weight=0.6 vs stored=0.5 → delta=0.1 exactly → no UPDATE.

---

### Interface Consistency

**Status**: PASS

**Evidence**:

The OVERVIEW.md integration surface table is the master reference. Consistency verified across components:

| Constant / Function | OVERVIEW.md | Component Pseudocode | Match |
|--------------------|-------------|---------------------|-------|
| `EDGE_SOURCE_CO_ACCESS` | `pub const &str = "co_access"` | store_constants.md same | Yes |
| `CO_ACCESS_GRAPH_MIN_COUNT` | `pub const i64 = 3` | store_constants.md same | Yes |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `const f64 = 0.1` (module-private) | co_access_promotion_tick.md same | Yes |
| `run_co_access_promotion_tick` | `async fn(store: &Store, config: &InferenceConfig, current_tick: u32)` | co_access_promotion_tick.md same; background_tick_insertion.md uses same sig | Yes |
| `max_co_access_promotion_per_tick` | `usize`, default 200, range [1,10000] | config_extension.md same | Yes |

**Function signature discrepancy from ARCHITECTURE.md** is noted and resolved: the architecture integration surface table shows the 2-parameter form `async fn(store: &Store, config: &InferenceConfig)`. ADR-005 adds the `current_tick: u32` parameter for SR-05 detectability. OVERVIEW.md explicitly flags this as a documented deviation and identifies the 3-parameter form as authoritative. This is a pre-resolved architectural evolution, not an inconsistency.

**Data flow through components**: co_access (read) → co_access_promotion_tick → graph_edges (write) → TypedGraphState::rebuild() reads in same tick cycle. This is coherent across all pseudocode files.

**No contradictions** between component pseudocode files on shared types.

---

### WARN: PROMOTION_EARLY_RUN_WARN_TICKS constant access in tick module

**Status**: WARN

**Evidence**: `background_tick_insertion.md` defines `PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5` as a constant in `background.rs`. `co_access_promotion_tick.md` pseudocode references the symbol `PROMOTION_EARLY_RUN_WARN_TICKS` directly inside the tick function body. However, if the constant is defined in `background.rs` (file-private), the tick module cannot access it without it being re-declared or made `pub(crate)`.

ADR-005 acknowledges this: "In practice the SR-05 warn is emitted from within `run_co_access_promotion_tick` itself" with `current_tick` passed as a parameter. The tick function compares `current_tick` against a threshold — but needs the threshold value to be accessible inside the function. Options: (1) duplicate the constant as `const PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5` in `co_access_promotion_tick.rs`, (2) make it `pub(crate)` in `background.rs` and import it, or (3) inline the literal `5` in the tick function.

The test-plan agent flags this as Open Question 4. Implementation agent should resolve before coding; any of the three options is acceptable. This does not block implementation. **No rework required at this stage**.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

- `crt-034-agent-1-pseudocode-report.md` contains `## Knowledge Stewardship` with 3 `Queried:` entries documenting searches for background tick patterns, ADRs, and briefing results. Per gate rules, pseudocode agents are read-only and require `Queried:` entries only — no `Stored:` entry is required. Compliant.

- `crt-034-agent-2-testplan-report.md` contains `## Knowledge Stewardship` with 3 `Queried:` entries and `Stored: nothing novel — {reason}`. Compliant.

- Architecture and risk-strategy source documents also contain stewardship blocks (verified in source docs read).

---

## Rework Required

None. All checks passed.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this gate pass reflects clean execution of established patterns. The PROMOTION_EARLY_RUN_WARN_TICKS constant access ambiguity is a WARN that will resolve during implementation; not yet a lesson-learned.
