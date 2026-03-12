# nan-002 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `unimatrix-server import --input <path>` CLI subcommand exists and restores a knowledge base from a nan-001 export file | test | Integration test: export populated DB, import into fresh DB, verify all data present |
| AC-02 | `--force` flag drops all existing data before import, enabling restore into a populated database | test | Integration test: populate DB, import with --force, verify old data gone and new data present |
| AC-03 | Header line validated: format_version must equal 1; schema_version must be <= CURRENT_SCHEMA_VERSION | test | Unit test: valid headers pass; invalid headers (format_version 0, 2; schema_version 999) produce errors |
| AC-04 | Import rejects format_version != 1 with an error message naming the unsupported version | test | Unit test: header with format_version 2, verify error message contains "2" |
| AC-05 | Import rejects schema_version > CURRENT_SCHEMA_VERSION with error suggesting binary upgrade | test | Unit test: header with schema_version 999, verify error message contains "upgrade" |
| AC-06 | Import into non-empty database rejected without --force; accepted with --force | test | Integration test: both paths exercised -- rejection message and successful force-import |
| AC-07 | All 8 tables restored: entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, counters | test | Integration test: round-trip export/import, query each table, verify row counts match |
| AC-08 | Imported entries preserve all 26 columns exactly including confidence, helpful_count, unhelpful_count, access_count, last_accessed_at, content_hash, previous_hash, version, trust_source, pre_quarantine_status | test | Integration test: compare each column of each entry before export and after import |
| AC-09 | Counter values restored; post-import inserts do not collide with imported IDs | test | Integration test: import entries up to ID 100, insert new entry, verify ID > 100 |
| AC-10 | All entries re-embedded with current ONNX model (384-dim) and inserted into HNSW index | test | Integration test: after import, verify VectorIndex contains correct entry count; perform semantic search and verify results |
| AC-11 | After import, MCP server starts and serves queries with working semantic search | test | Integration test: import, start server, issue context_search, verify results returned |
| AC-12 | Hash chain validation: non-empty previous_hash entries have matching content_hash in dataset (unless --skip-hash-validation) | test | Unit test: valid chain passes; broken chain (previous_hash with no matching content_hash) produces error with entry ID |
| AC-13 | Content hash validation: recomputed hash matches stored content_hash (unless --skip-hash-validation) | test | Unit test: valid entry passes; tampered content produces error listing entry ID |
| AC-14 | --skip-hash-validation bypasses checks with warning to stderr | test | Integration test: import with tampered hash and --skip-hash-validation, verify warning emitted and import succeeds |
| AC-15 | Round-trip: export, import, re-export produces identical output (excluding exported_at) | test | Integration test: byte-level comparison of two exports after normalizing exported_at timestamp |
| AC-16 | Empty export (header + counters, no entries) imports successfully | test | Integration test: import empty export, verify valid empty database with correct counters |
| AC-17 | 500-entry import completes re-embedding in under 60 seconds | test | Performance test: create 500-entry export, time the import, assert < 60s |
| AC-18 | Import does not require a running MCP server | file-check | Verified by design: subcommand opens database directly via Store::open(), no server dependency in code path |
| AC-19 | --project-dir flag respected by import subcommand | test | Integration test: import with --project-dir to non-default location, verify database created at specified path |
| AC-20 | Exit code 0 on success, non-zero on error; errors/warnings to stderr | test | Integration test: check exit codes for success case and at least 3 failure cases (bad header, non-empty DB, parse error) |
| AC-21 | Malformed JSONL line produces error with line number | test | Unit test: corrupt line 5 of 10, verify error message contains "line 5" |
| AC-22 | Entire import is atomic: failure rolls back transaction | test | Integration test: inject parse failure mid-import (corrupt line after valid entries), verify database has zero entries after rollback |
| AC-23 | Unit tests verify JSONL deserialization for each table type including edge cases (null fields, empty strings, unicode, max integer values) | test | Unit tests: one or more test per table type covering null optionals, empty strings, unicode content, i64::MAX, JSON-in-TEXT columns |
| AC-24 | Integration test: full round-trip with data across all 8 tables | test | Integration test: create database with entries, tags, co-access pairs, feature_entries, outcome_index, agent_registry, audit_log, counters; export, import, verify all tables |
| AC-25 | Progress reporting to stderr during import | shell | `unimatrix-server import --input test.jsonl 2>&1 >/dev/null` and verify stderr contains "Inserted" and "Embedding" progress text |
| AC-26 | Import operation recorded in audit log after restoring exported audit log | test | Integration test: after import, query audit_log for provenance entry with operation containing "import"; verify event_id does not collide with imported audit entries |
| AC-27 | --force on populated database drops existing data and imports successfully | test | Integration test: populate DB with 10 entries, force-import 5 different entries, verify only the 5 imported entries exist |
