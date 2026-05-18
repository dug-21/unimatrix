# Component 7: query_contradicts_edges_for_entry Fix

## Purpose

Fix `query_contradicts_edges_for_entry` in `crates/unimatrix-store/src/read.rs` to use a
bidirectional query. The current query uses `WHERE target_id = ?1` (asymmetric — finds edges
WHERE the entry is the target, but misses edges WHERE the entry is the source).

Once vnc-015 stores Contradicts edges bidirectionally (both A→B and B→A rows), both rows are
in GRAPH_EDGES and the asymmetric query is no longer meaningful. However, the OR-clause fix
also handles pre-vnc-015 unidirectional data (NLI-written edges whose direction depended on
detection order) during the transition period.

## File

`crates/unimatrix-store/src/read.rs` (modified)

## Current Query (BEFORE)

```sql
-- Current (read.rs:1529-1532, approximately):
SELECT source_id, target_id, relation_type, weight, created_at, created_by, source, metadata
FROM graph_edges
WHERE target_id = ?1
  AND relation_type = 'Contradicts'
```

The comment in the spec says the current query was `WHERE target_id = ?1`. This is
asymmetric: it finds rows where the given entry is the TARGET of a Contradicts edge,
but not where it is the SOURCE.

## Fixed Query (AFTER)

```sql
-- Fixed: bidirectional OR clause handles both pre-vnc-015 and post-vnc-015 data
SELECT source_id, target_id, relation_type, weight, created_at, created_by, source, metadata
FROM graph_edges
WHERE (source_id = ?1 OR target_id = ?1)
  AND relation_type = 'Contradicts'
```

This query returns all Contradicts edges where the given entry id appears on EITHER side.
For pre-vnc-015 unidirectional data: both calls (entry A, entry B) will each find the one
existing row regardless of which direction was stored. For post-vnc-015 bidirectional data:
each call returns 2 rows (both A→B and B→A).

## Function Pseudocode

```
FUNCTION query_contradicts_edges_for_entry(
    pool:     &ReadPool,    // or store: &Store — follows existing signature
    entry_id: u64,
) -> Result<Vec<GraphEdgeRow>, StoreError>
    // BEFORE: asymmetric query (WHERE target_id = ?1)
    // AFTER: bidirectional query (WHERE source_id = ?1 OR target_id = ?1)

    LET rows = sqlx::query_as::<_, GraphEdgeRow>(
        "SELECT source_id, target_id, relation_type, weight, created_at, created_by, source, metadata
         FROM graph_edges
         WHERE (source_id = ?1 OR target_id = ?1)
           AND relation_type = 'Contradicts'"
    )
    .bind(entry_id as i64)
    .fetch_all(pool)
    .await?

    RETURN Ok(rows)
END FUNCTION
```

## Behavior Change and Caller Audit (SR-06, R-07)

The behavior change: callers that previously received only "incoming Contradicts" rows now
receive "any Contradicts" rows (both directions).

Pre-implementation, the implementation agent MUST audit all call sites:

```
// Required audit: grep for query_contradicts_edges_for_entry across the entire workspace
// grep -r "query_contradicts_edges_for_entry" crates/
```

Known callers per ARCHITECTURE.md Component 7:
- `suppress_contradicts` (in tools.rs or related module) — existing caller

For each identified caller:
1. Determine if the caller processes the result as a scalar (`.first()`, single-row expectation)
   or as a collection.
2. Determine if the caller's behavior changes when 2 rows are returned instead of 0 or 1.
3. For `suppress_contradicts`: confirm the logic works correctly with 0, 1, or 2 rows returned.

If a caller does `.first()` and used to receive 0 or 1 rows but now may receive 2, it may
silently work (just uses the first) or may need updating (if it expected exactly-0 or exactly-1
semantics). Document the audit result before implementation begins.

## Post-vnc-015 Row Count Expectations

After this fix and the bidirectional write:

| Scenario | Before fix | After fix |
|----------|-----------|-----------|
| query_contradicts_for(A) — pre-vnc-015 data (A→B stored only) | 0 rows (missed — source direction) | 1 row |
| query_contradicts_for(B) — pre-vnc-015 data (A→B stored only) | 1 row (hit — target direction) | 1 row |
| query_contradicts_for(A) — post-vnc-015 data (A→B and B→A) | 0 rows (only found target matches) | 2 rows |
| query_contradicts_for(B) — post-vnc-015 data (A→B and B→A) | 1 row (only target match) | 2 rows |

The OR clause is intentionally chosen over `WHERE source_id = ?1` alone because pre-vnc-015
data has rows written in only one direction and that direction is not predictable. Using OR
ensures both old and new data are handled correctly during the transition period.

## Error Handling

| Error | Source | Behavior |
|-------|--------|----------|
| `StoreError` | sqlx pool/query error | Propagated to caller unchanged — same as before fix |

No new error types. The fix changes query semantics only, not error handling.

## Key Test Scenarios

1. Pre-vnc-015 compatibility: write single (A→B) Contradicts row directly; call function with
   A → assert 1 row returned; call with B → assert 1 row returned (R-07)
2. Post-vnc-015 bidirectional: write both (A→B) and (B→A) via validate_and_write_edges;
   call function with A → assert 2 rows; call with B → assert 2 rows (AC-16)
3. No Contradicts edge: call function with any entry → assert 0 rows
4. Regression: `suppress_contradicts` behavior verified correct after fix (R-07 scenario 4)
5. Caller audit result: document that all identified callers handle 0, 1, or 2 rows correctly
   (required before implementation — not a code test, a review gate)
