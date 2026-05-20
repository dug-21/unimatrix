# Gate 3b Report: vnc-019

> Gate: 3b (Code Review)
> Date: 2026-05-20
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | WARN | Direction default wrong (outgoing vs both); edge_type error message format differs from IMPLEMENTATION-BRIEF |
| Architecture compliance | PASS | ADR-001 through ADR-005 all followed; file split, lock discipline, BFS-only traversal, batch SQL |
| Interface implementation | PASS | All function signatures match; pub(super) visibility applied correctly |
| Test case alignment | PASS | All critical and high test scenarios covered; 74 graph_read tests pass |
| Code quality | PASS | Compiles clean; no unwrap() in prod code; no stubs; all files under 500 lines |
| Security | PASS | No hardcoded secrets; input validated; serde_json::from_str(..).ok() on metadata (SEC-05); no path traversal |
| Knowledge stewardship | PASS | Both agent reports include Queried + Stored entries with reasons |

## Detailed Findings

### Pseudocode Fidelity

**Status**: WARN (two deviations, one behavioral, one cosmetic)

**Evidence — deviation 1 (behavioral): direction default is "outgoing" not "both"**

Specification FR-05: "Default `"both"` when absent."
IMPLEMENTATION-BRIEF §BFS Algorithm Contract step 1: "direction in ["incoming", "outgoing", "both"]" with "default `"both"`".
Pseudocode `graph_read_subgraph.md` step 1e: `params.direction.as_deref().unwrap_or("both")`.

Implementation (graph_read_subgraph.rs line 100):
```rust
let direction_str = params.direction.as_deref().unwrap_or("outgoing");
```

The default is `"outgoing"` instead of `"both"`. This is a behavioral defect: a caller who omits `direction` and expects bidirectional traversal (the documented default) receives only outgoing edges. The deviation contradicts FR-05, the BFS algorithm contract, and the pseudocode.

**Evidence — deviation 2 (cosmetic): edge_type error message does not match exact strings**

IMPLEMENTATION-BRIEF §Validation Error Messages table (exact strings):
- `"unrecognized edge_type '{value}' — recognized types: Supports, Contradicts, ..."` (list all 16)

Implementation (graph_read_subgraph.rs lines 127–131):
```
"unknown edge_type '{}' -- valid types: Advances, Asserts, About, Cites, CoAccess, Contradicts, DerivedFrom, Informs, Mentions, Motivates, Prerequisite, Refutes, RelatedTo, Supports, Tests"
```

Deviations:
- Uses `"unknown"` instead of `"unrecognized"` 
- Uses `"--"` (double hyphen) instead of `"—"` (em-dash)
- Uses `"valid types"` instead of `"recognized types"`
- Lists 15 types (omits "Supersedes") — the brief specifies listing all 16 because subgraph mode permits explicit Supersedes traversal (FR-04)

The test in `test_validate_unknown_edge_type_rejected` only checks that the bad value is named and at least one valid type is present — it does not enforce the exact prefix or completeness of the type list. This means the test passes despite the message not matching the spec.

### Architecture Compliance

**Status**: PASS

All ADRs correctly implemented:
- ADR-001: `max_depth: Option<u8>` on `GraphParams`; rejected on chain/current/neighbors with exact messages; range [1,10] in handle_subgraph
- ADR-002: `handle_subgraph` in `graph_read_subgraph.rs` declared via `#[path]`; `SubgraphResponse` in `graph_read.rs`
- ADR-003: Post-BFS OR-chain metadata batch query; empty-edge guard present (R-04)
- ADR-004: No `graph_rebuilt_at` in response; staleness in tool description only
- Inherited ADR-005: BFS uses in-memory TypedRelationGraph only; cold-start returns empty result
- Lock discipline: `std::sync::RwLock::read().unwrap_or_else(|e| e.into_inner())` with graph cloned before any async work
- `SubgraphResponse` fields match architecture spec exactly: `{nodes, edges, truncated, seed_ids, depth_reached}`

### Interface Implementation

**Status**: PASS

- `handle_subgraph`: `pub(super) async fn` with correct signature — matches IMPLEMENTATION-BRIEF §Function Signatures
- `follow_to_current`: changed to `pub(super)` in `graph_read_neighbors.rs` line 34 — verified
- `all_non_supersedes_types`: already `pub(super)` — verified
- `handle_graph` dispatch: subgraph arm correctly uses `seed_ids` not `id`; id-required guard correctly scoped to chain/current/neighbors arm only
- `validate_no_unsupported_params`: subgraph arm permits seed_ids, max_nodes, max_depth; rejects from_id, to_id
- Unrecognized-mode error now lists "subgraph" — verified in implementation and test
- `SubgraphResponse`: 5-field struct with `#[derive(Debug, Clone, Serialize)]` — no Deserialize (outbound only)

### Test Case Alignment

**Status**: PASS

All critical and high risk scenarios from the test plan are covered by implemented tests:

| Test Plan Scenario | Test | Result |
|---|---|---|
| seed_ids absent → exact error | test_validate_seed_ids_absent_returns_error | PASS |
| seed_ids empty → exact error | test_validate_seed_ids_empty_returns_error | PASS |
| max_depth=0 rejected | test_validate_max_depth_zero_rejected | PASS |
| max_depth=11 rejected | test_validate_max_depth_eleven_rejected | PASS |
| max_depth=1,10 accepted | test_validate_max_depth_boundary_values_accepted | PASS |
| max_nodes=201 rejected | test_validate_max_nodes_above_200_rejected | PASS |
| max_nodes=0 rejected | test_validate_max_nodes_zero_rejected | PASS |
| Unknown edge_type error | test_validate_unknown_edge_type_rejected | PASS |
| direction="forward" invalid | test_validate_direction_forward_rejected | PASS |
| Cold-start empty result (R-04, AC-17) | test_bfs_cold_start_empty_result | PASS |
| Seed saturation truncated (R-03) | test_bfs_seed_saturation_sets_truncated | PASS |
| direction="both" no duplicates (R-02) | test_bfs_direction_both_no_duplicate_edges | PASS |
| All EdgeRecord.direction="outgoing" | test_bfs_edge_direction_always_outgoing | PASS |
| Two-hop depth_reached=2 | test_bfs_two_hop_chain_depth_reached_2 | PASS |
| max_depth=1 boundary | test_bfs_max_depth_one_only_direct_neighbors | PASS |
| Supports edge traversed | test_bfs_traverses_supports_edge | PASS |
| Not truncated under cap | test_bfs_not_truncated_under_cap | PASS |
| AC-13 tool description disclosures | test_tool_description_contains_staleness_disclosures | PASS |
| R-05: mode="walk" probe updated | test_validate_unrecognized_mode_fires_before_field_check | PASS |
| subgraph in supported modes list | test_validate_walk_mode_error_lists_valid_modes | PASS |
| SubgraphResponse all 5 fields serialize | test_subgraph_response_serializes_all_fields | PASS |
| max_depth on chain/current/neighbors rejected | test_validate_{chain,current,neighbors}_rejects_max_depth | PASS |

WARN: Some test plan scenarios from Section B–D (resolve_supersessions ordering tests, dangling-edge filter test, metadata hydration tests, circular supersession test) are defined in the test plan but not yet implemented as actual tests. These are described as skeletons/stubs in the test plan document. However, the critical behavioral invariants (R-01, R-02, R-03, R-04, R-05) are covered by the implemented tests. The missing scenarios cover R-06 (circular supersession), C-2 (dangling-edge filter integration), and D-1 (metadata hydration). These are not FAIL because the BFS implementation code correctly implements the dangling-edge filter (lines 276–277) and the empty-OR-chain guard (line 290), but there are no automated tests confirming these code paths fire. This is a coverage gap.

**FR-23 / AC-14 — Integration test in infra-001 suite**: The specification (FR-23) and AC-14 require an integration test in the infra-001 suite that writes 5+ entries with typed edges and asserts the returned subgraph topology. No such test was found in `crates/unimatrix-server/tests/`. The BFS tests in `graph_read_subgraph_bfs_tests.rs` are unit-level tests using a pre-built in-memory graph and a test store. They do not constitute an infra-001 integration test in the sense of the full MCP call path (capability check → handle_graph → handle_subgraph → SQL reads). This is a FAIL against FR-23 / AC-14.

### Code Quality

**Status**: PASS

- Compilation: `cargo build -p unimatrix-server` succeeds with 21 warnings (all pre-existing, no new warnings from vnc-019 files)
- No `unwrap()` calls in non-test code across the three modified files — verified via grep
- No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` placeholders — verified
- File line counts:
  - `graph_read.rs`: 385 lines — PASS (under 500)
  - `graph_read_subgraph.rs`: 434 lines — PASS (under 500)
  - `graph_read_neighbors.rs`: 356 lines — PASS (under 500)
  - `tools.rs`: 9,901 lines — pre-existing; vnc-019 touched only the description constant and one test; not a vnc-019 violation
- 74 graph_read tests pass; 0 failures

### Security

**Status**: PASS

- No hardcoded secrets or credentials
- Input validation at all MCP parameter boundaries (seed_ids, max_depth, max_nodes, edge_types, direction) before any SQL or graph access
- No path traversal vulnerabilities (file path operations not present in these components)
- `serde_json::from_str(s).ok()` on metadata column — malformed JSON silently returns None, no panic (SEC-05)
- SQL binds via parameterized queries (sqlx `.bind()` chain) — no string interpolation of user values into SQL
- `cargo audit` not installed; skipped per environment constraint

### Knowledge Stewardship

**Status**: PASS

**Agent 3 (vnc-019-agent-3-graph-read-report.md)**:
- Queried: `mcp__unimatrix__context_search` — entries #4474, #4301, #4486, #4493, #4490, #4491. Applied findings.
- Stored: entry #4510 "Post-seed cap check required for truncated=true when seeds exactly fill max_nodes"

**Agent 5 (vnc-019-agent-5-tools-report.md)**:
- Queried: `mcp__unimatrix__context_briefing` — not called; rationale given: "task narrowly scoped to string update, no architectural unknowns". This is borderline but the brief was clear enough that a briefing would be redundant. Acceptable.
- Stored: entry #4509 "Create graph_read_subgraph.rs stub immediately when mod declaration exists but file is absent"

Both reports include a `## Knowledge Stewardship` section with Queried and Stored entries.

---

## Rework Required

| Issue | Severity | Which Agent | What to Fix |
|-------|----------|-------------|-------------|
| direction default: `"outgoing"` must be `"both"` (FR-05) | FAIL — behavioral | rust-dev agent | In `graph_read_subgraph.rs` line 100: change `unwrap_or("outgoing")` to `unwrap_or("both")` |
| edge_type error message prefix does not match IMPLEMENTATION-BRIEF exact string | WARN | rust-dev agent | In `graph_read_subgraph.rs` lines 127–131: change `"unknown edge_type '{}' -- valid types:"` to `"unrecognized edge_type '{}' — recognized types:"` and add `"Supersedes"` to the list (16 total) |
| Missing infra-001 integration test (FR-23, AC-14) | FAIL — specification coverage | rust-dev agent or tester agent | Add integration test in `crates/unimatrix-server/tests/` that writes ≥5 entries with typed edges, calls `context_graph(mode="subgraph", ...)` via the full MCP call path (handle_graph), and asserts returned nodes and edges match expected topology |

---

## Scope Concerns

None. The feature is implementable within the approved scope. Both FAILs are reworkable defects.

---

## Knowledge Stewardship

- Stored: entry for "direction default 'outgoing' vs 'both' in subgraph mode — spec vs implementation divergence" via /uni-store-lesson — this is a recurring pattern (default-value deviation from spec not caught by tests that don't exercise the default case). Topic: `validation`. Category: `lesson-learned`.
- Queried: mcp__unimatrix__context_briefing (not called — gate review does not require it; patterns already loaded from spawn context)
