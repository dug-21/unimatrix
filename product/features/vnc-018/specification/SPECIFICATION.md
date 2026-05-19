# SPECIFICATION: vnc-018 — context_graph (chain, current, neighbors)

**Feature ID**: vnc-018
**GH Issue**: #596 (W1B-2a)
**Spec Agent**: vnc-018-agent-2-spec
**Status**: Draft

---

## Objective

Add `context_graph` as the 14th MCP tool in the Unimatrix server, exposing graph read operations that complement the existing typed edge write surface (W1B-1). This feature delivers three initial traversal modes — `chain`, `current`, and `neighbors` — enabling agents to walk supersession histories, resolve deprecated entries to their live successors, and retrieve typed-edge neighbors. It also completes the deferred `Advances`/`Motivates` PPR/BFS addition from W1B-1, and adds four schema indexes required for efficient traversal at scale.

---

## Functional Requirements

### FR-01: Tool Registration

The `context_graph` tool must be registered as the 14th `context_*` MCP tool in `tools.rs` using the `#[tool(...)]` attribute pattern. The tool handler in `tools.rs` must contain only the dispatch call; all mode logic lives in a new `mcp/graph_read.rs` module. Tool dispatch uses a `mode` parameter string, not `type`, consistent with ASS-057 Section 3 recommendation.

### FR-02: Capability Gate

All three modes require `Capability::Read`. The standard `require_cap(Capability::Read)` check must execute before any traversal logic. Failure returns the standard capability error response.

### FR-03: Mode Dispatch

`context_graph` accepts a `mode` parameter (required). Valid values are `"chain"`, `"current"`, and `"neighbors"`. An unrecognized `mode` value returns an error with the message: `"Unknown mode '{value}' — supported modes: chain, current, neighbors"`. No traversal is attempted.

### FR-04: chain mode — Supersession Chain Walk

Given an `id`, `chain` mode walks the supersession chain in both directions (or a specified direction), returning the full ordered set of entries in that chain.

- Traversal uses a SQL recursive CTE on `entries.supersedes` and `entries.superseded_by` — not the in-memory `TypedRelationGraph` and not `GRAPH_EDGES` rows.
- The `direction` parameter controls traversal: `"forward"` returns X and its descendants (entries that supersede X, toward newer); `"backward"` returns X and its ancestors (entries X supersedes, toward older); `"both"` (default) returns the union of both branches.
- Safety cap: `WHERE depth < 50` in the CTE recursive step, applied independently to each direction branch.
- When the cap fires on either branch, the response `truncated` field encodes which direction(s) were capped (see FR-11 for the `truncated` structure).
- If the requested `id` does not exist, `chain` mode returns an empty result — not an error.
- Result entries are ordered by chain position from oldest to newest.
- `resolve_supersessions` is not a valid parameter on `chain` mode; passing it returns an error: `"resolve_supersessions is not applicable to chain mode — chain IS the supersession audit"`. This mode IS the supersession audit; applying resolution within it is semantically circular.

### FR-05: current mode — Terminal-Active Lookup

Given an `id`, `current` mode follows `superseded_by` until a terminal entry is found whose `status = 'Active'`.

- Traversal uses a SQL recursive CTE on `entries.superseded_by` — not the in-memory graph.
- The terminal condition is `superseded_by IS NULL AND status = 'Active'`. An entry with `superseded_by IS NULL AND status = 'Deprecated'` is an orphaned deprecated entry (deprecated via `context_deprecate` without a successor) — this is NOT a valid terminal. The CTE must filter for `status = 'Active'` at the terminal step.
- If the input `id` has no `superseded_by` value and is already Active, returns that entry unchanged.
- If the chain depth exceeds 50 hops, returns an error: `"Supersession chain from entry {id} exceeds the 50-hop safety cap — the chain may contain a cycle or be abnormally long"`.
- If no terminal active entry is reachable — including when the chain terminates at an orphaned deprecated entry (`superseded_by IS NULL`, `status = 'Deprecated'`), or when the chain is too long, or when the starting `id` does not exist — returns an informative error: `"No active terminal found for entry {id}"` (or equivalent describing the condition).
- Returns a single `EntryRecord`.

### FR-06: neighbors mode — Typed-Edge Neighbor Retrieval

Given an `id`, edge types, direction, and depth, `neighbors` mode returns connected neighbor entries with edge metadata.

- For `depth = 1`: executes a direct SQL query against `GRAPH_EDGES` using the composite index `(source_id, relation_type)` or `(target_id, relation_type)`. This path queries the live database — all committed writes are reflected immediately.
- For `depth > 1`: performs BFS over the in-memory `TypedRelationGraph`. At each frontier node, calls `edges_of_type` for each requested type and direction. Maintains a visited set keyed by `node_id` only. Each node appears in the result at most once, at its minimum hop depth. A node reachable via multiple paths at different depths appears once at the shallowest depth — the alternative (keying by `(node_id, depth)`) would produce duplicate entries requiring agent-side deduplication, which is rejected.
- Safety cap: 50 hops per `follow_to_current` helper call when `resolve_supersessions=true`.
- Returns a flat `Vec<EdgeRecord>` (not grouped by depth). Each record carries `source_id`, `target_id`, `relation_type`, `direction` (relative to the traversal anchor), `depth`, and `metadata: None`.

### FR-07: edge_types Parameter Validation

The `edge_types` parameter on `neighbors` mode accepts a list of `RelationType` string values.

- Each string is validated via `RelationType::from_str()` before any traversal begins. An unrecognized type returns an error identifying the unknown value — no traversal occurs.
- If `Supersedes` is explicitly present in `edge_types`, reject before traversal with the error: `"Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation"`.
- If `edge_types` is absent or empty, traverse all edge types excluding `Supersedes`. This silent exclusion produces no warning in the response payload (see AC-10a).

### FR-08: resolve_supersessions Parameter

The `resolve_supersessions` parameter applies to `neighbors` mode only. Default: `false`.

- When `false`: edges are returned as stored, including deprecated endpoints without substitution.
- When `true`: at each hop, if the resolved neighbor entry is deprecated and has a `superseded_by` chain, the `follow_to_current` helper resolves it to the terminal active entry. The returned `EdgeRecord` reflects the resolved endpoint.
- `resolve_supersessions` is not a valid parameter on `chain` mode — `chain` IS the supersession audit; applying resolution within it is semantically circular. If passed to `chain`, it is rejected before any traversal with the error: `"resolve_supersessions is not applicable to chain mode — chain IS the supersession audit"`.

### FR-09: Forward-Compat Field Validation

`GraphParams` includes forward-compatibility fields for future modes (`seed_ids`, `from_id`, `to_id`, `max_nodes`). These fields are defined with real types and validated on receipt. If any of these fields are passed to a mode that does not support them, the handler returns a descriptive error before any traversal. Specific errors:

- `seed_ids` passed to `chain`, `current`, or `neighbors`: `"seed_ids is not supported in {mode} mode — use subgraph mode (coming in #597)"`.
- `from_id` or `to_id` passed to `chain`, `current`, or `neighbors`: `"from_id/to_id is not supported in {mode} mode — use path mode (coming in #598)"`.
- `max_nodes` passed to `chain`, `current`, or `neighbors`: `"max_nodes is not supported in {mode} mode — use subgraph mode (coming in #597)"`.

These fields must not be silently dropped by serde — they round-trip correctly and are validated on every request.

### FR-10: Schema Migration — Four New Indexes

The migration sequence must add four indexes atomically before any `context_graph` handler is reachable:

1. `CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes)`
2. `CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by)`
3. `CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type ON graph_edges(source_id, relation_type)`
4. `CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type ON graph_edges(target_id, relation_type)`

Indexes 3 and 4 are also used by `inverse` and `filter` modes (W1B-2c). Adding them now avoids a second migration.

### FR-11: ChainResponse Truncation Structure

The `truncated` field in a `chain` mode response must encode per-direction truncation — not a flat `bool`. The structure is:

```
truncated: {
    forward: bool,   // true if the forward (descendants) branch hit the 50-hop cap
    backward: bool,  // false if the backward (ancestors) branch did not hit the cap
}
```

When `direction="forward"`, only `truncated.forward` is meaningful. When `direction="backward"`, only `truncated.backward` is meaningful. When `direction="both"`, both fields are independently set. This structure is always present in the chain response (both fields default to `false` when the cap does not fire). A flat `bool` is insufficient because agents must be able to determine which direction was capped (AC-03b requirement).

### FR-12: Advances and Motivates PPR/BFS Addition

Add `Advances` and `Motivates` to the positive edge type sets in two files:

- `graph_ppr.rs`: `positive_out_degree_weight` and `personalized_pagerank` — add `RelationType::Advances` and `RelationType::Motivates` alongside existing positive types.
- `graph_expand.rs`: BFS positive-type filter — add both variants.

This completes the write-only deferral from W1B-1 (ADR-006 vnc-015). Approximately 16 lines across two files.

### FR-13: Behavioral Split Documentation in Tool Description

The `context_graph` tool description must include the following text (or equivalent) for the `neighbors` mode behavioral split:

> "depth=1 queries the live database and reflects all committed writes; depth>1 queries the in-memory graph, which may lag recent writes by up to one tick interval."

This asymmetry is intentional and must be documented so agents do not treat depth=2 silence after an immediate write as a bug.

### FR-14: Tool Count Protocol Test Update

`test_protocol.py` P-03 must be updated to assert exactly 14 `context_*` tools (currently asserts 13).

---

## Non-Functional Requirements

### NFR-01: Latency — chain and current modes

`chain` and `current` mode queries must execute in under 10ms for chains up to 50 hops at the database sizes in scope (up to ~30k entries), provided the four schema indexes from FR-10 are present. Without the indexes, per-hop full scans degrade to O(N) — acceptable at 3k entries (<2ms per 10-hop chain) but required at scale.

### NFR-02: Latency — neighbors depth=1

`neighbors` at `depth=1` uses the composite GRAPH_EDGES index and must complete in under 5ms for entries with up to 500 edges in scope.

### NFR-03: Latency — neighbors depth>1

`neighbors` at `depth>1` uses in-memory BFS. The tick-window staleness (edges written within the last tick may not appear) is an accepted behavioral constraint, not a defect. No latency SLA beyond the existing 50ms MCP hot path budget.

### NFR-04: Safety Cap

The 50-hop safety cap applies to:
- Supersession chain depth in `chain` mode (CTE `WHERE depth < 50`).
- Supersession chain depth in `current` mode (CTE `WHERE depth < 50`).
- `follow_to_current` helper depth in `neighbors` mode when `resolve_supersessions=true`.

The cap must be enforced at the SQL CTE level for `chain`/`current` modes — not as loop-based Rust code.

### NFR-05: Module Size Constraint

All `context_graph` logic (mode dispatch, `handle_chain`, `handle_current`, `handle_neighbors`, `follow_to_current`) goes in a new `mcp/graph_read.rs` module. The 500-line per-file limit applies to this new module. `tools.rs` contains only the `#[tool]` annotated handler function.

### NFR-06: Memory

`neighbors` depth>1 BFS maintains a visited set to prevent re-expansion. The 50-hop cap on `follow_to_current` bounds supersession resolution per node. No explicit memory cap on result set size for vnc-018 modes (bounded by graph density and depth limit).

### NFR-07: Compatibility

- `RelationEdge` in the in-memory graph does not carry `metadata`. `EdgeRecord.metadata` is always `None` in vnc-018.
- `EdgeRecord` fields defined now must not change in W1B-2b or W1B-2c without a breaking-change ADR.
- `GraphParams` forward-compat fields (`seed_ids`, `from_id`, `to_id`, `max_nodes`) must not be silently dropped by serde.

---

## Acceptance Criteria

### chain mode

**AC-01** — Full chain retrieval
`context_graph(mode="chain", id=X)` returns all entries in the supersession chain containing X, ordered from oldest (earliest ancestor) to newest (latest descendant). The result includes both ancestors and descendants of X.
_Verification_: Integration test — create a 5-entry supersession chain (A→B→C→D→E); call chain with id=C; assert all 5 entries returned in order A, B, C, D, E.

**AC-02** — Directional filtering
`context_graph(mode="chain", id=X, direction="forward")` returns only X and its descendants. `direction="backward"` returns only X and its ancestors.
_Verification_: Using the same 5-entry chain, call with direction="forward" from C; assert result contains C, D, E only. Call with direction="backward" from C; assert result contains A, B, C only.

**AC-03** — Truncation on cap fire (either direction)
`context_graph(mode="chain", id=X)` on a chain of length > 50 hops returns at most 50 entries from the seed outward in each direction. `truncated.forward` and/or `truncated.backward` is `true` when the respective cap fires.
_Verification_: Unit test with synthetic 60-entry chain; assert result length <= 50 per direction and `truncated.forward=true`.

**AC-03b** — Per-direction truncation distinguishability (SR-05 resolution)
`context_graph(mode="chain", id=X, direction="both")` where only the forward branch hits the 50-hop cap returns `truncated.forward=true` and `truncated.backward=false`. The backward branch (under cap) is returned in full.
_Verification_: Unit test — construct a chain where the seed has 55 forward hops and 3 backward hops. Assert `truncated.forward=true`, `truncated.backward=false`, and the backward branch contains all 3 ancestors.

**AC-04** — Non-existent ID returns empty
`context_graph(mode="chain", id=X)` where X does not exist in the entries table returns an empty result list, not an error.
_Verification_: Integration test — call with id=999999 (absent); assert empty list, no error code.

### current mode

**AC-05** — Active entry returns itself
`context_graph(mode="current", id=X)` where X is an Active entry with no `superseded_by` value returns X.
_Verification_: Integration test — store a new entry; call current with its id; assert same entry returned.

**AC-05a** — Non-existent ID returns error
`context_graph(mode="current", id=999999)` where 999999 does not exist returns an informative error ("No active terminal found for entry 999999" or equivalent) — it does NOT return an empty result. This is intentionally asymmetric with `chain` mode (AC-04 returns empty for a non-existent ID): asking for the current version of a non-existent entry is an error; asking for the chain of a non-existent entry is an empty traversal. See Constraints for a note on this behavioral asymmetry.
_Verification_: Integration test — call with id=999999 (absent); assert error response (not empty result); assert the error message identifies the entry ID.

**AC-06** — Deprecated entry resolves to terminal active entry
`context_graph(mode="current", id=X)` where X is deprecated follows `superseded_by` to the terminal entry and returns it only if `status = 'Active'`. An entry with `superseded_by IS NULL` but `status = 'Deprecated'` (orphaned deprecated) is NOT a valid terminal and must not be returned.
_Verification_: Integration test — create chain A (deprecated) → B (deprecated) → C (active); call current with id=A; assert entry C is returned and C.status is Active.

**AC-06b** — Orphaned deprecated terminal returns error
`context_graph(mode="current", id=X)` where X's supersession chain terminates at an orphaned deprecated entry (`superseded_by IS NULL`, `status = 'Deprecated'`) returns an error ("No active terminal found" or equivalent) — it does NOT return the deprecated entry.
_Verification_: Integration test — create entry D; deprecate D via `context_deprecate` (no successor, so `superseded_by` remains NULL); call current with id=D; assert error response; assert D is not returned as `entry`.

**AC-07** — Safety cap returns error
`context_graph(mode="current", id=X)` where the `superseded_by` chain exceeds 50 hops returns an error response indicating the chain is too long, not a partial result.
_Verification_: Unit test — mock a 55-hop superseded_by chain; assert error response contains "50-hop safety cap".

### neighbors mode

**AC-08** — Outgoing typed neighbor retrieval
`context_graph(mode="neighbors", id=X, edge_types=["Prerequisite"], direction="outgoing", depth=1)` returns all entries with a Prerequisite edge from X.
_Verification_: Integration test — write Prerequisite edges from X to Y and Z; call neighbors; assert both Y and Z appear in EdgeRecord list with `direction="outgoing"`.

**AC-09** — Incoming typed neighbor retrieval
`context_graph(mode="neighbors", id=X, edge_types=["Supports"], direction="incoming", depth=1)` returns all entries with a Supports edge pointing at X.
_Verification_: Integration test — write Supports edges from Y→X and Z→X; call neighbors with direction="incoming"; assert Y and Z appear.

**AC-10** — All-types expansion (empty edge_types)
`context_graph(mode="neighbors", id=X, edge_types=[], direction="both", depth=1)` returns all neighbors across all edge types in both directions. `Supersedes` edges are excluded without warning.
_Verification_: Integration test — write edges of type Supports, Informs, and Supersedes from X to different targets; call with edge_types=[]; assert Supports and Informs neighbors returned; assert Supersedes target absent; assert no `excluded_types` or warning field in response.

**AC-10a** — Supersedes silent exclusion produces no warning
When `edge_types` is absent or empty and the graph contains Supersedes edges from the anchor, the response payload includes no warning, no `excluded_types` field, and no indication that exclusion occurred. This is the specified behavior (OQ-03 resolution, OQ-06 resolution).
_Verification_: Inspect the response object from AC-10; confirm no top-level `excluded_types`, `warnings`, or similar field is present.

**AC-11** — Multi-hop BFS with depth field
`context_graph(mode="neighbors", id=X, depth=2)` returns both direct neighbors (depth=1) and their neighbors (depth=2). Each `EdgeRecord` carries the correct `depth` value.
_Verification_: Integration test — create chain X→Y→Z via typed edges; call neighbors depth=2; assert Y has depth=1 and Z has depth=2 in the returned records.

**AC-11a** — BFS visited set deduplication (node_id keying)
A node reachable at both depth=1 and depth=2 (via two different paths) appears exactly once in the result, at depth=1. The visited set is keyed by `node_id` only — not by `(node_id, depth)`.
_Verification_: Unit test — create graph where X→Y (direct) and X→Z→Y (two-hop); call neighbors depth=2; assert Y appears exactly once in `edges` with depth=1; assert no duplicate entries for Y.

**AC-12** — resolve_supersessions=true substitutes deprecated endpoints
`context_graph(mode="neighbors", id=X, ..., resolve_supersessions=true)` substitutes deprecated neighbor entries with their terminal active successors.
_Verification_: Integration test — write edge X→Y; correct Y→Z; call neighbors with resolve_supersessions=true; assert Z appears in result, not Y.

**AC-13** — resolve_supersessions=false returns raw edges
`context_graph(mode="neighbors", ..., resolve_supersessions=false)` returns edges as stored, including deprecated endpoints.
_Verification_: Using same setup as AC-12; call with resolve_supersessions=false; assert Y appears in result (the deprecated original target).

**AC-14** — Unrecognized mode returns error
`context_graph` with an unrecognized `mode` value returns an error message listing supported modes.
_Verification_: Call with mode="walk"; assert error response contains "chain, current, neighbors".

**AC-15** — Unknown edge type rejects before traversal
`context_graph(mode="neighbors", edge_types=["UnknownType"])` returns an error before any traversal.
_Verification_: Call with edge_types=["BogusEdge"]; assert error response; assert no edges retrieved.

**AC-15a** — Supersedes explicit rejection
`context_graph(mode="neighbors", edge_types=["Supersedes"])` returns an error with the exact message: `"Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation"`.
_Verification_: Call with edge_types=["Supersedes"]; assert exact error message string.

**AC-15b** — Forward-compat field error-on-misuse (SR-03 resolution)
Passing `seed_ids`, `from_id`, `to_id`, or `max_nodes` to `chain`, `current`, or `neighbors` mode returns a descriptive error before traversal. Each misuse produces a distinct error message identifying the field and the correct future mode.
_Verification_: Four unit tests — one per field — passed to `neighbors` mode; assert each returns an error containing the field name and the future mode name.

**AC-15c** — resolve_supersessions rejected on chain mode
`context_graph(mode="chain", ..., resolve_supersessions=true)` returns an error: `"resolve_supersessions is not applicable to chain mode — chain IS the supersession audit"`.
_Verification_: Call chain mode with resolve_supersessions=true; assert exact error string.

### Protocol and schema

**AC-16** — Protocol test asserts 14 tools
`test_protocol.py` P-03 asserts exactly 14 `context_*` tools (updated from 13).
_Verification_: P-03 passes in the infra-001 test suite after `context_graph` registration.

**AC-17** — Advances and Motivates in PPR positive types
`Advances` and `Motivates` participate in PPR expansion (`personalized_pagerank` and `positive_out_degree_weight` in `graph_ppr.rs`). A unit test confirms both variants appear in the positive-type set returned by the relevant function.
_Verification_: Unit test enumerates positive types from `graph_ppr.rs`; asserts `Advances` and `Motivates` are present.

**AC-18** — Advances and Motivates in BFS expansion
`Advances` and `Motivates` participate in positive BFS graph expansion (`graph_expand.rs`). A unit test confirms both types are traversed in the expansion.
_Verification_: Unit test with entries connected via `Advances` and `Motivates` edges; run BFS expansion; assert both are followed.

**AC-19** — Four new indexes present after migration
After migration, all four indexes exist: `idx_entries_supersedes`, `idx_entries_superseded_by`, `idx_graph_edges_source_type`, `idx_graph_edges_target_type`. A migration test confirms their presence via `sqlite_master` query.
_Verification_: Existing migration test suite extended to assert all four index names.

**AC-20** — Integration test coverage for all three modes
All three modes (`chain`, `current`, `neighbors`) are covered by at least one integration test in the infra-001 Python suite.
_Verification_: `product/test/infra-001/` suite passes with test functions exercising each mode.

---

## Domain Models

### GraphParams

The request parameter struct for `context_graph`. All fields except `mode` are optional.

| Field | Type | Modes | Description |
|-------|------|-------|-------------|
| `mode` | `String` | all | Required. `"chain"`, `"current"`, or `"neighbors"`. |
| `agent_id` | `Option<String>` | all | Caller identity for audit attribution. |
| `format` | `Option<String>` | all | Response format hint (e.g., `"markdown"`). |
| `id` | `Option<u64>` | chain, current, neighbors | Anchor entry ID. Required for all three current modes. |
| `direction` | `Option<String>` | chain, neighbors | `"forward"`, `"backward"`, or `"both"`. Default for chain: `"both"`. |
| `edge_types` | `Option<Vec<String>>` | neighbors | Edge type filter. Absent or empty = all types excluding Supersedes. |
| `depth` | `Option<u8>` | neighbors | BFS depth `1..=10`. Default: `1`. |
| `resolve_supersessions` | `Option<bool>` | neighbors only | Substitute deprecated endpoints with live successors. Default: `false`. Passing to `chain` mode returns an error. |
| `seed_ids` | `Option<Vec<u64>>` | forward-compat only | Subgraph mode (#597). Error if passed to current modes. |
| `from_id` | `Option<u64>` | forward-compat only | Path mode source (#598). Error if passed to current modes. |
| `to_id` | `Option<u64>` | forward-compat only | Path mode target (#598). Error if passed to current modes. |
| `max_nodes` | `Option<u32>` | forward-compat only | Subgraph cap (#597). Default 200. Error if passed to current modes. |

### EdgeRecord

The atomic element of the `neighbors` mode response. Defined now so W1B-2b (`subgraph` mode) can reuse without a breaking type change.

| Field | Type | Description |
|-------|------|-------------|
| `source_id` | `u64` | Source entry ID of the edge. |
| `target_id` | `u64` | Target entry ID of the edge. |
| `relation_type` | `String` | String representation of `RelationType`. |
| `direction` | `String` | `"incoming"` or `"outgoing"` relative to the traversal anchor. |
| `depth` | `u8` | Hop depth from the anchor at which this edge was found. |
| `metadata` | `Option<serde_json::Value>` | Always `None` in vnc-018. Populated by W1B-2b when `RelationEdge` gains metadata. |

### ChainResponse

The response envelope for `chain` mode.

| Field | Type | Description |
|-------|------|-------------|
| `entries` | `Vec<EntryRecord>` | Ordered chain from oldest ancestor to newest descendant. |
| `truncated` | `TruncationStatus` | Per-direction cap-fire indicator (see below). |

### TruncationStatus

| Field | Type | Description |
|-------|------|-------------|
| `forward` | `bool` | `true` if the forward (descendants) branch hit the 50-hop cap. |
| `backward` | `bool` | `true` if the backward (ancestors) branch hit the 50-hop cap. |

Both fields default to `false`. Always present in the chain response.

### CurrentResponse

| Field | Type | Description |
|-------|------|-------------|
| `entry` | `EntryRecord` | The terminal active entry at the end of the superseded_by chain. |

### NeighborsResponse

| Field | Type | Description |
|-------|------|-------------|
| `edges` | `Vec<EdgeRecord>` | Flat list of neighbor edges. Each item carries depth, direction, and type. |

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **chain** | The ordered sequence of entries connected by supersession relationships (supersedes/superseded_by), from oldest ancestor to newest descendant. |
| **current** | The terminal active entry at the end of a superseded_by chain — the live, non-deprecated successor. |
| **neighbors** | Entries directly connected to an anchor entry via typed graph edges in GRAPH_EDGES. |
| **supersession** | The act of one entry correcting and replacing another, recorded via `entries.supersedes` and `entries.superseded_by` fields. |
| **truncation** | Halting of chain traversal when the 50-hop safety cap fires. Encoded per direction in `TruncationStatus`. |
| **resolve_supersessions** | A `neighbors` mode flag that substitutes deprecated neighbor endpoints with their terminal active successors via `follow_to_current`. |
| **edge_type** | A string representation of a `RelationType` variant (e.g., `"Supports"`, `"Prerequisite"`). All 16 current variants are valid except `Supersedes` in neighbors mode. |
| **direction** | In `chain` mode: `forward` = descendants (toward newer), `backward` = ancestors (toward older). In `neighbors` mode: `outgoing` = edges from the anchor, `incoming` = edges pointing at the anchor. |
| **depth** | In `neighbors` mode: the hop count from the anchor entry at which a neighbor was found. Depth 1 = direct neighbor. |
| **orphaned deprecated entry** | An entry with `status = 'Deprecated'` and `superseded_by IS NULL` — deprecated via `context_deprecate` without a successor. Such entries have no active terminal and are NOT valid `current` mode results. `current` mode returns an error when the chain terminates at one. |
| **follow_to_current** | A Store-layer helper (~20 lines) that follows `superseded_by` links up to 50 hops, returning the terminal active entry ID or `None` on cap. |
| **BFS** | Breadth-first search over the in-memory `TypedRelationGraph`, used for `neighbors` mode at `depth > 1`. |
| **tick-staleness window** | The interval between in-memory graph rebuilds during which recently written GRAPH_EDGES rows may not appear in BFS results. Affects `neighbors` depth > 1 only. |
| **forward-compat field** | A typed field in `GraphParams` defined for a future mode and validated to error on misuse in the current modes. |

---

## User Workflows

### Workflow 1: Audit an Entry's Correction History

An agent holds entry ID `X` and wants to see its full supersession chain — all prior versions and all corrections that followed.

1. Call `context_graph(mode="chain", id=X)` (direction defaults to `"both"`).
2. Receive `ChainResponse` with ordered entries from oldest ancestor to newest descendant.
3. Check `truncated.forward` and `truncated.backward` — if either is `true`, the chain was capped at 50 hops; the agent knows the audit is incomplete.
4. Use the ordered list to inspect the full correction history.

### Workflow 2: Resolve a Deprecated Entry to Its Live Successor

An agent holds a stale reference to entry `X` (deprecated). It needs the live current version.

1. Call `context_graph(mode="current", id=X)`.
2. Receive `CurrentResponse` with the terminal active `EntryRecord`.
3. If X is already active, the same entry is returned — the call is idempotent.

### Workflow 3: Navigate Typed Relationships

An agent wants to find all entries that a Decision entry depends on (its `Prerequisite` edges).

1. Call `context_graph(mode="neighbors", id=X, edge_types=["Prerequisite"], direction="outgoing", depth=1)`.
2. Receive `NeighborsResponse` with `EdgeRecord` list.
3. Each record identifies a prerequisite entry with `relation_type="Prerequisite"` and `direction="outgoing"`.

### Workflow 4: Multi-Hop Relationship Traversal

An agent wants all entries reachable within 2 hops via `Supports` and `Informs` edges.

1. Call `context_graph(mode="neighbors", id=X, edge_types=["Supports", "Informs"], direction="both", depth=2)`.
2. Receive flat `NeighborsResponse` — records with `depth=1` are direct neighbors; records with `depth=2` are second-hop.
3. Note: depth>1 uses the in-memory graph; edges written within the last tick interval may not appear.

### Workflow 5: Research Domain ADR Chain Traversal

A research agent holds a Thesis entry and wants to navigate to all supporting Claims and then their referenced Findings.

1. Call `context_graph(mode="neighbors", id=thesis_id, edge_types=["Asserts"], direction="incoming", depth=1)` to get all Claims asserting this Thesis.
2. For each returned Claim entry ID, call `context_graph(mode="neighbors", id=claim_id, edge_types=["DerivedFrom"], direction="outgoing", depth=1)` to get the Findings.
3. Alternatively, combine into a single depth=2 BFS: `context_graph(mode="neighbors", id=thesis_id, edge_types=["Asserts", "DerivedFrom"], direction="both", depth=2)`.

---

## Constraints

### Implementation Constraints

1. **SQL CTE mandatory for chain/current**: `chain` and `current` modes must use SQL recursive CTEs on `entries.supersedes`/`entries.superseded_by`. Using the in-memory `TypedRelationGraph` or `find_terminal_active()` for these modes is prohibited. `find_terminal_active` requires a read lock on the tick-rebuilt cache, fails silently on cold-start, and returns stale results within the tick window. (Ref: pattern #4468, ADR-001 vnc-017 #4460.)
2. **Module constraint**: All traversal logic in `mcp/graph_read.rs`. `tools.rs` contains only the `#[tool]` dispatch point. The 500-line limit applies to `graph_read.rs`.
3. **Capability gate**: `Capability::Read` check runs first (in `tools.rs`) before the handler is entered. Validation of unsupported parameters (forward-compat fields, `resolve_supersessions` on `chain`) occurs inside the handler, before mode dispatch. The security model is: capability check first, then parameter validation, then traversal.
4. **Pool accessor**: Read operations use `read_pool()` per C-07 (vnc-017). Write pool is never used in `context_graph`.
5. **No new RelationType variants**: All 16 current variants are registered. `graph_read.rs` must not add new variants.
6. **Supersedes exclusion**: `Supersedes` excluded from neighbors mode — both silently from "all types" default expansion (mirrors `query_incoming_edges` in vnc-017) and explicitly rejected with the defined error message when specified.
7. **EdgeRecord.metadata is always None**: Until W1B-2b extends `RelationEdge`, `metadata` must be `None`. It is defined now only for forward-compat wire stability.
8. **tools.rs wiring**: Every call from `tools.rs` to `graph_read.rs` must use a fully-qualified module path. (Ref: pattern #4436.)

### Safety Constraints

1. **50-hop cap**: Applied at the SQL CTE `WHERE depth < 50` for chain/current modes. Applied in the `follow_to_current` loop for neighbors mode. This cap was established in ASS-057.
2. **depth parameter range**: `depth` on `neighbors` accepts `1..=10`. Values outside this range return a validation error before traversal.
3. **Forward-compat fields are never silently dropped**: serde must not silently discard `seed_ids`, `from_id`, `to_id`, or `max_nodes`.
4. **chain/current non-existent ID asymmetry**: `chain` mode returns an empty result for a non-existent ID (AC-04); `current` mode returns an error (AC-05a). This asymmetry is intentional: requesting the chain of a non-existent entry is an empty traversal (vacuously correct); requesting the current version of a non-existent entry is a lookup failure that should surface to the caller. Implementations must not silently unify these behaviors.

---

## Dependencies

### Hard Dependencies (must be merged before delivery)

| Dependency | Feature | Status |
|------------|---------|--------|
| W1B-1 Typed Edge Write Path | vnc-015, PR #600 | Must be merged |
| vnc-017 auto-redirect | feature/vnc-017 (current branch) | Must be merged to main first — delivery branches from post-vnc-017 state |

The vnc-017 merge is a hard gate-0 for delivery. The codebase state after vnc-017 provides: updated `graph.rs` with 16 `RelationType` variants, `edge_write.rs`, `query_incoming_edges`, and the post-vnc-017 `tools.rs`. Delivery on a pre-vnc-017 branch produces wrong base code. (Ref: SR-08.)

### Crates and Libraries

| Dependency | Source | Usage |
|------------|--------|-------|
| `sqlx` | Workspace | SQL recursive CTEs for chain/current; composite index queries for neighbors depth=1 |
| `petgraph` | Workspace (via unimatrix-vector/core) | `TypedRelationGraph` BFS for neighbors depth>1 |
| `serde_json` | Workspace | `EdgeRecord.metadata: Option<serde_json::Value>` |
| `rmcp 0.16` | Workspace | `#[tool]` attribute and MCP dispatch |
| `tracing` | Workspace | `tracing::warn!` for unrecognized relation_type strings in BFS |

### Existing Components

| Component | File | Usage |
|-----------|------|-------|
| `TypedRelationGraph` | `graph.rs` | BFS for neighbors depth>1 — `edges_of_type()`, `node_index_for(id: u64) -> Option<NodeIndex>` (pub accessor added in this feature; R-07 resolved) |
| `find_terminal_active` | `graph.rs:523` | Reference only — NOT used by chain/current modes (SQL CTE preferred). Used conceptually by `follow_to_current` helper |
| `query_incoming_edges` | `edge_write.rs` (vnc-017) | Pattern reference for Supersedes exclusion from traversal |
| `McpServerImpl` | `tools.rs` | `#[tool]` handler registration |
| `require_cap` | `tools.rs` / service layer | Capability gate |
| `read_pool()` | `db.rs:294` | Read accessor for SQL queries |
| `graph_ppr.rs` | `graph_ppr.rs` | Modified for FR-12 (Advances/Motivates addition) |
| `graph_expand.rs` | `graph_expand.rs` | Modified for FR-12 (Advances/Motivates addition) |
| infra-001 test suite | `product/test/infra-001/` | Extended for AC-20 integration tests |

### In-Memory Graph Tick Staleness

The `TypedRelationGraph` is tick-rebuilt. Edges written within the last tick interval may not appear in depth>1 BFS results. This staleness window is a documented behavioral constraint (OQ-B-4 from ASS-057), not a defect. It affects `neighbors` depth>1 only — `chain`/`current` modes use SQL CTEs and are not affected.

---

## NOT In Scope

The following are explicitly excluded from vnc-018 to prevent scope creep:

1. **`subgraph` mode** — multi-seed BFS returning node and edge sets with 200-node cap. W1B-2b (#597).
2. **`inverse` mode** — antijoin: entries with no incoming edges of a given type. W1B-2c (#598).
3. **`path` mode** — shortest path between two entries. W1B-2c (#598).
4. **`filter` mode** — property and edge-count filter. W1B-2c (#598).
5. **`metadata` field population on EdgeRecord** — `RelationEdge` does not carry metadata in the current in-memory graph. W1B-2b extends `RelationEdge`.
6. **New `RelationType` variants** — W1B-1 added all 10 new variants. No additions in this feature.
7. **`resolve_supersessions` on chain mode** — it is semantically circular (chain IS the supersession query).
8. **`revision_reason` accessibility via supersession chain** — `GRAPH_EDGES` Supersedes rows are skip-loaded; `revision_reason` is accessible via direct SQL only. Not addressed here.
9. **`context_batch_write`** — HNSW atomicity open question; out of roadmap scope.
10. **Research domain configuration** (`research-domain.toml`, category provisioning) — separate feature.
11. **NLI `contradicts_category_pairs` scoping** — Wave 3 intelligence enhancement.
12. **`excluded_types` response field** — Silent Supersedes exclusion in "all types" expansion produces no warning and no `excluded_types` field in the response (SR-04 considered and rejected; OQ-06 resolution stands).

---

## Open Questions

**OQ-01 (OPEN for architect)**: Should `neighbors` mode return an error when `id` does not exist in the entries table, or return an empty `NeighborsResponse`? The `chain` mode spec (AC-04) returns empty for a non-existent ID. Consistency suggests `neighbors` should also return empty, but an explicit error aids debuggability. Recommend: return empty with no error (consistent with chain mode), but architect should confirm.

**OQ-02 (OPEN for architect)**: The `depth` upper bound is specified as `1..=10` in `GraphParams`. The SCOPE.md does not explicitly state 10 as the upper bound — it only states the default is 1. Should the upper bound be validated (error if depth > 10) or allowed up to the 50-hop cap? Recommend: validate to `1..=10` with a clear error message, to prevent inadvertently large BFS traversals.

**OQ-03 (RESOLVED — from SCOPE.md OQ-01 through OQ-06)**: All six SCOPE.md open questions are resolved. See SCOPE.md for details.

---

## Knowledge Stewardship

- Queried (initial pass): `mcp__unimatrix__context_briefing` — returned pattern #4468 (SQL CTE mandatory for supersession traversal), ADR-001 vnc-017 #4460 (terminal-active resolution), ADR-006 vnc-015 #4429 (PPR positive type deferral of Advances/Motivates), lesson #3953 (spec FR type model overrides architect type choice). All four were directly applicable to resolving SR-05 (truncation structure), SR-03 (forward-compat validation), and the PPR/BFS requirements.
- Queried (amendment pass — vnc-018-agent-2b-spec): `mcp__unimatrix__context_briefing` — no new entries beyond initial pass. Six design-review findings applied: orphaned deprecated entry defect in `current` mode (FR-05, AC-06, AC-06b); BFS visited-set keying by node_id (FR-06, AC-11a); `resolve_supersessions` explicit rejection on `chain` (FR-04, FR-08, AC-15c confirmed); `current` mode non-existent ID error contract (AC-05a, asymmetry constraint); validation ordering wording correction (Constraints); `node_index_for` accessor R-07 resolved (Dependencies).
