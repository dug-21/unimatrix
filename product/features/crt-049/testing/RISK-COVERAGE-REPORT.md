# Risk Coverage Report: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Triple-alias serde chain silent zero (delivery_count, tier1_reuse_count) | `test_search_exposure_count_deserializes_from_canonical_key`, `test_search_exposure_count_deserializes_from_delivery_count_alias`, `test_search_exposure_count_deserializes_from_tier1_reuse_count_alias`, `test_search_exposure_count_serializes_to_canonical_key`, `test_search_exposure_count_round_trip_all_alias_forms` | PASS | Full |
| R-02 | normalize_tool_name omission produces silent zero explicit_read_count | `test_extract_explicit_read_ids_prefixed_context_get_matched`, `test_extract_explicit_read_ids_prefixed_context_lookup_matched`, `test_compute_knowledge_reuse_for_sessions_prefixed_tool_name_normalized` | PASS | Full |
| R-03 | total_served semantics change silent on stale records | `test_compute_knowledge_reuse_total_served_excludes_search_exposures`, `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections`, advisory message text verified in tools.rs line 2725-2726 | PASS | Full |
| R-04 | explicit_read_by_category silently partial at cap | `test_explicit_read_meta_cap_constant_exists` (EXPLICIT_READ_META_CAP == 500), cap boundary logic verified via code review — 501-ID warn path in tools.rs line 3281-3285 | PASS | Partial (boundary behavior code-review only; no 501-ID runtime test — acceptable per test plan) |
| R-05 | Early-return guard retains old condition | `test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle`, `test_render_knowledge_reuse_injection_only_cycle_not_suppressed` | PASS | Full |
| R-06 | render_knowledge_reuse section-order regression | `test_render_knowledge_reuse_golden_output_all_sections`, `test_render_knowledge_reuse_no_legacy_distinct_entries_served_label`, `test_render_knowledge_reuse_explicit_read_categories_section` | PASS | Full |
| R-07 | attributed slice not threaded through | `test_compute_knowledge_reuse_for_sessions_explicit_read_count_from_attributed`, `test_compute_knowledge_reuse_for_sessions_prefixed_tool_name_normalized` | PASS | Full |
| R-08 | SUMMARY_SCHEMA_VERSION not bumped | `test_summary_schema_version_is_three` (CRS-V24-U-01) | PASS | Full |
| R-09 | explicit_read_by_category field contract break for Group 10 | `test_compute_knowledge_reuse_explicit_read_by_category_populated`, `test_compute_knowledge_reuse_explicit_read_by_category_empty_when_no_reads`, `test_explicit_read_by_category_serde_round_trip`, `test_explicit_read_by_category_defaults_to_empty_map_when_absent` | PASS | Full |
| R-10 | Filter-based context_lookup included in explicit reads | `test_extract_explicit_read_ids_filter_lookup_excluded`, `test_extract_explicit_read_ids_null_id_excluded`, `test_extract_explicit_read_ids_single_id_lookup_included` | PASS | Full |
| R-11 | N+1 query pattern in batch_entry_meta_lookup | Code review: single `batch_entry_meta_lookup` call in `compute_knowledge_reuse_for_sessions` (tools.rs line 3294-3299); `test_compute_knowledge_reuse_for_sessions_explicit_read_count_from_attributed` (no N+1 panic in integration test) | PASS | Full |
| R-12 | total_served deduplication not applied | `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections`, `test_compute_knowledge_reuse_total_served_deduplication_overlap`, `test_compute_knowledge_reuse_total_served_disjoint_sets` | PASS | Full |
| R-13 | Fixture updates for delivery_count rename incomplete | Compilation: `cargo test --workspace` passes with zero errors — all `delivery_count` Rust field references resolved; golden-output assertions checked: no literal `"delivery_count"` in assertions (legacy key only appears in JSON input fixtures for backward-compat tests) | PASS | Full |

---

## Gate Items Verification

| Gate AC | Description | Test | Result |
|---------|-------------|------|--------|
| AC-02 | Triple-alias serde chain (5 assertions) | `test_search_exposure_count_deserializes_from_canonical_key`, `..._from_delivery_count_alias`, `..._from_tier1_reuse_count_alias`, `..._serializes_to_canonical_key`, `..._round_trip_all_alias_forms` | PASS |
| AC-06 | normalize_tool_name called — prefixed name matched | `test_extract_explicit_read_ids_prefixed_context_get_matched` | PASS |
| AC-13 | explicit_read_by_category field contract | `test_compute_knowledge_reuse_explicit_read_by_category_populated` | PASS |
| AC-14 | total_served excludes search exposures | `test_compute_knowledge_reuse_total_served_excludes_search_exposures` | PASS |
| AC-15 | total_served deduplication | `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections` (explicit reads {1,2}, injections {2,3} → total_served=3) | PASS |
| AC-16 | String-form ID handling | `test_extract_explicit_read_ids_string_form_id_handled` (integer form + string form both produce 42/99 in result) | PASS |
| AC-17 | Injection-only render guard (`total_served==0 && search_exposure_count==0`) | `test_render_knowledge_reuse_injection_only_cycle_not_suppressed`; guard at `retrospective.rs:998` confirmed | PASS |

**All 7 gate ACs: PASS.**

---

## Test Results

### Unit Tests

- Total across workspace: 4,336 (pre-crt-049 baseline was ~4,254; delta of ~82 new tests from crt-049)
- Passed: 4,336
- Failed: 0
- Ignored: 28 (pre-existing, not related to crt-049)

#### crt-049 Specific Test Counts by Module

| Module | New Tests | Source |
|--------|-----------|--------|
| `unimatrix-observe/src/types.rs` | 8 | AC-02 (5) + AC-01/AC-13 field defaults (3) |
| `unimatrix-server/src/mcp/knowledge_reuse.rs` | ~24 | AC-12 (13 extract_explicit_read_ids) + AC-13/AC-14/AC-15/AC-09/AC-17 (7 compute) + 4 edge cases |
| `unimatrix-server/src/mcp/response/retrospective.rs` | 6 | AC-07 golden output (3) + AC-17 (2) + legacy label check (1) |
| `unimatrix-server/src/mcp/tools.rs` | 4 | AC-05 (2 new) + 1 updated + constant check (1) |
| `unimatrix-store/src/cycle_review_index.rs` | 1 updated | CRS-V24-U-01 renamed/updated: `test_summary_schema_version_is_three` |

All unit tests: **PASS**

### Integration Tests (infra-001)

#### Smoke Gate (Mandatory)
- Suite: `pytest -m smoke`
- Total: 23
- Passed: 23
- Failed: 0
- XFailed: 0
- Result: **PASS**

#### Lifecycle Suite
- Suite: `suites/test_lifecycle.py`
- Total collected: 55
- Passed: 48
- XFailed: 5 (all pre-existing — tick interval dependencies; unrelated to crt-049)
- XPassed: 2 (pre-existing xfail markers now passing; not caused by crt-049; existing GH issues should be closed)
- Failed: 0
- Result: **PASS** (xpassed tests are not failures; they indicate a pre-existing issue was fixed elsewhere)

#### Tools Suite
- Suite: `suites/test_tools.py`
- Total: 119
- Passed: 117
- XFailed: 2 (pre-existing)
- Failed: 0
- Result: **PASS**

#### Integration Test Totals
- Smoke: 23 passed / 0 failed
- Lifecycle: 48 passed / 0 failed / 5 xfail / 2 xpass (pre-existing)
- Tools: 117 passed / 0 failed / 2 xfail (pre-existing)
- **Total integration: 188 passed / 0 failed**

---

## Gaps

None. All 13 risks from RISK-TEST-STRATEGY.md have test coverage. R-04 boundary behavior (501-ID cap warning) is covered structurally via code review and the constant existence assertion (`test_explicit_read_meta_cap_constant_exists`); a full runtime boundary test is not practical without a mock store and is documented as a code review check in the test plan.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `FeatureKnowledgeReuse.explicit_read_count: u64` with `#[serde(default)]` in types.rs line 296; `test_explicit_read_count_defaults_to_zero_when_absent` passes |
| AC-02 [GATE] | PASS | All 5 serde round-trip tests pass: canonical, delivery_count alias, tier1_reuse_count alias, canonical serialization output, full round-trip |
| AC-03 | PASS | `test_extract_explicit_read_ids_context_get_included`, `test_extract_explicit_read_ids_single_id_lookup_included` |
| AC-04 | PASS | `test_extract_explicit_read_ids_filter_lookup_excluded`, `test_extract_explicit_read_ids_null_id_excluded` |
| AC-05 | PASS | `test_compute_knowledge_reuse_for_sessions_explicit_read_count_from_attributed` (store-backed, returns explicit_read_count=1) |
| AC-06 [GATE] | PASS | `test_extract_explicit_read_ids_prefixed_context_get_matched` confirms mcp__unimatrix__context_get resolves to ID 7 via normalize_tool_name |
| AC-07 | PASS | `test_render_knowledge_reuse_golden_output_all_sections` verifies all 5 assertions: label text, values, ordering, legacy label absent |
| AC-08 | PASS | `test_summary_schema_version_is_three` (CRS-V24-U-01): SUMMARY_SCHEMA_VERSION == 3u32 |
| AC-09 | PASS | `test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle` (search_exposure_count=0, explicit_read_ids={5} → explicit_read_count=1) |
| AC-10 | PASS | Full workspace test suite green; no existing tests weakened |
| AC-11 | PASS | `test_compute_knowledge_reuse_for_sessions_no_block_on_panic` updated to pass &[] for attributed; existing assertions still pass |
| AC-12 | PASS | 13 tests covering all 5 sub-cases: (a) context_get included, (b) filter lookup excluded, (c) single-ID lookup included, (d) prefixed name matched [GATE], (e) empty slice returns empty |
| AC-13 [GATE] | PASS | `test_compute_knowledge_reuse_explicit_read_by_category_populated`: {"decision":2,"pattern":1} for IDs with known categories; serde round-trip; empty map default |
| AC-14 [GATE] | PASS | `test_compute_knowledge_reuse_total_served_excludes_search_exposures`: total_served=0 when only search exposures; `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections`: total_served=3 not 6 |
| AC-15 [GATE] | PASS | `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections`: explicit_reads={1,2}, injection_ids={2,3} → total_served=3 (overlap deduplicated) |
| AC-16 [GATE] | PASS | `test_extract_explicit_read_ids_string_form_id_handled`: integer form {"id":42} and string form {"id":"99"} both produce correct u64 values in returned HashSet |
| AC-17 [GATE] | PASS | `test_render_knowledge_reuse_injection_only_cycle_not_suppressed`: total_served=3, search_exposure_count=0 → output non-empty, contains "Entries served to agents"; guard at retrospective.rs:998 is `total_served==0 && search_exposure_count==0` |

**All 17 ACs: PASS. All 7 gate ACs: PASS.**

---

## XFailed / XPassed Test Notes

### XFailed Tests (pre-existing, not caused by crt-049)

All lifecycle xfails are pre-existing tick-interval dependencies not related to crt-049:
- `test_auto_quarantine_after_consecutive_bad_ticks` — requires UNIMATRIX_TICK_INTERVAL_SECONDS env var
- `test_dead_knowledge_entries_deprecated_by_tick` — background tick at 15-min interval
- `test_context_status_supports_edge_count_increases_after_tick` — MCP-visible tick
- `test_s1_edges_visible_in_status_after_tick` — fast_tick_server fixture needed
- `test_inferred_edge_count_unchanged_by_s1_s2_s8` — bugfix-491 related

Tools suite xfails (2): pre-existing, not related to crt-049.

No new xfail markers were added for crt-049. No GH Issues filed — no crt-049-caused failures.

### XPassed Tests (2 in lifecycle suite)

- `test_search_multihop_injects_terminal_active` — was xfail, now passing
- `test_inferred_edge_count_unchanged_by_cosine_supports` — was xfail, now passing

These tests started passing due to other feature work unrelated to crt-049. The xfail markers should be removed and their corresponding GH issues closed in a follow-up.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found entries #3806 (gate 3b REWORKABLE pattern), #238 (test infrastructure conventions), #4218 (crt-049 ADR-001), #748/#747 (test infrastructure patterns). Directly informed execution order and code verification approach.
- Stored: nothing novel to store — all test patterns applied here (serde alias round-trip, normalize_tool_name prefix coverage, golden-output assertions) are already captured in existing Unimatrix entries #885, #4211, #3426. No new execution patterns discovered.
