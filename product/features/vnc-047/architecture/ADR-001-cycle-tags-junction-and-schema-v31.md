## ADR-001 vnc-047: `cycle_tags` junction is the source of truth; `CURRENT_SCHEMA_VERSION` 30→31 (version cascade #1)

### Context

Cycle tags need a durable, queryable home. Four candidate substrates exist at HEAD, and issue #940's
premise (that tags "must ride the durable `cycle_review_index` aggregate row") is wrong:

- `sessions` (+ `keywords` JSON) — **purgeable** by `gc_cycle_activity` (retention.rs:169); disqualified.
- `cycle_events` — protected (survives GC), but keyed `(cycle_id, seq, event_type)`; a multi-valued
  tag set does not fit a per-event row and would not be independently queryable by tag.
- `cycle_review_index` — written **only at review time** (`store_cycle_review`); the row does not
  exist during the run, so it cannot be the source of truth for a value set at start.
- A new junction — mirrors the proven `entry_tags` model exactly.

The durability correction: protection is by omission from retention's DELETE list, not membership in
`cycle_review_index` (see ADR-005). So durability forces "not `sessions`," not "`cycle_review_index`."

The queryable-substrate requirement (deferred filter/learn-by-tag, SR-04) argues for a junction with a
`(tag)` index rather than a JSON blob (pattern #373: junction when a column is queried by element).

### Decision

Create a new junction table, source of truth for cycle tags:

```sql
CREATE TABLE cycle_tags (
    feature_cycle TEXT NOT NULL,
    tag           TEXT NOT NULL,
    PRIMARY KEY (feature_cycle, tag)
);
CREATE INDEX idx_cycle_tags_tag ON cycle_tags(tag);
```

This is `entry_tags` (migration.rs:1689) re-keyed `entry_id → feature_cycle`. No FK: `feature_cycle`
is a free-text cycle id, not a row id (there is no single parent table to CASCADE from — parity with
how `cycle_events.cycle_id` carries no FK). The `(tag)` index is the substrate the deferred
cross-cycle query direction builds on with no re-migration (SR-04).

**Version cascade #1 — `CURRENT_SCHEMA_VERSION` 30 → 31** (migration.rs:26). This is a real DB
migration and must land on **three paths**, each a discrete implementation line-item (SR-01):

1. **Fresh-create** — add `CREATE TABLE IF NOT EXISTS cycle_tags (...)` + the index to
   `create_tables_if_needed` (db.rs:534) beside the `entry_tags` block (db.rs ~:574).
2. **Migration step** — add `if current_version < 31 { CREATE TABLE IF NOT EXISTS cycle_tags ...;
   CREATE INDEX IF NOT EXISTS idx_cycle_tags_tag ...; UPDATE counters SET value = 31 WHERE name =
   'schema_version' }` after the existing `if current_version < 30` block (migration.rs:1474).
3. **Idempotency guard** — `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` are the guard
   for a brand-new table (the `pragma_table_info` COUNT pre-check is only needed for `ALTER TABLE ADD
   COLUMN`, which this is not). Re-running the step is a no-op (NFR-06 parity).

Re-verify 31 is free at implementation start, not just at design (SR-02, lesson #4095): a parallel
feature merging first may claim 31, forcing renumber.

### Consequences

- Easier: physical + semantic parity with `entry_tags`; the write/read primitives port 1:1 (ADR-002/004).
- Easier: `(tag)` index makes the deferred query direction additive — no future migration of stored data.
- Harder: a genuinely new table means all three creation paths must be updated; missing the
  fresh-create path yields a table that exists only on upgraded DBs (a known class of miss).
- Harder: this is one of two independent version cascades in the feature (the other is ADR-004);
  they must be tracked separately (SR-01) — do not lump "bump versions" into one task.
- No FK means orphaned tag rows are possible if a `feature_cycle` is otherwise fully purged; accepted —
  `cycle_tags` is GC-protected (ADR-005) and long-run analysis wants the labels retained regardless.
