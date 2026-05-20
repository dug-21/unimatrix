## ADR-005: path Mode Response Format — from_id Top-Level, hops Array, No null relation_type

### Context

path mode returns the shortest typed-edge path between two entries. The response must
convey: whether a path was found, the start and end entry IDs, the sequence of entries
traversed (excluding start), the edge type used at each hop, and the total hop count.

Three structural options were evaluated for the node sequence:

- **Option A**: Flat array including start node.
  `hops: [{ entry_id: 123, relation_type: null }, { entry_id: 456, relation_type: "Advances" }, ...]`
  The first element (start node) has `relation_type: null` because no edge was traversed
  to reach it. `null` in the hops array is ambiguous — implementers and agents must
  special-case the first element.

- **Option B**: start node as a top-level field, remaining nodes in hops array.
  `from_id: 123, hops: [{ entry_id: 456, relation_type: "Advances" }]`
  Each `PathHop` describes "the entry arrived at AND the edge traversed to reach it."
  No null relation_type; no special-case first element. `length = hops.len()`.
  Agents reconstruct the full node sequence as `[from_id] + hops.map(h -> h.entry_id)`.

- **Option C**: Separate nodes and edges arrays (same as subgraph mode).
  Adds structural overhead and requires joining for sequence reconstruction.
  Subgraph mode's format is designed for topology exploration, not linear path display.

`to_id` is mirrored as a top-level response field (alongside `from_id`) to confirm the
resolved endpoint IDs when `resolve_supersessions=true` changes them from the caller's
input values.

### Decision

Option B: `PathResponse { found: bool, from_id: u64, to_id: u64, hops: Vec<PathHop>, length: u8 }`
where `PathHop { entry_id: u64, relation_type: String }`.

`from_id` and `to_id` are top-level fields reflecting resolved IDs (post
`follow_to_current` when `resolve_supersessions=true`). They match the caller's input
when `resolve_supersessions=false`.

`hops` contains only the traversed steps — not the start node. Each hop = "arrived at
`entry_id` via `relation_type` edge." `relation_type` is always a non-null, non-empty
string (no special-casing needed for the first element). `length = hops.len()`.

`found: false` response: `{ found: false, from_id: N, to_id: M, hops: [], length: 0 }`.
Used for both "no path found within depth" and "endpoint not in current graph snapshot"
(AC-14, AC-15). The `found` field is the unambiguous discriminant.

Example for path A→B→C via Advances→Supports:
```json
{
  "found": true,
  "from_id": 100,
  "to_id": 300,
  "hops": [
    { "entry_id": 200, "relation_type": "Advances" },
    { "entry_id": 300, "relation_type": "Supports" }
  ],
  "length": 2
}
```

### Consequences

Easier: No null relation_type in any hop. No special-case first element for parsers.
`found` bool is unambiguous. `from_id`/`to_id` in response confirms resolved endpoints.
Agents reconstruct full sequence trivially.

Harder: Agents must understand that `from_id` is not in `hops` — the tool description
must state this explicitly. The `length` field is redundant with `hops.len()` but
eliminates an agent-side computation and was requested in SCOPE.md.
