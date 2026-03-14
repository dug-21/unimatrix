# Risk-Based Test Strategy: crt-014 — Topology-Aware Supersession

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `graph_penalty` priority rule ordering is wrong — a condition matches earlier than expected, masking the correct topology scenario | High | Med | Critical |
| R-02 | Cycle detection misses a valid cycle (petgraph `is_cyclic_directed` not invoked on correct graph structure) | High | Low | High |
| R-03 | `find_terminal_active` returns wrong node — follows edges in wrong direction, or terminates early at an intermediate non-Active node | High | Med | Critical |
| R-04 | `build_supersession_graph` edge direction is reversed — edge added as `entry.id → pred_id` instead of `pred_id → entry.id` | High | Med | Critical |
| R-05 | Test migration window: constant-value tests removed before behavioral ordering tests added — CI briefly passes with no penalty coverage | High | Med | Critical |
| R-06 | search.rs Step 6b injects wrong successor after multi-hop upgrade — injects `superseded_by` (single-hop) instead of `find_terminal_active` result | High | Med | Critical |
| R-07 | `MAX_TRAVERSAL_DEPTH` not enforced — `find_terminal_active` traverses beyond 10 hops, hangs or panics on pathological chains | Med | Low | High |
| R-08 | Cycle fallback in search.rs applies to wrong scope — `use_fallback` logic applies `FALLBACK_PENALTY` to all entries, not just deprecated/superseded ones | Med | Med | High |
| R-09 | Dangling `supersedes` reference causes `graph_penalty` to panic (missing node_index entry) instead of warn+skip | Med | Low | High |
| R-10 | Full-store graph construction added outside `spawn_blocking` — blocks the async executor thread | Med | Med | High |
| R-11 | `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` remain imported in search.rs after removal — compile error or dead import | Med | High | High |
| R-12 | Penalty hop decay formula under/overflow: `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1)` produces value outside `[0.10, CLEAN_REPLACEMENT_PENALTY]` | Med | Low | High |
| R-13 | petgraph `stable_graph` feature imports unnecessary transitive features or conflicts with workspace `Cargo.lock` | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: `graph_penalty` Priority Rule Ordering

**Severity**: High
**Likelihood**: Med
**Impact**: Orphan entries penalized as if they have active successors, or partial-supersession entries treated as clean replacements. Silent retrieval quality regression.

**Test Scenarios**:
1. Entry with `status == Deprecated` and zero outgoing edges → assert result == `ORPHAN_PENALTY` (not `DEAD_END_PENALTY`)
2. Entry with `status == Deprecated`, one outgoing edge to Active node → assert result == `CLEAN_REPLACEMENT_PENALTY` (not `ORPHAN_PENALTY`)
3. Entry with `status == Active` and `superseded_by.is_some()`, two outgoing edges, both active → assert result == `PARTIAL_SUPERSESSION_PENALTY`
4. Entry with no active-reachable node, one outgoing edge to deprecated terminal → assert result == `DEAD_END_PENALTY`
5. Traverse all 6 priority branches in isolation; each returns the correct constant

**Coverage Requirement**: All 5 priority branches of `graph_penalty` must be exercised by distinct unit test inputs. No branch may share a test.

---

### R-02: Cycle Detection Miss

**Severity**: High
**Likelihood**: Low
**Impact**: Graph with A→B→A supersession cycle is treated as a valid DAG. `find_terminal_active` enters infinite traversal (without depth cap) or terminates incorrectly.

**Test Scenarios**:
1. Two entries: A.supersedes = Some(B.id), B.supersedes = Some(A.id) → assert `build_supersession_graph` returns `Err(GraphError::CycleDetected)`
2. Three entries: A→B→C→A (triangle cycle) → assert `Err(GraphError::CycleDetected)`
3. Valid chain A→B→C (no cycle) → assert `Ok(_)`
4. Single entry with no `supersedes` → assert `Ok(_)` (empty graph is a DAG)
5. Self-referential: A.supersedes = Some(A.id) → assert `Err(GraphError::CycleDetected)`

**Coverage Requirement**: Both cycle and non-cycle cases must be tested. Minimum 2 cycle shapes (2-node, 3-node).

---

### R-03: `find_terminal_active` Returns Wrong Node

**Severity**: High
**Likelihood**: Med
**Impact**: Search injects a non-Active or intermediate-superseded node as the successor. Agent receives stale knowledge.

**Test Scenarios**:
1. Chain A→B→C: A and B are superseded, C is Active and `superseded_by.is_none()` → assert result == `Some(C.id)`
2. Chain A→B: B is Active → assert result == `Some(B.id)` (depth-1 case)
3. Chain A→B→C: C is Deprecated (no active terminal) → assert result == `None`
4. Chain A→B→C→D: C is Active but `superseded_by.is_some()` (chain continues); D is Active and `superseded_by.is_none()` → assert result == `Some(D.id)`, not `Some(C.id)`
5. `node_id` not in graph → assert result == `None`

**Coverage Requirement**: Terminal selection must be validated against the exact combination of `Status::Active` AND `superseded_by.is_none()`.

---

### R-04: Edge Direction Reversed in Graph Construction

**Severity**: High
**Likelihood**: Med
**Impact**: All traversal logic inverted. `find_terminal_active` goes backwards (toward predecessors, not successors). Penalty derivation based on wrong direction (predecessor count instead of successor count).

**Test Scenarios**:
1. Entry B with `supersedes = Some(A.id)` → in graph, directed edge must be `A → B`; assert `graph.inner.edges_directed(A_index, Outgoing)` contains B
2. `find_terminal_active(A.id, ...)` follows forward edges to reach B (Active) → assert `Some(B.id)` (validates direction)
3. `graph_penalty` on A computes `successor_count` by looking at outgoing edges — assert count == 1 when A has one successor B

**Coverage Requirement**: Edge direction must be explicitly verified via graph structure inspection in at least one unit test.

---

### R-05: Test Migration Window — Coverage Gap

**Severity**: High
**Likelihood**: Med
**Impact**: Tests for `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` removed without corresponding behavioral tests — CI green but penalty logic uncovered.

**Test Scenarios**:
1. After removing `deprecated_penalty_value`, `superseded_penalty_value`, `superseded_penalty_harsher_than_deprecated`, `penalties_independent_of_confidence_formula` from `confidence.rs`, the behavioral ordering tests in `graph.rs` must be in the same commit
2. Behavioral ordering: assert `ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY` (orphan is softer numerically since higher value = less penalty)
3. Behavioral ordering: assert `graph_penalty(2-hop)` < `graph_penalty(1-hop)` (2-hop is harsher = lower multiplier)
4. Behavioral ordering: assert `PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY`
5. `confidence.rs` must compile with no reference to `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` after the change

**Coverage Requirement**: The 4 deleted `confidence.rs` tests must each have an equivalent behavioral ordering test in `graph.rs` tests. No net loss of penalty coverage.

---

### R-06: search.rs Injects Wrong Successor After Multi-Hop Upgrade

**Severity**: High
**Likelihood**: Med
**Impact**: Integration tests pass for single-hop cases but multi-hop chains (A→B→C) inject B instead of C. Agent receives superseded intermediate node.

**Test Scenarios**:
1. Integration test: store A (superseded by B), B (superseded by C), C (Active) — search returning A must inject C, not B
2. Single-hop regression: store A (superseded by B), B (Active) — search returning A must inject B (unchanged behavior)
3. Fallback mode: inject cycle data → verify single-hop fallback injects `entry.superseded_by` directly, not `find_terminal_active` result

**Coverage Requirement**: Multi-hop injection test (A→B→C) must be an integration test using a real store. AC-13 (injects C, not B) is the primary verification.

---

### R-07: `MAX_TRAVERSAL_DEPTH` Not Enforced

**Severity**: Med
**Likelihood**: Low
**Impact**: Pathological chains (11+ entries) cause excessive traversal time or panic (stack overflow in recursive implementations).

**Test Scenarios**:
1. Chain of 11 entries: A→B→C→…→K where K is Active → assert `find_terminal_active(A.id, ...)` returns `None` (depth cap hit)
2. Chain of exactly 10 entries: J is Active at depth 10 → assert `Some(J.id)` returned (boundary inclusive)
3. Chain of 9 entries → assert `Some` (within bound)

**Coverage Requirement**: Depth boundary (exactly MAX_TRAVERSAL_DEPTH = 10) must be tested, along with exceeding it (11 hops).

---

### R-08: Cycle Fallback Applied to Wrong Scope

**Severity**: Med
**Likelihood**: Med
**Impact**: On `CycleDetected`, `FALLBACK_PENALTY` is applied to Active entries or all entries rather than only deprecated/superseded ones. Active entry scores degraded.

**Test Scenarios**:
1. Inject cycle data → `build_supersession_graph` returns `Err(CycleDetected)` → confirm only entries where `superseded_by.is_some() || status == Deprecated` receive `FALLBACK_PENALTY`
2. Active entry in same search as cycle data → confirm its penalty_map entry is absent (no penalty applied)

**Coverage Requirement**: Integration test with mixed Active + Deprecated entries when cycle is detected.

---

### R-09: Dangling `supersedes` Reference Panics Instead of Warn+Skip

**Severity**: Med
**Likelihood**: Low
**Impact**: Entry references a non-existent predecessor ID. `build_supersession_graph` panics instead of logging warning and continuing.

**Test Scenarios**:
1. One entry with `supersedes = Some(9999)` where 9999 is not in the entry slice → assert `Ok(graph)` returned (not panic or error)
2. Verify `tracing::warn!` is emitted (via tracing subscriber in test or log capture)
3. Graph has correct node count (only the one entry, no dangling edge)

**Coverage Requirement**: AC-17 must be verified explicitly.

---

### R-10: Graph Construction Blocks Async Executor

**Severity**: Med
**Likelihood**: Med
**Impact**: `build_supersession_graph` called on the async executor thread (not inside `spawn_blocking`) blocks all async operations for 1-5ms per search. Under load, this degrades server throughput.

**Test Scenarios**:
1. Code review: confirm `build_supersession_graph` call in `search.rs` is inside a `spawn_blocking` block or a block already running in `spawn_blocking` context
2. NFR-01: benchmark test asserting graph construction completes ≤5ms at 1,000 entries (validates the time budget is safe)

**Coverage Requirement**: Code structure review + NFR-01 benchmark.

---

### R-11: Dead Import Causes Compile Error or Warning

**Severity**: Med
**Likelihood**: High
**Impact**: After constant removal, `search.rs` still imports `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY`. Either compile error (if removed from `confidence.rs`) or dead_code warning.

**Test Scenarios**:
1. AC-18: `cargo build --workspace 2>&1 | grep "^error" | wc -l` == 0 after all changes
2. `cargo build --workspace 2>&1 | grep "unused import"` returns no hits for the penalty constants
3. AC-14: `grep -r DEPRECATED_PENALTY crates/ --include="*.rs"` returns no non-test hits

**Coverage Requirement**: CI must pass without warnings (workspace-level).

---

### R-12: Penalty Hop Decay Formula Out-of-Range

**Severity**: Med
**Likelihood**: Low
**Impact**: Deep chains produce penalty values below `0.10` (clamped) or above `CLEAN_REPLACEMENT_PENALTY` (logic error). Penalty ordering invariants violated.

**Test Scenarios**:
1. Depth-1 chain → `graph_penalty` == `CLEAN_REPLACEMENT_PENALTY` (0.40)
2. Depth-2 chain → `graph_penalty` == `0.40 * 0.60` == 0.24 (within [0.10, 0.40])
3. Depth-5 chain → `graph_penalty` == `0.40 * 0.60^4` ≈ 0.052 → clamped to 0.10
4. Depth-10 chain → result clamped to 0.10 (floor asserted)
5. No depth produces a value above `CLEAN_REPLACEMENT_PENALTY`

**Coverage Requirement**: Test depths 1, 2, 5, 10 for the decay formula. Assert clamp behavior at both ends.

---

## Integration Risks

**IR-01: `Store::query(QueryFilter::default())` — full-store read API**
- `QueryFilter::default()` must return all entries regardless of status. If the implementation filters by status (returning only Active), the graph misses Deprecated nodes, breaking orphan detection and chain depth computation.
- Test: assert graph built from `QueryFilter::default()` includes entries with `Status::Deprecated`.

**IR-02: `search.rs` Step 6a and 6b — unified penalty guard condition**
- Old code used two separate conditions (`entry.superseded_by.is_some()` for superseded, `entry.status == Deprecated` for deprecated). The unified condition `entry.superseded_by.is_some() || entry.status == Deprecated` must cover both cases correctly.
- Test: entry with `superseded_by.is_some()` but `status == Active` (superseded-but-marked-active) must receive a graph penalty.

**IR-03: `graph_penalty` called for non-deprecated/superseded entries**
- If search.rs accidentally calls `graph_penalty` for Active entries with `superseded_by.is_none()`, the function returns `1.0` (no penalty), which is safe — but the performance cost of calling the function for every Active entry should be avoided.
- Test: confirm no entry with `status == Active && superseded_by.is_none()` has a penalty_map entry after Step 6a.

**IR-04: `thiserror` availability in `unimatrix-engine`**
- `GraphError` uses `#[derive(thiserror::Error)]`. If `thiserror` is not already in `unimatrix-engine/Cargo.toml`, the build fails. ARCHITECTURE.md notes "Also add thiserror if not already present."
- Verify during implementation: check existing `Cargo.toml` before adding.

---

## Edge Cases

| Edge Case | Risk | Test |
|-----------|------|------|
| Empty entry slice | `build_supersession_graph` with `entries = &[]` | Assert `Ok(graph)` with zero nodes |
| Single entry, no `supersedes` | Trivial graph | Assert `Ok(graph)` with one node, no edges |
| All entries Active, none superseded | No penalties applied | Assert `penalty_map` is empty after Step 6a |
| Entry in graph but not in entries slice | `graph_penalty` looks up entry by id — miss | Should return `1.0` (no penalty) without panic |
| `find_terminal_active` starting node is already Active | Starting node is the terminal | Behavior undefined by spec; should return `Some(node_id)` |
| Two successors, one Active, one Deprecated | Partial supersession scenario | `successor_count > 1` triggers `PARTIAL_SUPERSESSION_PENALTY` even if one branch is dead |
| `node_id` is 0 (boundary u64) | No special handling needed | Assert no panic, returns `1.0` if not in graph |

---

## Security Risks

**graph.rs accepts no external/untrusted input directly.** All inputs flow through:
1. `Store::query()` → returns `Vec<EntryRecord>` from the internal redb store
2. `node_id: u64` → derived from `entry.id` values already in the store

**Blast radius**: Minimal. An attacker who can write malformed `supersedes` values to the store could:
- Create a cycle → triggers `CycleDetected` → fallback mode (search degrades to flat penalties, not a failure)
- Create a very long chain → traversal capped at `MAX_TRAVERSAL_DEPTH = 10`, bounded cost
- Create dangling references → skipped with `tracing::warn!`, no panic

**Conclusion**: No path traversal, injection, or deserialization risks in `graph.rs`. The only attack surface is the store write path (existing trust boundaries from `context_store` handle authorization). The cycle and depth cap defenses ensure graph.rs cannot be driven to unbounded computation through malformed data.

---

## Failure Modes

| Failure | Expected Behavior | Verifiable By |
|---------|------------------|---------------|
| `build_supersession_graph` → `CycleDetected` | `tracing::error!`, `use_fallback = true`, all penalized entries get `FALLBACK_PENALTY`, single-hop injection | Integration test: cycle data → search succeeds, log contains error |
| `Store::query` fails | Propagate existing `ServiceError` — search fails as before (no change) | Existing search error tests |
| `find_terminal_active` hits depth cap | Returns `None`, caller skips injection (no successor injected) | Unit test: 11-hop chain → `None` |
| Dangling `supersedes` reference | `tracing::warn!`, edge skipped, graph construction continues | Unit test: R-09 scenario |
| All entries deprecated with no active terminal | `graph_penalty` returns `DEAD_END_PENALTY` for all; no successor injection | Unit test + integration test |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (petgraph feature creep) | R-13 | ADR-001 specifies `stable_graph` only with explicit comment; CI will catch if features expand via `cargo build` |
| SR-02 (full-store read latency) | R-10 | NFR-01 defines ≤5ms at 1,000 entries; `spawn_blocking` placement validated by code review; benchmark in integration tests |
| SR-03 (judgment-call coefficients) | R-01, R-12 | Behavioral ordering tests (AC-05 through AC-08) assert relative ordering invariants without hardcoding absolute values; R-12 tests decay formula bounds |
| SR-04 (unbounded DFS depth) | R-07 | `MAX_TRAVERSAL_DEPTH = 10` as named `pub const`; AC-11 and R-07 tests verify the cap |
| SR-05 (fallback reintroduces constants) | R-08 | `FALLBACK_PENALTY` defined in `graph.rs` alongside other constants; `confidence.rs` has zero penalty constants after crt-014 |
| SR-06 (context_status cycle surface) | — | Resolved as log-only in v1 (ADR-005); no struct change required; `tracing::error!` is the surface |
| SR-07 (constant-value test migration) | R-05, R-11 | Atomic commit: remove 4 constant tests, add behavioral ordering tests in same commit; verified by AC-14 + AC-15 |
| SR-08 (single-hop test injection regression) | R-06 | AC-13 integration test for A→B→C multi-hop; regression test for A→B single-hop (existing behavior preserved when chain is terminal) |
| SR-09 (crt-017 API contract) | — | `SupersessionGraph` is a named opaque struct; public API stable for crt-017 extension; ADR-001 notes crt-017 forward compatibility |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01, R-03, R-04, R-05, R-06) | 22 scenarios across unit + integration tests |
| High | 6 (R-02, R-07, R-08, R-09, R-10, R-11) | 16 scenarios across unit + integration tests |
| Medium | 2 (R-12, IR-01 through IR-04) | 8 scenarios across unit + integration tests |
| Low | 1 (R-13) | 1 scenario (build clean) |
