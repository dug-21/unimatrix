# Test Plan: migrate-module

## Unit Tests (format.rs inline tests)

### T-03: Base64 round-trip
- Test `encode_blob` / `decode_blob` for blob sizes: 0, 1, 2, 3, 100, 100_000 bytes
- Verify `decode(encode(bytes)) == bytes` for each size
- Verify padding edge cases (sizes 1, 2, 3 produce different padding)

### Format serde tests
- `TableHeader` serializes to expected JSON shape and deserializes back
- `DataRow` with u64 key + blob value round-trips
- `DataRow` with composite key `["topic", 42]` round-trips
- `DataRow` with null value (unit type) round-trips
- `KeyType` and `ValueType` enum serialization uses snake_case

### I/O helper tests
- `write_header` produces single JSON line ending with newline
- `write_row` produces single JSON line ending with newline
- `read_line` returns None on empty input
- `read_line` returns parsed JSON for valid input

## Integration Tests

### T-01: Full 17-table round-trip (migrate_export.rs + migrate_import.rs)

**Export side** (redb backend):
1. Create temp redb database via `Store::open()`
2. Populate all 17 tables with test data:
   - entries: 3 EntryRecords with varying fields
   - topic_index: 3 (topic, entry_id) pairs
   - category_index: 3 (category, entry_id) pairs
   - tag_index: 5 (tag, entry_id) pairs (multimap: tag "rust" maps to entries 1,2,3)
   - time_index: 3 (timestamp, entry_id) pairs
   - status_index: 3 (status, entry_id) pairs
   - vector_map: 2 (entry_id, hnsw_data_id) pairs
   - counters: 7 counters (next_entry_id=4, schema_version=5, status totals, etc.)
   - agent_registry: 2 agent records
   - audit_log: 2 audit records
   - feature_entries: 4 (feature, entry_id) pairs (multimap)
   - co_access: 2 co-access pairs (with entry_id_a < entry_id_b)
   - outcome_index: 2 (feature_cycle, entry_id) pairs
   - observation_metrics: 1 metric record
   - signal_queue: 1 signal record
   - sessions: 1 session record
   - injection_log: 1 injection log record
3. Export to temp file
4. Verify file has 17 table headers
5. Verify row counts in headers match inserted data

**Import side** (SQLite backend):
1. Read the intermediate file (either from export or from a pre-built fixture)
2. Import into temp SQLite database
3. Verify `SELECT COUNT(*)` for all 17 tables matches expected counts
4. Spot-check: read entry with id=1, deserialize from blob, verify fields match

### T-02: Blob fidelity
- Create specific records (EntryRecord, CoAccessRecord, AgentRecord, AuditRecord, etc.)
- Export from redb, import to SQLite
- Read each record from SQLite, deserialize bincode blob, compare field-by-field

### T-08: Multimap round-trip (R-04)
- Create entries with 5 tags each: tags "alpha", "beta", "gamma", "delta", "epsilon" all map to entry 1
- Also: tag "alpha" maps to entries 1, 2, 3 (testing many values per key)
- feature_entries: feature "nxs-006" maps to entries 1, 2, 3
- After round-trip, verify all associations survive
- Query tag_index: tag "alpha" returns {1, 2, 3}
- Query tag_index: entry 1 has 5 tags

### T-09: Multimap row count (R-04)
- Verify intermediate file's `row_count` for tag_index equals total (tag, entry_id) pairs, not unique tags
- E.g., 3 tags with 2 entries each = row_count 6

### T-10: Counter verification (R-05)
- After import, verify:
  - `next_entry_id` > MAX(entries.id)
  - `schema_version` == 5
  - `next_signal_id`, `next_log_id`, `next_audit_event_id` are correct
  - Status counters match actual entry counts per status

### T-11: Counter overwrite (R-05)
- Store::open() initializes `next_entry_id=1`. Import sets it to the exported value (e.g., 54).
- After import, verify `next_entry_id` is 54, not 1.

### T-12: i64::MAX boundary (R-06)
- Create entry with `id = i64::MAX as u64` (9,223,372,036,854,775,807)
- Export, import, verify id survives round-trip

### T-13: u64 overflow detection (R-06)
- Attempt to validate value `i64::MAX as u64 + 1`
- Verify `validate_i64_range` returns error

### T-14: Empty database round-trip (R-07)
- Export a freshly opened redb database (only counters populated by Store::open)
- Import into SQLite
- Verify all 17 tables exist with correct (mostly zero) row counts
- Verify counters are imported correctly

### T-15: PID file check (R-08)
- Manual verification or unit test:
  - Create a PID file with a non-unimatrix PID -> export should proceed
  - Create a PID file with current process PID (which is not a unimatrix process) -> export should proceed
