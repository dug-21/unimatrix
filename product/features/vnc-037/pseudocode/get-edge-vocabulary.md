# Component: get-edge-vocabulary

## Purpose

Define the thin **discovery-list** edge vocabulary surfaced on `context_get`: `GetEdge`,
`EdgeTotals`, `EdgesView`. The per-edge payload is **EXACTLY** the 5 fields — a deliberate
projection of `context_graph`'s `EdgeRecord` with fields dropped, not added. No enrichment field
may be introduced without a new ADR (ADR-002 guardrail, FR-4/FR-15, C-5).

## Location

`crates/unimatrix-server/src/mcp/response/edges.rs` (new — pre-authorized OQ-B). Keeps
`entries.rs` under 500 lines. Holds the types here; the 3-format render helpers live in
serializer-seam (also `edges.rs`) — see that file.

## Types (binding — match OVERVIEW.md exactly)

```
struct GetEdge {
    edge_type:    String           // = RawEdgeRow.relation_type (renamed for the get vocabulary)
    direction:    &'static str     // "inbound" | "outbound" | "both"   ("both" ⇒ ↔ render)
    target_id:    u64              // the OTHER endpoint — entry point back into context_graph
    target_title: Option<String>   // None when target unresolved (dangling — retained, DNB-1)
    authored:     bool             // RawEdgeRow.source == "agent"
}
// EXACTLY these 5 fields. NO source_id, depth, metadata, source string, weight, target_confidence.

struct EdgeTotals { inbound: usize, outbound: usize }   // uncapped, ↔ once (= EdgeCountSplit)

struct EdgesView  { edges: Vec<GetEdge> /* len ≤ GET_EDGE_DISPLAY_LIMIT */, totals: EdgeTotals }
```

## Direction values (the D-02 fix / ADR-007)

- `"both"` — a canonicalized symmetric type (`Contradicts`/`CoAccess`/`Informs`); renders `↔`;
  carries **no** `→`/`←`.
- `"outbound"` — asymmetric, anchor is `source_id`; renders `→`.
- `"inbound"` — asymmetric, anchor is `target_id`; renders `←`.

These are `&'static str` (not an enum) to keep the JSON serialization trivial and aligned with
`EdgeRecord`'s string directions. The `"both"` value is the get-only addition that MUST NOT leak
into the neighbors contract (FR-15/SR-06).

## Construction

These are plain data structs (no behavior). They are built by get-edge-assembly from the ranked
`RawEdgeRow`s + the title map + the `EdgeCountSplit`. The projection rule (in get-edge-assembly):

```
GetEdge {
    edge_type:    row.relation_type
    direction:    row.direction_hint           // "both"/"outbound"/"inbound" from the ranked SQL
    target_id:    row.target_id                // the other endpoint
    target_title: title_map.get(&row.target_id).cloned()   // Option — None ⇒ dangling
    authored:     row.source == EDGE_SOURCE_AGENT           // exact match (see below)
}
```

> Use the existing `EDGE_SOURCE_AGENT` constant (re-exported from `unimatrix-store`,
> `lib.rs:51-52` neighbourhood) for the `"agent"` comparison — exact-match, no case/whitespace
> fuzz (R-09). Do NOT inline the literal `"agent"`.

## Data Flow

- **Inputs**: ranked `RawEdgeRow`s (+ direction hint), `HashMap<u64,String>` title map,
  `EdgeCountSplit`.
- **Outputs**: `EdgesView` passed by reference (`Option<&EdgesView>`) into `format_single_entry`.

## Error Handling

None at the type level — pure data. `target_confidence` is intentionally **absent** from `GetEdge`
(it was consumed by the SQL `ORDER BY` and dropped at projection — enrichment forbidden).

## Key Test Scenarios

- **exact 5-field shape (AC-02, FR-4)** — a serialized `GetEdge` has exactly
  `{edge_type, direction, target_id, target_title, authored}` and nothing else. No `source_id`,
  `depth`, `metadata`, `source`, `weight`, `target_confidence`.
- **direction values (R-10)** — `"both"` for a canonicalized symmetric edge (no `→`/`←`);
  `"outbound"`/`"inbound"` for asymmetric, with `target_id` = the other endpoint (never the anchor).
- **authored exact-match (R-09)** — `source == "agent"` ⇒ `authored=true`; any other live source
  ⇒ `false`; near-miss strings do not flip true.
- **projection fidelity to EdgeRecord (AC-09, FR-15)** — `edge_type`/`target_id`/direction semantics
  align with `EdgeRecord`'s `relation_type`/`target_id`/`incoming|outgoing`; `"both"` is the
  documented get-only addition.
