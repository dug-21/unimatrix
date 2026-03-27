# Component: store-query-helpers

## Purpose

Two new async query methods on `Store` in `crates/unimatrix-store/src/read.rs`:

- `query_entries_without_edges()` — returns IDs of active entries with no non-bootstrap graph
  edge on either endpoint. Used in Phase 2 to build the `isolated_ids` set.
- `query_existing_supports_pairs()` — returns all non-bootstrap Supports edge pairs as a
  `HashSet<(u64, u64)>`. Used in Phase 2 as the pre-filter skip set for Phase 4.

Both use `read_pool()` — they are read-only queries (C-02). Both are additive; no existing
`read.rs` methods are modified (C-10).

---

## File Modified

`crates/unimatrix-store/src/read.rs`

Note: `read.rs` is already well past 500 lines (>1570 lines at spec time). These two methods
are additive query additions. No file split is in scope for crt-029 (separate housekeeping).
Place the two methods near `query_graph_edges()` (line ~1323) as thematic siblings.

---

## Method 1: `query_entries_without_edges`

### Signature (from ARCHITECTURE.md integration surface)

```rust
pub async fn query_entries_without_edges(&self) -> Result<Vec<u64>>
```

### SQL

```sql
SELECT id FROM entries
WHERE status = 0
  AND id NOT IN (
    SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
    UNION
    SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
  )
```

`status = 0` is `Status::Active` (integer discriminant). `bootstrap_only = 0` means false.

### Pseudocode Body

```
FUNCTION query_entries_without_edges(self: &Store) -> Result<Vec<u64>>

    sql = "SELECT id FROM entries WHERE status = 0 AND id NOT IN (
               SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
               UNION
               SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
           )"

    rows = sqlx::query(sql)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
        // OR propagate sqlx error directly if StoreError wraps it

    ids = rows.into_iter()
        .map(|row| {
            let id: i64 = row.get(0);
            id as u64
        })
        .collect::<Vec<u64>>()

    return Ok(ids)

END FUNCTION
```

### Error Handling

- `sqlx::Error` from `fetch_all` is propagated as `StoreError`. Follow the existing
  `query_by_status` error handling pattern — it uses `.map_err()` or `?` with sqlx error
  wrapped in `StoreError::Database`.
- Empty result (no isolated entries) is `Ok(vec![])` — not an error.

### Notes

- Returns `Vec<u64>` not `HashSet<u64>`. The caller (`run_graph_inference_tick`) converts
  to `HashSet<u64>` for O(1) membership tests.
- No ORDER BY needed — the caller only needs membership.

---

## Method 2: `query_existing_supports_pairs`

### Signature (from ARCHITECTURE.md integration surface)

```rust
pub async fn query_existing_supports_pairs(&self) -> Result<HashSet<(u64, u64)>>
```

### SQL (ADR-004)

```sql
SELECT source_id, target_id
FROM graph_edges
WHERE relation_type = 'Supports'
  AND bootstrap_only = 0
```

Uses the `UNIQUE(source_id, target_id, relation_type)` index covering `relation_type`.
Returns only the two columns needed — no full row fetch (lighter than `query_graph_edges()`).

### Pseudocode Body

```
FUNCTION query_existing_supports_pairs(self: &Store) -> Result<HashSet<(u64, u64)>>

    sql = "SELECT source_id, target_id FROM graph_edges
           WHERE relation_type = 'Supports' AND bootstrap_only = 0"

    rows = sqlx::query(sql)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?

    pairs = rows.into_iter()
        .map(|row| {
            let source_id: i64 = row.get(0);
            let target_id: i64 = row.get(1);
            // Normalise to (min, max) for symmetric dedup matching Phase 4
            let a = source_id as u64;
            let b = target_id as u64;
            (a.min(b), a.max(b))
        })
        .collect::<HashSet<(u64, u64)>>()

    return Ok(pairs)

END FUNCTION
```

### Error Handling

- `sqlx::Error` propagated as `StoreError` — same pattern as Method 1.
- Empty table returns `Ok(HashSet::new())`.

### Notes on Pair Normalisation

Pairs are stored in the DB as directed edges `(source_id, target_id)`. The pre-filter needs
to match against `(min(a,b), max(a,b))` — the same canonical form used in Phase 4
deduplication. Normalising here avoids a second normalisation pass in the caller.

**IMPORTANT**: Phase 4 checks `existing_supports_pairs.contains(&(min(src,tgt), max(src,tgt)))`.
This check must use the same normalisation. Both sides normalise to `(min, max)` — this is
the contract between the store helper and the tick function.

### `HashSet` Import

`std::collections::HashSet` — already used in `read.rs` (`pub const EDGE_SOURCE_NLI` is in
the same file; check existing imports). The caller also needs it for the method's return type.

---

## Key Test Scenarios

### `query_entries_without_edges` — Four required cases (R-04, AC-15)

**Case 1: Empty DB**
```
seed: no entries, no edges
call: query_entries_without_edges()
expect: Ok(vec![])
```

**Case 2: Entries with non-bootstrap edges are excluded**
```
seed: entry id=1 with non-bootstrap Supports edge (source_id=1 in graph_edges)
      entry id=2 with non-bootstrap Supports edge (target_id=2 in graph_edges)
      entry id=3 with no edges
call: query_entries_without_edges()
expect: Ok(vec![3])   // only id=3 has no edges
```

**Case 3: Bootstrap-only edges do not count as edges**
```
seed: entry id=1 with bootstrap_only=1 edge (source_id=1)
      entry id=2 with no edges
call: query_entries_without_edges()
expect: Ok(vec![1, 2])  // bootstrap edges excluded; both are "without edges"
```

**Case 4: Deprecated entries excluded**
```
seed: entry id=1 with status=Active, no edges
      entry id=2 with status=Deprecated, no edges  (status != 0)
call: query_entries_without_edges()
expect: Ok(vec![1])   // only Active entries returned
```

### `query_existing_supports_pairs` — Four required cases (R-04, AC-15)

**Case 1: Empty GRAPH_EDGES**
```
seed: no graph_edges rows
call: query_existing_supports_pairs()
expect: Ok(HashSet::new())
```

**Case 2: Only bootstrap Supports rows**
```
seed: graph_edges row (source=1, target=2, relation_type='Supports', bootstrap_only=1)
call: query_existing_supports_pairs()
expect: Ok(HashSet::new())   // bootstrap rows excluded
```

**Case 3: Mixed bootstrap and non-bootstrap Supports rows**
```
seed: (source=1, target=2, 'Supports', bootstrap_only=1)
      (source=3, target=4, 'Supports', bootstrap_only=0)
call: query_existing_supports_pairs()
expect: Ok({(3, 4)})   // only non-bootstrap pair; normalised (min=3, max=4)
```

**Case 4: Non-Supports edges excluded**
```
seed: (source=1, target=2, 'Contradicts', bootstrap_only=0)
      (source=3, target=4, 'CoAccess', bootstrap_only=0)
      (source=5, target=6, 'Supports', bootstrap_only=0)
call: query_existing_supports_pairs()
expect: Ok({(5, 6)})   // only Supports pair
```

**Case 5 (idempotency / normalisation): Pair normalisation**
```
seed: (source=2, target=1, 'Supports', bootstrap_only=0)  // stored as (2,1)
call: query_existing_supports_pairs()
expect: Ok({(1, 2)})   // normalised to (min=1, max=2)
// Phase 4 checks (min(1,2), max(1,2)) = (1,2) — matches
```
