# Gate 3a Report: vnc-019

> Gate: 3a (Design Review)
> Date: 2026-05-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 4 components, all ADRs, lock discipline, BFS contract |
| Specification coverage | PASS | All FR-01–FR-23 and NFR-01–08 covered |
| Risk coverage | PASS | All 16 risks and 5 integration risks mapped to test scenarios |
| Interface consistency | WARN | `None | Some([])` slice pattern in pseudocode step 1a will not compile as written; minor approximation |
| Knowledge stewardship | WARN | agent-1-pseudocode and agent-2-spec have stewardship sections with `Queried:` but missing explicit "nothing novel to store -- {reason}" form |

---

## Detailed Findings

### Check 1: Architecture Alignment
**Status**: PASS

**Evidence**:
- `pseudocode/OVERVIEW.md` lists the same 4 components as ARCHITECTURE.md §Component Breakdown: `graph_read.rs`, `graph_read_subgraph.rs` (new), `graph_read_neighbors.rs`, `tools.rs`. No extra components added.
- `graph_read.md` implements ADR-001 (`max_depth: Option<u8>` appended to `GraphParams`), defines `SubgraphResponse` in `graph_read.rs` alongside existing response envelopes (FR-17, FR-18), and restructures `handle_graph` with the two-level match pattern described in ARCHITECTURE.md to avoid the broken unconditional `id` extraction.
- `graph_read_subgraph.md` declares the module via `#[path = "graph_read_subgraph.rs"]` (ADR-002), acquires `std::sync::RwLock` once, clones the graph, then releases before any async work (ARCHITECTURE.md §Lock Discipline), implements the OR-chain metadata batch (ADR-003), and guards the query with `if !collected_edges.is_empty()` (R-04, AC-19).
- `graph_read_neighbors.md` restricts its change to one keyword addition (`pub(super)` on `follow_to_current`) and explains the `pub(super)` rationale consistent with ARCHITECTURE.md §3.
- `tools.md` is a description-only change; the dispatch chain is unchanged (ARCHITECTURE.md §4).
- All 4 ADRs (001–004) are fully implemented in the pseudocode.
- `unimatrix-engine` and `unimatrix-store` are confirmed read-only; no modifications to either.
- `handle_subgraph` function signature in pseudocode matches ARCHITECTURE.md §Integration Surface exactly: `pub(super) async fn handle_subgraph(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<SubgraphResponse, ErrorData>`.

### Check 2: Specification Coverage
**Status**: PASS

**Evidence**:
All 23 functional requirements are addressed in pseudocode:

| FR | Pseudocode Location | Coverage |
|----|---------------------|----------|
| FR-01 (dispatch) | graph_read.md §5 `handle_graph` subgraph arm | PASS |
| FR-02 (cap gate) | OVERVIEW.md data flow shows `require_cap(Read)` before `handle_graph` | PASS |
| FR-03 (seed_ids required, exact error) | graph_read_subgraph.md step 1a — exact message quoted | PASS |
| FR-04 (edge_types default = all non-Supersedes) | step 1d — explicit `all_non_supersedes_types()` call | PASS |
| FR-05 (direction default "both") | step 1e | PASS |
| FR-06 (max_depth [1,10], exact error) | step 1b — exact message format `"max_depth must be in range 1..=10, got {max_depth}"` | PASS |
| FR-07 (max_nodes [1,200], rejection not clamping) | step 1c — `n > 200` returns error | PASS |
| FR-08 (resolve_supersessions substitution before visited check) | seed phase and BFS step — substitution before `visited.contains()` check | PASS |
| FR-09 (in-memory BFS, lock clone pattern) | step 2 | PASS |
| FR-10 (seeds always in nodes) | seed phase adds to `collected_node_ids` regardless | PASS |
| FR-11 (pre-enqueue cap) | seed phase checks `collected_node_ids.len() >= max_nodes` before insert | PASS |
| FR-12 (edge dedup, direction always "outgoing") | edge_key canonical triple, `direction: "outgoing".to_string()` in step 8 | PASS |
| FR-13 (batch node hydration) | step 6 `store.get_many` | PASS |
| FR-14 (post-BFS metadata batch, skip when empty) | step 7 guarded by `if !collected_edges.is_empty()` | PASS |
| FR-15 (missing seed = empty, not error) | seed phase: node goes to `collected_node_ids` but not frontier | PASS |
| FR-16 (depth_reached computation) | step 9 `edges.iter().map(|e| e.depth).max().unwrap_or(0)` | PASS |
| FR-17 (SubgraphResponse wire type) | graph_read.md §3 struct definition matches spec exactly | PASS |
| FR-18 (file placement ADR-002) | `#[path]` declaration and test file placement specified | PASS |
| FR-19 (staleness disclosure, direction semantics first) | tools.md — disclosure text leads with `direction:"outgoing"` in first sentence | PASS |
| FR-20 (subgraph arm, vnc-018 test update) | graph_read.md §4 and OVERVIEW.md sequencing constraint 5 | PASS |
| FR-21 (no engine changes) | No unimatrix-engine modifications in any pseudocode file | PASS |
| FR-22 (no new migration) | Confirmed in OVERVIEW.md | PASS |
| FR-23 (integration test) | test-plan OVERVIEW: `test_graph_subgraph_topology_traversal` in infra-001 | PASS |

NFR-01 (BFS latency) through NFR-08 (SR-05 bound) are all structurally addressed: no per-node queries in BFS inner loop, lock held only for clone, OR-chain bounded by 200-node cap, follow_to_current capped at 50 hops.

No scope additions found: no new MCP tools, no new tables, no new crates, no new migrations, no modes beyond `subgraph`.

### Check 3: Risk Coverage
**Status**: PASS

**Evidence**: All 16 risks from RISK-TEST-STRATEGY.md map to named test functions in the test plan. Coverage counts meet minimums:

- **Critical (R-01, R-02, R-03)**: 3+3+4 = 10 scenarios (required: ≥10) ✓
- **High (R-04, R-05, R-06, R-07, R-11)**: 2+7+2+4+2 = 17 scenarios (required: ≥15) ✓
- **Med (R-08 through R-15)**: 4+1+1+2+1+2+1+3 = 15 scenarios (required: ≥14) ✓
- **Low (R-16)**: 1 (covered via AC-13 tool description review) ✓

All 5 integration risks are addressed:
- IR-01: `pub(super)` visibility — compile gate + R-01/R-06 tests exercise the import path
- IR-02: `all_non_supersedes_types` scope — R-14 scenarios
- IR-03: dispatch signature — compile gate
- IR-04: schema v27 indexes — noted in test-plan OVERVIEW as an infra-001 requirement
- IR-05: cold-start — `test_bfs_empty_graph_cold_start_no_error` (unit) + `test_graph_subgraph_cold_start_empty_result` (integration, noted as potential xfail)

The dangling-edge filter (ARCHITECTURE step 5b) has an explicit test `test_bfs_dangling_edges_removed_after_truncation` — this correctness invariant is not a named risk but is correctly identified and covered.

### Check 4: Interface Consistency
**Status**: WARN

**Evidence**: Interfaces are consistent across all pseudocode files. OVERVIEW.md shared types match component-file usage:
- `SubgraphResponse` in graph_read.rs, imported as `super::SubgraphResponse` in subgraph module ✓
- `EdgeRecord`, `GraphParams` — same import path ✓
- BFS internal state types (`VecDeque`, `HashSet`, `Vec`, `HashMap`) match between OVERVIEW.md and graph_read_subgraph.md ✓
- `validate_no_unsupported_params` extension consistent between graph_read.md and test-plan/graph_read.md ✓

**Issue (WARN)**: `graph_read_subgraph.md` step 1a uses the pattern `None | Some([]) =>` in a `match`. The `Some([])` empty-slice pattern requires Rust nightly or is not supported in all contexts for `Vec<T>`. The intent is correct but the pattern would likely need to be expressed as:

```rust
let seed_ids = match &params.seed_ids {
    None => return Err(...),
    Some(ids) if ids.is_empty() => return Err(...),
    Some(ids) => ids.clone(),
};
```

This is a pseudocode approximation, not a functional bug — the correct behavior is clearly specified. The delivery agent must use the compilable form. Flagged as WARN (implementor will catch at compile time).

### Check 5: Knowledge Stewardship Compliance
**Status**: WARN

**Evidence**:

| Agent | Section Present | Queried | Stored |
|-------|----------------|---------|--------|
| vnc-019-agent-1-pseudocode | YES | YES (3 queries) | WARN — "Deviations from established patterns: none" is missing the explicit "nothing novel to store -- {reason}" form |
| vnc-019-agent-2-spec | YES | YES (1 query) | WARN — no Stored or "nothing novel" entry at all |
| vnc-019-agent-2-testplan | YES | YES (3 queries) | YES — entry #4501 stored |
| vnc-019-agent-3-risk | YES | YES (4 queries) | YES — entry #4494 stored |

Two of four agents have incomplete stewardship: agent-1 expresses the absence but not with the required "nothing novel to store -- {reason}" form; agent-2-spec has no stored entry at all. Per gate rules: "Present but no reason after 'nothing novel' = WARN" — these are WARNs, not FAILs, since the stewardship section exists and queries are documented.

---

## Warnings Summary

| Warning | Agent/File | Severity |
|---------|------------|----------|
| `None \| Some([])` slice pattern in pseudocode — not compilable as written; implementor must use `if ids.is_empty()` form | pseudocode/graph_read_subgraph.md step 1a | WARN (caught at compile, correct semantics) |
| Stewardship section missing explicit "nothing novel to store -- {reason}" format | agents/vnc-019-agent-1-pseudocode-report.md | WARN |
| Stewardship section has no Stored or "nothing novel" entry | agents/vnc-019-agent-2-spec-report.md | WARN |

---

## Rework Required

None. All checks PASS or WARN. No FAIL items.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate-3a findings for vnc-019 are feature-specific; the warning patterns (incomplete stewardship entries) are already captured as a systemic pattern in prior validation entries.
