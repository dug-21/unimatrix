# Risk-Based Test Strategy: nan-001

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Hardcoded column list in export diverges from actual schema v11 — missing or mis-typed column silently produces lossy export that becomes the nan-002 contract | High | Med | Critical |
| R-02 | f64 confidence values lose precision through JSON serialization, producing non-identical round-trip values on import | High | Low | High |
| R-03 | JSON-in-TEXT columns (capabilities, allowed_topics, allowed_categories, target_ids) double-encoded or parsed, corrupting stored JSON strings | High | Med | Critical |
| R-04 | NULL columns omitted from JSON output instead of serialized as `null`, causing nan-002 to treat missing keys as default values rather than NULL | High | Med | Critical |
| R-05 | Transaction not held for full export duration — concurrent MCP server writes produce cross-table inconsistency (orphan tags, broken co-access references) | High | Med | Critical |
| R-06 | JSON key ordering non-deterministic — preserve_order feature not enabled or insertion order inconsistent, breaking AC-14 byte-identical guarantee | Med | Med | High |
| R-07 | Excluded tables leak into export output — new tables added to schema not added to exclusion awareness, or existing excluded tables accidentally matched by wildcard logic | Med | Low | Medium |
| R-08 | Entry ordering within tables violates primary key sort — composite key tables (entry_tags, co_access, feature_entries, outcome_index) sorted incorrectly | Med | Med | High |
| R-09 | Store::open() migration side-effect — export opens database and triggers migration, altering database state as a side-effect of a read-only operation | Med | Low | Medium |
| R-10 | Output file partial write on error — export fails mid-write, leaves partial JSONL file that looks valid but is truncated | Med | Med | High |
| R-11 | preserve_order feature on serde_json breaks existing MCP server JSON serialization (Map iteration order changes globally for the crate) | Med | Low | Medium |
| R-12 | Empty database export produces invalid output — missing counter rows or malformed header when no entries exist | Low | Med | Medium |
| R-13 | Unicode content in entries (title, content, tags) corrupted or escaped incorrectly in JSON serialization | Med | Low | Medium |
| R-14 | Large integer values (timestamps near i64::MAX, large entry IDs) overflow or lose precision in JSON number encoding | Med | Low | Medium |
| R-15 | --project-dir flag not wired to export subcommand, silently using default project path | Low | Med | Medium |

## Risk-to-Scenario Mapping

### R-01: Hardcoded Column List Divergence
**Severity**: High
**Likelihood**: Med
**Impact**: Export silently drops columns. nan-002 imports incomplete data. User believes backup is complete but it is lossy. Data loss discovered only on restore.

**Test Scenarios**:
1. Insert an entry with non-default values for ALL 26 columns. Export and verify every column appears in the JSONL row with the correct value.
2. Compare the export column list against `PRAGMA table_info(entries)` output to verify no columns are missing.
3. For each of the 8 exported tables, verify the exported key count matches the SQL column count plus the `_table` discriminator.

**Coverage Requirement**: Every column of every exported table must have a test that verifies its presence and correct value in the JSONL output.

### R-02: f64 Precision Loss
**Severity**: High
**Likelihood**: Low
**Impact**: Confidence scores shift after import. Search ranking changes. Users see different results from the same knowledge base.

**Test Scenarios**:
1. Export entries with confidence values: 0.0, 1.0, 0.123456789012345, f64::MIN_POSITIVE, a value with maximum mantissa bits (e.g., 0.1 + 0.2).
2. Parse the exported JSON, convert back to f64, verify bitwise equality with the original.
3. Verify serde_json serializes confidence using ryu (shortest exact representation), not a fixed-decimal format.

**Coverage Requirement**: Round-trip f64 fidelity verified for at least 5 edge-case values including subnormals and values that do not have exact decimal representations.

### R-03: JSON-in-TEXT Double Encoding
**Severity**: High
**Likelihood**: Med
**Impact**: On import, the capabilities/allowed_topics strings contain extra escape characters. Agent registry becomes corrupt. Trust/capability lookups fail.

**Test Scenarios**:
1. Insert agent_registry row with `capabilities = '["Admin","Read"]'`, export, verify the JSONL value is `"capabilities":"[\"Admin\",\"Read\"]"` (a JSON string containing the raw text, not a JSON array).
2. Insert agent_registry row with `allowed_topics = null`, export, verify `"allowed_topics":null`.
3. Insert audit_log row with `target_ids = '[1,2,3]'`, export, verify the value is a JSON string not a JSON array.
4. Round-trip test: the exported string value, when stored back in SQLite as TEXT, must be byte-identical to the original.

**Coverage Requirement**: Every JSON-in-TEXT column tested with non-trivial JSON content and with null values.

### R-04: NULL Encoding
**Severity**: High
**Likelihood**: Med
**Impact**: nan-002 cannot distinguish "column was NULL" from "column was not exported." On import, nullable columns get wrong default values.

**Test Scenarios**:
1. Insert entry with `supersedes = NULL`, `superseded_by = NULL`, `pre_quarantine_status = NULL`. Export and verify all three appear as JSON `null`.
2. Insert agent_registry with `allowed_topics = NULL`, `allowed_categories = NULL`. Export and verify both are JSON `null`.
3. Verify no JSONL row has fewer keys than expected (no key omission on NULL).

**Coverage Requirement**: Every nullable column across all 8 tables tested with NULL values.

### R-05: Transaction Isolation Failure
**Severity**: High
**Likelihood**: Med
**Impact**: Export contains orphaned entry_tags (referencing entries not in the export), or co_access pairs referencing entries that were created after the entries table was read. nan-002 import fails or produces inconsistent state.

**Test Scenarios**:
1. Verify that `BEGIN DEFERRED` is executed before any table read and `COMMIT` after the last table read.
2. Integration test: start export with a hook or mock that inserts a new entry + tag between the entries read and the entry_tags read. Verify the new entry and its tag are BOTH absent from the export (snapshot isolation).
3. Verify the connection mutex is held for the entire transaction duration (no unlock-relock between table reads).

**Coverage Requirement**: At least one test that demonstrates cross-table consistency under concurrent modification.

### R-06: Non-Deterministic Key Ordering
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-14 violated. Byte-comparison fails between exports. Breaks any tooling that relies on diff-based change detection of exports.

**Test Scenarios**:
1. Export the same database twice (with `exported_at` held constant). Compare byte-for-byte.
2. Verify that `_table` is the first key in every data row.
3. Verify keys in entries rows follow the SQL column declaration order (id, title, content, topic, ..., pre_quarantine_status).
4. Run the export 10 times and verify all outputs are identical.

**Coverage Requirement**: Determinism verified via repeated export with fixed timestamp.

### R-07: Excluded Table Leakage
**Severity**: Med
**Likelihood**: Low
**Impact**: Export contains ephemeral or derived data, inflating file size and potentially exposing operational data (session IDs, query patterns).

**Test Scenarios**:
1. Populate all excluded tables (vector_map, sessions, observations, injection_log, signal_queue, query_log, observation_metrics, observation_phase_metrics, shadow_evaluations, topic_deliveries) with data. Export. Verify no `_table` values from the excluded set appear.
2. Collect all distinct `_table` values from export output. Verify the set is exactly: {counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log}.

**Coverage Requirement**: Full excluded-table population test.

### R-08: Incorrect Row Ordering
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-07 violated. Determinism broken for repeated exports if SQLite returns rows in different order without ORDER BY. nan-002 streaming import may fail if dependency order is violated.

**Test Scenarios**:
1. Insert entries with IDs out of natural insertion order (e.g., insert id=5, then id=2, then id=8). Export and verify id ordering is 2, 5, 8.
2. Insert entry_tags for the same entry_id with tags in reverse alphabetical order. Verify export sorts by (entry_id ASC, tag ASC).
3. For co_access, verify (entry_id_a, entry_id_b) ordering with multiple pairs.
4. Verify table emission order: counters before entries, entries before entry_tags, etc.

**Coverage Requirement**: Each table's ordering verified with at least 3 rows inserted out of natural order.

### R-09: Migration Side-Effect
**Severity**: Med
**Likelihood**: Low
**Impact**: Export modifies the database (schema migration writes) when the user expects a read-only operation. Could corrupt a database the user was trying to back up before a destructive operation.

**Test Scenarios**:
1. Open a database on the current schema, record its modification time, run export, verify modification time is unchanged (no writes occurred).
2. Verify export works on a database already at schema v11 without triggering migration logic beyond the initial open.

**Coverage Requirement**: At least one test verifying export does not modify the database file.

### R-10: Partial Output on Error
**Severity**: Med
**Likelihood**: Med
**Impact**: User believes export succeeded because a file exists, but it is truncated. Using this file for import causes data loss.

**Test Scenarios**:
1. Export to a read-only path. Verify non-zero exit code and error on stderr.
2. Export with a simulated write failure mid-stream (e.g., write to /dev/full on Linux, or mock writer that fails after N bytes). Verify non-zero exit code.
3. Verify that error messages clearly indicate the export is incomplete.

**Coverage Requirement**: At least one error-path test for write failure, one for database open failure.

### R-11: preserve_order Global Side-Effect
**Severity**: Med
**Likelihood**: Low
**Impact**: Existing MCP server tests fail because JSON Map iteration order changed. Server responses have different key ordering, potentially breaking clients that depend on key order.

**Test Scenarios**:
1. Run the full existing test suite with `preserve_order` enabled. Verify no regressions.
2. If any test compares JSON output byte-for-byte, verify it still passes.

**Coverage Requirement**: Full regression suite passes with the feature enabled.

### R-12: Empty Database Export
**Severity**: Low
**Likelihood**: Med
**Impact**: Export crashes or produces invalid JSONL when no entries exist. Blocks the "fresh backup" workflow.

**Test Scenarios**:
1. Initialize a fresh database (no entries, no tags, no agents). Export. Verify: header line present with `entry_count: 0`, counter rows present (schema_version, next_entry_id, etc.), no data rows for other tables.
2. Parse every line as valid JSON.

**Coverage Requirement**: Empty database export produces valid, parseable JSONL.

### R-13: Unicode Content Corruption
**Severity**: Med
**Likelihood**: Low
**Impact**: Knowledge entries with non-ASCII content (CJK, emoji, combining characters, RTL text) are corrupted in export. Lossy backup.

**Test Scenarios**:
1. Insert entry with title containing CJK characters, content with emoji, tag with accented characters.
2. Export and verify the JSON strings decode to the original Unicode.
3. Test content with embedded newlines, tabs, and JSON-special characters (quotes, backslashes).

**Coverage Requirement**: At least one test with multi-byte Unicode in each text column.

### R-14: Large Integer Values
**Severity**: Med
**Likelihood**: Low
**Impact**: Timestamps or IDs near i64::MAX lose precision when serialized as JSON numbers. JavaScript consumers (jq, web tools) may truncate to 53-bit integers.

**Test Scenarios**:
1. Insert entry with `created_at = i64::MAX` (or a large realistic timestamp). Export and verify the number is exact in JSON.
2. Verify counter values with large integers round-trip correctly.

**Coverage Requirement**: At least one large-integer test per integer column type.

### R-15: --project-dir Not Wired
**Severity**: Low
**Likelihood**: Med
**Impact**: Export always reads the default project database, ignoring user's explicit path override. User exports wrong database without knowing.

**Test Scenarios**:
1. Create a database in a non-default directory with distinct data. Invoke export with --project-dir pointing to that directory. Verify exported data matches the non-default database.
2. Verify that omitting --project-dir uses the default project resolution.

**Coverage Requirement**: At least one test with explicit --project-dir.

## Integration Risks

1. **Store::open() + lock_conn() contract**: Export depends on `lock_conn()` returning a `MutexGuard<Connection>` that supports raw SQL execution. If `Store` internals change (e.g., connection pooling, async migration), export breaks silently.
2. **ensure_data_directory() path resolution**: Export reuses the engine's path resolution. If the project hash algorithm changes, export reads a different database than expected.
3. **Clap Command enum extension**: Adding the `Export` variant changes the CLI parser. If the `Command` enum has `#[clap(subcommand_required = true)]` or similar, the new variant must integrate correctly without breaking `Hook` or the default server startup.
4. **serde_json preserve_order crate-wide impact**: The feature flag affects all `serde_json::Map` usage in `unimatrix-server`, not just the export module. Any code path that constructs or inspects Maps will see insertion-order semantics.

## Edge Cases

1. **Entry with all nullable fields set to NULL**: supersedes, superseded_by, pre_quarantine_status all NULL simultaneously.
2. **Entry with empty string fields**: title = "", content = "", created_by = "". These are NOT NULL but are empty — JSON should have `""`, not `null`.
3. **Co-access pair at boundary**: entry_id_a = 1, entry_id_b = 2 — minimum valid pair. Also test entry_id_a = entry_id_b (should not exist due to CHECK constraint, but export should not crash if it does).
4. **Agent with empty capabilities**: capabilities = "[]" (valid JSON array, empty). Should export as `"[]"` string.
5. **Audit log with empty detail**: detail = "" (NOT NULL DEFAULT ''). Must appear as `""`.
6. **Counter with value 0**: next_entry_id = 0 on a fresh database. Must not be omitted.
7. **Entry with maximum version number**: version near i64::MAX.
8. **Single entry, no tags, no co-access**: Minimal non-empty database.
9. **Entry with content containing JSONL-breaking characters**: Content with literal `\n` (newline) must be escaped in JSON so each JSONL line remains a single line.
10. **Timestamp of 0**: created_at = 0 (epoch). Valid, must not be treated as NULL or missing.

## Security Risks

- **Untrusted input**: Export reads from the local SQLite database only. No external/untrusted input beyond the `--output` path and `--project-dir` path.
- **Path traversal on --output**: The output path is user-provided. If the binary runs with elevated privileges, a malicious path could overwrite system files. Mitigation: export runs as the current user with normal filesystem permissions. No special path validation needed beyond OS-level permissions.
- **Path traversal on --project-dir**: Same analysis as --output. Uses existing path resolution code.
- **Sensitive data in export**: The audit_log and agent_registry contain security-relevant data (agent trust levels, operation history). The export file should be treated as sensitive. This is a user awareness concern, not a code concern.
- **Blast radius**: If the export module has a bug, the worst case is a corrupt/incomplete JSONL file. Export is read-only with respect to the database (aside from Store::open() migration). It cannot corrupt the database.

## Failure Modes

| Failure | Expected Behavior |
|---------|------------------|
| Database file does not exist | Error to stderr, exit non-zero. No output file created. |
| Database is locked exclusively (another process holds EXCLUSIVE) | rusqlite error, propagated to stderr, exit non-zero. |
| Output path is not writable | io::Error to stderr, exit non-zero. |
| Disk full during write | io::Error to stderr, exit non-zero. Partial file may remain. |
| Broken pipe (stdout closed by consumer) | io::Error (BrokenPipe), exit non-zero. |
| SQL query returns unexpected type | rusqlite type conversion error, propagated to stderr, exit non-zero. |
| Empty entries table but non-empty entry_tags | Valid scenario (orphaned tags from a bug). Export emits both. nan-002 handles validation. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Format contract mistakes lock in breaking changes) | R-01, R-03, R-04 | ADR-002 defines explicit column-to-JSON mapping. Spec includes full field mapping tables. Test coverage verifies every column of every table. |
| SR-02 (Direct SQL bypasses Store API type guarantees) | R-01 | ADR-002 chooses explicit column lists. Acknowledged as hardcoded for v1 with schema v11 assumed stable. |
| SR-03 (f64 precision loss) | R-02 | ADR-002 confirms serde_json/ryu provides lossless f64 round-trip. Spec requires edge-case float testing. |
| SR-04 (Schema version coupling) | R-01 | Accepted for v1 — column lists hardcoded against schema v11. Future enhancement to derive from shared definition. |
| SR-05 (JSON key ordering non-deterministic) | R-06 | ADR-003 specifies preserve_order feature on serde_json for insertion-order determinism. |
| SR-06 (No incremental export) | -- | Accepted for v1. Not an architecture risk — scope boundary decision. |
| SR-07 (Concurrent access inconsistency) | R-05 | ADR-001 specifies BEGIN DEFERRED transaction wrapping all reads. Architecture directly addresses this. |
| SR-08 (Store::open() migration race) | R-09 | Architecture confirms: if server running, migration done; if not, export migrates safely alone. Low risk accepted. |
| SR-09 (CLI binary shared with MCP server) | R-11 | Architecture keeps export module self-contained. preserve_order feature is the only crate-wide change — requires regression verification. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-03, R-04, R-05) | 12 scenarios |
| High | 4 (R-02, R-06, R-08, R-10) | 13 scenarios |
| Medium | 6 (R-07, R-09, R-11, R-12, R-13, R-14) | 10 scenarios |
| Low | 1 (R-15) | 2 scenarios |
| **Total** | **15** | **37 scenarios** |
