# Gate 3a Report: crt-014

> Gate: 3a (Component Design Review)
> Date: 2026-03-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All component boundaries, interfaces, and technology choices match ARCHITECTURE.md |
| Specification coverage | PASS | All 10 FRs and 7 NFRs traced to pseudocode; no scope additions |
| Risk coverage | PASS | All 13 risks (R-01 through R-13) and 4 integration risks mapped to test scenarios |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; no contradictions |
| Knowledge stewardship | PASS (WARN x2) | Both agent reports have stewardship blocks; evidence of query; reasons given for not storing |

---

## Detailed Findings

### 1. Architecture Alignment

**Status**: PASS

**Evidence**:

The component decomposition in the pseudocode exactly mirrors ARCHITECTURE.md:

- `graph.rs` (NEW): `SupersessionGraph`, `GraphError`, and the three public functions (`build_supersession_graph`, `graph_penalty`, `find_terminal_active`) are all defined in `pseudocode/graph.md` with signatures matching the ARCHITECTURE.md Integration Surface table verbatim.
- `lib.rs` (one-line `pub mod graph;` addition): documented in `pseudocode/OVERVIEW.md`.
- `Cargo.toml` (petgraph + thiserror): documented in `pseudocode/OVERVIEW.md` with the exact feature string `features = ["stable_graph"]` matching FR-01 / ADR-001.
- `search.rs` (Step 6a/6b replacement): `pseudocode/search.md` documents all four changes (import swap, graph construction spawn_blocking block, penalty map replacement, multi-hop injection replacement) matching ARCHITECTURE.md Component 4 exactly.
- `confidence.rs` (removals only): `pseudocode/confidence.md` identifies the exact constants and four tests to remove, matching ARCHITECTURE.md Component 5.

ADR decisions are correctly propagated:
- ADR-001 (stable_graph only): enforced in Cargo.toml pseudocode.
- ADR-002 (per-query rebuild): enforced via `spawn_blocking` placement in `search.md`.
- ADR-003 (supersedes old ADR-003 single-hop limit): multi-hop DFS in `find_terminal_active` implements this.
- ADR-004 (supersedes old ADR-005 hardcoded penalties): `graph_penalty` replaces all constant usage.
- ADR-005 (cycle fallback): `use_fallback = true` with `FALLBACK_PENALTY` and single-hop fallback documented in `search.md`.
- ADR-006 (named constants): all seven `pub const` values declared at module top in `graph.md`.

The design revalidation researcher report (agent 3) verified that crt-018b line-number drift does not invalidate any of the pseudocode — structural placements are correct against the actual worktree.

**One implementation note surfaced** (not a gate failure): `ServiceError::Internal` variant availability must be checked during implementation (noted as Gap 1 in agent-1 report). The pseudocode documents the contingency. This is a forward flag, not a design error.

### 2. Specification Coverage

**Status**: PASS

**Evidence**:

All 10 functional requirements traced:

| FR | Coverage in Pseudocode |
|----|----------------------|
| FR-01 (petgraph dep) | `OVERVIEW.md` Cargo.toml section |
| FR-02 (graph.rs module) | `graph.md` full module; `OVERVIEW.md` lib.rs section |
| FR-03 (build_supersession_graph) | `graph.md` Algorithm: two-pass (nodes then edges), dangling ref warn+skip, cycle detection |
| FR-04 (graph_penalty) | `graph.md` Algorithm: 6-rule priority derivation with dfs_active_reachable + bfs_chain_depth helpers |
| FR-05 (find_terminal_active) | `graph.md` Algorithm: iterative DFS, depth cap, visited set |
| FR-06 (search Step 6a) | `search.md` Change 3: unified OR condition, graph_penalty vs FALLBACK_PENALTY branch |
| FR-07 (search Step 6b multi-hop) | `search.md` Change 4: find_terminal_active vs single-hop fallback |
| FR-08 (cycle fallback) | `search.md` Change 2: tracing::error!, use_fallback, single-hop fallback |
| FR-09 (constant removal) | `confidence.md`: exact lines identified, removal documented |
| FR-10 (penalty constants in graph.rs) | `graph.md` Constants section: all 7 pub const with values |

All 7 NFRs traced:

| NFR | Coverage |
|-----|---------|
| NFR-01 (≤5ms at 1,000 entries) | `test-plan/graph.md` NFR checks: benchmark test specified |
| NFR-02 (graph_penalty purity) | `graph.md` Notes: "pure function: no I/O, no side effects, deterministic" |
| NFR-03 (depth cap) | `graph.md` find_terminal_active algorithm with explicit depth >= MAX_TRAVERSAL_DEPTH guard |
| NFR-04 (no unsafe) | workspace-level; acknowledged in pseudocode notes |
| NFR-05 (no async in graph.rs) | `graph.md`: "This module has no async functions" |
| NFR-06 (no schema changes) | not mentioned in pseudocode (correctly, since no action needed) |
| NFR-07 (cumulative test infrastructure) | `test-plan/OVERVIEW.md` philosophy; `test-plan/graph.md` helper uses existing pattern |

**No scope additions detected.** The pseudocode does not introduce any feature beyond what SPECIFICATION.md defines. The "NOT In Scope" items (co-access graph, DOT export, graph caching, context_status cycle surface) are all absent from the pseudocode.

**One self-correction noted and handled**: `graph.md` identifies an off-by-one in `find_terminal_active` depth boundary at the bottom of the Notes section and provides the corrected pseudocode inline. This is exemplary — the agent caught the issue before implementation, documented both the wrong and correct logic, and called for unit tests (AC-11, R-07) to verify the exact boundary. This is not a deficiency.

### 3. Risk Coverage

**Status**: PASS

**Evidence**:

All 13 risks from RISK-TEST-STRATEGY.md have corresponding test scenarios. Tracing the five Critical risks:

**R-01 (graph_penalty priority rule ordering)**: `test-plan/graph.md` specifies five distinct test functions, one per priority branch (`penalty_orphan`, `penalty_dead_end`, `penalty_partial_supersession`, `penalty_clean_replacement_depth1`, `penalty_clean_replacement_depth2`). The RISK-TEST-STRATEGY requirement for "no branch shares a test" is met.

**R-03 (find_terminal_active wrong node)**: `test-plan/graph.md` specifies five scenarios including the critical R-03 scenario 4 (`terminal_skips_superseded_active`: C is Active but superseded_by set, D is terminal — correctly uses both `Status::Active` AND `superseded_by.is_none()` as the terminal condition).

**R-04 (edge direction reversed)**: `test-plan/graph.md` specifies `fn edge_direction_pred_to_successor` which explicitly inspects `graph.inner.edges_directed(a_index, Outgoing)` to verify the edge points A→B when B.supersedes = Some(A.id). This directly tests the structure-level assertion required by RISK-TEST-STRATEGY.

**R-05 (test migration window)**: `test-plan/confidence.md` documents the atomic commit requirement explicitly, lists the 4 removed tests, maps each to its behavioral replacement in `graph.rs`, and even handles the `penalties_independent_of_confidence_formula` edge case (it can be renamed and retained because its body does not reference the removed constants). The mapping table shows net-zero or better coverage for all four removed tests.

**R-06 (wrong successor after multi-hop upgrade)**: `test-plan/search.md` specifies `test_search_multihop_injects_terminal_active` as an infra-001 lifecycle integration test using `context_correct` chains to build real A→B→C data. The test asserts `id_c in result_ids`. A single-hop regression test is also specified.

High-priority risks R-02 (cycle shapes), R-07 (depth boundary), R-08 (fallback scope), R-09 (dangling ref), R-10 (spawn_blocking placement — code review), R-11 (dead import — grep), R-12 (decay formula clamp) all have explicit test scenarios.

Integration risks IR-01 through IR-04 are each addressed: IR-01 (QueryFilter::default() includes all statuses) via unit check and code review; IR-02 (unified OR guard) via `fn unified_penalty_guard_covers_superseded_active_entry`; IR-03 (no unnecessary penalty calls) via code review; IR-04 (thiserror availability) flagged by revalidation researcher and documented as build-time check.

**AC-16 gap acknowledged and resolved**: The test plan correctly identifies that a supersession cycle cannot be injected through the MCP interface. The plan designates AC-16 verification as a `search.rs` unit test using raw `EntryRecord` values — a valid and complete alternative. The `fn cycle_fallback_uses_fallback_penalty` unit test in `search.md` covers this.

### 4. Interface Consistency

**Status**: PASS

**Evidence**:

The shared types in `pseudocode/OVERVIEW.md` are used consistently across all component files:

| Type/Constant | OVERVIEW.md | graph.md | search.md | confidence.md |
|---------------|-------------|----------|-----------|---------------|
| `SupersessionGraph` | defined | defined (verbatim) | consumed (graph_opt) | not referenced |
| `GraphError::CycleDetected` | defined | defined | consumed (match arm) | not referenced |
| `FALLBACK_PENALTY: f64 = 0.70` | defined | defined | imported + used | removed |
| `ORPHAN_PENALTY: f64 = 0.75` | defined | defined | not imported (graph_penalty returns it internally) | removed |
| `DEPRECATED_PENALTY` (REMOVED) | listed as removed | not present | removed from import | lines identified for removal |
| `SUPERSEDED_PENALTY` (REMOVED) | listed as removed | not present | removed from import | lines identified for removal |

The data flow documented in `OVERVIEW.md` ("all_entries slice loaded once before Step 6a via Store::query, passed to both graph_penalty and find_terminal_active, read-only throughout") is consistent with:
- `search.md` Change 2: single `spawn_blocking` call produces `(all_entries, graph_result)`
- `search.md` Change 3: `&all_entries` passed to `graph_penalty`
- `search.md` Change 4: `&all_entries` passed to `find_terminal_active`

No contradictions between component pseudocode files were found. The sequencing constraints in `OVERVIEW.md` (graph.rs first, lib.rs before search.rs, atomic commit for confidence.rs) are consistent with what all other component files require.

The `pseudocode/search.md` correctly preserves crt-018b additions (EffectivenessStateHandle, utility_delta, generation cache, lock ordering) in the "What Is NOT Changed" section, avoiding any conflict with the concurrent feature on this branch.

**One minor note** (not a failure): `pseudocode/search.md` Change 2 uses `unimatrix_core::Store::query(...)` notation but also acknowledges in Implementation Notes that the actual call site may be `store_for_graph.query(QueryFilter::default())`. This is a known implementation-time resolution documented appropriately.

### 5. Knowledge Stewardship Compliance

**Status**: PASS (WARN x2)

**Evidence**:

Both producing agents include `## Knowledge Stewardship` sections.

**crt-014-agent-1-pseudocode** (pseudocode agent — read-only role):
- `Queried:` entry present: "/uni-query-patterns for unimatrix-engine graph penalty supersession -- no results returned"
- No `Stored:` required for read-only agents. Agent correctly notes deviations found (none) and identifies the spawn_blocking pattern as matching existing usage.
- WARN: The query method reference is inconsistent — the agent says "/uni-query-patterns ... (skill loaded but no MCP query results in output, likely no prior patterns stored)". The skill was invoked but MCP returned no results, which is a valid outcome. This is acceptable — the agent demonstrated the query attempt.

**crt-014-agent-2-testplan** (test plan agent — read-only role):
- `Queried:` entry present: "/uni-knowledge-search for testing procedures"
- `Stored:` entry present: "nothing novel to store -- findings are feature-specific... not a generalizable pattern"
- WARN: Agent-2 used `/uni-knowledge-search` instead of `/uni-query-patterns`. The tool name differs from the protocol-specified skill. This is an environment-level inconsistency (tool unavailable in that spawn context), not a policy violation. The intent was present and documented.

**crt-014-design-revalidation-researcher** (additional design-phase agent):
- `## Knowledge Stewardship` section present.
- `Queried:` entry: "/uni-query-patterns for crt-018b search.rs modifications"
- `Stored:` entry: "nothing novel to store -- findings are feature-specific line-number drift from crt-018b insertion"
- Both entries include reasons. Compliant.

No missing stewardship blocks. No entries with bare "nothing novel to store" without reason.

---

## Rework Required

None.

---

## Gate Warnings (non-blocking)

| Warning | Detail |
|---------|--------|
| Agent-1 query tool invocation | `/uni-query-patterns` skill loaded but returned no MCP results; documented as "likely no prior patterns stored." Non-blocking — intent demonstrated. |
| Agent-2 query tool name | Used `/uni-knowledge-search` instead of `/uni-query-patterns`; MCP unavailable in spawn context. Intent present, documented. |
| `ServiceError::Internal` variant | Gap 1 in agent-1 report: the pseudocode uses `ServiceError::Internal(e.to_string())` but notes this variant may not exist. Implementer must check — contingency documented in pseudocode. Flag for implementation agent attention in Stage 3b spawn prompt. |
| `find_terminal_active` depth boundary off-by-one | Pseudocode self-corrects the boundary logic inline. Test coverage (AC-11 + R-07 scenarios) will catch any residual error at Stage 3b. |

---

## Knowledge Stewardship

- Stored: nothing novel to store -- this gate found no recurring failure pattern; all checks passed with only minor stewardship warnings. Feature-specific findings belong in the gate report, not in the knowledge base.
