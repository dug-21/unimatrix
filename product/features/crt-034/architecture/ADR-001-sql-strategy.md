## ADR-001: Single-Query Batch Fetch with Subquery MAX Normalization

### Context

The promotion tick must:
1. Normalize each pair's weight as `count / MAX(count)` across ALL qualifying pairs
   (not just the capped batch) to keep weights on the same scale as bootstrapped edges
   (SCOPE.md §Design Decision 3, AC-13).
2. Cap the batch to `max_co_access_promotion_per_tick` pairs, selecting the highest-count
   pairs first (SCOPE.md §Design Decision 2).

SR-01 (SCOPE-RISK-ASSESSMENT.md) flags that a separate `SELECT MAX(count)` query adds a
second read-pool round-trip competing with `write_pool_server()` on every tick.

SR-02 flags that the per-pair INSERT OR IGNORE + conditional UPDATE is a two-step write
loop; individual statement timeouts under write-pool contention leave edges un-promoted
until the next tick (absorbed by the infallible contract).

Three SQL strategies were evaluated:

**Option A — Two separate queries**: `SELECT MAX(count) FROM co_access WHERE count >= ?`
followed by `SELECT entry_id_a, entry_id_b, count FROM co_access WHERE count >= ?
ORDER BY count DESC LIMIT ?`. Two round-trips to `write_pool_server()`.

**Option B — Subquery-embedded single fetch**: Fold the MAX into the batch query as a
correlated subquery:
```sql
SELECT
    entry_id_a,
    entry_id_b,
    count,
    (SELECT MAX(count) FROM co_access WHERE count >= ?) AS max_count
FROM co_access
WHERE count >= ?
ORDER BY count DESC
LIMIT ?
```
One round-trip. `max_count` is the same value on every row — computed once by SQLite's
query planner, not re-evaluated per row when expressed as a scalar subquery.

**Option C — CTE/window UPSERT**: Collapse INSERT and UPDATE into a single `INSERT OR
REPLACE` or a CTE with `ON CONFLICT`. SQLite's `INSERT OR REPLACE` deletes the
conflicting row and re-inserts, which resets `created_at` and `created_by` — incorrect
semantics. An `ON CONFLICT DO UPDATE` (UPSERT) would be correct, but requires
SQLite 3.24+ and does not support "only if delta > threshold" conditional logic without
a CHECK expression, which cannot reference the current row's existing value.

### Decision

Use **Option B**: a single SQL batch fetch with a scalar subquery for `MAX(count)`.

```sql
SELECT
    entry_id_a,
    entry_id_b,
    count,
    (SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count
FROM co_access
WHERE count >= ?1
ORDER BY count DESC
LIMIT ?2
```

Bind `?1 = CO_ACCESS_GRAPH_MIN_COUNT`, `?2 = max_co_access_promotion_per_tick`.

The Rust side computes normalized weight per row: `count as f32 / max_count as f32`.
When `max_count` is zero (empty table after WHERE filter), the query returns no rows and
the tick is a clean no-op — no division by zero in Rust.

For the per-pair write step, retain the two-step INSERT OR IGNORE + conditional SELECT +
UPDATE pattern. A single UPSERT cannot express the delta guard. The two-step write is
correct: contention causes a timeout on individual pairs (logged at `warn!`), and the
next tick retries. This is acceptable under the infallible tick contract.

### Consequences

- Eliminates one read-pool round-trip per tick (Option A → Option B).
- Single `.fetch_all()` call returns all data needed to compute normalized weights
  without any subsequent read query.
- SQLite's scalar subquery optimization ensures `MAX(count)` is computed once, not
  N times per row.
- Per-pair write contention is acceptable: one failed INSERT or UPDATE means that pair
  waits one tick cycle, not a correctness failure.
- Option C (UPSERT) is explicitly rejected: semantics mismatch (`INSERT OR REPLACE`
  resets metadata) and conditional-delta logic cannot be expressed as a SQL UPSERT
  without stored procedure support.
- Co-references: SR-01 (eliminated), SR-02 (mitigated; per-pair contention accepted).
