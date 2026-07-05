# Component: eager-delete-helper

**File:** `crates/unimatrix-server/src/mcp/edge_write.rs` (NEW fn + `RemovedEdge` struct, beside `delete_graph_edge:244`, reusing its `write_pool_server()` pattern).

## Purpose

Execute the one synchronous, indexed, agent-only, both-directions delete of a deprecated entry's
`graph_edges` rows and return the removed tuples. This is the single source of truth for both the
inline count (`tuples.len()`) and the audit metadata. LOCKED predicate; unguarded by design
(safety rests on the single chokepoint caller — R-06 / C-10).

## New Type

```rust
/// A graph edge removed by the eager cleanup. Serialize keys are the audit metadata JSON shape.
#[derive(Debug, serde::Serialize)]
pub(crate) struct RemovedEdge {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
}
```

`EdgeDeleteError` (`edge_write.rs:76`, variant `StoreError(StoreError)`) is REUSED — no new error type.

## New Function

```
async fn delete_agent_edges_for_entry(store: &Store, entry_id: u64)
    -> Result<Vec<RemovedEdge>, EdgeDeleteError>
```

### Pseudocode

```
FUNCTION delete_agent_edges_for_entry(store, entry_id):
    pool = store.write_pool_server()                          # C-05 / NFR-02: write pool only

    # R-03 ATOMICITY: ONE statement. DELETE … RETURNING both deletes and returns the
    # removed rows in a single fetch_all — no delete-then-separate-SELECT window, so there is
    # never a "gone with no record" gap. count is derived from these same tuples.
    rows = sqlx::query(
        "DELETE FROM graph_edges
           WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2
           RETURNING source_id, target_id, relation_type"      # LOCKED — never widen by
    )                                                          #   relation_type; never add a
        .bind(entry_id AS i64)          # ?1                    #   runtime superseded_by clause
        .bind(EDGE_SOURCE_AGENT)        # ?2 = "agent" constant (edge_write.rs:28), NOT user input
        .fetch_all(pool)               # single await; single statement
        .await
        MAP_ERR e -> EdgeDeleteError::StoreError(StoreError::Database(e.into()))
        # `?` / early return on Err — the caller (handler) treats Err as non-fatal (warn, None).

    # Marshal RETURNING rows. source_id/target_id are stored/bound as i64 (see delete_graph_edge);
    # cast back to u64. A self-loop (source_id == target_id == entry) matches the OR once →
    # exactly one row → counted once (R-10).
    removed = EMPTY Vec<RemovedEdge>
    FOR row IN rows:
        removed.push(RemovedEdge {
            source_id:     row.get::<i64,_>("source_id")   AS u64,
            target_id:     row.get::<i64,_>("target_id")   AS u64,
            relation_type: row.get::<String,_>("relation_type"),
        })

    RETURN Ok(removed)                 # count = removed.len(); Ok(empty) is valid (Some(0) case)
```

### Notes

- `use sqlx::Row;` in scope for `row.get` (mirror `repoint_deprecated_target_edges`, `background.rs:870`).
- Single statement, no per-edge loop over the DB — the only loop is in-memory marshaling of the
  returned rows (NFR-01). Served by `idx_graph_edges_source_id` + `idx_graph_edges_target_id`.
- Leave a code-adjacency doc comment on the fn: links it to `run_orphaned_edge_compaction`
  (`background.rs:805`) as the backstop and to the eager ⊆ tick invariant (ADR-003), and states the
  single-caller contract (C-11 / R-06) so any second caller is a conscious design change.
- Zero-row RETURNING (nothing matched, or a concurrent tick already swept — R-07) returns `Ok(vec![])`
  without error → handler maps to `Some(0)`.

## Error Handling

- Only `Err` source is the `fetch_all` DB error → `EdgeDeleteError::StoreError`. Returned to the
  caller; the caller (deprecate-handler) is where non-fatal handling lives (warn, `edges_removed = None`).
- Because DELETE + capture are one statement, there is no post-commit marshaling window that could
  drop tuples after the rows are gone (closes R-03). If `fetch_all` returns `Ok`, the rows are both
  deleted AND in hand.

## Data Flow

- **In:** `&Store`, `entry_id: u64` (already flipped to Deprecated by step 6).
- **Out:** `Ok(Vec<RemovedEdge>)` (possibly empty) or `Err(EdgeDeleteError)`.
- **Transform:** `graph_edges` rows → typed `RemovedEdge` structs (i64→u64 cast).

## Key Test Scenarios (hints)

- FR-01 / AC-01: seed ≥1 inbound (`target_id=E`) and ≥1 outbound (`source_id=E`) `source='agent'`
  edge → both absent after call; returned vec has both.
- FR-02 / AC-04 per-source: seed one edge of each source (`agent`, `nli`, `co_access`,
  `cosine_supports`, `S1`, `S2`, `S8`) → only the `agent` edge in the returned set; machine edges remain.
- AC-05 / R-07: entry with zero agent edges → `Ok(vec![])` (no error).
- R-10 self-loop: `source_id == target_id == E`, `source='agent'` → returned once, count 1.
- R-10 high-degree: many agent edges → all returned in one statement.
- R-02 predicate pin: snapshot the exact SQL string so a `WHERE`/`RETURNING` edit (e.g. relation_type
  creep) is caught.
- R-03 atomicity: assert delete + capture is a single statement (no separate SELECT).
