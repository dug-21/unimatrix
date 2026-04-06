## ADR-001: Baseline Window Ordering Key — `feature_cycle` Not `computed_at`

### Context

The rolling σ baseline for curation health reads the last N rows from
`cycle_review_index` to compute mean/stddev. The naive ordering key is `computed_at
DESC`, which is the write timestamp.

SR-07 flags that `computed_at` is mutable: calling `context_cycle_review force=true` on
any historical cycle updates its `computed_at` to the current time, because
`store_cycle_review()` uses `INSERT OR REPLACE` and the caller sets `computed_at` to
`now`. This means a force-recompute of a 6-month-old cycle pushes that row to the top of
a `computed_at DESC` ordering, displacing newer cycles from the baseline window. The
baseline window becomes non-deterministic and unrelated to the temporal sequence of
cycles.

`feature_cycle` is the table's primary key. It is a stable string identifier (e.g.,
`"crt-047"`, `"vnc-004"`) that does not change after first write. Alphabetical ordering
of `feature_cycle DESC` does not correspond to calendar time in general, because
feature IDs reflect phase prefixes and sequence numbers that do not sort temporally
across phases.

The cycle start timestamp (`MIN(timestamp) WHERE event_type = 'cycle_start'`) in
`cycle_events` is the authoritative temporal anchor for each cycle. However, this
requires a join at baseline query time, increasing complexity for a read-only path.

The `context_status` aggregate view needs a stable, deterministic window. The
`context_cycle_review` output baseline needs the N cycles immediately preceding the
current one.

### Decision

Use `rowid` as the ordering key for baseline window selection.

`cycle_review_index` is a SQLite table with a TEXT primary key (`feature_cycle`). SQLite
assigns `rowid` implicitly; for a TEXT primary key table, each row has a stable `rowid`
that increases monotonically with insertion order (not update order). `INSERT OR REPLACE`
on an existing key deletes the old row and inserts a new one, which **does** reassign the
`rowid`. This means `rowid` is also mutable under force-recompute.

Given that both `computed_at` and `rowid` are mutable under `INSERT OR REPLACE`:

The resolution is to add a `first_computed_at` column (INTEGER, NOT NULL) that is set
only on first insert and never overwritten. The query becomes:

```sql
SELECT corrections_total, corrections_agent, corrections_human,
       corrections_system, deprecations_total, orphan_deprecations,
       feature_cycle
FROM cycle_review_index
ORDER BY first_computed_at DESC
LIMIT ?1
```

`first_computed_at` is set to `computed_at` on the initial write and left unchanged on
subsequent `force=true` overwrites. The implementation must distinguish first-write from
overwrite to avoid clobbering `first_computed_at`.

Concretely: `store_cycle_review()` uses an `INSERT OR IGNORE` for `first_computed_at`
combined with a separate `UPDATE` for the mutable fields, or a two-step `INSERT OR
REPLACE` that reads and re-uses the existing `first_computed_at` if a row already exists.
The simpler pattern: add `first_computed_at` as a column, and let the caller pass it
(computed from the cycle_events start timestamp if available, or `now` on first write).

This is one additional column added to the v23→v24 migration block (six columns total,
not five). The five snapshot columns plus `first_computed_at` are all part of the single
v24 migration.

### Consequences

- **Easier**: Baseline window is stable across force-recompute calls. Repeated `force=true`
  on historical cycles does not perturb the baseline ordering.
- **Harder**: `store_cycle_review()` must handle the first-write vs. overwrite distinction
  for `first_computed_at`. The caller (`context_cycle_review`) must provide the cycle's
  start timestamp from `cycle_events` (or fall back to `now` if no cycle_start event
  exists). This adds one additional query to the review pipeline (already present via
  `get_cycle_start_goal`).
- `CycleReviewRecord` gains a `first_computed_at: i64` field.
- The `INSERT OR REPLACE` pattern in `store_cycle_review()` is replaced by an upsert
  that preserves `first_computed_at`.
- `get_curation_baseline_window()` in `cycle_review_index.rs` orders by
  `first_computed_at DESC`.
