# Gate 3a Report: vnc-020

> Gate: 3a (Component Design Review)
> Date: 2026-05-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 5 components match architecture decomposition; module boundaries, dispatch, and ADRs followed |
| Specification coverage | PASS | All 18 FRs and 32 ACs have corresponding pseudocode; no scope additions found |
| Risk coverage | PASS | All 14 risks mapped to test scenarios; 4 Critical risks each have explicit named tests |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; import paths consistent |
| Knowledge stewardship compliance | PASS | All four agent reports contain stewardship sections with Queried/Stored entries |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**: The pseudocode decomposition exactly matches the architecture:

- `graph_read.md` (Wave 1) adds the 8 `GraphParams` fields, 4 response types, 3 `#[path]` module declarations, 3 dispatch arms, and expanded `validate_no_unsupported_params` — matching the architecture Component 1 spec precisely.
- `graph_read_inverse.md` (Wave 2) implements the N-LEFT-JOIN antijoin SQL builder with `QueryBuilder`, `AND e.status = 0` guard, and `idx_graph_edges_target_type` usage — matching architecture Component 2.
- `graph_read_filter.md` (Wave 2) implements two independent correlated subqueries for `min_edge_count`/`max_edge_count`, typed-params-only SQL (no injection surface), and integer epoch arithmetic for `min_age_days` — matching architecture Component 3.
- `graph_read_path.md` (Wave 2) implements path-carrying BFS, lock-acquire-clone-release discipline, `pub(super)` imports from `graph_read_neighbors`, and visited-set keyed on resolved ID — matching architecture Component 4.
- `tools.md` (Wave 1) updates `CONTEXT_GRAPH_DESCRIPTION` only — no logic changes, matching architecture Component 5.
- ADR-001 through ADR-007 are all respected: sibling module split (ADR-001), Option<T> extension only (ADR-002), AND semantics (ADR-003), depth field reuse (ADR-004), path response format (ADR-005), resolve_supersessions in path mode (ADR-006), no raw SQL (ADR-007).

Technology choices are consistent with established patterns: `sqlx::QueryBuilder` with `push_bind` (pattern #4058), `std::sync::RwLock` with `unwrap_or_else(|e| e.into_inner())` (poison recovery), `pub(super)` import path for `follow_to_current` and `all_non_supersedes_types`.

**One minor observation** (does not block): The architecture rejection matrix shows `direction` as R (rejected) for all three new modes (inverse, filter, path), but the pseudocode validation arms for these modes do not include an explicit `direction` rejection guard. The existing `chain` and `current` arms already reject `direction`; the `path` mode description explicitly states "No `direction` parameter accepted" but the pseudocode arm does not include the rejection check. There is no dedicated AC for this, and the implementation agent will need to add it. This is a WARN — not a FAIL because no AC specifies it and the architecture notes it as a `direction` not-yet-needed scenario.

---

### Specification Coverage

**Status**: PASS

**Evidence**:

All 18 functional requirements have corresponding pseudocode:

- FR-01–04 (inverse mode): `graph_read_inverse.md` Steps 1–6 cover the antijoin, required params, AND semantics, limit/response.
- FR-05–09 (filter mode): `graph_read_filter.md` Steps 1–6 cover correlated subquery, param validation, property filters, edge-count filters, deprecated-entry exclusion.
- FR-10–15 (path mode): `graph_read_path.md` Steps 1–10 cover BFS, required params, response shapes (found/not-found), resolve_supersessions, outgoing-only direction.
- FR-16 (`validate_no_unsupported_params` expansion): `graph_read.md` §5 covers the 3 new arms and 8-field rejections on 4 existing arms.
- FR-17 (depth rejection on 5 modes): `graph_read.md` §5b explicitly adds depth rejection to chain, current, subgraph, and §5d adds depth rejection to the new inverse and filter arms.
- FR-18/FR-18a (no new tool, self-path): OVERVIEW.md confirms tool count 14; `graph_read_path.md` Step 2 handles self-path guard.

Non-functional requirements addressed:
- NFR-01/NFR-02 (query performance): Both SQL handlers use the indexed composite keys already established in schema v27 (architecture §1 and §3 both reference `idx_graph_edges_target_type` and `idx_graph_edges_source_type`).
- NFR-04 (freshness contract): `tools.md` includes verbatim staleness disclosure for path mode; inverse/filter explicitly labeled "Queries the live database — no staleness."
- NFR-05 (backward compat): All new fields are `Option<T>` per ADR-002.
- NFR-06 (no SQL injection): `graph_read_filter.md` uses `push_bind` throughout; injection surface analysis in SR-A is addressed.
- C5 (500-line limit): `graph_read.md` §Line Budget explicitly documents the risk (~467 lines with compact formatting vs. ~578 with spaced) and requires the implementation agent to enforce it. This is flagged as WARN — the pseudocode acknowledges the risk and provides the mitigation path, but the line count depends on implementation style.

No scope additions were found. All pseudocode content maps directly to specification requirements.

---

### Risk Coverage

**Status**: PASS

**Evidence — specifically addressing the four key risks from the spawn prompt:**

**R-02 (max_edge_count=0 boundary)**: The `graph_read_filter.md` Step 4f is explicit: "CRITICAL: max_edge_count=0 is valid and must work correctly (R-02). The `<= ?` binding with value 0 returns entries where COUNT(*) = 0. Do NOT special-case 0." The pseudocode uses `qb.push_bind(max_n as i64)` unconditionally — no branch for the zero case. Test plan `graph_read_filter.md` has `test_filter_max_edge_count_zero_uses_lte_binding` and AC-29 integration test with 4-entry fixture. PASS.

**R-03 (BFS visited set keyed on RESOLVED NodeIndex)**: `graph_read_path.md` Step 9 has two explicit `CRITICAL` annotations: "The visited set is keyed on the RESOLVED node ID (effective neighbor after `follow_to_current`), not the raw/deprecated ID." The pseudocode uses `visited.contains(&effective_neighbor)` where `effective_neighbor` is the post-`follow_to_current` ID. The BFS invariants table explicitly documents this. Test plan `graph_read_path.md` has `test_handle_path_bfs_visited_set_keyed_on_resolved_id` with a forked deprecated graph fixture that directly validates the double-enqueue prevention. PASS.

**R-04 (validate_no_unsupported_params — all 8 new fields rejected on non-owning modes)**: `graph_read.md` §5c and §5d provide explicit rejection blocks for all 8 fields across all 4 existing arms and the 3 new arms. The test plan OVERVIEW.md rejection matrix table documents one test per new field × one wrong mode (8 minimum). The pseudocode inverse arm explicitly rejects `edge_types` with a named-parameter hint ("use missing_edge_types instead") per AC-03a. PASS.

**R-07 (depth rejection on 5 modes)**: `graph_read.md` §5b adds depth rejection to `chain`, `current`, and `subgraph` arms. §5d's inverse and filter arms each include: "depth rejected: only neighbors and path (ADR-004, FR-17)." The test plan `graph_read.md` specifies `test_depth_rejected_on_{chain|current|subgraph|inverse|filter}_mode` — 5 distinct tests. PASS.

**AC-03a (edge_types rejected on inverse mode)**: The inverse arm in `graph_read.md` §5d includes: "edge_types is the wrong parameter for inverse — inverse uses missing_edge_types (AC-03a)" with the rejection: `"edge_types is not supported in inverse mode — use missing_edge_types instead"`. The test plan `graph_read.md` has `test_edge_types_rejected_on_inverse_mode`. PASS.

**Wave 1 stub files (compile-time correctness)**: `pseudocode/OVERVIEW.md` §Wave Structure Rationale and `graph_read.md` §1 both explicitly call out pattern #4509: "Wave 2 agents must create their `.rs` files immediately on spawn — even if initially empty — so that Wave 1 compilation succeeds." The `#[path]` declaration approach is explicitly documented with the compile constraint. PASS.

All 14 risks (R-01 through R-14) are mapped to named test scenarios with explicit test function names in the test plan OVERVIEW.md risk-to-test mapping table. Integration and edge case scenarios are present. Risk priorities are reflected in the test plan emphasis (4 Critical risks each have named unit + integration tests).

---

### Interface Consistency

**Status**: PASS

**Evidence**:

Shared types defined in `pseudocode/OVERVIEW.md` (Integration Surface table) are consistent with per-component usage:

- `InverseResponse { entries: Vec<EntryRecord>, total_returned: usize }` — defined in OVERVIEW.md and `graph_read.md` §3; imported in `graph_read_inverse.md` as `use super::{GraphParams, InverseResponse}`. Match.
- `FilterResponse` — same pattern. Match.
- `PathHop { entry_id: u64, relation_type: String }` and `PathResponse` — defined in both OVERVIEW.md and `graph_read.md` §3; imported in `graph_read_path.md` as `use super::{GraphParams, PathHop, PathResponse}`. Match.
- `follow_to_current` and `all_non_supersedes_types` — both listed as `pub(super)` from `graph_read_neighbors.rs` in OVERVIEW.md Integration Surface table; imported in `graph_read_path.md` as `use super::graph_read_neighbors::{all_non_supersedes_types, follow_to_current}`. Match.
- Function signatures match the architecture Integration Surface table:
  - `handle_inverse(store: &Store, params: &GraphParams) -> Result<InverseResponse, ErrorData>` — consistent.
  - `handle_filter(store: &Store, params: &GraphParams) -> Result<FilterResponse, ErrorData>` — consistent.
  - `handle_path(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<PathResponse, ErrorData>` — consistent.

Data flow in OVERVIEW.md matches `graph_read.md` dispatch pseudocode (§4). No contradictions found between component pseudocode files.

One minor gap: The architecture shows `edge_types` is present in the `filter` domain model table for neighbors/subgraph/filter/path, but the `filter` arm's accepted params comment in `graph_read.md` §5d does not explicitly list `direction` as accepted or rejected. As noted under Architecture Alignment, `direction` rejection is missing from the new mode arms. This is the same WARN item — not a blocking inconsistency.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

All four agent reports contain `## Knowledge Stewardship` sections:

- `vnc-020-agent-1-pseudocode-report.md`: Has `Queried:` entries (briefing, context_search ×3). Has `Deviations from established patterns: none` — explicitly accounts for no novel patterns. This satisfies the "Queried" requirement.
- `vnc-020-agent-2-spec-report.md`: Has `Queried:` entries (briefing, retrieved 3 full entries). Has `No new patterns identified for storage (specification decisions are feature-specific)` — explicit reason. PASS.
- `vnc-020-agent-2-testplan-report.md`: Has `Queried:` entries (briefing, context_search ×2). Has `Stored: entry #4517` (pattern for integration test tick-window dependency). PASS.
- `vnc-020-agent-3-risk-report.md`: Has `Queried:` entries (4 knowledge-search queries). Has `Stored: nothing novel to store — R-03 pattern already entry #4494; R-09 pattern already entry #4497` — explicit reason with entry IDs. PASS.

No missing stewardship blocks.

---

## Warnings (non-blocking)

| Warning | Location | Notes |
|---------|----------|-------|
| `direction` field not rejected in new validation arms | `pseudocode/graph_read.md` §5d | Architecture matrix shows `direction` as R for inverse, filter, path. No AC exists for this rejection; implementation agent should add it per the architecture contract. |
| `graph_read.rs` line budget borderline | `pseudocode/graph_read.md` §Line Budget | Pseudocode explicitly identifies ~467–578 line range and provides mitigation (compact formatting or helper extraction). Implementation agent must verify before committing. |

---

## Rework Required

None. Gate result is PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — gate-3a findings for this feature are feature-specific. No recurring cross-feature patterns were identified that are not already captured in Unimatrix (R-03 pattern is #4494, R-09 is #4497, line-budget lesson is #1203, visited-set pattern is #4494).
