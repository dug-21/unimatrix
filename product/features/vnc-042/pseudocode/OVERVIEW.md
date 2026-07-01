# vnc-042 Pseudocode — OVERVIEW

`context_get` resolves a requested deprecated id to its active terminal **by default**.
Surgical single-tool change in `unimatrix-server`, reusing `follow_to_current`. No schema,
no SQL, no other-tool changes (NG-3/4/5).

## Components (per architecture Component Breakdown)

| # | Component | File | Change | Pseudocode |
|---|-----------|------|--------|-----------|
| 1 | `context_get` handler + `GetParams` field + tool-desc | `mcp/tools.rs` | resolution branch, effective_id threading, formatter route, desc strings | `context-get-handler.md` |
| 2 | response formatter | `mcp/response/entries.rs` | NEW `format_single_entry_with_note` + `ResolutionNote` enum; `format_single_entry` UNCHANGED | `response-formatter.md` |
| 3 | `follow_to_current` visibility | `mcp/graph_read_neighbors.rs` + `mcp/graph_read.rs` | widen `pub(super)`→`pub(crate)` + re-export | `follow-to-current-reexport.md` |

## Data Flow (what crosses boundaries)

```
context_get(id, follow_supersessions?, include_edges?, format)
  │ handler
  ├─ id: u64        ← validated_id(params.id)             (no cast)
  ├─ effective_id: u64 + note: Option<ResolutionNote>     ← resolution branch (calls Component 3)
  ├─ entry: EntryRecord ← self.entry_store.get(effective_id)   (SINGLE fetch)
  ├─ edges: Option<EdgesView> ← build_edges_view(&self.store, effective_id)  (SAME effective_id)
  └─ CallToolResult ← Component 2:
         note == None  → format_single_entry(&entry, fmt, edges)          (byte-identical)
         note == Some  → format_single_entry_with_note(&entry, fmt, edges, &note)
```

Boundary contracts:
- Handler → Component 3: `follow_to_current(&self.store, id) -> Option<u64>` (async).
- Handler → Component 2: `&ResolutionNote` describing what happened; formatter only renders.
- `effective_id` MUST thread to BOTH `entry_store.get` AND `build_edges_view` (R-03, single-fetch invariant).

## Sequencing Constraint

Component 3 (visibility widen + re-export) must land **before/with** Component 1 — the
handler calls `crate::mcp::graph_read::follow_to_current` and will not compile otherwise.
Components 1 and 2 are co-dependent (handler routes to the new formatter) but the formatter
(2) is independently testable and can be built first.

## Shared Types

### `ResolutionNote` (defined in Component 2 `response/entries.rs`, produced by Component 1)

Handler → formatter. Carried only on **non-clean** paths; clean passthrough carries `None`
and routes to the base formatter (byte-identity, SR-04/R-01).

```
enum ResolutionNote {
    Followed          { from: u64, to: u64 },              // AC-02 hop occurred
    DeadEnd           { requested: u64 },                  // AC-04 no active successor
    AsStoredDeprecated{ requested: u64, superseded_by: Option<u64> }, // AC-03 / AC-08 (R-08)
    // clean passthrough => no ResolutionNote value at all (handler passes None)
}
```

### `ResolutionStatus` (json discriminant — string values emitted under `format="json"`)

Not necessarily a distinct Rust enum; it is the stable `resolution.status` string contract
(ADR-003). The formatter maps each `ResolutionNote` variant to it. Present ONLY on non-clean
paths — the `resolution` key is **absent** on clean passthrough (R-07 / byte-identity).

| `ResolutionNote` variant | `resolution.status` | other json fields |
|--------------------------|---------------------|-------------------|
| `Followed{from,to}`      | `"followed"`             | `requested_id=from`, `returned_id=to` |
| `DeadEnd{requested}`     | `"no_active_successor"`  | `requested_id=requested` |
| `AsStoredDeprecated{requested, Some(z)}` | `"as_stored_deprecated"` | `requested_id=requested`, `superseded_by=z` |
| `AsStoredDeprecated{requested, None}`    | `"as_stored_deprecated"` | `requested_id=requested`, `superseded_by=null` |
| (clean passthrough)      | *(no `resolution` key)*  | — |

### Reused types (unchanged, from codebase)

- `EntryRecord` (`unimatrix-store`) — fields used: `id`, `status: Status`, `superseded_by: Option<u64>`, `content`, ...
- `Status` (`schema.rs:10`) — `Active=0`, `Deprecated=1`, `Proposed=2`, `Quarantined=3`.
- `EdgesView` (`mcp/response/edges.rs`), `ResponseFormat` (`Summary`/`Markdown`/`Json`).
- `follow_to_current(store: &Store, id: u64) -> Option<u64>` (async) — Component 3.

## Orthogonality (AC-07)

`follow_supersessions` picks *which* entry (`effective_id`); `include_edges` decides *whether*
edges surface (keyed on `effective_id`); `format` decides *how* it renders. The three are
independent — see the matrix in `context-get-handler.md`.
