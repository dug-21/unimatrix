# Wave 2: write_graph_edge — Generalized Edge Writer

## Purpose

Add a new `pub(crate) async fn write_graph_edge` to `nli_detection.rs` as a sibling to
the existing `write_nli_edge`. This function accepts `source: &str` as a parameter so
Path C can write edges tagged `source = 'cosine_supports'` without modifying `write_nli_edge`.

`write_nli_edge` MUST NOT be modified. It retains its hardcoded `source = 'nli'` and
`created_by = 'nli'` literals and its existing signature (ADR-001, FR-12, WARN-04).

---

## File Modified

`crates/unimatrix-server/src/services/nli_detection.rs`

---

## Existing Function (DO NOT MODIFY)

```
pub(crate) async fn write_nli_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    metadata: &str,
) -> bool {
    // INSERT OR IGNORE with hardcoded 'nli' for created_by and source
    // ... (unchanged)
}
```

The function's SQL literal `'nli', 'nli'` for `(created_by, source)` must remain as-is.
Any refactoring of `write_nli_edge` to delegate to `write_graph_edge` is OPTIONAL and
deferred (WARN-04: the spec wins over the architecture's "refactored to delegate" note).
The safe delivery path is to add `write_graph_edge` as a completely independent sibling.

---

## New Function: `write_graph_edge`

Add immediately after the closing `}` of `write_nli_edge`, before `format_nli_metadata`.

### Signature

```
pub(crate) async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,      // e.g. EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports"
    metadata: &str,    // pre-serialized JSON string
) -> bool
```

### Body pseudocode

```
FUNCTION write_graph_edge(store, source_id, target_id, relation_type, weight,
                           created_at, source, metadata) -> bool:

    // INSERT OR IGNORE for idempotency on UNIQUE(source_id, target_id, relation_type).
    // The `source` column is NOT in the UNIQUE constraint — first writer wins.
    // created_by is set to the same value as `source` (ADR-001: auditing consistency).
    result = sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, \
          source, bootstrap_only, metadata) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0, ?7)"
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .bind(weight as f64)
    .bind(created_at as i64)
    .bind(source)          // bound to ?6 — used for BOTH created_by and source
    .bind(metadata)        // bound to ?7
    .execute(store.write_pool_server())
    .await

    MATCH result:
        Ok(query_result) =>
            // Distinguish between new insert and silent UNIQUE conflict.
            // rows_affected = 1: row was inserted — return true.
            // rows_affected = 0: INSERT OR IGNORE silently deduped (UNIQUE conflict) — return false.
            // This matches path-c-loop.md: budget counter increments ONLY on true return.
            // UNIQUE conflict is NOT an error — no warn! emitted here.
            RETURN query_result.rows_affected() > 0

        Err(e) =>
            // SQL error: pool exhaustion, locked DB, schema mismatch, etc.
            // Log at warn! — NOT error! (tick is infallible; warn is sufficient)
            // Do NOT propagate: tick returns ()
            warn!(
                source_id = source_id,
                target_id = target_id,
                relation_type = relation_type,
                source = source,
                error = %e,
                "write_graph_edge: failed to write graph edge"
            )
            RETURN false
```

### Return value contract

`write_graph_edge` returns `bool` with the following semantics:

| Return | Condition | Log inside fn |
|--------|-----------|---------------|
| `true` | Row inserted (`rows_affected = 1`) | none |
| `false` | UNIQUE conflict — INSERT OR IGNORE silently deduped (`rows_affected = 0`) | none |
| `false` | SQL error (pool exhaustion, locked DB, schema mismatch, etc.) | `warn!` |

The distinction between new insert and UNIQUE conflict is made by inspecting
`query_result.rows_affected()` inside the `Ok` arm. sqlx returns `Ok` with
`rows_affected = 0` for a silently ignored duplicate — NOT `Err`. This is different from
how `write_nli_edge` works (which returns `true` unconditionally on `Ok`); `write_graph_edge`
is more precise because Path C's budget counter must count only real inserts.

The caller (Path C loop) MUST:
- Increment `cosine_supports_written` ONLY on `true` return
- NOT emit an additional `warn!` on `false` return (UNIQUE conflict path has no log;
  SQL error path already logged inside this function — double-logging is wrong)
- NOT treat `false` as a fatal condition — the loop continues normally

---

## Import Update Required

`nli_detection_tick.rs` must extend its import of `nli_detection` symbols to include
`write_graph_edge`:

```
// Current import in nli_detection_tick.rs (around line 43):
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};

// Updated:
use crate::services::nli_detection::{
    current_timestamp_secs, format_nli_metadata, write_graph_edge, write_nli_edge,
};
```

Also verify that `EDGE_SOURCE_COSINE_SUPPORTS` from `unimatrix-store` is in scope in
`nli_detection_tick.rs`. Check existing imports at the top of that file:

```
// Confirm or add:
use unimatrix_store::{EDGE_SOURCE_COSINE_SUPPORTS, /* existing imports */};
// Or if unimatrix_store::* is already wildcarded, no change needed.
```

---

## Module Doc Comment Update

Update the module-level doc comment at the top of `nli_detection.rs` to reflect the
addition of `write_graph_edge`:

```
// Current:
//! This module provides three pub(crate) helpers consumed by `nli_detection_tick.rs`:
//! `write_nli_edge`, `format_nli_metadata`, and `current_timestamp_secs`.

// Updated:
//! This module provides pub(crate) helpers consumed by `nli_detection_tick.rs`:
//! `write_nli_edge`, `write_graph_edge`, `format_nli_metadata`, and `current_timestamp_secs`.
//!
//! `write_nli_edge`: hardcodes source='nli'; used by Path A and Path B callers.
//! `write_graph_edge`: accepts source as a parameter; used by Path C (crt-040) and
//!   future edge signal sources. Adding a new source: call `write_graph_edge` with the
//!   corresponding `EDGE_SOURCE_*` constant from `unimatrix-store`. Do NOT add source
//!   parameters to `write_nli_edge` (pattern #4025).
```

---

## Error Handling

| Condition | Behavior | Log Level |
|-----------|----------|-----------|
| Successful insert (`rows_affected = 1`) | `Ok` from sqlx; return `true` | none |
| UNIQUE conflict (`rows_affected = 0`) | `Ok` from sqlx with `rows_affected = 0`; return `false` | none |
| SQL error (pool contention, locked DB) | `warn!` with source_id, target_id, relation_type, source, error; return `false` | `warn!` |

The `false` return on SQL error is the ONLY case where a `warn!` is emitted from inside
this function. The caller (Path C loop) must NOT emit an additional `warn!` on `false`
return — that would double-log SQL errors and spuriously log expected UNIQUE dedup.

---

## Key Test Scenarios

### R-02: write_nli_edge still writes source='nli' after this change

```
fn test_write_nli_edge_source_is_nli() {
    // call write_nli_edge(store, src_id, tgt_id, "Informs", 0.7, ts, metadata)
    // query graph_edges for the row
    // assert row.source == "nli"
    // assert row.created_by == "nli"
}
```

### R-02: write_graph_edge writes the passed source value

```
fn test_write_graph_edge_source_matches_parameter() {
    // call write_graph_edge(store, src_id, tgt_id, "Supports", 0.70, ts,
    //                       "cosine_supports", '{"cosine": 0.70}')
    // query graph_edges for the row
    // assert row.source == "cosine_supports"
    // assert row.created_by == "cosine_supports"
    // assert row.relation_type == "Supports"
    // assert row.weight ≈ 0.70 (f32/f64 rounding tolerance)
    // assert row.metadata == '{"cosine": 0.7}'  (or equivalent)
}
```

### R-07: write_graph_edge returns false on UNIQUE conflict (no error log)

```
fn test_write_graph_edge_returns_false_on_duplicate() {
    // Insert (src=1, tgt=2, rel="Supports") via write_graph_edge → first call returns true
    // Insert (src=1, tgt=2, rel="Supports") via write_graph_edge → second call returns false
    //   (INSERT OR IGNORE silently discards; sqlx returns Ok with rows_affected=0)
    //   rows_affected() == 0  →  write_graph_edge returns false
    // assert no panic
    // assert DB has exactly 1 row for that (source_id, target_id, relation_type) pair
    // assert NO warn! emitted (requires tracing test subscriber or manual inspection)
}
```

### R-07: write_graph_edge returns false on SQL error (not from dedup)

```
fn test_write_graph_edge_returns_false_on_sql_error() {
    // Use a closed/read-only store or inject a query error
    // call write_graph_edge
    // assert returns false
    // assert warn! was emitted (use tracing_test or similar)
}
```

---

## Checklist

- [ ] `write_nli_edge` is NOT modified — its SQL literal `'nli', 'nli'` is unchanged
- [ ] `write_graph_edge` is placed immediately after `write_nli_edge` in the file
- [ ] SQL uses `?6` bound twice (for `created_by` and `source`) — correct parameterization
- [ ] `Err` branch emits `warn!` with structured fields (source_id, target_id, relation_type, source, error)
- [ ] `Ok` branch returns `query_result.rows_affected() > 0` (true=inserted, false=UNIQUE conflict)
- [ ] Module doc comment updated to list `write_graph_edge`
- [ ] `nli_detection_tick.rs` import extended to include `write_graph_edge`
- [ ] Unit test verifies `write_nli_edge` still writes `source='nli'`
- [ ] Unit test verifies `write_graph_edge` writes the passed `source` value
- [ ] Unit test verifies duplicate returns `false`, no error log (UNIQUE conflict silent)
