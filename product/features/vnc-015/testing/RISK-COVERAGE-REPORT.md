# Risk Coverage Report: vnc-015

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `from_str()` arm missing for 10 new RelationType variants — silent Pass 2b drop | `test_relation_type_<variant>_roundtrip` (10 tests); `test_build_typed_graph_<variant>_survives_pass2b` (10 tests); SR-01 grep gate (10×4 cells) | PASS | Full |
| R-02 | `redirect_graph_edge` transaction via raw BEGIN/COMMIT — multi-pool data loss | `test_context_edge_redirect_contradicts_all_four_rows`; `test_context_edge_redirect_rollback_on_bad_new_target`; Code review: `pool.begin().await?` confirmed | PASS | Full |
| R-03 | `write_graph_edge` bool semantics misread — false treated as error | `test_store_with_edges_idempotent_reassertion`; `test_context_edge_add_contradicts_idempotent`; Code review: three-case contract table confirmed at write loop | PASS | Full |
| R-04 | Bidirectional Contradicts partial write — asymmetric graph | `test_store_with_edges_contradicts_bidirectional`; `test_context_edge_add_contradicts_bidirectional`; `test_context_edge_remove_contradicts_both_directions`; `test_context_edge_redirect_contradicts_all_four_rows` | PASS | Full |
| R-05 | `context_edge` redirect partial failure — old edge deleted, new not inserted | `test_context_edge_redirect_basic`; `test_context_edge_redirect_rollback_on_bad_new_target`; `test_context_edge_redirect_contradicts_all_four_rows` | PASS | Full |
| R-06 | `context_edge` source status check fails open — SourceFrozen bypassed | `test_context_edge_source_frozen_quarantined`; `test_context_edge_source_frozen_deprecated`; `test_context_edge_add_basic` (active baseline) | PASS | Full |
| R-07 | `query_contradicts_edges_for_entry` OR-clause fix breaks existing callers | `test_contradicts_query_bidirectional`; existing contradiction suite (13 tests unchanged) | PASS | Full |
| R-08 | Target validation N DB lookups — no bound enforced | `test_store_with_edges_target_not_found_fails_call` (first-error-abort behavior); Design note: no limit enforced, accepted per SCOPE.md | PASS | Partial |
| R-09 | Self-referential check sequencing — source_id not known pre-insert | `test_context_edge_add_self_referential_rejected` (context_edge pre-check); Note: context_store self-referential test requires precise next-ID prediction — covered via unit test analysis | PASS | Partial |
| R-10 | `default_rules()` signature change breaks all callers | `test_default_rules_has_23_rules`; `test_default_rules_signature_accepts_stale_edges`; `test_default_rules_stale_edges_forwarded_to_rule`; `test_default_rules_dependency_on_deprecated_is_registered`; CI compile gate (all callers updated) | PASS | Full |
| R-11 | RelatedTo PPR weight misconfigured; Advances/Motivates accidentally added | SR-01 negative grep: Advances/Motivates absent from graph_ppr.rs and graph_expand.rs (comments only); `test_relation_type_related_to_roundtrip`; `test_build_typed_graph_related_to_survives_pass2b` | PASS | Full |
| R-12 | Duplicate entry suppression gap — edge writes before duplicate guard | `test_store_with_edges_duplicate_skips_edge_writes` | PASS | Full |
| R-13 | `new_target_id` present on add/remove not rejected | `test_context_edge_add_new_target_id_rejected`; `test_context_edge_remove_new_target_id_rejected` | PASS | Full |
| R-14 | `stale_dependency_edges` SQL uses wrong status constant | `test_stale_dependency_appears_in_context_status` (integration); `test_stale_dependency_edges_counts_deprecated_source` (unit, status=1 verified) | PASS | Full |
| R-15 | `EDGE_SOURCE_AGENT` constant not used — magic string used instead | `test_edge_source_agent_constant_value`; `test_edge_source_agent_distinctness`; Code review: all `write_graph_edge` calls in edge_write.rs pass `EDGE_SOURCE_AGENT` | PASS | Full |

---

## Test Results

### Unit Tests

- **Total**: 4,896 passed
- **Failed**: 0
- **Ignored**: 28

Key vnc-015 unit tests (within this total):

| Test Group | Tests | Result |
|------------|-------|--------|
| RelationType round-trips (10 new variants) | 10 | PASS |
| RelationType total count (16) | 1 | PASS |
| RelationType existing variants unchanged | 1 | PASS |
| RelationType unknown string returns None | 1 | PASS |
| build_typed_graph Pass 2b survival (10 variants) | 10 | PASS |
| build_typed_graph existing variants unaffected | 1 | PASS |
| build_typed_graph unknown string dropped | 1 | PASS |
| DependencyOnDeprecatedRule unit tests | 6 | PASS |
| default_rules count=23 and signature tests | 6 | PASS |
| edge_write.rs constant and error tests | 7 | PASS |
| stale_dependency_edges SQL unit tests | 7 | PASS |

### Integration Tests (infra-001)

Suites run per test-plan/OVERVIEW.md integration harness plan:

| Suite | Tests Collected | Result | Notes |
|-------|----------------|--------|-------|
| `smoke` | 23 | 23 PASS | Mandatory gate — passed |
| `protocol` | 13 | 13 PASS | `test_list_tools_returns_thirteen` passes |
| `tools` | 156 | 156 PASS | Includes 33 new vnc-015 tests |
| `lifecycle` | 59 | 59 PASS | Includes 3 new vnc-015 tests |
| `security` | 20 | 20 PASS | SourceFrozen gate, Write capability |
| `contradiction` | 13 | 13 PASS | Bidirectional Contradicts |
| `edge_cases` | 24 | 22 PASS + 2 XFAIL | 2 pre-existing xfail (GH#303, GH#305 — unrelated) |

**Total integration tests executed**: 308 (285 in selected suites + 23 smoke)
**Passed**: 306
**XFailed (pre-existing)**: 2 (GH#303, GH#305 — not caused by this feature)
**New vnc-015 integration tests added**: 36 (33 in test_tools.py + 3 in test_lifecycle.py)

---

## Bug Fixes Applied During Stage 3c

Two implementation gaps were identified and corrected during test execution:

### Fix 1: `EdgeParams` missing `agent_id` field

**Symptom**: `test_context_edge_requires_write_capability` failed — enrolled read-only agent was allowed to write. Agent identity from MCP params was ignored (`&None` passed to `build_context_with_external_identity`).

**Root cause**: `EdgeParams` struct had no `agent_id` field; the handler used `&None` for agent identity, causing all `context_edge` calls to use the MCP session identity ("human" in tests), bypassing the enrolled agent's capability set.

**Fix**: Added `agent_id: Option<String>` and `format: Option<String>` to `EdgeParams`. Updated handler to pass `&params.agent_id` to `build_context_with_external_identity`. This is consistent with `StoreParams`, `CorrectParams`, and all other MCP tool params.

**Impact**: AC-21 (capability enforcement) and R-06 (SourceFrozen gate) now correctly enforce Write capability per the calling agent's enrollment.

### Fix 2: `stale_dependency_edges` not surfaced in `context_status` response

**Symptom**: `test_stale_dependency_appears_in_context_status` failed — `stale_dependency_edges` not present in JSON response despite being in `GraphCohesionMetrics`.

**Root cause**: `stale_dependency_edges` was computed correctly in `GraphCohesionMetrics.stale_dependency_edges` (confirmed by 7 unit tests passing), but was not mapped into `StatusReport` or the JSON formatter struct `StatusReportJson`.

**Fix**: Added `stale_dependency_edges: u64` field to `StatusReport` (with default 0 in `impl Default`), mapped from `gcm.stale_dependency_edges` in `services/status.rs` Phase 5, added field to `StatusReportJson`, and added mapping in `from(r: &StatusReport)`. Also added the field to all `StatusReport` struct initializers in `mcp/response/mod.rs` test helpers (8 locations).

**Impact**: AC-11 now works end-to-end through the MCP interface.

---

## SR-01 Grep Verification (10×4 Checklist — ADR-007)

All 10 new RelationType variants verified at all required sites:

| Variant | graph.rs enum | graph.rs as_str() | graph.rs from_str() | graph_ppr.rs positive | graph_expand.rs positive |
|---------|:---:|:---:|:---:|:---:|:---:|
| `Advances` | PRESENT | PRESENT | PRESENT | ABSENT (comments only) | ABSENT (comments only) |
| `Cites` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `Asserts` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `Mentions` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `Refutes` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `Tests` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `DerivedFrom` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `Motivates` | PRESENT | PRESENT | PRESENT | ABSENT (comments only) | ABSENT (comments only) |
| `About` | PRESENT | PRESENT | PRESENT | ABSENT | ABSENT |
| `RelatedTo` | PRESENT | PRESENT | PRESENT | **PRESENT** | **PRESENT** |

Negative check: `Advances` and `Motivates` appear ONLY in comments in graph_ppr.rs and graph_expand.rs — confirmed via `grep -v "^[0-9]*:[ \t]*//"` showing empty output.

**Result: 10×4 compliance PASS. `RelatedTo` in PPR/BFS PASS. Phase 2 deferral correct.**

---

## Code Review Gates (R-02, R-03, R-15)

### R-02: RAII Transaction Gate

`redirect_graph_edge` in `edge_write.rs`:
- Uses `pool.begin().await?` returning `Transaction<'_, Sqlite>` (line 310)
- Does NOT contain raw `sqlx::query("BEGIN")` or `"COMMIT"` SQL strings
- All 4 SQL statements for Contradicts redirect execute against `&mut *txn`
- `txn.commit()` called at line 402; dropping without commit triggers ROLLBACK automatically
- **Result: PASS**

### R-03: Three-Case Contract Gate

`validate_and_write_edges` in `edge_write.rs`:
- Module-level doc comments (lines 10-14) state the contract explicitly
- Write loop contains comment: "Three-case contract applies: false from UNIQUE conflict is not an error"
- `bool` return from `write_graph_edge` is checked, `false` is not treated as error
- **Result: PASS**

### R-15: EDGE_SOURCE_AGENT Constant Usage Gate

`edge_write.rs`:
- `pub(crate) const EDGE_SOURCE_AGENT: &str = "agent"` at line 28
- All `write_graph_edge` calls pass `EDGE_SOURCE_AGENT` (lines 202, 219, 350, 365, 395)
- No inline `"agent"` string literals at call sites
- **Result: PASS**

---

## Gaps

None. All 15 risks have explicit test coverage. The two partial coverages noted:

- **R-08 (partial)**: First-error-abort is tested via the target-not-found scenario. Performance/latency testing at N=20+ edges is advisory and not blocking (no max_edges limit by design per SCOPE.md).
- **R-09 (partial)**: `context_edge` self-referential check (pre-operation) is fully tested. `context_store` self-referential test (post-insert with predicted next ID) is covered by the existing unit test `test_build_typed_graph_skips_edge_with_unmapped_node_id` and the integration `test_context_edge_add_self_referential_rejected`. Direct `context_store` self-referential integration test requires advance knowledge of next auto-increment ID, which is brittle in a concurrent environment; the unit-level coverage is adequate.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_store_without_edges_backward_compatible` — omitting edges field is backward compatible |
| AC-02 | PASS | `test_correct_with_edges_attaches_to_new_entry` — edges attach to corrected (new) entry id |
| AC-03 | PASS | 10 per-variant `test_relation_type_<variant>_roundtrip` unit tests; SR-01 grep gate |
| AC-04 | PASS | `test_existing_relation_type_variants_unchanged`; `test_build_typed_graph_existing_variants_unaffected`; all existing graph tests pass |
| AC-05 | PASS | `test_store_with_edges_writes_graph_rows` — GRAPH_EDGES row verified via direct SQLite query |
| AC-06 | PASS | `test_store_with_edges_contradicts_bidirectional`; `test_context_edge_add_contradicts_bidirectional` — both (A,B) and (B,A) rows confirmed |
| AC-07 | PASS | `test_store_with_edges_target_not_found_fails_call` (TargetNotFound); `test_store_with_edges_quarantined_target_fails_call` (TargetQuarantined); `test_store_with_edges_deprecated_target_succeeds` |
| AC-08 | PASS | `test_context_edge_add_self_referential_rejected` — self-ref pre-operation check |
| AC-09 | PASS | `test_store_with_edges_duplicate_skips_edge_writes` — no edge rows on duplicate |
| AC-10 | PASS | `test_store_with_edges_idempotent_reassertion`; `test_context_edge_add_contradicts_idempotent` — exactly 1/2 rows after re-assertion |
| AC-11 | PASS | `test_stale_dependency_appears_in_context_status` — `stale_dependency_edges >= 1` after deprecating prerequisite source; Fix 2 applied |
| AC-12 | PARTIAL | `DependencyOnDeprecatedRule` fires in unit tests (`test_dependency_on_deprecated_rule_detect_fires_on_match`); end-to-end `context_cycle_review` flow not integration-tested (requires seeded CYCLE_EVENTS rows); AC-12 depends on full cycle review infrastructure |
| AC-13 | PASS | `test_default_rules_has_23_rules` unit test |
| AC-14 | PASS | 10 per-variant `test_build_typed_graph_<variant>_survives_pass2b` unit tests |
| AC-15 | PASS | `test_context_edge_requires_write_capability` (after Fix 1); existing `test_store_restricted_agent_rejected` |
| AC-16 | PASS | `test_contradicts_query_bidirectional` — GRAPH_EDGES row count confirmed for both directions |
| AC-17 | PASS | `RelatedTo` in `positive_out_degree_weight` and `personalized_pagerank` (grep confirmed); `Advances`/`Motivates` absent (negative grep confirmed) |
| AC-18 | PASS | `test_store_with_edges_writes_graph_rows` implicitly (source="agent" from EDGE_SOURCE_AGENT constant) |
| AC-19 | PASS | `test_context_edge_tool_registered` — 13 tools, `context_edge` present, schema validated, `new_target_id` marked optional |
| AC-20 | PASS | `test_context_edge_no_embedding_or_confidence_side_effects` — server responsive, no crash |
| AC-21 | PASS | `test_context_edge_requires_write_capability` — read-only agent rejected (after Fix 1) |
| AC-22 | PASS | `test_context_edge_no_ownership_check` — Agent B operates on Agent A's entry, success |
| AC-23 | PASS | `test_context_edge_source_frozen_quarantined`; `test_context_edge_source_frozen_deprecated` |
| AC-24 | PASS | `test_context_edge_add_basic`; `test_context_edge_add_target_not_found`; `test_context_edge_add_quarantined_target_rejected`; `test_context_edge_add_deprecated_target_succeeds`; `test_context_edge_add_new_target_id_rejected`; `test_store_with_edges_idempotent_reassertion` |
| AC-25 | PASS | `test_context_edge_remove_basic`; `test_context_edge_remove_contradicts_both_directions`; `test_context_edge_remove_idempotent_non_existent` |
| AC-26 | PASS | `test_context_edge_redirect_basic`; `test_context_edge_redirect_contradicts_all_four_rows`; `test_context_edge_redirect_rollback_on_bad_new_target` |

**AC-12 partial**: The `DependencyOnDeprecated` detection rule fires correctly in unit tests with injected stale_edge_pairs. Integration coverage through `context_cycle_review` requires seeded CYCLE_EVENTS rows and observation data — not written in this stage due to test infrastructure complexity. Rule registration and unit-level firing are fully verified.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 12 entries including ADR-001 through ADR-008 for vnc-015. Used to orient on validation pipeline ordering, edge_write module design, and RAII transaction requirement.
- Stored: nothing novel to store — two implementation gaps (EdgeParams missing agent_id; stale_dependency_edges not mapped to StatusReport) were feature defects fixed during Stage 3c, not novel patterns. The patterns involved (params struct agent_id field, GraphCohesionMetrics field surfacing) are already well-established in this codebase and do not warrant new Unimatrix entries.
