## ADR-004: pending_cycle_reviews K-window Scoping via query_log.feature_cycle

### Context

`context_status` needs to report cycles that have accumulated raw signals but have no
`cycle_review_index` row. Two scoping questions must be answered:

**Q1: Which table defines "has raw signals"?**

Options considered:
- `cycle_events` table: has cycle_id column; directly tracks cycles. But `cycle_events`
  rows represent lifecycle events (start/stop), not signal accumulation. A cycle with a
  `cycle_start` event but no observations is not "pending" in the #409 sense.
- `query_log` with `feature_cycle` set: `query_log` rows are written when Unimatrix tools
  are called during an active session attributed to a feature. A `feature_cycle` value in
  `query_log` means agents were actively querying Unimatrix for that cycle — a meaningful
  signal of activity. This matches #409's purge window concept (query_log rows are what
  #409's retention policy would delete).

**Decision**: use `query_log.feature_cycle` (NOT `cycle_events.cycle_id`) as the signal
existence indicator. This is consistent with #409's domain and avoids inflating the list
with lifecycle-only cycles that had no Unimatrix queries.

**Q2: What is the K-window boundary?**

SR-04 flagged that GH #409's K-window constant is not yet merged. Deferring to delivery
risks an arbitrary placeholder. The architecture pins a default:

- Default: **90 days** (`PENDING_REVIEWS_K_WINDOW_SECS = 90 * 24 * 3600 = 7_776_000`)
- Named constant: `PENDING_REVIEWS_K_WINDOW_SECS` in `services/status.rs`
- Cutoff computation: `let cutoff_ts = now_unix_secs() - PENDING_REVIEWS_K_WINDOW_SECS`
- Not inlined at the call site

Rationale for 90 days: active development cycles complete within weeks. 90 days captures
all recently active cycles without pulling in historical data that predates the
`cycle_review_index` table itself (those records will never have a review row and should
be excluded, not flagged as pending forever).

Reconciliation at merge time: when #409 merges with its own retention window constant,
delivery aligns `PENDING_REVIEWS_K_WINDOW_SECS` with that value. If #409 exposes the
constant publicly, import it. Otherwise keep the local constant with a comment:
`// Must match #409's RETENTION_WINDOW_SECS when that feature merges`.

**Q3: Should the query be always-on or opt-in?**

The set-difference SQL operates on two K-window-bounded tables:
```sql
SELECT DISTINCT ql.feature_cycle
FROM query_log ql
WHERE ql.feature_cycle IS NOT NULL
  AND ql.feature_cycle != ''
  AND ql.queried_at >= ?
  AND ql.feature_cycle NOT IN (
      SELECT feature_cycle FROM cycle_review_index
  )
ORDER BY ql.feature_cycle
```

Both `query_log` and `cycle_review_index` are small relative to the full database. The
`queried_at >= ?` predicate prunes `query_log` to the K-window. The subquery on
`cycle_review_index` is an indexed PK scan. Total cost is low even at scale. No opt-in
gate needed — pending reviews are a health signal that belongs in the always-on status
report (consistent with the `maintain` precedent in crt-005 ADR-002 which made maintenance
opt-in, but status aggregates always-on).

**Q4: Pool selection for pending_cycle_reviews()**

Entry #3619 lesson: for read-only aggregates called from `context_status`, the correct
pool is `read_pool()`, not `write_pool_server()`. `compute_status_aggregates()` (the direct
structural precedent) uses `read_pool()`. No write-adjacent read path exists here.

### Decision

- `pending_cycle_reviews(k_window_cutoff: i64) -> Result<Vec<String>>` uses `read_pool()`
- SQL: set difference of K-window `query_log.feature_cycle` vs `cycle_review_index`
- Default K-window: 90 days, named `PENDING_REVIEWS_K_WINDOW_SECS` in `services/status.rs`
- Always computed in `compute_report()` Phase 7b (no opt-in parameter)
- NULL and empty `feature_cycle` values excluded in the SQL WHERE clause

### Consequences

**Easier**:
- Operators always see pending backlog without having to request it.
- K-window prevents stale pre-table cycles from flooding the list permanently.
- `read_pool()` avoids write pool contention during status queries.

**Harder**:
- The 90-day default must be reconciled with #409 at merge time. If #409 uses a shorter
  window (e.g., 30 days), cycles in the 31–90 day range would appear in `pending_reviews`
  but would not be at risk from #409's purge — a false positive for operators.
- `query_log` NULL handling: the WHERE clause guards `IS NOT NULL AND != ''`, but if
  `query_log.feature_cycle` is consistently unpopulated for a deployment, the list will
  always be empty. This is documented as a known assumption in the scope (pre-existing
  `query_log.feature_cycle` population reliability).
