# Test Plan: graph_read_path.rs — BFS Shortest-Path Handler

Component: `crates/unimatrix-server/src/mcp/graph_read_path.rs`
Responsibility: `handle_path` — validate path-mode params, resolve endpoints (optional),
acquire graph snapshot, run path-carrying BFS outgoing-only, return `PathResponse`.

---

## Unit Test Expectations

### AC-16 — from_id Required

**Test**: `test_handle_path_missing_from_id_returns_error`
**Arrange**: `GraphParams { mode: "path", from_id: None, to_id: Some(1), ... }`.
**Act**: `handle_path(store, typed_graph_state, &params).await`.
**Assert**: `Err(ErrorData)` with message `"path mode requires from_id"` (exact text).

**Risk**: R-04

---

### AC-17 — to_id Required

**Test**: `test_handle_path_missing_to_id_returns_error`
**Arrange**: `GraphParams { mode: "path", from_id: Some(1), to_id: None, ... }`.
**Assert**: `Err(ErrorData)` with message `"path mode requires to_id"` (exact text).

**Risk**: R-04

---

### AC-18 — depth Boundary Validation

Three tests:

**Test**: `test_handle_path_depth_default_is_5`
**Arrange**: `depth: None`. Provide a graph with a 5-hop path from from_id to to_id.
**Assert**: BFS finds the path (default depth 5 is applied, not 0 or 1).

**Test**: `test_handle_path_depth_zero_returns_error`
**Arrange**: `depth: Some(0)`.
**Assert**: `Err(ErrorData)` with range statement [1, 10].

**Test**: `test_handle_path_depth_11_returns_error`
**Arrange**: `depth: Some(11)`.
**Assert**: `Err(ErrorData)` with range statement [1, 10].

**Risk**: R-07 (depth on path mode must be accepted within range; rejected outside range)

---

### AC-15 — from_id Not in Snapshot Returns found: false, Not Error

**CRITICAL**: This test must use a DISTINCT fixture from AC-14. Different internal path.

**Test**: `test_handle_path_from_id_not_in_snapshot_returns_not_found`
**Arrange**: Inject an `Arc<RwLock<TypedGraphState>>` that contains a valid graph with
entries A and B but NOT entry_id=99999. Both `from_id` and `to_id` exist in the DB (store
them), but only the graph state is bypassed.
Use pattern #4501 (inject pre-populated TypedGraphState bypassing the tick).
**Act**: `handle_path(store, &graph_state, &params { from_id: 99999, to_id: B })`.
**Assert**: Returns `Ok(PathResponse { found: false, hops: [], length: 0 })` — NOT an
`Err(ErrorData)`. The handler signature is `Result<PathResponse, ErrorData>`; the
not-in-snapshot path returns `Ok`, never `Err`.

Confirm the handler function returns `Result<PathResponse, ErrorData>` (not an infallible
`PathResponse`) — lesson #4497: infallible handler signatures mask failure-path tests.

**Risk**: R-09, R-01 (AC-15)

**Test**: `test_handle_path_to_id_not_in_snapshot_returns_not_found`
Same pattern as above with to_id absent from the snapshot. `from_id` IS in the graph;
`to_id` is NOT. Assert `found: false`, not `Err`.

---

### AC-32 — Self-Path (from_id == to_id) Returns found: false

**Test**: `test_handle_path_self_path_returns_not_found`
**Arrange**: Inject a graph with entry A present. `from_id = A`, `to_id = A`.
**Act**: `handle_path`.
**Assert**: `Ok(PathResponse { found: false, hops: [], length: 0 })`.
The BFS must NOT return `{ found: true, hops: [], length: 0 }` — a self-path is "not found"
per FR-18a. The destination check fires only when a neighbor is reached, never on the seed.

**Risk**: R-12

---

### R-03 — BFS Visited Set Deduplicates by Resolved ID (Double-Enqueue Prevention)

**CRITICAL**: Directly validates pattern #4494.

**Test**: `test_handle_path_bfs_visited_set_keyed_on_resolved_id`

**Fixture**: Construct a `TypedRelationGraph` with:
- Node `from_node` (entry_id = 1)
- Nodes `D1_dep` (entry_id = 2, deprecated) and `D2_dep` (entry_id = 3, deprecated)
- Node `C_active` (entry_id = 4, active) — both D1 and D2 are superseded by C_active
- Edges: from_node → D1_dep (Supports), from_node → D2_dep (Supports)
- DB entries for D1 and D2 with `superseded_by = 4` (C_active)

**Act**: `handle_path(store, graph, params { from_id: 1, to_id: 4, resolve_supersessions: true })`.

**Assertions**:
- Returns `Ok(PathResponse { found: true, ... })`.
- `hops.len() == 1` — exactly ONE hop to C_active.
- `hops[0].entry_id == 4` — C_active appears exactly once (not twice, not more).
- BFS terminates normally (no loop, no duplicate hops).

The test is explicit about: C_active is enqueued once despite two deprecated paths
converging on it. The visited set keyed on `4` (C_active's resolved ID) prevents
double-enqueue.

**Risk**: R-03 (Critical, pattern #4494)

---

### R-12 — Path Response Shape: 1-hop and 2-hop

**Test**: `test_handle_path_1hop_from_id_not_in_hops`
**Fixture**: Graph with A→B edge (Advances). `from_id=A, to_id=B`.
**Assert**:
- `found: true`.
- `from_id == A` (top-level, NOT in hops).
- `hops.len() == 1`.
- `hops[0].entry_id == B`.
- `hops[0].relation_type == "Advances"` (not null, not empty).
- `length == 1`.

**Test**: `test_handle_path_2hop_from_id_not_in_hops`
**Fixture**: Graph with A→B (Advances), B→C (Supports). `from_id=A, to_id=C`.
**Assert**:
- `found: true`.
- `from_id == A` (NOT in hops array).
- `hops.len() == 2`.
- `hops[0].entry_id == B`, `hops[0].relation_type == "Advances"`.
- `hops[1].entry_id == C`, `hops[1].relation_type == "Supports"`.
- `length == 2`.
- A is NOT present anywhere in hops (explicit check: `assert all(h["entry_id"] != A for h in hops)`).

**Risk**: R-12 (AC-13, AC-31 unit-level complement)

---

### R-06 — Endpoint Resolution Reflected in Response

**Test**: `test_handle_path_resolve_supersessions_from_id_reflected`
**Arrange**: DB has entry D (deprecated, id=10) with `superseded_by=11`. Entry A (active, id=11).
Graph has A in it. Target entry B (id=20). Edge A→B in graph.
**Act**: `handle_path(store, graph, { from_id: 10, to_id: 20, resolve_supersessions: true })`.
**Assert**: `response.from_id == 11` (resolved ID, NOT 10). BFS started from A (hops reflect
edges from A to B).

**Test**: `test_handle_path_resolve_supersessions_to_id_reflected`
**Arrange**: DB has deprecated D2 (id=30) with `superseded_by=31`. Entry T (active, id=31) in graph.
Edge A→T in graph. `from_id=A, to_id=30, resolve_supersessions=true`.
**Assert**: `response.to_id == 31` (resolved, not 30).

**Test**: `test_handle_path_resolve_supersessions_false_uses_original_id`
**Arrange**: Same deprecated entry D (id=10). `resolve_supersessions: false`.
**Assert**: `response.from_id == 10` (original, not resolved). If D has no edges in graph,
result is `{ found: false }` with `from_id: 10`.

**Risk**: R-06

---

### AC-14 vs AC-15 Distinct Fixture Requirement

These two scenarios produce the same wire shape (`found: false`) but must have SEPARATE test
fixtures:

| Test | Scenario | Internal Path |
|------|----------|---------------|
| `test_handle_path_to_id_not_in_snapshot_returns_not_found` | from_id in graph, to_id not in graph | early-exit before BFS |
| `test_context_graph_path_no_path_disconnected` (integration) | both from/to in graph, no path between them | BFS exhausts frontier |

Never combine these in one test. The snapshot-absence path is tested at the unit level
(injecting a graph without to_id). The no-path case is tested at the integration level
(real server, both entries written, no edge written between them).

**Risk**: R-09

---

### SR-C — BFS Cycle Termination

**Test**: `test_handle_path_bfs_terminates_on_cyclic_graph`
**Fixture**: Graph with cycle A→B→C→A. Target D is unreachable (not in graph).
`from_id=A, to_id=D, depth=5`.
**Assert**: BFS terminates with `{ found: false }` and does NOT loop indefinitely.
The visited set prevents revisiting A.

---

## Integration Test Expectations (AC-13/AC-14, AC-20/AC-21, AC-31)

### AC-31 — test_context_graph_path_found

**Location**: `test_tools.py`.
**Fixture**: `server` (fresh DB).

**Fixture setup**: Build a 2-hop chain A→B→C using `context_edge`:
```python
id_a = server.context_store("path-entry-A", topic="testing", category="goal", agent_id="human")
id_b = server.context_store("path-entry-B", topic="testing", category="goal", agent_id="human")
id_c = server.context_store("path-entry-C", topic="testing", category="goal", agent_id="human")
server.context_edge("add", id_a, "Advances", id_b, agent_id="human")
server.context_edge("add", id_b, "Supports", id_c, agent_id="human")
# Allow graph tick to rebuild (or trigger rebuild if supported)
```

**Note**: The path mode BFS uses the in-memory graph. The integration test must either
wait for a tick or use whatever test-mode graph-rebuild mechanism the harness provides.
Document this dependency explicitly. If the harness has no tick-force mechanism, the test
must note this as a known limitation (staleness contract is tested by AC-19 disclosure check).

**Action**:
```python
resp = server.context_graph(
    "path",
    from_id=id_a,
    to_id=id_c,
    edge_types=["Advances", "Supports"],
    depth=5,
    agent_id="human",
    format="json",
)
```

**Assertions**:
- `data["found"] == True`.
- `data["from_id"] == id_a` (NOT in hops).
- `data["to_id"] == id_c`.
- `data["hops"]` is a list of length 2.
- `data["hops"][0]["entry_id"] == id_b`, `data["hops"][0]["relation_type"] == "Advances"`.
- `data["hops"][1]["entry_id"] == id_c`, `data["hops"][1]["relation_type"] == "Supports"`.
- `id_a` is NOT in `{h["entry_id"] for h in data["hops"]}` (explicit from_id exclusion check).
- `data["length"] == 2`.

**Risks mitigated**: R-12, AC-13, AC-31

---

### AC-14 — test_context_graph_path_no_path_disconnected

**Fixture setup**: Store entry A and entry B. Add NO edge between them.

**Action**: `context_graph("path", from_id=A, to_id=B, depth=5)`.

**Assertions**:
- Response is a tool success (no error code — `found: false` is not an error).
- `data["found"] == False`.
- `data["hops"] == []`.
- `data["length"] == 0`.

**Risk**: R-09

---

### AC-20/AC-21 — test_context_graph_path_resolve_supersessions

Two tests (one for resolve=true, one for resolve=false):

**test_context_graph_path_resolve_supersessions_from_id_resolved**:
1. Store deprecated entry D, correct it to get active entry A (D superseded by A).
2. Store target entry B. Add edge A→B.
3. Call `context_graph("path", from_id=D.id, to_id=B.id, resolve_supersessions=True)`.
4. Assert: `data["from_id"] == A.id` (resolved, not D).

**test_context_graph_path_no_resolve_uses_deprecated_from_id**:
1. Same setup.
2. Call with `resolve_supersessions=False` (or omit — default false).
3. Assert: `data["from_id"] == D.id` (original deprecated ID reflected back).

**Risk**: R-06

---

## Edge Cases

- `depth=1` where path is 2 hops: BFS explores depth 1, does not find to_id.
  `test_handle_path_depth_1_misses_2hop_path` — assert `{ found: false }` not an error.
- `from_id == to_id` with resolve_supersessions=true where from_id resolves to a different
  active entry: after resolution, from_id_resolved != to_id. Should proceed as normal path
  search. `test_handle_path_self_resolves_to_different_target_proceeds_normally`.
- `follow_to_current` returns `None` (orphaned deprecated chain — 50-hop cap exceeded):
  `test_handle_path_follow_to_current_none_fallback_uses_original_id` — assert fallback to
  original ID, no panic.
