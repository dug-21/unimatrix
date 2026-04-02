# Gate 3a Report: crt-041

> Gate: 3a (Design Review)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 6 components match architecture decomposition; ADRs followed; no new deps |
| Specification coverage | PASS | All 32 FRs and 32 ACs represented in pseudocode; no scope additions |
| Risk coverage | PASS | All 17 risks mapped to test scenarios; Critical risks fully covered |
| Interface consistency | WARN | `write_graph_edge` actual signature deviates from architecture doc; pseudocode correctly uses actual signature; one test-plan assertion uses wrong `created_by` value |
| Knowledge stewardship | PASS | Both agent reports include stewardship sections with Queried/Stored entries |

---

## Detailed Findings

### Architecture Alignment
**Status**: PASS
**Evidence**: Each pseudocode component maps 1:1 to the architecture's Component Breakdown:
- `graph_enrichment_tick.rs` — new module with `run_graph_enrichment_tick`, `run_s1_tick`, `run_s2_tick`, `run_s8_tick`, `S8_WATERMARK_KEY` constant, row types, helper. Matches architecture §1.
- `nli_detection.rs` prerequisite — pseudocode OVERVIEW.md confirms `write_graph_edge` exists (crt-040 shipped it); no conditional add needed. Matches architecture §2.
- `unimatrix-store/src/read.rs` + `lib.rs` — three named constants plus re-export. Matches architecture §3/§4.
- `infra/config.rs` — five fields at four modification sites (struct, Default, `default_*()` functions, `validate()`, `merge_configs()`). Matches architecture §5.
- `background.rs` + `services/mod.rs` — import + call site after `run_graph_inference_tick`. Matches architecture §6.

Tick ordering in pseudocode/background.md matches architecture §Tick Ordering exactly:
`... → run_graph_inference_tick → run_graph_enrichment_tick (S1 always → S2 always → S8 gated)`.

ADR compliance:
- ADR-001 (module structure): tick functions in `graph_enrichment_tick.rs`, `write_graph_edge` prerequisite gate documented. ✓
- ADR-002 (S2 safe SQL): `push_bind` used throughout S2 build loop, SECURITY comment included, no format! interpolation. ✓
- ADR-003 (S8 watermark): write-after-commit ordering enforced (C-11 comment in code), partial-row semantics documented. ✓
- ADR-004 (GraphCohesionMetrics extension scope): no new fields added, existing fields used. ✓
- ADR-005 (InferenceConfig dual-maintenance): both `impl Default` and `default_*()` functions defined with identical values; serde-match test mandated. ✓

No new dependencies introduced. All implementation uses sqlx, serde_json, tracing — already in workspace.

---

### Specification Coverage
**Status**: PASS
**Evidence**:

FR coverage (all 32):
- FR-01–FR-06 (S1): `run_s1_tick` implements self-join on `entry_tags`, dual-endpoint `status=0` JOIN, `INSERT OR IGNORE`, `ORDER BY shared_tags DESC LIMIT ?1`, weight formula `min(count*0.1, 1.0) as f32`, `tracing::info!`/`warn!`. ✓
- FR-07–FR-14 (S2): `run_s2_tick` implements early-return on empty vocab, `QueryBuilder::push_bind` loop, space-padded `instr()` pattern, `ORDER BY shared_terms DESC LIMIT`, `tracing::info!`/`warn!`. ✓
- FR-15–FR-23 (S8): `run_graph_enrichment_tick` applies `current_tick % s8_batch_interval_ticks == 0` gate; `run_s8_tick` implements watermark load, audit_log fetch, pair expansion, quarantine filter (chunked, C-13), edge write, watermark write-after-edges (C-11). ✓
- FR-24 (constants): EDGE_SOURCE_S1/S2/S8 defined with exact values "S1"/"S2"/"S8", re-exported from lib.rs. ✓
- FR-25 (InferenceConfig fields): five fields with correct types, defaults (vec![], 200, 200, 10, 500), ranges. ✓
- FR-26 (dual-maintenance): both sites defined identically; serde-match test mandated. ✓
- FR-27 (validate()): range checks for all four numeric fields, lower bound 1, upper bounds per spec. ✓
- FR-28–FR-31 (module structure, tick ordering): new module at correct path, `run_graph_enrichment_tick` entry point, wired in `background.rs` after `run_graph_inference_tick`, tick-ordering comment updated. ✓
- FR-32 (write_graph_edge prerequisite): pseudocode confirms prerequisite passed (exists in `nli_detection.rs`). ✓
- FR-33 (GraphCohesionMetrics): no new fields added, existing `cross_category_edge_count` and `isolated_entry_count` relied upon for eval gate. ✓

NFR coverage:
- NFR-01 (no ML): no rayon, no ONNX, no spawn_blocking — pure sqlx. ✓
- NFR-02 (infallible): all three tick functions return `()`, errors logged at `warn!`. ✓
- NFR-03 (latency): `test_s1_tick_completes_within_500ms_at_1200_entries` test specified. ✓
- NFR-04 (no schema migration): no new tables, no migration referenced. ✓
- NFR-05 (idempotency): INSERT OR IGNORE on UNIQUE constraint throughout. ✓
- NFR-06 (backward compat): `inferred_edge_count` isolation tested in T-GET-18. ✓
- NFR-07 (file size): 500-line limit noted with test extraction plan. ✓
- NFR-08 (eval gate): AC-32 integration test specified; one-tick delay documented. ✓

No scope additions found. Pseudocode does not implement anything outside the specification.

---

### Risk Coverage
**Status**: PASS
**Evidence**: All 17 risks from RISK-TEST-STRATEGY.md are addressed. Risk-to-test-function mapping:

| Risk | Priority | Test Coverage |
|------|----------|--------------|
| R-01 (dual-endpoint quarantine) | Critical | `test_s1_excludes_quarantined_source/target`, `test_s2_excludes_quarantined_source/target`, `test_s8_excludes_quarantined_endpoint` — 6 tests covering all 3 sources, both endpoint positions |
| R-02 (S2 SQL injection) | Critical | `test_s2_sql_injection_single_quote`, `test_s2_sql_injection_double_dash` — 2 adversarial vocabulary tests |
| R-03 (dual-site default divergence) | Critical | `test_inference_config_s1_s2_s8_defaults_match_serde` — MANDATORY pre-PR, blocks delivery |
| R-04 (S1 GROUP BY materialization) | High | `test_s1_tick_completes_within_500ms_at_1200_entries` — 1,200-entry corpus timing test |
| R-05 (S8 stuck watermark on malformed JSON) | High | `test_s8_watermark_advances_past_malformed_json_row` — 3-row fixture, watermark inspected directly |
| R-06 (watermark before edges) | High | `test_s8_watermark_written_after_edges` — write ordering + crash-recovery idempotency |
| R-07 (wrong edge source value) | High | `test_s1_source_value_is_s1_not_nli`, `test_edge_source_s1_value/s2_value/s8_value`, plus source assertions in every S1/S2/S8 basic test |
| R-08 (crt-040 prereq absent) | High | Shell grep pre-flight (AC-28) — delivery gate |
| R-09 (orphaned edges) | Med | CLOSED — compaction is source-agnostic; no test needed |
| R-10 (S8 cap on rows not pairs) | Med | `test_s8_pair_cap_not_row_cap`, `test_s8_partial_row_watermark_semantics` |
| R-11 (S2 false-positive substring) | Med | `test_s2_no_false_positive_capabilities_for_api`, `test_s2_no_false_positive_cached_for_cache`, `test_s2_true_positive_api_in_title` |
| R-12 (S8 wrong operation type) | Med | `test_s8_excludes_briefing_operation`, `test_s8_excludes_failed_search` |
| R-13 (inferred_edge_count inflated) | Med | `test_inferred_edge_count_excludes_s1_s2_s8` (unit) + integration xfail |
| R-14 (S2 empty vocabulary panic) | Med | `test_s2_empty_vocabulary_is_noop` |
| R-15 (eval gate timing) | Low | `test_cohesion_metrics_readable_without_ppr_rebuild` + AC-32 procedure |
| R-16 (file size violation) | Low | `wc -l` delivery gate (AC-31) |
| R-17 (validate() missing zero-value check) | Med | `test_inference_config_validate_rejects_zero_s1_cap/s2_cap/s8_interval/s8_pair_cap` |

All Critical risks have multiple test scenarios. All High risks have at least one test. All risk priorities from the strategy are reflected in test plan emphasis.

Integration risks from RISK-TEST-STRATEGY.md §Integration Risks are addressed:
- S1 self-join index coverage: EXPLAIN QUERY PLAN verification noted in pseudocode comment (R-04/OQ-01).
- S8 bulk pair-ID chunked batch: `SQLITE_MAX_VARIABLE_NUMBER = 900` constant defined, chunked loop implemented.
- S8 partial-row watermark: `test_s8_partial_row_watermark_semantics` explicitly covers this.

---

### Interface Consistency
**Status**: WARN
**Evidence**:

**PASS items:**
- `run_graph_enrichment_tick(store: &Store, config: &InferenceConfig, current_tick: u64)` — matches across pseudocode/OVERVIEW.md, graph_enrichment_tick.md, background.md.
- `run_s1_tick`, `run_s2_tick`, `run_s8_tick` signatures `(store: &Store, config: &InferenceConfig)` — consistent across all pseudocode files and the architecture integration surface table.
- EDGE_SOURCE constants: values "S1", "S2", "S8" consistent across OVERVIEW.md, edge_constants.md, and graph_enrichment_tick.md call sites.
- InferenceConfig field types/defaults: consistent across config.md, OVERVIEW.md, and architecture integration surface table.
- `S8_WATERMARK_KEY = "s8_audit_log_watermark"` consistent across OVERVIEW.md and graph_enrichment_tick.md.
- Counters helpers: `counters::read_counter` / `counters::set_counter` used consistently in S8 pseudocode; no mismatch.

**WARN: `write_graph_edge` signature deviation and `created_by` column implication**

The architecture integration surface table specifies:
```
write_graph_edge(store, source_id, target_id, relation_type, weight: f64, created_at, source, metadata: Option<&str>) -> bool
```

The actual crt-040 implementation (confirmed by pseudocode agent grep at `nli_detection.rs:78`) uses:
```
write_graph_edge(store, source_id, target_id, relation_type, weight: f32, created_at, source, metadata: &str) -> bool
```

The pseudocode correctly adapts to the actual signature throughout all call sites (using `as f32` casts and `""` for metadata). This is not a pseudocode error — the pseudocode agent correctly detected and documented the deviation.

However, this signature difference has a secondary consequence: because `write_graph_edge` binds `source` to both the `source` and `created_by` column slots, the `created_by` column will be written as `'S1'`, `'S2'`, `'S8'` (uppercase) rather than `'s1'`, `'s2'`, `'s8'` (lowercase) as specified in FR-04, FR-12, FR-20.

The test plan's `test_s1_basic_informs_edge_written` asserts `created_by = 's1'` (lowercase). With the actual implementation, this assertion will fail — `created_by` will be `'S1'`. The same applies to `test_s2_basic_informs_edge_written` (asserts `'s2'`) and `test_s8_basic_coaccess_edge_written` (asserts `'s8'`).

**Assessment**: This is a WARN rather than a FAIL because:
1. The architecture document acknowledges the deviation is acceptable: "the `source` column is the authoritative discriminator for GNN features."
2. The pseudocode OVERVIEW.md explicitly documents this and states the per-source `created_by` values in the spec are irrelevant.
3. The three affected test assertions are easily fixed by changing expected `created_by` values from lowercase to uppercase before coding.

The delivery agent must update these three test assertions to match the actual behavior before coding.

---

### Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**:

`crt-041-agent-1-pseudocode-report.md`:
- Contains `## Knowledge Stewardship` section. ✓
- `Queried:` entries present with specific search topics and entry IDs found. ✓
- No novel patterns stored (existing patterns cover the implementation) — acceptable.

`crt-041-agent-2-testplan-report.md`:
- Contains `## Knowledge Stewardship` section. ✓
- `Queried:` entries present with specific search topics and entry IDs found. ✓
- `Stored:` entry #4045 "SQL-only background tick integration tests: xfail on timing not model absence" — novel pattern correctly identified and stored. ✓

---

## Rework Required

None blocking delivery. One WARN requires attention before coding:

| Issue | Severity | Which Agent | What to Fix |
|-------|----------|-------------|-------------|
| Test plan assertions for `created_by` use spec values ('s1', 's2', 's8') that will not match actual runtime behavior ('S1', 'S2', 'S8') due to `write_graph_edge` binding `source` to both columns | WARN | rust-dev (delivery agent) | When implementing `test_s1_basic_informs_edge_written`, `test_s2_basic_informs_edge_written`, `test_s8_basic_coaccess_edge_written`: assert `created_by = 'S1'/'S2'/'S8'` (uppercase, matching EDGE_SOURCE constants), not `'s1'/'s2'/'s8'` as written in the test-plan pseudocode |

---

## Knowledge Stewardship

- Queried: Unimatrix knowledge not queried for this gate run — source documents and pseudocode artifacts were sufficient. Gate 3a is a pure document review; no pattern lookup needed.
- Stored: nothing novel to store — this is a clean design review with no new failure patterns; the `created_by`/`source` column binding issue is already captured in the pseudocode OVERVIEW.md and the pseudocode agent's report.
