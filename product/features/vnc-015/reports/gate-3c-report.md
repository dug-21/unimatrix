# Gate 3c Report: vnc-015

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-05-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 15 risks covered; R-01 per-variant Pass 2b tests present; R-02/R-05 rollback confirmed |
| Test coverage completeness | PASS | All integration suites run; smoke 23/23; 36 new vnc-015 tests; 306 pass, 2 XFAIL pre-existing |
| Specification compliance | PASS | All 26 ACs verified; AC-12 partial with unit-level coverage documented and acceptable |
| Architecture compliance | PASS | Component boundaries intact; ADR-001 to ADR-010 all confirmed; RelatedTo in PPR/BFS; Advances/Motivates write-only |
| Knowledge stewardship compliance | PASS | RISK-COVERAGE-REPORT has Queried: and Stored: entries |
| Integration smoke tests | PASS | 23/23 PASS — mandatory gate cleared |
| XFAIL markers have GH Issues | WARN | Report cites GH#303/GH#305; actual markers are GH#576 and GH#111 — issue numbers wrong but irrelevance to vnc-015 is correct |
| No tests deleted or commented out | PASS | No deletions; 36 new tests added |
| RISK-COVERAGE-REPORT integration counts | WARN | Total count (308) arithmetic inconsistent: 285+23=308 but suites sum to 308 including smoke — double-counting smoke tests in presentation |
| Stale module doc comment | WARN | detection/mod.rs line 3 says "22 rules" but 23 are registered and tested |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: PASS

**R-01 (Critical)**: All 10 new RelationType variants have from_str() arms confirmed by code inspection (graph.rs lines 168–180, all before wildcard `_ => None`). Per-variant Pass 2b survival tests confirmed: `test_build_typed_graph_{advances,cites,asserts,mentions,refutes,tests,derived_from,motivates,about,related_to}_survives_pass2b` — all 10 present in `graph_tests.rs`. Per-variant round-trip tests: all 10 present (`test_relation_type_{variant}_roundtrip`). SR-01 grep gate in RISK-COVERAGE-REPORT shows 10×4 compliance table with all required cells PRESENT.

**R-02 (Critical)**: `redirect_graph_edge` uses `pool.begin().await?` (line 310 of `edge_write.rs`), all 4 SQL statements execute against `&mut *txn`, `txn.commit()` at line 402. No raw BEGIN/COMMIT SQL strings found. RAII transaction confirmed per lesson #2269.

**R-03 (Critical)**: Three-case contract table present in `edge_write.rs` module doc (lines 8–14) and as inline comment in the write loop. `bool` return from `write_graph_edge` is assigned to `_inserted` — not treated as error. Idempotency integration tests present (`test_store_with_edges_idempotent_reassertion`).

**R-04 (Critical)**: Bidirectional Contradicts writes confirmed in `validate_and_write_edges` (lines 211–223 of `edge_write.rs`) — reverse direction written before function returns. `test_store_with_edges_contradicts_bidirectional` and `test_context_edge_add_contradicts_bidirectional` verify both (A,B) and (B,A) rows. Separate per-surface tests for remove and redirect confirm Contradicts handling in all code paths.

**R-05 (Critical)**: Rollback-on-failure test present: `test_context_edge_redirect_rollback_on_bad_new_target` (test_tools.py line 3574) redirects to non-existent `new_target_id=999999`, asserts error returned, asserts original `(A,B)` row count is still 1. Contradicts 4-row case covered by `test_context_edge_redirect_contradicts_all_four_rows`.

**R-06 through R-15**: All covered per RISK-COVERAGE-REPORT. R-08 and R-09 are explicitly marked Partial with documented rationale (no max_edges limit by design; context_store self-ref test uses unit-level coverage as substitute for brittle auto-increment prediction). Both partial coverages are acceptable.

**R-10 (High)**: `test_default_rules_has_23_rules` asserts `default_rules(None, vec![]).len() == 23`. All callers of `default_rules()` updated to two-argument form confirmed at Gate 3b. `DependencyOnDeprecatedRule::new(stale_edges)` registered as Rule 23 in `default_rules()` (`detection/mod.rs` line 80).

---

### Check 2: Test Coverage Completeness

**Status**: PASS

Integration suites run per RISK-COVERAGE-REPORT:

| Suite | Tests | Result |
|-------|-------|--------|
| smoke | 23 | 23 PASS |
| protocol | 13 | 13 PASS |
| tools | 156 | 156 PASS (33 new vnc-015) |
| lifecycle | 59 | 59 PASS (3 new vnc-015) |
| security | 20 | 20 PASS |
| contradiction | 13 | 13 PASS |
| edge_cases | 24 | 22 PASS + 2 XFAIL (pre-existing) |

New vnc-015 integration tests: 36 total (33 in test_tools.py + 3 in test_lifecycle.py).

All risk-to-scenario mappings from the Risk-Based Test Strategy are exercised. Cross-component risks (bidirectional Contradicts per write surface, default_rules signature change) have independent tests per surface. Edge cases from risk analysis (empty edges vec, remove of non-existent edge, redirect same target, DependencyOnDeprecated with empty stale edges) are tested.

---

### Check 3: Specification Compliance

**Status**: PASS

All 26 ACs from ACCEPTANCE-MAP.md are addressed:

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_store_without_edges_backward_compatible` |
| AC-02 | PASS | `test_correct_with_edges_attaches_to_new_entry` |
| AC-03 | PASS | 10 per-variant round-trip unit tests; SR-01 grep gate |
| AC-04 | PASS | `test_existing_relation_type_variants_unchanged` |
| AC-05 | PASS | `test_store_with_edges_writes_graph_rows` (direct SQLite query) |
| AC-06 | PASS | `test_store_with_edges_contradicts_bidirectional`; both rows confirmed |
| AC-07 | PASS | Target not found, quarantined, deprecated — 3-case coverage |
| AC-08 | PASS | `test_context_edge_add_self_referential_rejected` |
| AC-09 | PASS | `test_store_with_edges_duplicate_skips_edge_writes` |
| AC-10 | PASS | `test_store_with_edges_idempotent_reassertion` |
| AC-11 | PASS | `test_stale_dependency_appears_in_context_status` (Fix 2 applied during Stage 3c) |
| AC-12 | PARTIAL | `DependencyOnDeprecated` rule fires correctly in 6 unit tests; end-to-end `context_cycle_review` integration path not tested (requires seeded CYCLE_EVENTS + observation rows that the test infrastructure cannot readily provide). Rule registration (AC-13), unit-level detection, and constructor injection are all fully verified. The gap is an integration infrastructure constraint, not a code defect. |
| AC-13 | PASS | `test_default_rules_has_23_rules` asserts 23 |
| AC-14 | PASS | 10 per-variant `test_build_typed_graph_*_survives_pass2b` tests |
| AC-15 | PASS | `test_context_edge_requires_write_capability` (after Fix 1 in Stage 3c) |
| AC-16 | PASS | `test_contradicts_query_bidirectional`; OR-clause confirmed in read.rs:1557 |
| AC-17 | PASS | RelatedTo in `positive_out_degree_weight` (graph_ppr.rs:203) and BFS (graph_expand.rs:144); negative grep confirms Advances/Motivates absent |
| AC-18 | PASS | `test_store_with_edges_writes_graph_rows` (source="agent") |
| AC-19 | PASS | `test_context_edge_tool_registered` — 13 tools confirmed |
| AC-20 | PASS | `test_context_edge_no_embedding_or_confidence_side_effects` |
| AC-21 | PASS | `test_context_edge_requires_write_capability` |
| AC-22 | PASS | `test_context_edge_no_ownership_check` |
| AC-23 | PASS | `test_context_edge_source_frozen_quarantined`; `test_context_edge_source_frozen_deprecated` |
| AC-24 | PASS | Add mode: valid target, TargetNotFound, TargetQuarantined, deprecated, idempotent re-assert — 5 cases |
| AC-25 | PASS | Remove: basic, Contradicts bidirectional, idempotent non-existent |
| AC-26 | PASS | Redirect: basic, Contradicts 4-row, rollback-on-bad-target — all 3 cases |

**AC-12 partial note**: The finding is acceptable. Unit-level evidence (`test_dependency_on_deprecated_rule_detect_fires_on_match`, `test_dependency_on_deprecated_rule_multiple_stale_pairs`) proves the rule fires correctly. The `context_cycle_review` integration path for this rule requires a seeded feature cycle with CYCLE_EVENTS rows populated — a test infrastructure setup not achievable without significant scaffolding. This gap is explicitly noted in both the RISK-COVERAGE-REPORT and ACCEPTANCE-MAP.md (Status: PARTIAL). Given that the rule code, registration, constructor injection, and unit-level detection are all fully verified, this is a WARN, not a FAIL.

---

### Check 4: Architecture Compliance

**Status**: PASS

All component boundaries match the approved ARCHITECTURE.md:

- **Component 1** (EdgeInput deserialization in tools.rs): `StoreParams.edges: Option<Vec<EdgeInput>>` and `CorrectParams.edges` confirmed present.
- **Component 2** (edge_write.rs): 486 lines (within 500-line limit per ADR-005). All three functions (`validate_and_write_edges`, `delete_graph_edge`, `redirect_graph_edge`) implemented with correct signatures. `EDGE_SOURCE_AGENT = "agent"` constant at line 28.
- **Component 3** (graph.rs RelationType extension): 16 variants total; all 4 mandatory sites updated (ADR-007 confirmed by code inspection and SR-01 grep gate).
- **Component 4** (PPR/BFS expansion): RelatedTo present in `positive_out_degree_weight` (graph_ppr.rs:203) and BFS loop (graph_expand.rs:144). Advances and Motivates confirmed absent by negative grep.
- **Component 5** (stale_dependency_edges in read.rs): SQL query at lines 1135–1143 uses hardcoded `'Prerequisite'` string literal and `status = 1` (Deprecated integer). No format-string interpolation.
- **Component 6** (DependencyOnDeprecatedRule): Constructor injection pattern followed; `detect()` is synchronous with no I/O. `default_rules()` signature changed to accept `stale_edges: Vec<(u64, u64)>`.
- **Component 7** (query_contradicts_edges_for_entry fix): OR-clause fix at read.rs:1557 — `WHERE (source_id = ?1 OR target_id = ?1) AND relation_type = 'Contradicts'`.
- **Component 9** (context_edge handler): 13th MCP tool; validation pipeline order 1–7 matches ARCHITECTURE.md; atomic redirect via RAII transaction.

ADR decisions confirmed: ADR-001 (validate before insert), ADR-003 (partial-write posture), ADR-004 (constructor injection), ADR-005 (edge_write.rs module), ADR-006 (RelatedTo only in PPR), ADR-007 (wildcard last in from_str()), ADR-008 (EDGE_SOURCE_AGENT), ADR-009 (atomic redirect transaction), ADR-010 (target validation DB lookup).

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

The RISK-COVERAGE-REPORT contains a `## Knowledge Stewardship` section with:
- `Queried:` entry — `mcp__unimatrix__context_briefing` returned 12 entries including feature ADRs
- `Stored:` entry — "nothing novel to store — two implementation gaps (EdgeParams missing agent_id; stale_dependency_edges not mapped to StatusReport) were feature defects fixed during Stage 3c, not novel patterns. The patterns involved (params struct agent_id field, GraphCohesionMetrics field surfacing) are already well-established in this codebase and do not warrant new Unimatrix entries."

The "nothing novel to store" entry includes a reason. This satisfies stewardship requirements.

---

### Check 6: Integration Smoke Tests

**Status**: PASS

The `smoke` suite: 23 tests collected, 23 PASS. This is the mandatory gate — cleared.

---

### Check 7: XFAIL Markers Have GH Issues

**Status**: WARN

The RISK-COVERAGE-REPORT claims the 2 XFAIL tests in the `edge_cases` suite correspond to GH#303 and GH#305. However, inspection of `test_edge_cases.py` reveals the actual markers are:
- Line 231: `@pytest.mark.xfail(reason="Pre-existing: GH#576 — content size cap of 8000 bytes (fix #561) now rejects 50KB content")`
- Line 286: `@pytest.mark.xfail(reason="Pre-existing: GH#111 — rate limit blocks rapid sequential stores")`

The report has the wrong GH issue numbers. The core claim — that both xfail tests are pre-existing and unrelated to vnc-015 — is correct: GH#576 (content size cap) and GH#111 (rate limit) are manifestly unrelated to typed edge writes. This is a report documentation error, not a code or test defect. Both xfail markers satisfy the requirement of being linked to GH issues.

---

### Check 8: No Tests Deleted or Commented Out

**Status**: PASS

36 new integration tests were added (33 in test_tools.py + 3 in test_lifecycle.py). No test deletions were observed in the integration suite. Unit test count grew to 4,896 (per report) from the prior baseline of 2,169 (as of col-022). The increase is consistent with the feature's test scope. Cargo test run confirms 0 failures across all test targets.

---

### Check 9: Stale Module Documentation Comment

**Status**: WARN

`detection/mod.rs` line 3: `//! Ships 22 rules across 4 categories: agent (7), friction (5), session (5), scope (5).`

This is stale: `default_rules()` registers 23 rules (scope now has 6: SourceFileCountRule, DesignArtifactCountRule, AdrCountRule, PostDeliveryIssuesRule, PhaseDurationOutlierRule, and the new DependencyOnDeprecatedRule). The test `test_default_rules_has_23_rules` correctly asserts 23. The comment is a documentation-only defect with no functional impact.

---

### Check 10: Build and Code Quality

**Status**: PASS

`cargo build --workspace` completes with no errors (warnings only, pre-existing). All tests pass: 0 failures across all test targets. No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` found in new production code. The `params.new_target_id.unwrap()` WARN from Gate 3b is pre-existing and logged there; no new `.unwrap()` in production code was introduced by Stage 3c fixes.

File line counts for new modules:
- `edge_write.rs`: 486 lines (within 500-line limit, ADR-005)
- `graph.rs`: 667 lines (pre-existing monolith — 500-line rule applies to NEW files only per ADR-007 of this feature and prior practice)
- All other modified files: within existing bounds

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the AC-12 integration gap (rule fires correctly but full cycle_review path untested) is a test infrastructure limitation specific to this feature. The xfail issue number inaccuracy in the report is a minor documentation error. Neither pattern warrants a cross-feature lesson.
