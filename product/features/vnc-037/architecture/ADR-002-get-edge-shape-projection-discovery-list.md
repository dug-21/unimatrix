## ADR-002 (vnc-037): The get-edge shape is a thin projection of `EdgeRecord` — a discovery list, not a detail view

### Context

`context_graph` neighbors mode returns `EdgeRecord`
(`mcp/graph_read.rs:135`): `{ source_id, target_id, relation_type, direction, depth,
metadata }`. SCOPE D-02/AC-09 require the get-edge vocabulary to **align** with
`EdgeRecord` (same `relation_type`/`target_id`/`direction` semantics) so the system does
not grow two inconsistent edge shapes (SR-06).

A human guardrail was set at scope approval: **`context_get` edges are a DISCOVERY LIST,
not a detail view.** The per-edge payload must be exactly enough for an agent to decide
whether to go read a related entry — nothing more. It must NOT be enriched with metadata,
weights, depth, the raw `source` string, or `source_id`. Full edge detail and multi-hop
traversal remain the job of `context_graph`.

**Reinforced under the next-hop reframe (cap 10 → 3).** With only **3 display slots**
(D-05) the discovery-vs-detail boundary matters *more*, not less. The list's entire job is
"which one should I open next?" — three slots leave no room for enrichment that does not
serve that single decision. The cap does not change the per-edge shape; it raises the cost
of any deviation from it, so the no-enrichment rule below is a harder invariant now than at
cap-10. (The `direction` field gains a third value, `↔`, for canonicalized symmetric edges
— see D-10 / ADR-007 — but no new *field* is added.)

### Decision

The get-edge shape is a deliberate **projection** of `EdgeRecord` — fields are *dropped*,
two reader-facing fields are *added*, and **no enrichment field may be introduced**:

```
GetEdge {
  edge_type:    String,                    // = EdgeRecord.relation_type (renamed for the get vocabulary)
  direction:    "inbound"|"outbound"|"both", // = EdgeRecord.direction; "both" (↔) for canonicalized
                                           //   symmetric types (Contradicts/CoAccess/Informs), D-10/ADR-007;
                                           //   "inbound"/"outbound" (→/←) only for asymmetric types
  target_id:    u64,                       // = the OTHER endpoint (entry point back into context_graph)
  target_title: Option<String>,            // ADDED: human label; null when the target is unresolved (D-02)
  authored:     bool,                      // ADDED: source == "agent" (see ADR-004 / D-03)
}
```

**Dropped from `EdgeRecord`** (intentionally, and these may NOT be re-added on the get
path without a new ADR):
- `source_id` — anchor-constant on a single-entry read (it is always the entry being read,
  or the resolved other-endpoint already exposed as `target_id`).
- `depth` — always 1 at the get layer; multi-hop is `context_graph`'s job.
- `metadata` — detail-view enrichment; stays in `context_graph`.
- the raw `source` string — collapsed to the `authored` boolean (kept underneath in
  `RawEdgeRow` for future revival, see ADR-004); never surfaced on get.

The boundary is explicit:

| Tool | Role | Per-edge payload |
|------|------|------------------|
| `context_get` | thin pointer list ("what is near this entry?") | `{edge_type, direction, target_id, target_title, authored}` |
| `context_graph` | detail + multi-hop traversal | full `EdgeRecord` |

The surfaced `target_id`s are the entry points: an agent that wants detail follows a
`target_id` into `context_graph`.

### Consequences

- **Easier:** one shared edge vocabulary, no shape drift; the get payload stays small and
  scannable (its only job is "should I go read that?"); a clear, documented rule for
  reviewers — any proposal to add weight/metadata/source to the get edge is a boundary
  violation requiring a new ADR.
- **Harder:** the two tools no longer return byte-identical edge objects, so a consumer
  that wants full detail must make a second `context_graph` call (by design — that is the
  discovery-vs-detail split). The `edge_type`-vs-`relation_type` rename is a small
  vocabulary divergence that must be documented so the projection is understood as
  deliberate, not accidental.
