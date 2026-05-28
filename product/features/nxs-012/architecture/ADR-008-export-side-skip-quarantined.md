## ADR-008: Export-Side Skip-Quarantined Filter via Pre-Query HashSet

### Context

When rebuilding a Unimatrix database via export/import, quarantined entries (status = 3) are entries already identified as unwanted -- stale, contradicted, or manually rejected. Carrying them in the export file serves no purpose.

ADR-007 originally placed the `--skip-quarantined` filter on the import side, building a `HashSet<i64>` from entry rows during `ingest_rows`. That design has been superseded (see ADR-007 SUPERSEDED notice) because:

1. **The export file still contained quarantined data.** Anyone inspecting the export or using it for auditing would see entries marked as unwanted.
2. **Import became non-trivial.** Import had to track skip state, manage counters for skipped rows, and conditionally insert -- adding complexity to what should be a simple full-restore.
3. **Hash integrity was fragile.** Skipping rows during import meant the imported data did not match the export file's hash, requiring either `--skip-hash-validation` or a separate hash computation that excluded skipped rows.

By moving filtering to export time, the export file is a clean snapshot. Import remains a simple full-restore where every row in the file is inserted and hash integrity is preserved end-to-end.

Five table types reference entry IDs and are affected:
- `entries` -- the source; `id` column, `status` column determines skip-set membership
- `entry_tags` -- `entry_id` references entries
- `feature_entries` -- `entry_id` references entries
- `co_access` -- `entry_id_a` and `entry_id_b` both reference entries
- `graph_edges` -- `source_id` and `target_id` both reference entries

Six table types do NOT reference entry IDs and are unaffected:
- `counters`, `outcome_index`, `agent_registry`, `audit_log` -- no entry ID columns
- `observations` -- references sessions, not entries
- `cycle_events` -- references cycles, not entries

Three alternatives were considered:

**Option A: Pre-query HashSet inside the DEFERRED transaction.** Before any table export function runs, query `SELECT id FROM entries WHERE status = 3` to build the skip set. Pass it to each of the 5 affected table exporters. The query runs inside the existing `BEGIN DEFERRED` snapshot transaction, ensuring consistency with the actual rows being exported.

**Option B: SQL WHERE clause filtering.** Modify each table exporter's SQL query to add `WHERE status != 3` (entries) or `WHERE entry_id NOT IN (SELECT id FROM entries WHERE status = 3)` (dependents). Correct but repeats the subquery 5 times, adds query complexity, and makes the skip logic implicit in SQL rather than explicit in Rust.

**Option C: Post-export filtering.** Export all rows, then strip quarantined rows from the JSONL file in a second pass. Requires parsing and rewriting the entire file, recomputing the footer hash, and doubles I/O.

### Decision

Option A. Add a `--skip-quarantined` boolean flag to the **export** CLI (default: false). When enabled:

1. **Skip-set construction**: Inside the existing `BEGIN DEFERRED` snapshot transaction (before any `export_*` call in `do_export`), execute:
   ```sql
   SELECT id FROM entries WHERE status = 3
   ```
   Collect results into a `HashSet<i64>` named `skip_ids`. This query shares the same snapshot as all subsequent table reads, eliminating TOCTOU races (SR-02).

2. **Signature change**: `do_export` gains a parameter `skip_ids: &HashSet<i64>`. When `--skip-quarantined` is false, pass an empty `HashSet` -- zero overhead on the default path. Each of the 5 affected table exporters gains the same parameter.

3. **entries**: After fetching each row, check `row.get::<i64, _>(0)` (the `id` column) against `skip_ids`. If present, skip the `write_row` call. Increment `skipped_entries` counter.

4. **entry_tags**: Check `row.get::<i64, _>(0)` (`entry_id`) against `skip_ids`. Skip if present.

5. **feature_entries**: Check `row.get::<i64, _>(1)` (`entry_id`) against `skip_ids`. Skip if present.

6. **co_access**: Check both `row.get::<i64, _>(0)` (`entry_id_a`) and `row.get::<i64, _>(1)` (`entry_id_b`) against `skip_ids`. Skip if either is present.

7. **graph_edges**: Check both `row.get::<i64, _>(0)` (`source_id`) and `row.get::<i64, _>(1)` (`target_id`) against `skip_ids`. Skip if either is present.

8. **counters, outcome_index, agent_registry, audit_log, observations, cycle_events**: No entry ID references. Pass `skip_ids` is unnecessary; these functions are unchanged.

9. **Export summary**: When `--skip-quarantined` is active, report to stderr: count of skipped entries and count of skipped dependent rows across all affected tables (AC-28).

10. **Header metadata**: The export header gains an optional `skip_quarantined: true` field when the flag is active, so downstream consumers can identify filtered exports.

11. **Default path (AC-29)**: When `--skip-quarantined` is false, `skip_ids` is an empty `HashSet`. The `contains()` calls are O(1) no-ops on an empty set. No behavioral change to the existing export path.

12. **Hash integrity (AC-31)**: The export file contains only the rows that passed filtering. The footer hash covers exactly these rows. Import validates against the same set -- no mismatch, no need for `--skip-hash-validation`.

The flag threads through: `run_export` -> `run_export_inner` -> async block -> `do_export(pool, writer, &skip_ids)`.

### Consequences

- The export file is a clean snapshot -- no quarantined entries, no orphaned dependent rows
- Import remains a simple full-restore with no skip logic, no extra parameters, no conditional inserts
- Hash integrity is preserved end-to-end: export hashes the filtered set, import validates the same set
- The skip-set query and all table reads share one `BEGIN DEFERRED` snapshot -- no TOCTOU race
- All 5 affected table exporters must consistently check `skip_ids` -- a missed check produces orphaned rows in the export file (SR-08). The round-trip integration test must verify no orphaned references.
- Memory overhead is negligible (typical skip sets are 0-50 entries, ~24 bytes each)
- `audit_log` rows are NOT filtered even when their metadata references quarantined entry IDs -- audit_log is an append-only integrity record
- Future table types that reference entry IDs must add their own `skip_ids.contains()` check in the export function
- Supersedes ADR-007 (import-side design)
