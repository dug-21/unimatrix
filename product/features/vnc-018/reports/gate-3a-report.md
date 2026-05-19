# Gate 3a Report: vnc-018

> Gate: 3a (Design Review)
> Date: 2026-05-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All components match architecture decomposition; all 8 ADRs reflected in pseudocode |
| Specification coverage | PASS | All FR-01 through FR-14 and all AC-01 through AC-20 have corresponding pseudocode |
| Risk coverage | PASS | All 21 risks (R-01 through R-21) mapped to test scenarios in per-component test plans |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; data flow coherent |
| Critical check 1 — SQL CTE only for chain/current | PASS | find_terminal_active explicitly prohibited; query_supersession_chain and query_current_terminal both use CTE |
| Critical check 2 — validate_no_unsupported_params placement | PASS | Runs inside handle_graph AFTER require_cap in tools.rs; ordering documented correctly |
| Critical check 3 — current mode AND e.status = 'Active' | PASS | Filter present and prominently annotated as mandatory in store_queries.md and graph_read.md |
| Critical check 4 — BFS visited set HashSet<u64> by node_id only | PASS | Explicitly documented and code comment references AC-11a and R-18 |
| Critical check 5 — EdgeRecord.metadata no skip_serializing_if | PASS | Annotation "NEVER skip_serializing_if" present in OVERVIEW.md and graph_read.md |
| Critical check 6 — chain empty / current error for non-existent ID | PASS | Asymmetry correctly specified with mandatory comment requirement |
| Critical check 7 — all 7 schema cascade touch points | PASS | All 7 touch points documented in migration.md with explicit per-file change specifications |
| Critical check 8 — node_index_for on TypedRelationGraph | PASS | ADR-008 resolved; accessor specified in graph_read.md (graph.rs section) |
| Critical check 9 — Test plans cover AC-01 through AC-20 | PASS | All 20 ACs mapped in test-plan/OVERVIEW.md risk-to-test table |
| Critical check 10 — Test plan OVERVIEW.md includes integration harness plan | PASS | Full harness plan present with suite selection, fixture plan, and Python test specs |
| Critical check 11 — AC-04/AC-05a matched pair with asymmetry note | PASS | Both matched-pair tests specified with mandatory comment requirements in graph_read test plan |
| Knowledge stewardship compliance | WARN | Architect report missing ADR-008 Unimatrix entry ID; risk agent report has minor risk count discrepancy |

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**: The pseudocode OVERVIEW.md lists all 7 modified/created files matching the architecture's Component Breakdown exactly: `mcp/graph_read.rs` (new), `mcp/tools.rs` (add handler), `unimatrix-store/src/db.rs` (new query functions + index DDL), `graph_ppr.rs` (Advances/Motivates), `graph_expand.rs` (Advances/Motivates), `graph.rs` (node_index_for), `migration.rs` (v26→v27).

The data flow diagram in OVERVIEW.md matches the architecture's Component Interactions section precisely: require_cap in tools.rs → handle_graph → validate_no_unsupported_params → mode dispatch. Technology choices align: SQL recursive CTEs (ADR-001), per-direction Truncated struct (ADR-002), centralized validation (ADR-003), EdgeRecord in graph_read.rs (ADR-004), depth=1 SQL / depth>1 BFS split (ADR-005), Advances/Motivates PPR additions (ADR-006), v26→v27 index migration (ADR-007), node_index_for accessor (ADR-008).

Module boundary is maintained: tools.rs contains only the dispatch ceremony; all mode logic is in graph_read.rs.

### Specification Coverage

**Status**: PASS

**Evidence**:

- FR-01 (tool registration): tools_dispatch.md specifies the `#[tool]` method on McpServerImpl with dispatch-only body.
- FR-02 (capability gate): `require_cap(Capability::Read)` in tools_dispatch.md Step 2, explicitly before handle_graph.
- FR-03 (mode dispatch): handle_graph in graph_read.md dispatches on mode string; validate_no_unsupported_params's `_` arm returns the correct error listing supported modes.
- FR-04 (chain mode CTE): graph_read.md handle_chain calls query_supersession_chain; store_queries.md contains the exact CTE from ARCHITECTURE.md with depth_cap=50.
- FR-05 (current mode CTE + AND status='Active'): graph_read.md handle_current calls query_current_terminal; store_queries.md contains the CTE with explicit `AND e.status = 'Active'` and Critical annotation.
- FR-06 (neighbors mode): handle_neighbors in graph_read.md dispatches to handle_neighbors_sql (depth=1) or handle_neighbors_bfs (depth>1); BFS uses `HashSet<u64>` visited set.
- FR-07 (edge_types validation): Step 3 in handle_neighbors validates each type via RelationType::from_str, rejects Supersedes explicitly, handles empty list via all_non_supersedes().
- FR-08 (resolve_supersessions): Handled in BFS path; rejected on chain mode in validate_no_unsupported_params.
- FR-09 (forward-compat field validation): validate_no_unsupported_params covers all four fields (seed_ids, from_id, to_id, max_nodes) with mode-specific error messages.
- FR-10 (schema migration): migration.md covers all 4 indexes; store_queries.md DDL section covers create_tables_if_needed additions.
- FR-11 (ChainResponse truncation): Truncated struct defined with forward+backward bools; ChainResult wraps it.
- FR-12 (Advances/Motivates PPR/BFS): ppr_bfs.md covers both graph_ppr.rs insertion points and graph_expand.rs insertion point.
- FR-13 (tool description staleness text): tools_dispatch.md includes the exact required text from ADR-005 in the `#[tool(description = "...")]` pseudocode.
- FR-14 (test_protocol.py P-03 update): tools_dispatch.md test plan references AC-16 update; test-plan/tools_dispatch.md specifies the exact Python test modification.

All NFRs addressed: NFR-01/02 (latency via indexes), NFR-03 (BFS no hard SLA, staleness documented), NFR-04 (50-hop cap at SQL CTE level for chain/current), NFR-05 (graph_read.rs 500-line limit noted with split guidance), NFR-06 (visited set bounds BFS memory), NFR-07 (EdgeRecord.metadata always None, no skip_serializing_if).

### Risk Coverage

**Status**: PASS

**Evidence**: test-plan/OVERVIEW.md provides a complete risk-to-test mapping table for all R-01 through R-21. All Critical risks have multiple test scenarios:

- R-01 (in-memory path used instead of CTE): cold-start test in store_queries test plan; unit test on query_supersession_chain with empty DB.
- R-02 (flat bool instead of Truncated struct): AC-03b wire format JSON inspection test in graph_read test plan.
- R-03 (depth staleness behavioral asymmetry): depth=1 sees edge, depth=2 does NOT (xfail with comment) — in harness plan.
- R-04 (validate ordering — unrecognized mode fires before field checks): `test_validate_unrecognized_mode_fires_before_field_check` in graph_read test plan.
- R-05 (schema cascade 7 touch points): migration.md test plan covers all 7; grep gate check specified.
- R-06 (two Supersedes paths untested): AC-15a + AC-10/AC-10a both mapped, raw JSON inspection specified.
- R-07 (node_index visibility — resolved): node_index_for unit tests in ppr_bfs test plan.
- R-08 (resolve_supersessions not rejected on chain): AC-15c in graph_read test plan unit tests validate_no_unsupported_params directly.
- R-09 (PPR normalization shift): AC-17 + AC-18 in ppr_bfs test plan.
- R-10 (follow_to_current None silent fallback): orphaned + 50-hop tests in graph_read test plan.
- R-11 (depth range not validated): depth=0 and depth=11 error tests in graph_read test plan.
- R-12 (neighbors non-existent ID — OQ-01 resolved): empty result confirmed; test specified.
- R-13 (unqualified module path in tools.rs): code review check + AC-20 runtime proof.
- R-14 (P-03 not updated): mandatory test update in tools_dispatch test plan.
- R-15 (EdgeRecord.metadata wire format): raw JSON inspection test in graph_read test plan.
- R-16 (forward-compat fields silently dropped): AC-15b four unit tests.
- R-17 (direction param not validated on neighbors): direction="forward" rejection test.
- R-18 (BFS visited set keyed on (node_id, depth)): AC-11a diamond-graph dedup test.
- R-19 (vnc-017 not merged — pre-delivery gate): pre-delivery gate check noted.
- R-20 (AND e.status='Active' filter missing): AC-06b orphaned-deprecated test; noted as "only guard."
- R-21 (current/chain asymmetry): AC-04/AC-05a matched pair with mandatory comment.

Priority split in the Risk-Test Strategy lists Critical as 8 risks (R-01, R-02, R-03, R-04, R-05, R-06, R-19, R-20), which matches the risk register. The test-plan OVERVIEW.md risk-to-test mapping table lists R-20 as Critical, consistent with the Risk Test Strategy's "Coverage Summary" (8 Critical risks).

### Interface Consistency

**Status**: PASS

**Evidence**:

OVERVIEW.md defines the canonical types: `GraphParams`, `EdgeRecord`, `Truncated`, `ChainResult`, `CurrentResponse`, `NeighborsResponse`, `ChainDirection`, `NeighborDirection`, `ChainQueryResult`, `RawEdgeRow`. These are used consistently across all pseudocode files:

- graph_read.md: types defined as structs with exact matching field names and types.
- store_queries.md: ChainDirection/NeighborDirection/ChainQueryResult/RawEdgeRow match OVERVIEW exactly.
- tools_dispatch.md: references `crate::mcp::graph_read::GraphParams` and `graph_read::handle_graph`.
- ppr_bfs.md: no shared types consumed; self-contained insertions.
- migration.md: no shared types.

Data flow is coherent: tools.rs passes `&self.store`, `&typed_graph_state`, `params`, `&ctx` to handle_graph; handle_graph passes `store.read_pool()` to db.rs functions and `typed_graph_state` to BFS. The `Arc<RwLock<TypedGraphState>>` flow through handle_graph → handle_neighbors_bfs is consistent.

One minor internal note: graph_read.md introduces `query_current_terminal` as a separate function (not originally listed in ARCHITECTURE.md's integration surface, which only lists `query_supersession_chain`). The pseudocode agent's OQ-1 flag acknowledges this deviation and provides rationale (the current mode CTE has fundamentally different semantics). The architecture integration surface table lists two query functions from db.rs; the pseudocode introduces a third (`query_current_terminal`). This is additive and architecturally sound — the architecture document specifies `query_supersession_chain` is used by chain mode; the addition of `query_current_terminal` for current mode is a clarification that improves correctness (the `AND e.status = 'Active'` filter cannot be cleanly bolted onto `query_supersession_chain`). This deviation is explicitly acknowledged and does not contradict any ADR.

### Critical Check 1 — chain/current Use SQL CTE Only (ADR-001)

**Status**: PASS

**Evidence**:

- graph_read.md handle_chain body: `query_supersession_chain(store.read_pool(), id, direction, 50)`. No in-memory graph reference.
- graph_read.md handle_current body: `query_current_terminal(store.read_pool(), id)`. No in-memory graph reference.
- ARCHITECTURE.md integration points table: `find_terminal_active — NOT used by chain/current`. This matches.
- store_queries.md documents both CTEs matching the exact SQL from ARCHITECTURE.md §chain mode and §current mode.
- The `query_current_terminal` CTE includes `AND e.status = 'Active'` as MANDATORY (ADR-001 requirement for current mode).
- No pseudocode file references `find_terminal_active` in any execution path for chain or current modes.

### Critical Check 2 — validate_no_unsupported_params Ordering (ADR-003)

**Status**: PASS

**Evidence**:

- tools_dispatch.md explicitly documents the ordering in the Validation Ordering section: Step 2 `require_cap(Read)` runs BEFORE Step 4 `graph_read::handle_graph(...)`.
- graph_read.md handle_graph body: Step 1 is `validate_no_unsupported_params(&params)` — the first action inside handle_graph.
- ADR-003 specifies `validate_no_unsupported_params` is "called as the first action in `handle_graph`, before capability check and before mode dispatch." However the architecture's Component Interactions section clarifies this means before mode dispatch but AFTER the tools.rs capability check. The pseudocode correctly implements this.
- Note: ADR-003 text says "called as the first action in `handle_graph`, before capability check and before mode dispatch" which appears contradictory (capability check is in tools.rs, not handle_graph). The pseudocode correctly resolves this by placing require_cap in tools.rs Step 2, and validate_no_unsupported_params as Step 1 inside handle_graph — i.e., capability check (in tools.rs) runs before handle_graph is entered, and validate_no_unsupported_params (inside handle_graph) runs before mode dispatch. This ordering matches ARCHITECTURE.md §Component Interactions.

### Critical Check 3 — current Mode AND e.status = 'Active' (R-20)

**Status**: PASS

**Evidence**:

- store_queries.md `query_current_terminal` CTE (lines 229-278) contains `WHERE e.superseded_by IS NULL AND e.status = 'Active'` in the final SELECT.
- The annotation states "**Critical**: `AND e.status = 'Active'` in the final SELECT is MANDATORY."
- graph_read.md handle_current body: "CTE MUST include AND e.status = 'Active' — without it, orphaned deprecated entries... are silently returned (R-20, Critical)."
- Test plan test_handle_current_orphaned_deprecated_returns_error includes comment: "This is the only test that catches an accidentally omitted `AND e.status = 'Active'` filter in the CTE."

### Critical Check 4 — BFS Visited Set HashSet<u64> by node_id Only (AC-11a)

**Status**: PASS

**Evidence**:

- OVERVIEW.md: "visited: HashSet<u64> (node_id only)"
- graph_read.md handle_neighbors_bfs body: `let mut visited: HashSet<u64> = HashSet::new()` with comment "(keyed by node_id ONLY (AC-11a, R-18))"
- The BFS visited set invariant section explicitly states: "Do NOT key on `(node_id, depth)` — that produces duplicates (AC-11a, R-18)."
- Test plan graph_read.md: `test_handle_neighbors_bfs_deduplicates_by_node_id` with comment explaining the invariant.

### Critical Check 5 — EdgeRecord.metadata No skip_serializing_if (ADR-004)

**Status**: PASS

**Evidence**:

- OVERVIEW.md EdgeRecord definition: `metadata: Option<serde_json::Value>, // always None in vnc-018; NEVER skip_serializing_if`
- graph_read.md EdgeRecord struct definition: `metadata: Option<serde_json::Value>,   // always None; NO skip_serializing_if (ADR-004, R-15)`
- Test plan graph_read.md: `test_edge_record_metadata_serializes_as_null` with comment "skip_serializing_if = 'Option::is_none' is prohibited on this field. (ADR-004)"

### Critical Check 6 — chain Empty / current Error for Non-Existent ID (R-21)

**Status**: PASS

**Evidence**:

- graph_read.md handle_chain: "Non-existent ID → CTE returns zero rows → `entries: vec![]`, no error (AC-04)."
- graph_read.md handle_current: "Non-existent ID → error, NOT empty (AC-05a). This is intentionally asymmetric with chain mode (AC-04 returns empty). A comment in the code MUST state this asymmetry is intentional (R-21)."
- Test plan graph_read.md: `test_handle_chain_nonexistent_id_returns_empty` with comment "chain mode returns empty for non-existent ID — intentionally asymmetric with current mode which returns error. See R-21 and AC-04. Do not unify."
- Test plan graph_read.md: `test_handle_current_nonexistent_id_returns_error` with comment "Intentionally asymmetric with chain mode (returns empty for same ID)."
- Integration harness plan specifies both tests as a matched pair: `test_graph_chain_nonexistent_id_returns_empty` and `test_graph_current_nonexistent_id_returns_error`.

### Critical Check 7 — All 7 Schema Cascade Touch Points (ADR-007)

**Status**: PASS

**Evidence**: migration.md documents all 7 touch points explicitly in the "Modified Files" table:

1. `migration.rs` — CURRENT_SCHEMA_VERSION = 27 + v26→v27 block
2. `db.rs` — 4 index DDL in create_tables_if_needed + literal → 27
3. `sqlite_parity.rs` — version test → 27 + 4 index assertions
4. `server.rs` — all assert_eq!(version, 26) → 27
5. `migration_v25_to_v26.rs` — == 26 → >= 26
6. `migration_v26_to_v27.rs` (new) — asserts all 4 index names
7. `db.rs` test — expected version → 27

Each touch point has a corresponding test specification in migration.md. The mandatory grep gate check (`grep -r 'schema_version.*== 26' crates/`) is documented in both migration.md and store_queries test plan.

The ADR-007 "Schema cascade checklist" in ARCHITECTURE.md lists the same 7 items — they match exactly.

### Critical Check 8 — node_index_for Accessor on TypedRelationGraph (ADR-008)

**Status**: PASS

**Evidence**:

- graph_read.md covers the node_index_for accessor in the first section (graph.rs accessor, ADR-008): `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` with exact implementation pseudocode.
- OVERVIEW.md lists `unimatrix-engine/src/graph.rs` as a modified component with the node_index_for accessor.
- BFS pseudocode in handle_neighbors_bfs calls `graph.node_index_for(id)` to get the start node.
- Test plan ppr_bfs.md specifies unit tests for the accessor: `test_node_index_for_known_node_returns_index` and `test_node_index_for_unknown_node_returns_none`.
- ADR-008 is referenced in architect report's ADR list — however, the architect report was filed before ADR-008 was created (ADR-008 was added later per synthesizer-b-report). The ADR file exists and is complete.

### Critical Check 9 — Test Plans Cover AC-01 through AC-20

**Status**: PASS

**Evidence**: test-plan/OVERVIEW.md contains a complete risk-to-test mapping table. Per-component test plans cover all 20 ACs:

- AC-01, AC-02, AC-03, AC-03b: graph_read test plan (handle_chain tests)
- AC-04: graph_read test plan (chain nonexistent ID)
- AC-05, AC-05a, AC-06, AC-06b, AC-07: graph_read test plan (handle_current tests)
- AC-08, AC-09, AC-10, AC-10a, AC-11, AC-11a, AC-12, AC-13: graph_read test plan (handle_neighbors tests)
- AC-14, AC-15, AC-15a, AC-15b, AC-15c: graph_read test plan (validate tests)
- AC-16: tools_dispatch test plan (P-03 update)
- AC-17, AC-18: ppr_bfs test plan
- AC-19: migration test plan (migration_v26_to_v27.rs)
- AC-20: tools_dispatch + graph_read integration tests (all three modes exercised)

### Critical Check 10 — Test Plan OVERVIEW.md Integration Harness Plan

**Status**: PASS

**Evidence**: test-plan/OVERVIEW.md has a complete "Integration Harness Plan" section specifying:
- Suite selection rationale (smoke, protocol, tools, lifecycle, edge_cases)
- Existing suite coverage per suite
- New Python integration tests with full pseudocode for each test function
- Fixture usage (server function scope)
- Non-negotiable test requirements list (6 items matching the Risk Test Strategy)
- Coverage completeness self-check

### Critical Check 11 — AC-04/AC-05a Matched Pair with Asymmetry Note

**Status**: PASS

**Evidence**:

- graph_read.md test plan: `test_handle_chain_nonexistent_id_returns_empty` has COMMENT: "chain mode returns empty for non-existent ID — intentionally asymmetric with current mode which returns error. See R-21 and AC-04. Do not unify."
- graph_read.md test plan: `test_handle_current_nonexistent_id_returns_error` has COMMENT: "Intentionally asymmetric with chain mode (returns empty for same ID). This asymmetry is correct by design — current is a lookup that must succeed or fail, not a traversal that can return empty. See R-21."
- Integration harness plan specifies both tests with required comments.
- OVERVIEW.md non-negotiable list item #6: "AC-05a / R-21 asymmetry pair — Both AC-04 (chain empty) and AC-05a (current error) on same non-existent ID, with comment stating asymmetry is intentional design."

### Knowledge Stewardship Compliance

**Status**: WARN

**Evidence**:

Active-storage agents (architect, risk strategist) — checked for Stored/Declined entries:

- `vnc-018-agent-1-architect-report.md`: Contains Unimatrix entry IDs for 7 ADRs (#4475 through #4481). However ADR-008 (node_index_for accessor) does not appear in the Unimatrix ID table — it was added in a later amendment. The architect report has no explicit `Stored:` or `Declined:` block with that label; it lists entry IDs inline in the ADR table. No `## Knowledge Stewardship` section header is present. **This constitutes a missing stewardship block for the active-storage architect agent.**

- `vnc-018-agent-3-risk-report.md`: Has `## Knowledge Stewardship` section with `Queried:` entries and `Stored: nothing novel to store` with reason provided. Compliant.

Read-only agents (pseudocode, testplan) — checked for Queried entries:

- `vnc-018-agent-1-pseudocode-report.md`: Has `## Knowledge Stewardship` section with multiple `Queried:` entries and `Stored:` (nothing novel, with reason). Compliant.
- `vnc-018-agent-2-spec-report.md`: Has `## Knowledge Stewardship` section with `Queried:` entries. Missing explicit `Stored:` or `Declined:` entry for the amendment pass agent (vnc-018-agent-2b-spec), but the spec document itself has the knowledge stewardship block. Compliant enough — the spec agent reports in the SPECIFICATION.md stewardship block.
- `vnc-018-agent-2-testplan-report.md`: Has `## Knowledge Stewardship` section with `Queried:` and `Stored: nothing novel to store` with reason. Compliant.

**Issue**: `vnc-018-agent-1-architect-report.md` lacks a `## Knowledge Stewardship` section header, which is technically non-compliant. However, the ADR entry IDs in the body demonstrate the architect did store to Unimatrix (7 ADRs stored). ADR-008 ID is absent because it was created post-architect-report by the synthesizer-b amendment pass. The spirit of stewardship was fulfilled (7 ADRs stored); the format is non-compliant. Rated WARN rather than FAIL.

**Risk agent discrepancy**: The agent report summary says "Critical: 7, High: 7" but the Risk Register has 8 Critical risks (R-01, R-02, R-03, R-04, R-05, R-06, R-19, R-20). R-07 is in the register at Low priority (downgraded after ADR-008 resolution); R-20 was added in the amendment pass. The risk test strategy document itself correctly lists all risks and maps them. This discrepancy is in the summary table of the agent report only, not in the strategy document. WARN only.

---

## No Rework Required

All 11 gate checks pass. The two WARNs are:
1. Architect agent report lacks a `## Knowledge Stewardship` section header (body demonstrates compliance via ADR ID listing).
2. Risk agent report summary count discrepancy (strategy document itself is correct).

Neither WARN blocks delivery.

## Knowledge Stewardship

- Queried: none (gate validation does not require knowledge queries — source documents and artifacts are the inputs).
- Stored: nothing novel to store — the pseudocode/test-plan quality observed here (explicit risk annotations in code comments, matched-pair test with asymmetry notes, wire format tests specified at design phase) matches patterns already established in the codebase. No cross-feature gate-failure pattern identified in this review that isn't already captured.
