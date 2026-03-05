# nxs-008: Schema Normalization — Architecture

## Overview

nxs-008 decomposes bincode blobs into SQL columns across 7 tables, eliminates 5 manual index tables (replaced by SQL `CREATE INDEX`), removes the redb-pattern compat layer (`handles.rs`, `dispatch.rs`), and replaces client-side HashSet query logic with SQL WHERE clauses. The Store public API is unchanged — this is an internal restructuring with behavioral parity.

## Open Question Resolutions

| # | Question | Decision | ADR |
|---|----------|----------|-----|
| 1 | txn.rs fate | Keep `SqliteWriteTransaction` (RAII transaction safety); remove `SqliteReadTransaction` (zero-value wrapper) | [ADR-001](ADR-001-txn-wrappers.md) |
| 2 | Counter helpers location | New `counters.rs` module; all functions take `&Connection` | [ADR-002](ADR-002-counter-helpers.md) |
| 3 | Enum storage format | INTEGER using `#[repr(u8)]` discriminants; TEXT rejected | [ADR-003](ADR-003-enum-storage-integer.md) |

## Architectural Decisions

| ADR | Title | Unimatrix ID | Mitigates |
|-----|-------|-------------|-----------|
| ADR-001 | Keep SqliteWriteTransaction, Remove SqliteReadTransaction | #355 | SR-04 |
| ADR-002 | Counter Helpers Move to counters.rs Module | #356 | SR-04 |
| ADR-003 | Enum Storage as INTEGER | #357 | SR-04, SR-07 |
| ADR-004 | Mandatory Named Parameters for Multi-Column SQL | #358 | SR-02 |
| ADR-005 | Migration Compatibility Module for Bincode Deserializers | #359 | SR-01 |
| ADR-006 | entry_tags Junction Table with Foreign Key CASCADE | #360 | SR-08 |
| ADR-007 | JSON Array Columns for Non-Queried Vec Fields | #361 | SR-06 |
| ADR-008 | Wave Ordering and Cross-Crate Synchronization | #362 | SR-05 |

## Target Schema (v6)

### entries (24 columns, replaces bincode blob)

```sql
CREATE TABLE entries (
    id              INTEGER PRIMARY KEY,
    title           TEXT    NOT NULL,
    content         TEXT    NOT NULL,
    topic           TEXT    NOT NULL,
    category        TEXT    NOT NULL,
    source          TEXT    NOT NULL,
    status          INTEGER NOT NULL,  -- 0=Active, 1=Deprecated, 2=Proposed, 3=Quarantined
    confidence      REAL    NOT NULL DEFAULT 0.0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    last_accessed_at INTEGER NOT NULL DEFAULT 0,
    access_count    INTEGER NOT NULL DEFAULT 0,
    supersedes      INTEGER,           -- nullable FK-like
    superseded_by   INTEGER,           -- nullable FK-like
    correction_count INTEGER NOT NULL DEFAULT 0,
    embedding_dim   INTEGER NOT NULL DEFAULT 0,
    created_by      TEXT    NOT NULL DEFAULT '',
    modified_by     TEXT    NOT NULL DEFAULT '',
    content_hash    TEXT    NOT NULL DEFAULT '',
    previous_hash   TEXT    NOT NULL DEFAULT '',
    version         INTEGER NOT NULL DEFAULT 0,
    feature_cycle   TEXT    NOT NULL DEFAULT '',
    trust_source    TEXT    NOT NULL DEFAULT '',
    helpful_count   INTEGER NOT NULL DEFAULT 0,
    unhelpful_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_entries_topic      ON entries(topic);
CREATE INDEX idx_entries_category   ON entries(category);
CREATE INDEX idx_entries_status     ON entries(status);
CREATE INDEX idx_entries_created_at ON entries(created_at);
```

### entry_tags (junction table, replaces TAG_INDEX)

```sql
CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL,
    tag      TEXT    NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX idx_entry_tags_tag      ON entry_tags(tag);
CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id);
```

### co_access (2 data columns, replaces bincode blob)

```sql
CREATE TABLE co_access (
    entry_id_a   INTEGER NOT NULL,
    entry_id_b   INTEGER NOT NULL,
    count        INTEGER NOT NULL DEFAULT 1,
    last_updated INTEGER NOT NULL,
    PRIMARY KEY (entry_id_a, entry_id_b),
    CHECK (entry_id_a < entry_id_b)
);

CREATE INDEX idx_co_access_b ON co_access(entry_id_b);
```

### sessions (9 columns, replaces bincode blob)

```sql
CREATE TABLE sessions (
    session_id       TEXT    PRIMARY KEY,
    feature_cycle    TEXT,              -- nullable
    agent_role       TEXT,              -- nullable
    started_at       INTEGER NOT NULL,
    ended_at         INTEGER,           -- nullable
    status           INTEGER NOT NULL,  -- 0=Active, 1=Completed, 2=TimedOut, 3=Abandoned
    compaction_count INTEGER NOT NULL DEFAULT 0,
    outcome          TEXT,              -- nullable: "success" | "rework" | "abandoned"
    total_injections INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle);
CREATE INDEX idx_sessions_status        ON sessions(status);
```

### injection_log (5 columns, replaces bincode blob)

```sql
CREATE TABLE injection_log (
    log_id     INTEGER PRIMARY KEY,
    session_id TEXT    NOT NULL,
    entry_id   INTEGER NOT NULL,
    confidence REAL    NOT NULL,
    timestamp  INTEGER NOT NULL
);

CREATE INDEX idx_injection_log_session ON injection_log(session_id);
CREATE INDEX idx_injection_log_entry   ON injection_log(entry_id);
```

### signal_queue (6 columns, replaces bincode blob)

```sql
CREATE TABLE signal_queue (
    signal_id     INTEGER PRIMARY KEY,
    session_id    TEXT    NOT NULL,
    created_at    INTEGER NOT NULL,
    entry_ids     TEXT    NOT NULL DEFAULT '[]',  -- JSON array of u64
    signal_type   INTEGER NOT NULL,               -- 0=Helpful, 1=Flagged
    signal_source INTEGER NOT NULL                -- 0=ImplicitOutcome, 1=ImplicitRework
);
```

### agent_registry (8 columns, replaces bincode blob)

```sql
CREATE TABLE agent_registry (
    agent_id           TEXT    PRIMARY KEY,
    trust_level        INTEGER NOT NULL,  -- 0=System, 1=Privileged, 2=Internal, 3=Restricted
    capabilities       TEXT    NOT NULL DEFAULT '[]',  -- JSON array
    allowed_topics     TEXT,                            -- JSON array, nullable
    allowed_categories TEXT,                            -- JSON array, nullable
    enrolled_at        INTEGER NOT NULL,
    last_seen_at       INTEGER NOT NULL,
    active             INTEGER NOT NULL DEFAULT 1       -- boolean as integer
);
```

### audit_log (8 columns, replaces bincode blob)

```sql
CREATE TABLE audit_log (
    event_id   INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    session_id TEXT    NOT NULL,
    agent_id   TEXT    NOT NULL,
    operation  TEXT    NOT NULL,
    target_ids TEXT    NOT NULL DEFAULT '[]',  -- JSON array of u64
    outcome    INTEGER NOT NULL,               -- 0=Success, 1=Denied, 2=Error, 3=NotImplemented
    detail     TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX idx_audit_log_agent     ON audit_log(agent_id);
CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
```

### Tables Unchanged

| Table | Reason |
|-------|--------|
| vector_map | Simple KV, already normalized |
| counters | Simple KV, already normalized |
| feature_entries | Simple composite key, no blob |
| outcome_index | Simple composite key, no blob |
| observation_metrics | Excluded per ADR #354 (dynamic-shape MetricVector stays bincode) |

### Tables Eliminated

| Table | Replacement |
|-------|-------------|
| topic_index | `idx_entries_topic` on entries(topic) |
| category_index | `idx_entries_category` on entries(category) |
| tag_index | `entry_tags` junction table |
| time_index | `idx_entries_created_at` on entries(created_at) |
| status_index | `idx_entries_status` on entries(status) |

## Query Architecture

### Current: HashSet Intersection (read.rs)

```
For each filter dimension:
    SELECT entry_id FROM {index_table} WHERE {key} = ?  ->  HashSet<u64>
Intersect all HashSets in Rust
For each result ID:
    SELECT data FROM entries WHERE id = ?  ->  deserialize_entry()
```

~200 lines, N+1 fetch pattern, client-side filtering.

### After: SQL WHERE Clauses

```sql
-- Single query with dynamic WHERE clause building
SELECT id, title, content, topic, category, source, status, confidence,
       created_at, updated_at, last_accessed_at, access_count,
       supersedes, superseded_by, correction_count, embedding_dim,
       created_by, modified_by, content_hash, previous_hash,
       version, feature_cycle, trust_source, helpful_count, unhelpful_count
FROM entries
WHERE topic = :topic
  AND category = :category
  AND status = :status
  AND created_at BETWEEN :time_start AND :time_end
  AND id IN (
      SELECT entry_id FROM entry_tags
      WHERE tag IN (:tag1, :tag2)
      GROUP BY entry_id
      HAVING COUNT(DISTINCT tag) = :tag_count
  )
```

Tags loaded in batch:
```sql
SELECT entry_id, tag FROM entry_tags WHERE entry_id IN (:id1, :id2, :id3)
```

~40 lines, single-pass, database-side filtering.

### Query Semantics Preservation

| Semantic | Current Behavior | Normalized Behavior |
|----------|-----------------|---------------------|
| Tag filtering | AND across tags (HashSet intersection) | `GROUP BY HAVING COUNT = tag_count` (same AND semantics) |
| Empty filter | Default to Status::Active | Same: add `WHERE status = 0` when no filters set |
| Empty tags list | Skip tag filter | Same: omit tag subquery |
| Status filter | Integer match via `status as u8` | Same: `WHERE status = :status` |
| Time range | `timestamp >= start AND timestamp <= end` | `created_at BETWEEN :start AND :end` |
| NULL options | bincode stores None, deserialized as None | SQL NULL, read as `row.get::<_, Option<i64>>()` |

### ASS-016 Future JOIN Readiness

The normalized schema enables the entry effectiveness query from ASS-016 research:

```sql
SELECT e.id, e.title,
    COUNT(CASE WHEN s.outcome = 'success' THEN 1 END) as success_count,
    COUNT(CASE WHEN s.outcome = 'rework' THEN 1 END) as rework_count
FROM injection_log il
JOIN sessions s ON il.session_id = s.session_id
JOIN entries e ON il.entry_id = e.id
WHERE s.feature_cycle = :feature
GROUP BY e.id, e.title
```

Key indexes that enable this:
- `injection_log.session_id` — indexed (new, replaces full-table scan)
- `injection_log.entry_id` — indexed (new)
- `sessions.feature_cycle` — indexed (new, replaces full-table scan)
- `entries.id` — primary key

## Write Architecture

### EntryRecord Construction from Row

A single helper function constructs an `EntryRecord` from a SQLite row:

```rust
fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    Ok(EntryRecord {
        id: row.get::<_, i64>("id")? as u64,
        title: row.get("title")?,
        content: row.get("content")?,
        topic: row.get("topic")?,
        category: row.get("category")?,
        tags: vec![],  // populated separately via load_tags_for_entries
        source: row.get("source")?,
        status: Status::try_from(row.get::<_, u8>("status")?)
            .unwrap_or(Status::Active),
        confidence: row.get("confidence")?,
        created_at: row.get::<_, i64>("created_at")? as u64,
        // ... remaining fields
    })
}
```

Tags are always populated via `load_tags_for_entries()` after row construction (ADR-006).

### Insert Path (write.rs)

```
1. BEGIN IMMEDIATE
2. next_entry_id() from counters.rs
3. compute_content_hash()
4. INSERT INTO entries (...24 cols...) VALUES (...named params...)
5. INSERT INTO entry_tags for each tag
6. increment_counter(status_counter_key)
7. COMMIT
```

2+ SQL statements (down from 7+). Index maintenance is automatic.

### Update Path (write.rs)

```
1. BEGIN IMMEDIATE
2. UPDATE entries SET ...24 cols... WHERE id = :id
3. DELETE FROM entry_tags WHERE entry_id = :id
4. INSERT INTO entry_tags for each tag
5. Update status counters if status changed
6. COMMIT
```

No diff-based index sync. Tags are always replaced (delete-all + re-insert).

### Server Write Path (store_ops.rs, store_correct.rs)

Server code currently writes to ENTRIES + 5 index tables + VECTOR_MAP + OUTCOME_INDEX in a single transaction. After normalization:

```
1. BEGIN IMMEDIATE (via SqliteWriteTransaction)
2. next_entry_id() from counters.rs
3. INSERT INTO entries (...) VALUES (:named_params...)
4. INSERT INTO entry_tags for each tag
5. INSERT INTO vector_map
6. INSERT INTO outcome_index (if applicable)
7. increment_counter(status_counter_key)
8. audit_log INSERT
9. COMMIT
```

Server accesses the raw connection via `&*txn.guard` for direct SQL. The `open_table` dispatch pattern is eliminated.

## Migration Architecture (v5 -> v6)

### Sequence

```
1. Copy database file to {path}.v5-backup
2. BEGIN IMMEDIATE
3. For each table being normalized:
   a. Create new table schema (entries_v6, sessions_v6, etc.)
   b. SELECT all rows from old table
   c. Deserialize each bincode blob via migration_compat.rs
   d. INSERT into new table with SQL columns
   e. For entries: also populate entry_tags from deserialized tags
4. Drop old tables (entries, topic_index, category_index, tag_index, time_index, status_index, etc.)
5. ALTER TABLE entries_v6 RENAME TO entries
6. Create SQL indexes
7. UPDATE counters SET value = 6 WHERE name = 'schema_version'
8. COMMIT
```

### Migration Compat Module (ADR-005)

`migration_compat.rs` retains all bincode deserializers:

| Function | Source Type |
|----------|------------|
| `deserialize_entry_v5` | EntryRecord |
| `deserialize_co_access_v5` | CoAccessRecord |
| `deserialize_session_v5` | SessionRecord |
| `deserialize_injection_log_v5` | InjectionLogRecord |
| `deserialize_signal_v5` | SignalRecord |
| `deserialize_agent_v5` | AgentRecord |
| `deserialize_audit_event_v5` | AuditEvent |

Created in Wave 0, before any runtime bincode removal.

### Server-Crate Type Migration

`AgentRecord` and `AuditEvent` are currently defined in the server crate. For the migration to deserialize their blobs, these types must be accessible to the store crate. Two approaches:

**Recommended**: Move the struct definitions and their Serialize/Deserialize impls to `unimatrix-store::schema`. The server crate re-imports them. The enum types (`TrustLevel`, `Capability`, `Outcome`) also move to store. This is clean because these are data types, not server logic.

**Alternative**: Keep types in server crate; migration uses `serde_json::Value` as intermediate format. Deserialize blob to JSON value, extract fields, write SQL columns. This avoids moving types but adds fragility.

## Wave Execution Plan

### Wave 0: Migration Infrastructure

**Files created**: `counters.rs`, `migration_compat.rs`
**Files modified**: `migration.rs`, `lib.rs`
**No runtime behavior changes**

| Task | Files | Risk |
|------|-------|------|
| Create `counters.rs` with consolidated helpers | new file | Low |
| Create `migration_compat.rs` with all deserializers | new file | Low |
| Write v5-to-v6 migration in `migration.rs` | migration.rs | HIGH (SR-01) |
| Migration round-trip tests (all 7 tables) | tests | Low |
| Database backup before migration | migration.rs | Low |

Gate: `cargo test --workspace`, migration tests pass on synthetic v5 database.

### Wave 1: ENTRIES + entry_tags + Index Elimination

**Files modified**: `db.rs`, `write.rs`, `read.rs`, `schema.rs`, `lib.rs`
**Server files modified**: `store_ops.rs`, `store_correct.rs`, `status.rs`, `contradiction.rs`
**Tables affected**: entries, entry_tags (new), topic_index (dropped), category_index (dropped), tag_index (dropped), time_index (dropped), status_index (dropped)

| Task | Files | Risk |
|------|-------|------|
| New entries DDL (24 columns) in `db.rs` | db.rs | Medium |
| `entry_from_row()` + `load_tags_for_entries()` helpers | read.rs | HIGH (SR-03) |
| Rewrite `Store::insert()` with named params | write.rs | HIGH (SR-02) |
| Rewrite `Store::update()` — replace diff-based index sync | write.rs | Medium |
| Rewrite `Store::update_status()` — direct column UPDATE | write.rs | Low |
| Rewrite `Store::delete()` — CASCADE handles tags | write.rs | Low |
| Rewrite `Store::query()` — SQL WHERE builder | read.rs | HIGH (SR-03) |
| Rewrite `Store::get()` — entry_from_row + tags | read.rs | Low |
| Rewrite `store_ops.rs` insert — direct SQL, named params | store_ops.rs | HIGH (SR-02) |
| Rewrite `store_correct.rs` — direct SQL, named params | store_correct.rs | HIGH (SR-02) |
| Update `status.rs` — query entries table directly | status.rs | Low |
| Update `contradiction.rs` — query entries table directly | contradiction.rs | Low |
| Enable `PRAGMA foreign_keys = ON` | db.rs | Low |
| Write behavioral parity tests (query semantics) | tests | Medium |

Gate: `cargo build --workspace && cargo test --workspace`.

### Wave 2: Store-Crate Operational Tables

**Files modified**: `db.rs`, `sessions.rs`, `injection_log.rs`, `signal.rs`, `write_ext.rs`

| Task | Files | Risk |
|------|-------|------|
| CO_ACCESS: replace blob with `count` + `last_updated` columns | db.rs, write_ext.rs, read.rs | Low |
| SESSIONS: 9 SQL columns + indexed `feature_cycle` + `status` | db.rs, sessions.rs | Medium |
| INJECTION_LOG: 5 SQL columns + indexed `session_id` + `entry_id` | db.rs, injection_log.rs | Low |
| SIGNAL_QUEUE: 6 SQL columns + JSON `entry_ids` | db.rs, signal.rs | Medium |
| GC cascade uses indexed `session_id` column (replaces full scan) | sessions.rs | Medium |
| Session feature scan uses indexed `feature_cycle` column | sessions.rs | Low |

Gate: `cargo build --workspace && cargo test --workspace`.

### Wave 3: Server-Crate Tables

**Files modified**: `registry.rs`, `audit.rs`
**Types potentially moved**: `AgentRecord`, `AuditEvent`, `TrustLevel`, `Capability`, `Outcome`

| Task | Files | Risk |
|------|-------|------|
| Move `AgentRecord`, `TrustLevel`, `Capability` to store crate | schema.rs, registry.rs | Medium |
| Move `AuditEvent`, `Outcome` to store crate | schema.rs, audit.rs | Medium |
| AGENT_REGISTRY: 8 SQL columns + JSON arrays | db.rs, registry.rs | Medium |
| AUDIT_LOG: 8 SQL columns + JSON `target_ids` + indexes | db.rs, audit.rs | Medium |
| Rewrite `AuditLog::log_event()` — direct SQL, named params | audit.rs | Low |
| Rewrite `AuditLog::write_in_txn()` — direct SQL via `&*txn.guard` | audit.rs | Medium |
| Rewrite `AgentRegistry` methods — direct SQL, named params | registry.rs | Medium |
| Add `TryFrom<u8>` for TrustLevel, Capability, Outcome | schema.rs | Low |

Gate: `cargo build --workspace && cargo test --workspace`.

### Wave 4: Compat Layer Removal + Cleanup

**Files deleted**: `handles.rs`, `dispatch.rs`
**Files modified**: `tables.rs` (gutted), `txn.rs` (simplified), `lib.rs` (re-exports cleaned)

| Task | Files | Risk |
|------|-------|------|
| Delete `handles.rs` | handles.rs | Low |
| Delete `dispatch.rs` | dispatch.rs | Low |
| Gut `tables.rs` — remove all table constants, marker types, guard types | tables.rs | Low |
| Simplify `txn.rs` — remove `SqliteReadTransaction`, column-mapping functions | txn.rs | Low |
| Remove `Store::begin_read()` | db.rs | Low |
| Remove runtime bincode serialize/deserialize for normalized tables | schema.rs, sessions.rs, injection_log.rs, signal.rs | Low |
| Clean `lib.rs` re-exports — remove compat layer exports | lib.rs | Low |
| Verify no references to deleted types remain | all files | Low |

Gate: `cargo build --workspace && cargo test --workspace`, zero references to deleted types.

### Wave 5: Verification

| Check | Method |
|-------|--------|
| All 12 MCP tools produce identical results | Integration test or manual verification |
| No bincode for normalized tables | `grep -r "serialize_entry\|deserialize_entry\|serialize_co_access\|serialize_session\|serialize_injection_log\|serialize_signal\|serialize_agent\|serialize_audit" --include="*.rs"` — only `migration_compat.rs` and OBSERVATION_METRICS |
| Schema version is 6 | `SELECT value FROM counters WHERE name = 'schema_version'` |
| Migration works from v5 | Synthetic v5 database test |
| Entry additions use ALTER TABLE | Documented, no scan-and-rewrite migration for future fields |

## Integration Surface Summary

### Store Crate (unimatrix-store)

| File | Current Lines | Change Type | Wave |
|------|--------------|-------------|------|
| `db.rs` | 185 | Heavy rewrite (DDL) | 1, 2, 3 |
| `read.rs` | 443 | Heavy rewrite (SQL WHERE) | 1, 2 |
| `write.rs` | 425 | Heavy rewrite (named params) | 1 |
| `write_ext.rs` | 382 | Heavy rewrite (co_access, usage) | 1, 2 |
| `schema.rs` | 590 | Moderate (remove bincode helpers, add types) | 1, 3, 4 |
| `sessions.rs` | 354 | Heavy rewrite (SQL columns) | 2 |
| `injection_log.rs` | 137 | Heavy rewrite (SQL columns) | 2 |
| `signal.rs` | 292 | Heavy rewrite (SQL columns + JSON) | 2 |
| `migration.rs` | 134 | Heavy extension (v5->v6) | 0 |
| `counters.rs` | new (~60) | New module | 0 |
| `migration_compat.rs` | new (~100) | New module | 0 |
| `txn.rs` | 90 | Simplified (remove read txn) | 4 |
| `tables.rs` | 182 | Gutted (remove all) | 4 |
| `handles.rs` | 428 | Deleted | 4 |
| `dispatch.rs` | 134 | Deleted | 4 |
| `lib.rs` | 62 | Moderate (re-exports) | 1, 3, 4 |

### Server Crate (unimatrix-server)

| File | Current Lines | Change Type | Wave |
|------|--------------|-------------|------|
| `services/store_ops.rs` | 351 | Heavy rewrite (direct SQL) | 1 |
| `services/store_correct.rs` | 327 | Heavy rewrite (direct SQL) | 1 |
| `infra/registry.rs` | 936 | Heavy rewrite (direct SQL + JSON) | 3 |
| `infra/audit.rs` | 595 | Heavy rewrite (direct SQL + JSON) | 3 |
| `services/status.rs` | est. ~100 | Moderate (query entries table) | 1 |
| `infra/contradiction.rs` | est. ~100 | Moderate (query entries table) | 1 |

### Lines Impact Estimate

| Metric | Estimate |
|--------|----------|
| Lines deleted | ~1,200 (handles.rs, dispatch.rs, tables.rs, index sync code, bincode helpers) |
| Lines added | ~400 (migration_compat, counters, DDL, entry_from_row, load_tags) |
| Lines rewritten | ~2,000 (read.rs, write.rs, sessions.rs, signal.rs, injection_log.rs, write_ext.rs, store_ops.rs, store_correct.rs, registry.rs, audit.rs) |
| Net change | ~-800 lines |

## Risk Mitigation Summary

| Risk | Severity | Mitigation | ADR |
|------|----------|------------|-----|
| SR-01: Migration data fidelity | HIGH | migration_compat.rs, automatic backup, round-trip tests | ADR-005 |
| SR-02: 24-column bind parameter accuracy | HIGH | Named params mandatory | ADR-004 |
| SR-03: SQL query semantic equivalence | HIGH | Behavioral parity tests before/after rewrite | (testing) |
| SR-04: Compat layer open questions | MEDIUM | All 3 resolved toward removal; Waves 1-3 bypass compat | ADR-001, 002, 003 |
| SR-05: Cross-crate coupling | MEDIUM | Each wave includes both crates; workspace build gate | ADR-008 |
| SR-06: JSON columns vs future analytics | MEDIUM | JSON for non-queried fields; ASS-016 path uses indexed cols | ADR-007 |
| SR-07: Enum-to-integer mapping stability | MEDIUM | INTEGER with repr(u8); TryFrom validation on read | ADR-003 |
| SR-08: entry_tags consistency | LOW | Foreign key CASCADE; PRAGMA foreign_keys ON | ADR-006 |

## Dependencies

| Dependency | Status | Impact |
|-----------|--------|--------|
| nxs-007 (redb removal) | Must be merged | Prerequisite — SQLite sole backend |
| rusqlite | Already in Cargo.toml | No change |
| serde_json | Add to unimatrix-store | JSON array columns |
| bincode | Remains in Cargo.toml | migration_compat + OBSERVATION_METRICS |
