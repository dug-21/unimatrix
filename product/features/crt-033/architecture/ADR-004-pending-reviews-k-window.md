## ADR-004: pending_cycle_reviews K-window Scoping via cycle_events.cycle_start

### Context

`context_status` needs to report cycles that have accumulated raw signals but have no
`cycle_review_index` row. Two scoping questions must be answered:

**Q1: Which table defines "has raw signals"?**

Options considered:
- `query_log` with `feature_cycle` set: would capture cycles where Unimatrix was actively
  queried. However, `query_log.feature_cycle` does not exist as a column in the current
  schema. Introducing it would require a separate migration not in scope for crt-033.
- `cycle_events` table with `event_type = 'cycle_start'`: `cycle_events` rows are written
  when `context_cycle(type="start")` is called. A `cycle_start` event means the cycle was
  formally initiated and attribution was declared. This is the cleanest available signal:
  already timestamped (enabling K-window filtering), already scoped to post-col-022 cycles
  (naturally excludes pre-cycle_events era without extra conditions), and directly keyed by
  `cycle_id` matching `cycle_review_index.feature_cycle`.

**Decision**: use `cycle_events` with `event_type = 'cycle_start'` as the signal existence
indicator. Post-col-022, a properly-run cycle always has a `cycle_start` event. This is
cleaner than `query_log` because it is already timestamped, avoids requiring a new column,
and the semantic shift ("formally started" vs "Unimatrix was queried") is acceptable for
the operational use case.

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
- SQL: set difference of K-window `cycle_events` (`event_type = 'cycle_start'`) vs `cycle_review_index`
- Default K-window: 90 days, named `PENDING_REVIEWS_K_WINDOW_SECS` in `services/status.rs`
- Always computed in `compute_report()` Phase 7b (no opt-in parameter)
- Pre-cycle_events cycles excluded by definition (no `cycle_events` rows)

### Consequences

**Easier**:
- Operators always see pending backlog without having to request it.
- K-window prevents stale pre-table cycles from flooding the list permanently.
- `read_pool()` avoids write pool contention during status queries.
- `cycle_events.timestamp` column already exists — no schema change required.
- Pre-cycle_events exclusion is automatic (cycles without `cycle_events` rows don't appear).

**Harder**:
- The 90-day default must be reconciled with #409 at merge time. If #409 uses a shorter
  window (e.g., 30 days), cycles in the 31–90 day range would appear in `pending_reviews`
  but would not be at risk from #409's purge — a false positive for operators.
- Cycles that had `context_cycle(start)` called but zero Unimatrix queries will appear as
  pending. These are genuine pending reviews (the cycle was declared), so this is accepted.
