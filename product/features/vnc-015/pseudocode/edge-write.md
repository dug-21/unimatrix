# Component 2: edge_write.rs Helper Module

## Purpose

New `pub(crate)` module extracted from the edge-write logic that would otherwise be inlined in
`tools.rs`. Provides `validate_and_write_edges`, `delete_graph_edge`, `redirect_graph_edge`,
and the `validate_target` helper. Defines `EDGE_SOURCE_AGENT`, `EdgeValidationError`,
`EdgeDeleteError`, and `EdgeRedirectError`.

tools.rs is 8209 lines; the 500-line rule mandates extraction (ADR-005). All functions here
are `pub(crate)` — consumed only within `unimatrix-server`.

## File

`crates/unimatrix-server/src/mcp/edge_write.rs` (new)

Module declaration in `crates/unimatrix-server/src/mcp/mod.rs` (or top of `tools.rs`):
```
pub(crate) mod edge_write;
```

## Imports Required

```
use unimatrix_store::Store;
use unimatrix_store::schema::Status;          // Status::Quarantined, Status::Deprecated
use unimatrix_engine::graph::RelationType;
use crate::mcp::tools::EdgeInput;             // defined in tools.rs
use crate::mcp::nli_detection::write_graph_edge;  // existing pub(crate) fn, same crate
use sqlx::Sqlite;                             // for Transaction type in redirect
```

## Constants

```
pub(crate) const EDGE_SOURCE_AGENT: &str = "agent";
// Written to both GRAPH_EDGES.source and GRAPH_EDGES.created_by for all agent-declared edges
// Analogous to EDGE_SOURCE_COSINE_SUPPORTS naming convention (ADR-008)
```

## Error Types

```
pub(crate) enum EdgeValidationError {
    UnknownType      { edge_type: String },
    SelfReferential  { id: u64 },
    TargetNotFound   { target_id: u64 },
    TargetQuarantined { target_id: u64 },
}
// Implements Display for MCP error message generation

pub(crate) enum EdgeDeleteError {
    StoreError(StoreError),
    // Idempotent: 0-row DELETE is not an error — only infrastructure failures propagate
}

pub(crate) enum EdgeRedirectError {
    TargetNotFound    { target_id: u64 },
    TargetQuarantined { target_id: u64 },
    TransactionError(sqlx::Error),
}
```

## Function: validate_target (private helper)

```
ASYNC FUNCTION validate_target(store: &Store, target_id: u64) -> Result<(), EdgeValidationError>
    // One DB read via read_pool (non-blocking)
    LET result = store.get_entry_by_id(target_id).await
        // Propagate StoreError as TargetNotFound (infrastructure failure treated as not found)

    MATCH result
        Err(store_err) → RETURN Err(EdgeValidationError::TargetNotFound { target_id })
            // OR propagate as a distinct InfraError variant — implementation choice
        Ok(None) → RETURN Err(EdgeValidationError::TargetNotFound { target_id })
        Ok(Some(entry)) →
            IF entry.status == Status::Quarantined THEN
                RETURN Err(EdgeValidationError::TargetQuarantined { target_id })
            END IF
            // Status::Active → allowed
            // Status::Deprecated → allowed (DependencyOnDeprecated surfaces these)
            RETURN Ok(())
    END MATCH
END FUNCTION
```

## Function: validate_and_write_edges

```
pub(crate) ASYNC FUNCTION validate_and_write_edges(
    store:      &Store,
    source_id:  u64,       // 0 if called pre-insert for type+target validation only
    edges:      &[EdgeInput],
    created_at: u64,
) -> Result<(), EdgeValidationError>

    // INVARIANT: Called post-insert only. Caller performs Phase A (type resolution + target
    // validation) inline in the handler before entry insert. This function receives the
    // actual post-insert source_id and performs the self-ref check + writes.
    //
    // See edge-input-params.md for the Phase A inline loop in the handler.

    IF edges is empty THEN
        RETURN Ok(())
    END IF

    // ── THREE-CASE CONTRACT FOR write_graph_edge (Pattern #4041) ──────────────────
    // write_graph_edge returns bool, NOT Result<bool, _>:
    //   true  → row inserted (rows_affected = 1)
    //   false → INSERT OR IGNORE hit UNIQUE constraint — row already exists (idempotent, not error)
    //   [Err is handled INSIDE write_graph_edge and logged; caller receives false]
    // DO NOT treat false as an error. DO NOT surface false to the MCP caller.
    // ─────────────────────────────────────────────────────────────────────────────

    // Phase A: type resolution + target validation (all before any write)
    // Collect resolved (RelationType, target_id) pairs
    LET resolved: Vec<(RelationType, u64)> = empty

    FOR EACH edge IN edges
        // 1. Edge type resolution (pure — no DB access)
        LET rel_type = RelationType::from_str(&edge.edge_type)
        IF rel_type is None THEN
            RETURN Err(EdgeValidationError::UnknownType { edge_type: edge.edge_type.clone() })
        END IF

        // 2. Self-referential check (source_id is the actual post-insert id)
        IF source_id == edge.target_id THEN
            RETURN Err(EdgeValidationError::SelfReferential { id: source_id })
        END IF

        // 3. Target validation (1 DB read per edge via read_pool)
        CALL validate_target(store, edge.target_id).await?

        PUSH (rel_type, edge.target_id) TO resolved
    END FOR

    // All edges passed Phase A — proceed to Phase B (writes)
    // Phase B: write loop
    FOR EACH (rel_type, target_id) IN resolved
        // write_graph_edge signature (existing, in nli_detection.rs):
        // write_graph_edge(store, source_id, target_id, relation_type, weight, created_at, source, metadata) -> bool
        LET inserted = write_graph_edge(
            store,
            source_id,
            target_id,
            rel_type.as_str(),
            1.0,
            created_at,
            EDGE_SOURCE_AGENT,
            ""
        ).await

        // Three-case contract:
        // true  → inserted; continue
        // false → UNIQUE conflict; idempotent; continue (no error)
        // [Err logged inside write_graph_edge; we receive false]

        // Bidirectional Contradicts: write reverse direction (ADR-003 — fire-and-forget)
        IF rel_type == RelationType::Contradicts THEN
            LET _inserted_reverse = write_graph_edge(
                store,
                target_id,      // reversed: target is now source
                source_id,      // reversed: source is now target
                "Contradicts",
                1.0,
                created_at,
                EDGE_SOURCE_AGENT,
                ""
            ).await
            // Both directions written before function returns (NFR-10, AC-06)
            // NOT transactional — fire-and-forget sequential (ADR-003)
            // If second write fails: first direction persists; graph is asymmetric until repair
        END IF
    END FOR

    RETURN Ok(())
END FUNCTION
```

## Function: delete_graph_edge

```
pub(crate) ASYNC FUNCTION delete_graph_edge(
    store:         &Store,
    source_id:     u64,
    target_id:     u64,
    relation_type: &str,
) -> Result<(), EdgeDeleteError>

    // Execute DELETE for the primary direction
    LET pool = store.write_pool_server()
    LET result = sqlx::query(
        "DELETE FROM graph_edges WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3"
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .execute(pool)
    .await

    IF result is Err(e) THEN
        RETURN Err(EdgeDeleteError::StoreError(e))
    END IF
    // rows_affected = 0 is NOT an error — idempotent delete (NFR-03, AC-25)

    // Bidirectional Contradicts: delete the reverse direction
    IF relation_type == "Contradicts" THEN
        LET result_rev = sqlx::query(
            "DELETE FROM graph_edges WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3"
        )
        .bind(target_id as i64)   // reversed
        .bind(source_id as i64)   // reversed
        .bind("Contradicts")
        .execute(pool)
        .await

        IF result_rev is Err(e) THEN
            RETURN Err(EdgeDeleteError::StoreError(e))
        END IF
        // 0 rows affected on reverse is still success (idempotent)
    END IF

    RETURN Ok(())
END FUNCTION
```

## Function: redirect_graph_edge

```
pub(crate) ASYNC FUNCTION redirect_graph_edge(
    store:         &Store,
    source_id:     u64,
    old_target_id: u64,
    new_target_id: u64,
    relation_type: &str,
    created_at:    u64,
) -> Result<(), EdgeRedirectError>

    // CRITICAL: Use RAII transaction via pool.begin().await? (lesson #2269)
    // DO NOT use raw "BEGIN"/"COMMIT" SQL strings — pool can acquire different connections
    // for each statement, making the transaction boundary meaningless under write_max_connections >= 2

    LET pool = store.write_pool_server()
    LET mut txn = pool.begin().await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?
    // txn: sqlx::Transaction<'_, Sqlite>
    // All SQL statements below execute against &mut *txn — same connection

    // Target validation is performed by the caller (context_edge handler) BEFORE this function
    // is called. validate_target(store, new_target_id) runs in the handler's validation pipeline.
    // No re-validation here — trust the validated caller.

    IF relation_type == "Contradicts" THEN
        // ── Contradicts: 4-row atomic operation ──────────────────────────────────
        // 1. Delete A→B
        sqlx::query(
            "DELETE FROM graph_edges WHERE source_id=?1 AND target_id=?2 AND relation_type='Contradicts'"
        )
        .bind(source_id as i64)
        .bind(old_target_id as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| { txn.rollback(); EdgeRedirectError::TransactionError(e) })?

        // 2. Delete B→A
        sqlx::query(
            "DELETE FROM graph_edges WHERE source_id=?1 AND target_id=?2 AND relation_type='Contradicts'"
        )
        .bind(old_target_id as i64)
        .bind(source_id as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?

        // 3. Insert A→B' (INSERT OR IGNORE — idempotent if already exists)
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)
             VALUES (?1, ?2, 'Contradicts', 1.0, ?3, ?4, ?4, 0, '')"
        )
        .bind(source_id as i64)
        .bind(new_target_id as i64)
        .bind(created_at as i64)
        .bind(EDGE_SOURCE_AGENT)
        .execute(&mut *txn)
        .await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?

        // 4. Insert B'→A
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)
             VALUES (?1, ?2, 'Contradicts', 1.0, ?3, ?4, ?4, 0, '')"
        )
        .bind(new_target_id as i64)
        .bind(source_id as i64)
        .bind(created_at as i64)
        .bind(EDGE_SOURCE_AGENT)
        .execute(&mut *txn)
        .await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?

    ELSE
        // ── Non-Contradicts: 2-row atomic operation ───────────────────────────────
        // 1. Delete old edge
        sqlx::query(
            "DELETE FROM graph_edges WHERE source_id=?1 AND target_id=?2 AND relation_type=?3"
        )
        .bind(source_id as i64)
        .bind(old_target_id as i64)
        .bind(relation_type)
        .execute(&mut *txn)
        .await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?

        // 2. Insert new edge
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)
             VALUES (?1, ?2, ?3, 1.0, ?4, ?5, ?5, 0, '')"
        )
        .bind(source_id as i64)
        .bind(new_target_id as i64)
        .bind(relation_type)
        .bind(created_at as i64)
        .bind(EDGE_SOURCE_AGENT)
        .execute(&mut *txn)
        .await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?
    END IF

    // Commit: RAII Transaction drops without commit = automatic ROLLBACK
    txn.commit().await
        .map_err(|e| EdgeRedirectError::TransactionError(e))?

    RETURN Ok(())
END FUNCTION
```

## Error Handling Summary

| Function | Error type | When | Propagation |
|----------|-----------|------|-------------|
| `validate_target` | `EdgeValidationError` | Target not found, quarantined | Propagated to caller |
| `validate_and_write_edges` | `EdgeValidationError` | Type unknown, self-ref, target invalid | Propagated to handler; aborts entire call |
| `validate_and_write_edges` | (internal) | Edge write fails (write_graph_edge returns false on Err) | Logged inside write_graph_edge; not propagated |
| `delete_graph_edge` | `EdgeDeleteError` | Pool/SQL error | Propagated to context_edge handler |
| `redirect_graph_edge` | `EdgeRedirectError` | Pool error, transaction failure | txn drops → ROLLBACK; error propagated to handler |

## Key Test Scenarios

1. `validate_and_write_edges` with empty slice → Ok(()) with no DB writes (R-08 edge case)
2. `validate_and_write_edges` with unknown edge type → Err(UnknownType) before any write
3. `validate_and_write_edges` with quarantined target → Err(TargetQuarantined) before any write
4. `validate_and_write_edges` with deprecated target → Ok(()) + edge written (deprecated allowed)
5. `validate_and_write_edges` with Contradicts edge → both (A,B) and (B,A) rows written (AC-06)
6. `validate_and_write_edges` called twice with same edge (idempotent) → no error, 1 row (AC-10)
7. `delete_graph_edge` on non-existent edge → Ok(()) (idempotent, AC-25)
8. `delete_graph_edge` on Contradicts → both direction rows removed (AC-25)
9. `redirect_graph_edge` success → old row absent, new row present (atomic, AC-26)
10. `redirect_graph_edge` Contradicts → all 4 rows updated atomically (R-02)
11. `redirect_graph_edge` with quarantined new_target → validation fired by caller before fn called
12. Transaction rollback on mid-redirect failure → original edge preserved (R-05)
13. EDGE_SOURCE_AGENT constant used in source + created_by columns (AC-18, R-15)
