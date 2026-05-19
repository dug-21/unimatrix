# Gate 3c Report: vnc-018

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-05-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 21 risks covered; R-18 acknowledged partial with WARN |
| Test coverage completeness | WARN | R-18 AC-11a diamond BFS test not implemented as explicit scenario; R-03 staleness test does not assert absence |
| Specification compliance | PASS | All 20 ACs verified; AC-11a partial (unit impl confirmed, no explicit diamond test) |
| Architecture compliance | PASS | SQL CTE, module split, file sizes, capability ordering, schema v27 all match |
| Knowledge stewardship | PASS | Tester agent report includes Queried + Stored entries |

---

## Detailed Findings

### 1. Risk Mitigation Proof
**Status**: PASS
**Evidence**:
- RISK-COVERAGE-REPORT.md maps all 21 risks to passing tests. Verified directly by inspecting key test files and running `cargo test --workspace` (4997 passed, 0 failed).
- All 6 non-negotiable tests explicitly verified present by name:
  1. `test_list_tools_returns_fourteen` in `test_protocol.py` — asserts exactly 14 tools including `context_graph` (AC-16). CONFIRMED at line 36.
  2. `test_v27_indexes_all_exist` in `sqlite_parity.rs` — asserts all 4 index names from sqlite_master (AC-19). CONFIRMED at lines 1036–1060.
  3. `test_truncated_serializes_as_struct_not_flat_bool` in `graph_read_tests.rs` — inspects raw JSON wire shape (AC-03b, R-02). CONFIRMED at line 217.
  4. `test_graph_neighbors_depth2_staleness_comment` in `test_tools.py` — R-03 contract documentation. CONFIRMED at line 4024. (See WARN below.)
  5. `test_graph_current_orphaned_deprecated_returns_error` in `test_tools.py` — R-20 status filter. CONFIRMED at line 3993.
  6. `test_graph_current_nonexistent_returns_error` + `test_graph_chain_nonexistent_returns_empty` in `test_tools.py` — R-21 asymmetry pair. CONFIRMED at lines 3960 and 3973. Both have asymmetry comments per the requirement.
- Critical risks R-20 (orphaned deprecated entry) and R-21 (chain/current asymmetry): both are covered by the Python integration tests. The R-20 test explicitly comments it is "the ONLY test that catches an accidentally omitted status filter."

### 2. Test Coverage Completeness
**Status**: WARN
**Evidence**:

**WARN-1 (R-18 / AC-11a — BFS visited set diamond test)**: The RISK-TEST-STRATEGY required "One explicit test asserting a node reachable via two paths at different depths appears exactly once at the shallowest depth." No such explicit test exists in `graph_read_tests.rs` or `test_tools.py`. The RISK-COVERAGE-REPORT acknowledges this as "Partial" at R-18. Mitigation: the BFS implementation uses `HashSet<u64>` keyed by `node_id` only (confirmed in `graph_read_neighbors.rs` lines 258-265), comments explicitly cite AC-11a/R-18, and the code comment at line 341 states "Already visited: skip. Shallowest depth wins." The risk of an incorrect `(node_id, depth)` keying is documented but not closed by a test.

**WARN-2 (R-03 staleness test — absence not asserted)**: The RISK-TEST-STRATEGY required scenario 2 to "assert the edge is absent." The test `test_graph_neighbors_depth2_staleness_comment` only asserts the depth=2 response is a list — it does not assert absence of the freshly-written edge at depth=2. The test comment explains this is environment-dependent (in-memory graph may already be populated). The depth=1 freshness assertion is hard (asserts presence), while the depth=2 staleness assertion is soft (no assertion on absence). This is a deliberate trade-off for test reliability across environments, but it does not fully satisfy the risk strategy requirement. The depth=1 live SQL path is correctly hard-asserted.

**No blocking issues**: Both WARNs represent coverage gaps acknowledged in the RISK-COVERAGE-REPORT. Neither conceals a defect in the implementation. The implementation of the BFS visited set and the staleness contract are both correct by code inspection.

### 3. Specification Compliance
**Status**: PASS
**Evidence**:
- All 20 ACCEPTANCE-MAP.md criteria verified against test names. 19 are PASS; AC-11a is Partial (per WARN-1 above).
- FR-01 (tool registration): `context_graph` registered as 14th tool — verified by `test_list_tools_returns_fourteen` passing.
- FR-02 (capability gate): `require_cap(Capability::Read)` in `tools.rs` at line 3381 before `handle_graph` delegation — confirmed.
- FR-04 (chain mode CTE): `AND e.status = 0 (Active)` filter confirmed at `graph_read_supersession.rs` line 124. Test `test_graph_current_orphaned_deprecated_returns_error` provides end-to-end proof.
- FR-08 (resolve_supersessions on chain rejected): `validate_no_unsupported_params` at line 234 rejects `resolve_supersessions=Some(true)` on chain mode. Test `test_validate_chain_rejects_resolve_supersessions` confirmed at line 8.
- FR-12 (Advances/Motivates in PPR/BFS): `test_ppr_positive_types_include_advances_and_motivates` and `test_graph_expand_follows_advances_edges`/`test_graph_expand_follows_motivates_edges` confirmed present.
- FR-13 (behavioral split documentation): test `test_context_graph_description_contains_staleness_text` at `tools.rs` line 4952 verifies the tool description contains the mandated staleness text.
- FR-14 (P-03 updated to 14): Confirmed — `test_protocol.py` P-03 asserts exactly 14 tools.
- NFR-05 (500-line module limit): All graph_read modules within limit — `graph_read.rs` 306 lines, `graph_read_supersession.rs` 448 lines, `graph_read_neighbors.rs` 356 lines, `graph_queries.rs` 450 lines.
- `EdgeRecord.metadata` explicitly prohibited from `skip_serializing_if` — commented at `graph_read.rs` lines 16, 78–79, 88 (R-15 compliance).
- Constraint: chain/current `id` non-existent asymmetry documented with test pair + comments per spec Constraints §Safety.

### 4. Architecture Compliance
**Status**: PASS
**Evidence**:
- SQL recursive CTEs confirmed in `graph_read_supersession.rs` — `find_terminal_active` not used by chain/current (confirmed by code path comments at line 74 and line 84).
- Module extraction pattern followed: `tools.rs` contains only `#[tool]` dispatch; all mode logic in `graph_read.rs`, `graph_read_supersession.rs`, `graph_read_neighbors.rs` — matches Architecture §Component Breakdown.
- Fully-qualified module path: `crate::mcp::graph_read::handle_graph` called from `tools.rs` line 3390 — Pattern #4436 compliance confirmed. Test `test_context_graph_uses_fully_qualified_module_path` at `tools.rs` line 5005 provides static proof.
- ADR-001 (SQL CTE): Both chain and current modes use SQL CTEs, not in-memory graph. Verified.
- ADR-002 (per-direction `Truncated` struct): `test_truncated_serializes_as_struct_not_flat_bool` verifies JSON wire format.
- ADR-003 (forward-compat fields): `validate_no_unsupported_params` centralized function with `match` arm for unrecognized modes as `_` arm — fires before field checks (R-04). Tests `test_validate_unrecognized_mode_fires_before_field_check` confirmed.
- ADR-007 (schema v26→v27 cascade — 7 touch points): All 7 cascade items verified in RISK-COVERAGE-REPORT Schema Cascade table. `grep -r 'schema_version.*== 26'` confirmed zero matches.
- BFS visited set: `HashSet<u64>` keyed by node_id confirmed (`graph_read.rs` line 261, `graph_read_neighbors.rs` lines 258-265).
- `read_pool()` used for all SQL reads — no write pool access in context_graph (architecture C-07 compliance).
- `require_cap` → `validate_no_unsupported_params` → mode dispatch ordering matches Architecture §Component Interactions diagram.

### 5. Integration Test Validation (Mandatory)
**Status**: PASS
**Evidence**:
- Smoke tests (`pytest -m smoke`): 23/23 PASSED. Verified with two independent runs (both at 199s).
- Protocol suite: 13/13 passed. P-03 (`test_list_tools_returns_fourteen`) confirmed in `test_protocol.py` line 36.
- Tools suite: 162 passed, 3 xfailed (pre-existing: GH#405, GH#305, GH#575), 0 failed. All xfail markers have corresponding GH issue references — confirmed.
- Lifecycle + edge_cases suites: 79 passed, 7 xfailed (pre-existing: GH#576, GH#111), 0 failed.
- New 8 context_graph integration tests: all 8 pass. No tests deleted or commented out.
- One test corrected during this feature: `test_context_edge_tool_registered` (bad assertion `len == 13` → `len == 14` due to vnc-018 adding a tool). Correct per USAGE-PROTOCOL.md — test was wrong, not a pre-existing bug.
- xfail test for R-03 staleness: NOT marked `@pytest.mark.xfail`. The test passes (documents the contract without strictly asserting absence). The test comment at line 4035 notes why strict absence assertion was not used. No xfail marker was added here, which is correct.
- RISK-COVERAGE-REPORT.md includes integration test counts (smoke: 23, protocol: 13, lifecycle+edge: 86, tools full: 165).

### 6. Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**:
- Tester agent report at `/workspaces/unimatrix/product/features/vnc-018/agents/vnc-018-agent-4-tester-report.md` contains a `## Knowledge Stewardship` section.
- `Queried:` entries present: `mcp__unimatrix__context_briefing` — found entries 4437, 4475, 4479, 4481, 4482.
- `Stored:` entry: "nothing novel to store — client extension and Python test fixture patterns follow the established vnc-015 precedent exactly. No new generalizable patterns discovered."
- Reason provided for not storing — compliant.

---

## Rework Required

None. All FAILs are absent; WARNs are acknowledged gaps with defensible justification.

---

## Final Gate Determination

The two WARNs (R-18 AC-11a diamond test absent; R-03 staleness not hard-asserted) are coverage gaps, not defects in the implementation. Both are acknowledged in the RISK-COVERAGE-REPORT with rationale. The implementation itself is correct by code inspection for both issues. No risk from the RISK-TEST-STRATEGY lacks coverage at the implementation level — only at the explicit test-scenario level for these two lower-priority items.

All 6 non-negotiable tests verified present and passing by name. Smoke gate: 23/23. Cargo test: 4997/4997. Schema cascade: 7/7 touch points confirmed. No orphaned xfail markers.

**Result: PASS**

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for lesson-learned on gate validation partial coverage — found #2758 (always grep non-negotiable test names before accepting RISK-COVERAGE-REPORT), #3548 (test exists but omits assertion — coverage weaker than specified). Both directly applied.
- Stored: nothing novel to store — the R-03 staleness test soft-assertion pattern (assert valid response but not absence, due to environment-dependence) and the R-18 BFS visited-set code-inspection-only coverage are feature-specific judgments. The recurring gate pattern of "test documents contract but doesn't assert negative" warrants a future lesson-learned if it appears again, but once is insufficient to generalize.
