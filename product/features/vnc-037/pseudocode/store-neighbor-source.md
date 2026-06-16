# Component: store-neighbor-source

## Purpose

Add the `graph_edges.source` provenance column to the **plain** neighbor read path additively, so
the get-edge projection can compute `authored = (source == "agent")` (FR-6 / D-03 / ADR-004). The
change is **read-only, no DDL, no migration** — `source` already exists on `graph_edges`
(`db.rs:960`). The rank/JOIN/canonicalization logic does NOT live here (it is confined to the new
ranked variant) so the `context_graph` neighbors contract stays byte-stable (SR-02/SR-06/C-3).

## Location

- `crates/unimatrix-store/src/graph_queries.rs` — `RawEdgeRow` struct (`:73`).
- `crates/unimatrix-store/src/graph_queries_neighbors.rs` — `map_edge_row` (`:95`) and all 4 SELECT
  branches (`run_outgoing_query` empty + IN, `run_incoming_query` empty + IN).

## Modified Types

```
// graph_queries.rs — RawEdgeRow gains two additive fields (shared definition, see OVERVIEW).
struct RawEdgeRow {
    source_id: u64                    // existing — untouched
    target_id: u64                    // existing — untouched
    relation_type: String             // existing — untouched
    source: String                    // ADDITIVE — populated by plain AND ranked paths
    target_confidence: Option<f64>    // ADDITIVE — populated by the RANKED variant only; None here
}
```

> The plain path populates `target_confidence: None`. Only `query_ranked_neighbors` sets it. This
> keeps one row type shared with `context_graph` neighbors (no shape drift) while letting the
> ranked variant carry its rank key.

## Modified Functions

### map_edge_row (graph_queries_neighbors.rs:95)

```
fn map_edge_row(row: &SqliteRow) -> Result<RawEdgeRow, StoreError>:
    Ok(RawEdgeRow {
        source_id:        row.try_get::<i64,_>("source_id").map_err(StoreError::Database)? as u64
        target_id:        row.try_get::<i64,_>("target_id").map_err(StoreError::Database)? as u64
        relation_type:    row.try_get("relation_type").map_err(StoreError::Database)?
        source:           row.try_get("source").map_err(StoreError::Database)?   // NEW — same try_get + mapping
        target_confidence: None                                                   // NEW — plain path never has it
    })
```

- Same `try_get` + `StoreError::Database(e.into())` mapping as the existing fields. **No `.unwrap()`**.
- `source` is `TEXT` on `graph_edges`; map to `String`.

### The 4 plain SELECTs (graph_queries_neighbors.rs)

Add `source` to the column list in **every** branch (front-load all four per #4831 — do not
discover one compile error at a time). WHERE clauses, indexes, binds, and the `!= 'Supersedes'`
filter are unchanged.

```
// run_outgoing_query, empty edge_types branch:
SELECT source_id, target_id, relation_type, source
FROM graph_edges WHERE source_id = ?1 AND relation_type != 'Supersedes'

// run_outgoing_query, IN(…) branch:
SELECT source_id, target_id, relation_type, source
FROM graph_edges WHERE source_id = ?1 AND relation_type IN (?2, …)

// run_incoming_query, empty edge_types branch:
SELECT source_id, target_id, relation_type, source
FROM graph_edges WHERE target_id = ?1 AND relation_type != 'Supersedes'

// run_incoming_query, IN(…) branch:
SELECT source_id, target_id, relation_type, source
FROM graph_edges WHERE target_id = ?1 AND relation_type IN (?2, …)
```

`query_direct_neighbors` (`:200`) and the `Both` `outgoing.extend(incoming)` body are **unchanged** —
they still return un-ranked, un-canonicalized, unbounded rows for `context_graph`.

## Data Flow

- **Inputs**: `graph_edges` rows (now selecting `source`).
- **Outputs**: `Vec<RawEdgeRow>` with `source` populated, `target_confidence = None`. Consumed by
  `context_graph` neighbors (`neighbors_sql` constructs `EdgeRecord` by field name — ignores
  `source`/`target_confidence`, so no behavior change) and unchanged for vnc-037's own plain use.

## Error Handling

- A missing/untyped `source` column surfaces as `StoreError::Database` via `try_get` — propagated,
  never unwrapped. (In practice `source` always exists; the mapping matches existing fields.)

## Key Test Scenarios

- **map_edge_row across ALL 4 branches (R-08, #4166)** — `source` populates correctly in both
  directions, both empty-type and IN-type branches. Audit all passes, not one.
- **context_graph neighbors suite green UNEDITED (R-08, AC-09, #4876 empirical)** — run the existing
  neighbors/graph tests after the additive change; zero edits, all green. `EdgeRecord` wire shape
  unchanged; no `source`/`target_confidence` leakage into neighbors output.
- **authored predicate downstream (R-09)** — seed `source='agent'` and `source='co_access'` rows;
  the projection (get-edge-vocabulary) maps them to `authored=true`/`false`. Near-miss strings
  (`'Agent'`, `' agent'`) do NOT flip authored true — exact match (cross-checks the ranked
  `(source='agent')` SQL term).
- **no migration (AC-03)** — confirm no migration file added; `source` is an existing column.
