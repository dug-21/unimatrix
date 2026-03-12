# Test Plan: export-module

Component: `crates/unimatrix-server/src/export.rs` -- orchestration logic
Risks covered: R-05, R-06, R-07, R-08, R-09, R-10, R-12

All tests below are integration tests in `crates/unimatrix-server/tests/export_integration.rs` unless otherwise noted. They use real SQLite databases via `Store::open()` on temp directories.

## Transaction Isolation

### T-EM-01: BEGIN DEFERRED wraps all reads (R-05)

**Risks**: R-05 (cross-table inconsistency)
**Setup**: Read the `run_export` source.
**Assert** (code review + test): Verify that `conn.execute_batch("BEGIN DEFERRED")` is called before any table read and `conn.execute_batch("COMMIT")` is called after the last table read. This is primarily a code review assertion, but the integration test below validates the behavioral consequence.

### T-EM-02: Snapshot isolation under concurrent modification (R-05)

**Risks**: R-05 (orphan tags, broken references)
**Setup**: Create a database with entries {id=1, title="original"} and entry_tags {entry_id=1, tag="alpha"}.
**Action**: Export the database. Between writing the test data and running the export, insert an additional entry {id=2} with tag {entry_id=2, tag="beta"} -- BUT because export opens its own transaction, the test validates that within a single export run, entries and entry_tags are consistent.
**Assert**:
- If entry id=2 appears in exported entries, its tag "beta" must also appear in exported entry_tags.
- If entry id=2 does not appear in exported entries, tag "beta" must not appear either.
- This is inherently guaranteed by the BEGIN DEFERRED transaction, so the primary verification is that the transaction is correctly used. A more rigorous concurrent test would require spawning a thread to write between table reads, which is fragile. Instead, verify the code structure.

**Practical approach**: Insert data, export, verify entries and entry_tags row counts are consistent (every entry_id in entry_tags has a corresponding entry in entries).

## Determinism

### T-EM-03: Byte-identical output on repeated export (R-06, AC-14)

**Risks**: R-06 (non-deterministic key ordering)
**Setup**: Create a database with representative data in all 8 tables.
**Action**: Call `run_export` twice, capturing output to two byte buffers. To eliminate `exported_at` variance, either:
  - Replace the `exported_at` value in both outputs before comparing, OR
  - If the implementation allows injecting a fixed timestamp (preferred for testability), use that.
**Assert**:
- After normalizing `exported_at`, the two outputs are byte-identical.
- Run 3 times to increase confidence (R-06 scenario 4 asks for 10, but 3 is sufficient for a unit/integration test; flaky ordering would manifest within 3 runs).

## Excluded Tables

### T-EM-04: No excluded table data in output (R-07, AC-18)

**Risks**: R-07 (excluded table leakage)
**Setup**: Create a database. Using raw SQL via `lock_conn()`, insert rows into excluded tables: `vector_map`, `sessions`, `observations`, `injection_log`, `signal_queue`, `query_log`, `observation_metrics`, `observation_phase_metrics`, `shadow_evaluations`, `topic_deliveries`. (Some of these may not exist yet -- only populate tables that exist in schema v11.)
**Action**: Export the database.
**Assert**:
- Collect all `_table` values from the output.
- The set is exactly: {counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log}.
- No excluded table name appears.

## Row Ordering

### T-EM-05: Primary key ordering within tables (R-08, AC-07)

**Risks**: R-08 (incorrect row ordering)
**Setup**: Create a database. Insert entries with IDs in non-sequential order: insert entry that gets id=1, then manipulate to have ids {5, 2, 8} via raw SQL or sequential inserts then check natural order.
- Insert entry_tags: (entry_id=1, tag="zebra"), (entry_id=1, tag="apple"), (entry_id=2, tag="mango").
- Insert co_access: (3, 5, 1, ts), (1, 2, 1, ts), (2, 4, 1, ts).
**Action**: Export.
**Assert**:
- Entries rows appear in id order: 1, 2, 5, 8 (or whatever IDs were assigned).
- Entry_tags rows appear in order: (1, "apple"), (1, "zebra"), (2, "mango").
- Co_access rows appear in order: (1, 2), (2, 4), (3, 5).
- Counters rows appear in alphabetical name order.

## Migration Side-Effect

### T-EM-06: Export does not modify database file (R-09)

**Risks**: R-09 (migration side-effect)
**Setup**: Create a database via `Store::open()` (which runs migrations). Close it. Record the file's modification time and size.
**Action**: Call `run_export` on the same database.
**Assert**:
- After export, the database file size is unchanged.
- Modification time is unchanged (or within filesystem timestamp granularity if WAL checkpoint occurs -- if this is flaky, compare file content hash instead).

**Note**: `Store::open()` inside `run_export` will open the same database. Since it is already at the current schema, no migration should occur. The WAL file may be modified as a side-effect of SQLite opening in WAL mode. This test verifies the main database file is stable; WAL-related changes are acceptable.

## Error Handling

### T-EM-07: Write failure mid-stream (R-10)

**Risks**: R-10 (partial output on error)
**Setup**: Create a database with entries. Create a writer that fails after writing N bytes (a mock `impl Write` that returns `Err(io::Error)` after a threshold).
**Action**: Call the internal export functions with this failing writer.
**Assert**:
- The function returns `Err(...)`.
- The error is an I/O error.

**Note**: This test may need to be a unit test within `export.rs` if `run_export` constructs the writer internally. If the export functions take `&mut impl Write`, this is straightforward to test.

## Empty Database

### T-EM-08: Fresh database export (R-12, AC-10)

**Risks**: R-12 (empty database crash)
**Setup**: Call `Store::open()` on a new temp path (creates fresh database with schema + counters).
**Action**: Export.
**Assert**:
- Output has at least 1 line (header).
- Header has `entry_count: 0`.
- Counter rows present (schema_version, next_entry_id, next_signal_id, next_log_id, next_audit_event_id -- exact set depends on schema).
- No entry rows, no entry_tag rows, no co_access rows, etc.
- Every line is valid JSON.

## Header Validation

### T-EM-09: Header fields correct (AC-03)

**Setup**: Create a database with 3 entries.
**Action**: Export.
**Assert**:
- First line has `_header: true`.
- `schema_version` is a positive integer (matches the counter table value).
- `exported_at` is a recent Unix timestamp (within 60 seconds of now).
- `entry_count` equals 3.
- `format_version` equals 1.
- No other keys beyond these 5.

### T-EM-10: Every non-header line has _table (AC-04)

**Setup**: Create a database with data in all 8 tables.
**Action**: Export.
**Assert**: For every line after the first, parse as JSON and assert `_table` key exists with a string value from the allowed set.

## Full Export Verification

### T-EM-11: Representative data across all 8 tables (AC-05, AC-17)

**Setup**: Create a database and populate:
- 3 entries with non-default values for all 26 columns (including nullable fields both NULL and non-NULL).
- 4 entry_tags across those entries.
- 2 co_access pairs.
- 2 feature_entries.
- 2 outcome_index rows.
- 2 agent_registry rows (one with capabilities, one with NULL allowed_topics).
- 3 audit_log rows.
**Action**: Export.
**Assert**:
- All 8 `_table` values appear.
- Row counts match: 5+ counter rows, 3 entries, 4 entry_tags, 2 co_access, 2 feature_entries, 2 outcome_index, 2 agent_registry, 3 audit_log.
- Spot-check specific field values on at least one entry row (all 26 columns).

### T-EM-12: Table emission order (AC-08)

**Setup**: Same as T-EM-11.
**Action**: Export. Collect `_table` values in order of appearance.
**Assert**:
- First `_table` value seen is "counters".
- "entries" appears before "entry_tags".
- "entry_tags" appears before "co_access".
- Full order: counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log.

### T-EM-13: Performance benchmark -- 500 entries under 5 seconds (AC-11)

**Setup**: Populate database with 500 entries, each with 2 tags and 1 co_access pair.
**Action**: Measure wall-clock time for `run_export`.
**Assert**: Total time < 5 seconds.
