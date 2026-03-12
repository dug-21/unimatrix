# Risk-Based Test Strategy: nan-002 (Knowledge Import)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Direct SQL INSERT diverges from schema DDL after future column additions or renames, causing silent data loss or insertion failures | High | High | Critical |
| R-02 | Format deserialization fails on edge-case field values (null optionals, empty strings, unicode, max i64, JSON-in-TEXT columns like capabilities/allowed_topics) | High | High | Critical |
| R-03 | Counter restoration race: Store::open() auto-initializes counters, then import overwrites with INSERT OR REPLACE -- if any counter name is misspelled or missing, post-import IDs collide with imported IDs | High | Med | High |
| R-04 | --force drops all data irreversibly; accidental use on production database with no prior export destroys the knowledge base | High | Med | High |
| R-05 | Embedding fails after DB commit (ADR-004 split), leaving database without vector index; server starts but semantic search returns zero results silently | High | Med | High |
| R-06 | Foreign key violation during import if JSONL line order deviates from dependency order (e.g., entry_tags line appears before its parent entry) | Med | Med | Med |
| R-07 | Hash chain validation misses edge cases: entries with empty title, empty content, or both -- compute_content_hash edge-case behavior must match export-time computation exactly | Med | Med | Med |
| R-08 | Concurrent MCP server holds database connection during import; SQLite write transaction contends with server's WAL writes, causing SQLITE_BUSY or deadlock | Med | Med | Med |
| R-09 | ONNX model unavailable in air-gapped/CI environments; import fails at step 5 after DB commit with unclear error messaging | Med | Med | Med |
| R-10 | Round-trip fidelity loss: floating-point confidence values (f64) lose precision through JSON serialization/deserialization, causing AC-15 round-trip test failure | Med | Med | Med |
| R-11 | ExportRow serde tagged enum with `#[serde(tag = "_table")]` fails on unknown _table values from future export versions, producing opaque deserialization error instead of actionable message | Med | Low | Low |
| R-12 | Large import (1000+ entries) exceeds 60-second embedding target or causes excessive memory from HNSW index growth | Low | Low | Low |
| R-13 | Audit log provenance entry written after restored audit log could have event_id collision if counter restoration does not cover audit event IDs | Med | Med | Med |
| R-14 | --project-dir resolution differs between import and export subcommands, causing import to write to wrong database location | Med | Low | Low |
| R-15 | Path traversal via crafted input file path or malicious JSONL content injecting SQL through string fields | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Direct SQL INSERT / Schema DDL Divergence
**Severity**: High
**Likelihood**: High
**Impact**: Silent data loss -- columns present in export but absent from INSERT statement are dropped. Or insertion failure if new NOT NULL columns lack values.

**Test Scenarios**:
1. Round-trip test (AC-15): export a fully populated database, import into fresh DB, re-export, compare byte-for-byte (excluding exported_at). Any column mismatch surfaces as a diff.
2. Per-column verification test (AC-08): after import, query each of the 26 entry columns and compare against the original values. Cover all column types: integers, floats, strings, nullable integers, empty strings.
3. Compile-time guard: verify that the format::EntryRow struct field count matches the entries DDL column count. A static assertion or test that queries `PRAGMA table_info(entries)` and compares column names against EntryRow fields.

**Coverage Requirement**: Every column in every table must round-trip without transformation. A single missing or misnamed column is a test failure.

### R-02: Format Deserialization Edge Cases
**Severity**: High
**Likelihood**: High
**Impact**: Import crashes or silently drops entries that contain uncommon but valid field values.

**Test Scenarios**:
1. Null optional fields: entry with `supersedes: null`, `superseded_by: null`, `pre_quarantine_status: null` deserializes correctly.
2. Empty string fields: entry with `previous_hash: ""`, `feature_cycle: ""`, `trust_source: ""` deserializes correctly.
3. Unicode content: entry with multi-byte UTF-8 in title and content (CJK, emoji, combining characters) round-trips correctly.
4. Maximum integer values: entry with `access_count: i64::MAX`, `helpful_count: i64::MAX` deserializes without overflow.
5. JSON-in-TEXT columns: AgentRegistryRow with `capabilities: "[\"admin\",\"read\"]"` and `allowed_topics: null` deserializes correctly. Verify the raw JSON string is preserved, not re-parsed.
6. Malformed JSON on specific line: corrupt line 5 of 10, verify error message contains "line 5".

**Coverage Requirement**: Each of the 8 table row types has at least one edge-case deserialization unit test (AC-23).

### R-03: Counter Restoration and ID Collision
**Severity**: High
**Likelihood**: Med
**Impact**: Post-import entry inserts reuse IDs of imported entries, corrupting the database.

**Test Scenarios**:
1. Import a database with entries up to ID 100, then insert a new entry via Store API. Verify the new entry ID is 101 (or higher), not 1.
2. Verify all counter names from export (next_entry_id, next_signal_id, schema_version) are present in the database after import with correct values.
3. Import with --force on a database that already has entries with IDs 1-50. After force-import of entries 1-100, insert a new entry. Verify ID > 100.

**Coverage Requirement**: Post-import insert must never collide with any imported ID (AC-09).

### R-04: Destructive --force Without Safety Net
**Severity**: High
**Likelihood**: Med
**Impact**: User loses entire knowledge base with no recovery path.

**Test Scenarios**:
1. --force on populated database: verify old entries are gone, new entries are present (AC-27).
2. --force emits stderr warning with the count of dropped entries (ADR-003 format).
3. Non-empty database WITHOUT --force: verify import is rejected with an error suggesting --force (AC-06).
4. --force on an empty database: verify it proceeds without error (no-op drop).

**Coverage Requirement**: Both paths (with and without --force) must be tested on both empty and populated databases.

### R-05: Embedding Failure After DB Commit
**Severity**: High
**Likelihood**: Med
**Impact**: Database is restored but search is non-functional. User may not realize search is broken.

**Test Scenarios**:
1. Simulate ONNX model unavailability after DB commit. Verify: (a) database contains all entries, (b) error message clearly states embedding failed, (c) exit code is non-zero, (d) summary distinguishes "DB restored" from "embedding failed".
2. Successful import: verify VectorIndex is persisted to vector/ directory and contains the correct number of entries (AC-10).
3. After successful import, perform a semantic search query and verify results are returned (AC-11).

**Coverage Requirement**: The two-phase success/failure must be clearly communicated. Test both phases independently.

### R-06: Foreign Key Violation on Misordered JSONL
**Severity**: Med
**Likelihood**: Med
**Impact**: Import fails with an opaque SQLite FK error instead of a clear message.

**Test Scenarios**:
1. Import a valid export file (dependency-ordered). Verify success.
2. Import a file with entry_tags appearing before entries. Verify FK violation produces an error (correct behavior) with the failing line number.
3. Verify co_access rows referencing non-existent entry IDs fail with FK error.

**Coverage Requirement**: FK enforcement must be active during import. Misordered files must fail, not silently skip rows.

### R-07: Hash Chain Validation Edge Cases
**Severity**: Med
**Likelihood**: Med
**Impact**: Content tampering goes undetected, or valid entries are falsely flagged as corrupted.

**Test Scenarios**:
1. Valid chain: entries A -> B (B.previous_hash == A.content_hash). Validation passes.
2. Broken chain: entry with previous_hash pointing to a non-existent content_hash. Validation fails with entry ID.
3. Content hash mismatch: modify content after export. Recomputed hash differs from stored. Validation fails with entry ID (AC-13).
4. Edge case: entry with empty title and non-empty content. Verify compute_content_hash produces the same hash as export-time computation.
5. Edge case: entry with empty previous_hash (chain root). Validation skips this entry.
6. --skip-hash-validation: import with tampered content succeeds with warning to stderr (AC-14).

**Coverage Requirement**: Both content hash and chain integrity validation must be tested with valid, invalid, and edge-case inputs.

### R-08: Concurrent Server During Import
**Severity**: Med
**Likelihood**: Med
**Impact**: SQLITE_BUSY errors or data corruption from concurrent writes.

**Test Scenarios**:
1. PID file exists (server running): verify import emits a warning to stderr but proceeds.
2. PID file absent: verify no warning emitted.

**Coverage Requirement**: Warning detection via PID file / flock must function. Import should not silently corrupt data under contention.

### R-09: ONNX Model Unavailable
**Severity**: Med
**Likelihood**: Med
**Impact**: Import fails at embedding phase with potentially unclear error.

**Test Scenarios**:
1. OnnxProvider::new() failure produces a clear error message mentioning the model name and suggesting how to pre-cache it.
2. Error occurs after DB commit -- verify database state is valid (entries queryable by ID).

**Coverage Requirement**: Error messaging must be actionable (AC-10 prerequisite).

### R-10: Floating-Point Round-Trip Fidelity
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-15 round-trip test fails due to f64 precision loss through JSON.

**Test Scenarios**:
1. Entry with confidence = 0.8723456789012345 (full f64 precision). Export, import, re-export. Compare values.
2. Entry with confidence = 0.0 and confidence = 1.0 (boundary values). Round-trip correctly.
3. co_access count values at i64 boundaries.

**Coverage Requirement**: JSON serialization must preserve f64 precision to at least 15 significant digits. serde_json does this by default, but test explicitly.

### R-11: Unknown _table Discriminator
**Severity**: Med
**Likelihood**: Low
**Impact**: Opaque serde error when importing from a newer export that added a new table type.

**Test Scenarios**:
1. JSONL line with `_table: "unknown_table"`. Verify error message includes the unrecognized table name and line number.

**Coverage Requirement**: Unknown discriminators must produce actionable errors, not panics.

### R-13: Audit Log Provenance Event ID Collision
**Severity**: Med
**Likelihood**: Med
**Impact**: Provenance entry overwrites an imported audit log entry or violates PK constraint.

**Test Scenarios**:
1. Import a database with audit log entries up to event_id 50. Verify the provenance entry has event_id > 50.
2. Verify counter for audit event IDs is restored before provenance entry is written.

**Coverage Requirement**: Provenance entry must not collide with any imported audit log entry (AC-26).

### R-14: --project-dir Resolution Mismatch
**Severity**: Med
**Likelihood**: Low
**Impact**: Import writes to the default data directory instead of the specified one, or vice versa.

**Test Scenarios**:
1. Import with --project-dir pointing to a non-default directory. Verify database is created at the specified path (AC-19).
2. Export from project-dir A, import into project-dir B. Verify data lands in B.

**Coverage Requirement**: Path resolution must match the export subcommand behavior.

### R-15: Malicious Input / Injection
**Severity**: Med
**Likelihood**: Low
**Impact**: SQL injection through crafted JSONL string fields, or path traversal.

**Test Scenarios**:
1. Entry with title containing SQL injection attempt: `'; DROP TABLE entries; --`. Verify parameterized queries prevent execution.
2. Entry with content containing extremely large strings (1MB+). Verify import handles gracefully (no OOM, correct insertion or bounded rejection).
3. JSONL with duplicate entry IDs. Verify PK violation is caught.

**Coverage Requirement**: All SQL inserts must use parameterized queries. No string interpolation into SQL.

## Integration Risks

1. **Export/Import format contract coupling** (R-01, R-02): The shared format.rs types (ADR-001) mitigate drift, but export still uses serde_json::Value serialization. Any export change that produces JSON keys not matching format struct field names breaks import silently at runtime. The round-trip test (AC-15) is the primary integration safety net.

2. **Store::open() initialization vs. import overwrite**: Store::open() applies migrations and initializes default counters. Import then overwrites counters with exported values. If Store::open() adds new tables or counters in a future schema version, import must also handle those. The schema_version check (header.schema_version <= CURRENT) guards against newer exports, but not against a newer binary importing an older export where new tables are absent.

3. **VectorIndex construction after commit**: VectorIndex::new() takes an Arc<Store>. If the store connection state is affected by the preceding transaction (e.g., WAL checkpoint needed), index construction could encounter unexpected state. Test that VectorIndex operations work correctly on a just-committed database.

4. **embed_entries batch size assumption**: The architecture specifies batch size 64. If embed_entries or OnnxProvider has internal batch limits or memory scaling issues, large batches could fail. Test with batch boundaries (exactly 64, 65, 128 entries).

## Edge Cases

1. **Empty export**: Header + counters only, zero entries. Must produce a valid empty database with correct counters (AC-16).
2. **Single-entry export**: Minimum viable import. One entry, one tag, no co-access. Verify all tables handled correctly with minimal data.
3. **Entry with all nullable fields null**: supersedes, superseded_by, pre_quarantine_status, allowed_topics, allowed_categories all null.
4. **Entry with maximum field lengths**: Title and content at practical upper bounds (64KB+). Verify no truncation.
5. **Co-access pair with entry_id_a == entry_id_b**: Self-referential co-access. Verify handling (accept or reject).
6. **Duplicate tags**: Same entry_id + tag pair appearing twice in JSONL. Verify PK/unique constraint behavior.
7. **Zero-length JSONL file**: No header line at all. Verify clear error, not panic.
8. **Header-only file**: Header line but no data lines, no counters. Verify behavior (may differ from empty-export case).
9. **Non-UTF-8 bytes in file**: Binary garbage. Verify JSON parse error with line number.

## Security Risks

**Untrusted input surface**: The JSONL file is the sole untrusted input. It is a user-provided file read from the local filesystem.

- **SQL injection via string fields**: All 8 table types contain string columns. If INSERT uses string interpolation instead of parameterized queries, any field could inject SQL. **Blast radius**: Full database compromise (DROP, exfiltration of all knowledge). **Mitigation**: Parameterized queries via rusqlite `params![]` macro. Test with SQL metacharacters in all string fields.

- **Path traversal**: The `--input` argument is a file path. No risk of traversal beyond reading the specified file (import only reads, it does not use file paths from within the JSONL content). The `--project-dir` argument could theoretically point to sensitive directories, but Store::open() creates/opens only a SQLite database file.

- **Deserialization of crafted JSON**: Extremely large JSON values (multi-GB strings), deeply nested objects, or NaN/Infinity floats could cause OOM or panics in serde_json. **Blast radius**: Import crash (DoS), no data corruption. **Mitigation**: Line-by-line reading bounds per-line memory. serde_json rejects NaN/Infinity by default.

- **Resource exhaustion**: A malicious JSONL with millions of lines could consume disk space and time. **Blast radius**: Disk full, long hang. **Mitigation**: Entry count from header provides an expected bound; implementation could validate actual count against header.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| Invalid header | Exit 1 with descriptive error. No database modification. | Fix input file or use correct export. |
| JSON parse error on line N | Exit 1 with "line N" in error. Transaction rollback, no partial state. | Fix corrupt line or re-export. |
| FK violation | Exit 1 with SQL error. Transaction rollback. | Re-export with current binary (ensures dependency order). |
| Hash validation failure | Exit 1 with entry IDs of mismatches. Transaction rollback. | Use --skip-hash-validation if content change is intentional, or re-export. |
| Non-empty DB without --force | Exit 1 with message suggesting --force. Database unchanged. | Add --force or use fresh project directory. |
| ONNX model unavailable | Exit 1 after DB commit. Database has entries but no vector index. | Pre-cache model, then re-run import --force. Server may re-embed on startup. |
| Embedding failure mid-batch | Exit 1 after DB commit. Partial vector index (not persisted). | Re-run import --force to retry embedding. |
| Disk full during VectorIndex::dump() | Exit 1. Database committed, vector index not persisted. | Free disk space, re-run import --force. |
| SQLITE_BUSY (concurrent server) | Depends on timeout. Likely exit 1. Transaction may not start. | Stop MCP server, retry import. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (ONNX model download required) | R-05, R-09 | ADR-004 places embedding after DB commit, so model failure does not lose imported data. Clear error messaging required. |
| SR-02 (Direct SQL bypasses Store API invariants) | R-01 | ADR-001 shared format types provide compile-time drift detection. Round-trip test (AC-15) is the runtime safety net. |
| SR-03 (Re-embedding blocks CLI on slow machines) | R-12 | Batch size 64 bounds per-batch latency. Progress reporting (FR-11) distinguishes slow from stuck. |
| SR-04 (--force drops data irreversibly) | R-04 | ADR-003 chose stderr warning without interactive confirmation. Tested via AC-06, AC-27. |
| SR-05 (Schema version must match exactly) | -- | Addressed in specification constraint 1. Header validation rejects schema_version > CURRENT. Not an architecture risk -- it is a validated precondition. |
| SR-06 (Stdin exclusion contradicts pipe example) | -- | Specification constraint 9 clarifies stdin is deferred. Not an architecture risk. |
| SR-07 (Concurrent server database contention) | R-08 | Architecture specifies PID file check with warning (FR-03 step 4). Not a blocking check -- warning only. |
| SR-08 (Implicit export format contract) | R-01, R-02 | ADR-001 introduces shared format.rs types. Compile-time contract enforcement. |
| SR-09 (Serde deserialization high surface area) | R-02, R-10, R-11 | Shared format types with explicit field definitions. Edge-case unit tests per AC-23. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 9 scenarios |
| High | 3 (R-03, R-04, R-05) | 10 scenarios |
| Medium | 7 (R-06, R-07, R-08, R-09, R-10, R-13, R-15) | 14 scenarios |
| Low | 3 (R-11, R-12, R-14) | 3 scenarios |
