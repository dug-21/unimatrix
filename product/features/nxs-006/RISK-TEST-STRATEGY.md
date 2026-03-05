# nxs-006: Risk & Test Strategy

## Risk Catalog

### R-01: Data Loss During Migration (Severity: CRITICAL)
**Source**: SR-02 (Intermediate Format Data Fidelity)

The export/import cycle is a one-way operation on production data. If any table's data is corrupted, truncated, or silently dropped during migration, the knowledge base is permanently damaged.

**Architectural mitigations**:
- ADR-001: JSON-lines format is human-inspectable; corrupted rows can be identified visually.
- ADR-003: Export reads redb directly (no mutations).
- ADR-004: Import uses Store::open() for schema correctness, then raw SQL for exact copy.

**Test coverage**:
- T-01: Full round-trip integration test — create redb with data in all 17 tables, export, import, verify every row.
- T-02: Blob fidelity test — deserialize every EntryRecord, CoAccessRecord, AgentRecord, AuditRecord, SessionRecord, InjectionLogRecord, SignalRecord from imported SQLite and compare field-by-field.
- T-03: Base64 round-trip — verify `decode(encode(blob)) == blob` for blobs of length 0, 1, 2, 3, 100, 100000 (covers padding edge cases).

### R-02: Database Filename Confusion (Severity: HIGH)
**Source**: SR-01 (Database Filename Mismatch)

After the default flip, the system looks for `unimatrix.db` but the production data is in `unimatrix.redb`. If the operator forgets to run the migration, the server starts with an empty database.

**Architectural mitigation**: ADR-002 (cfg-gated db_path).

**Test coverage**:
- T-04: Project path unit test — verify `ensure_data_directory()` returns `.db` under backend-sqlite and `.redb` without it.
- T-05: Documentation — IMPLEMENTATION-BRIEF must include explicit migration instructions.

**Residual risk**: Operator error (forgetting to place the imported `.db` file in the correct directory). Mitigated by clear CLI output and documentation.

### R-03: Feature Flag Compilation Failures (Severity: HIGH)
**Source**: SR-07 (Feature Flag Interaction Complexity)

The feature flag graph changes across three crates (store, engine, server). Incorrect propagation could cause compilation failures or wrong backend selection.

**Architectural mitigation**: Feature propagation chain: server/backend-sqlite -> store/backend-sqlite + engine/backend-sqlite.

**Test coverage**:
- T-06: Compilation matrix test — CI must verify all three scenarios compile:
  - Default (backend-sqlite)
  - `--no-default-features --features mcp-briefing` (redb)
  - Explicit `--features backend-sqlite`
- T-07: Backend selection test — verify Store type is correct for each feature combination (SQLite Store has `conn: Mutex<Connection>`, redb Store has `db: redb::Database`).

### R-04: Multimap Data Loss (Severity: HIGH)
**Source**: SR-04 (Multimap Table Semantics)

TAG_INDEX and FEATURE_ENTRIES are multimap tables. The export must emit one line per (key, value) pair, not one line per key. If the export code iterates keys and takes only the first value, data is silently lost.

**Architectural mitigation**: `ALL_TABLES` array classifies each table with a `multimap: bool` flag. Export uses `table.iter()` for multimaps, which yields all (key, MultimapValue) pairs. Each value in the MultimapValue iterator gets its own output line.

**Test coverage**:
- T-08: Multimap round-trip — create entries with 5 tags each and 3 feature associations. Export, import, verify all tags and feature associations survive.
- T-09: Multimap row count — verify intermediate file row_count for TAG_INDEX equals the total number of (tag, entry_id) pairs, not the number of unique tags.

### R-05: Counter State Corruption (Severity: HIGH)
**Source**: SR-05 (Counter State Consistency)

The counters table contains `next_entry_id`, `next_signal_id`, `next_log_id`, `next_audit_event_id`, and status totals. If these are not imported correctly, the next write operation generates duplicate IDs.

**Architectural mitigation**: The import writes counter values as-is from the export. Store::open() initializes counters with defaults, but the import overwrites them with `INSERT OR REPLACE`.

**Test coverage**:
- T-10: Counter verification — after import, verify `next_entry_id` > MAX(entries.id), `schema_version` == 5, `next_signal_id` > MAX(signal_queue.signal_id), etc.
- T-11: Counter overwrite — verify that Store::open()'s default counter initialization does not persist over the import's counter values (INSERT OR REPLACE overwrites the defaults).

### R-06: u64/i64 Boundary Overflow (Severity: MEDIUM)
**Source**: Identified during architecture (SQLite stores INTEGER as i64)

Entry IDs, timestamps, and other u64 values above i64::MAX (2^63 - 1) would be truncated or misrepresented when stored as SQLite INTEGER. In practice, the production database has IDs < 1000 and timestamps in the 1.7 billion range, well within i64 range.

**Architectural mitigation**: The import validates that no u64 value exceeds i64::MAX and aborts if found.

**Test coverage**:
- T-12: Boundary value test — create an entry with `id = i64::MAX as u64` (9,223,372,036,854,775,807), export, import, verify it survives.
- T-13: Overflow detection — attempt to export an entry with `id = i64::MAX as u64 + 1` (not possible in production, but the validation should catch it if it ever occurs).

### R-07: Empty Table Handling (Severity: LOW)
**Source**: Edge case

Tables like signal_queue, injection_log may be empty if the server was recently restarted (signals are drained, sessions are GC'd). The export must handle empty tables without error.

**Test coverage**:
- T-14: Empty database round-trip — export a freshly created database (only counters have data), import, verify all 17 tables exist with correct row counts.

### R-08: PID File False Positive (Severity: LOW)
**Source**: SR-08 (Concurrent Access)

The export checks for a PID file to prevent concurrent access. If a stale PID file exists (server crashed), the export incorrectly refuses to run. However, the existing `is_unimatrix_process` check (vnc-004) verifies the PID's cmdline, so stale PIDs are correctly identified.

**Architectural mitigation**: Use the existing `pidfile::handle_stale_pid_file()` logic from vnc-004.

**Test coverage**:
- T-15: PID file integration — verify export proceeds when PID file contains a non-unimatrix PID (stale). Verify export aborts when PID file contains a running unimatrix process PID.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Test Coverage |
|-----------|------------------|---------------|
| SR-01 (Filename Mismatch) | R-02 | T-04, T-05 |
| SR-02 (Data Fidelity) | R-01 | T-01, T-02, T-03 |
| SR-03 (Schema Divergence) | Mitigated by ADR-004 (Store::open creates correct schema) | T-01, T-02 |
| SR-04 (Multimap Semantics) | R-04 | T-08, T-09 |
| SR-05 (Counter Consistency) | R-05 | T-10, T-11 |
| SR-06 (Co-Access Ordering) | Subsumed by R-01 | T-01 (co_access rows verified in round-trip) |
| SR-07 (Feature Flag Complexity) | R-03 | T-06, T-07 |
| SR-08 (Concurrent Access) | R-08 | T-15 |

---

## Test Strategy Summary

### Unit Tests (in crates/unimatrix-store/src/migrate/)

| Test | Risk | Description |
|------|------|-------------|
| T-03 | R-01 | Base64 round-trip for various blob sizes (0, 1, 2, 3, 100, 100000 bytes) |
| T-13 | R-06 | u64 overflow detection validates values > i64::MAX |
| T-14 | R-07 | Empty table export/import produces correct header with row_count=0 |

### Integration Tests (in crates/unimatrix-store/tests/)

| Test | Risk | Description |
|------|------|-------------|
| T-01 | R-01 | Full 17-table round-trip: create redb, populate, export, import, verify all row counts |
| T-02 | R-01 | Blob fidelity: deserialize every record type from imported SQLite, field-by-field compare |
| T-08 | R-04 | Multimap round-trip: entries with 5 tags, 3 feature associations |
| T-09 | R-04 | Multimap row count verification in intermediate file |
| T-10 | R-05 | Counter state verification post-import |
| T-11 | R-05 | Counter overwrite: Store::open defaults overwritten by import |
| T-12 | R-06 | i64::MAX boundary value round-trip |

### Build Tests (CI)

| Test | Risk | Description |
|------|------|-------------|
| T-06 | R-03 | Compilation matrix: default, no-default+redb, explicit backend-sqlite |
| T-07 | R-03 | Backend selection: correct Store type for each feature combination |

### Unit Tests (in crates/unimatrix-engine/)

| Test | Risk | Description |
|------|------|-------------|
| T-04 | R-02 | Project path suffix: .db under backend-sqlite, .redb without |

### Manual Verification

| Test | Risk | Description |
|------|------|-------------|
| T-05 | R-02 | Documentation includes explicit migration instructions |
| T-15 | R-08 | PID file prevents concurrent export with running server |

---

## Test Infrastructure

Integration tests for the full round-trip (T-01, T-02, T-08-T-12) require BOTH backends to be available. Since the store crate's cfg gates make them mutually exclusive at compile time, these tests must be structured as:

1. A test binary compiled WITHOUT `backend-sqlite` (redb) that exports to a temp file.
2. A test binary compiled WITH `backend-sqlite` (SQLite) that imports from the temp file and verifies.

In practice, this is tested via:
- Step 1: `cargo test -p unimatrix-store --no-default-features --test migrate_export` (runs the export test)
- Step 2: `cargo test -p unimatrix-store --features backend-sqlite --test migrate_import` (runs the import test, reading the file produced by step 1)

The tests share data through a well-known temp file path passed via environment variable or created in a shared temp directory.

**Alternative**: A single integration test that shells out to both binaries. This is more complex but avoids the two-step manual process. The architect should decide which approach to use during implementation.

---

## Risk Coverage Matrix

| Risk | Severity | Tests | Coverage |
|------|----------|-------|----------|
| R-01 (Data Loss) | CRITICAL | T-01, T-02, T-03 | Full: all record types, all table types, blob edge cases |
| R-02 (Filename) | HIGH | T-04, T-05 | Full: cfg-gated assertion + documentation |
| R-03 (Feature Flags) | HIGH | T-06, T-07 | Full: compilation matrix verified |
| R-04 (Multimap) | HIGH | T-08, T-09 | Full: multi-value keys + row count check |
| R-05 (Counters) | HIGH | T-10, T-11 | Full: post-import state + overwrite behavior |
| R-06 (u64/i64) | MEDIUM | T-12, T-13 | Full: boundary + overflow detection |
| R-07 (Empty Tables) | LOW | T-14 | Full: empty database round-trip |
| R-08 (PID File) | LOW | T-15 | Partial: manual verification |
