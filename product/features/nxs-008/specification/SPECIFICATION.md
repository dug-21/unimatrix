# nxs-008: Schema Normalization — Specification

**Feature**: nxs-008
**Status**: Draft
**Date**: 2026-03-05
**Predecessor**: nxs-007 (redb removal), ASS-016 (retrospective data architecture)
**ADRs**: #352 (server decoupling rejected), #354 (OBSERVATION_METRICS excluded)

---

## 1. Overview

nxs-008 decomposes 7 tables from bincode blobs to SQL columns, eliminates 5 manual index tables (replacing them with native SQL indexes), removes the redb-pattern compat layer, and replaces client-side HashSet intersection query logic with SQL WHERE clauses. Schema version advances from v5 to v6. The Store public API (`EntryRecord`, `Store` methods) is unchanged — this is an internal restructuring with strict behavioral parity.

### 1.1 Scope Boundaries

**In scope**: ENTRIES, CO_ACCESS, SESSIONS, INJECTION_LOG, SIGNAL_QUEUE, AGENT_REGISTRY, AUDIT_LOG decomposition; entry_tags junction table; 5 index table elimination; compat layer removal (handles.rs, dispatch.rs); v5-to-v6 migration.

**Out of scope**: OBSERVATION_METRICS normalization (ADR #354); VECTOR_MAP changes; HNSW index changes; Store public API changes; new MCP tools; server decoupling (ADR #352).

---

## 2. Architectural Decisions

### 2.1 AD-01: Enum Storage Format — INTEGER

**Decision**: All enum types use INTEGER storage with `#[repr(u8)]` discriminant values, matching the existing bincode representation.

**Rationale**: Seven enum types (Status, SessionLifecycleStatus, SignalType, SignalSource, Outcome, TrustLevel, Capability) already use `#[repr(u8)]`. INTEGER is compact, sortable, and the existing `TryFrom<u8>` implementations provide safe conversion. TEXT would add string parsing overhead and increase storage without benefit. **Resolves SCOPE Open Question #3.**

**Risk mitigation (SR-07)**: The migration path fully deserializes bincode records before extracting enum values — it never reads raw discriminant bytes from blobs. A unit test for each enum asserts `bincode_roundtrip_value == discriminant as u8`.

### 2.2 AD-02: Transaction Wrappers — Keep Simplified

**Decision**: Keep `SqliteWriteTransaction` in simplified form; remove `SqliteReadTransaction`. The server's `write_in_txn` pattern (audit.rs) requires a transaction wrapper with `commit()`/rollback-on-drop semantics. `SqliteReadTransaction` is a trivial MutexGuard wrapper with no behavioral value.

**Rationale**: `SqliteWriteTransaction` provides real RAII transaction safety (BEGIN IMMEDIATE on create, ROLLBACK on drop, COMMIT on explicit call). Removing it would force every server call site to manually manage BEGIN/COMMIT/ROLLBACK. `SqliteReadTransaction` adds no safety — it's a type alias for `MutexGuard`. **Resolves SCOPE Open Question #1.**

**Impact**: After nxs-008, `SqliteWriteTransaction` loses `open_table`/`open_multimap_table` methods (dispatch.rs removed). Server code that uses `write_in_txn` accesses the connection directly via `txn.guard` for raw SQL.

### 2.3 AD-03: Counter Helpers — Inline into Callers

**Decision**: Remove `tables::next_entry_id`, `tables::increment_counter`, `tables::decrement_counter`. The equivalent functions already exist in `write.rs` (`read_counter`, `set_counter`, `increment_counter`, `decrement_counter`). Server call sites that use `tables::` counter helpers via `SqliteWriteTransaction` switch to direct SQL on `txn.guard`.

**Rationale**: Two parallel counter implementations exist (tables.rs for server, write.rs for store). After compat removal, a single set of counter helpers in write.rs (or a small `counter.rs` module) suffices. **Resolves SCOPE Open Question #2.**

### 2.4 AD-04: Waves 1-3 Bypass Compat Types

**Decision**: Waves 1-3 write direct SQL without using compat types (tables.rs handles, dispatch.rs traits). Wave 4 then removes dead code rather than performing a second rewrite.

**Rationale (SR-04)**: If Waves 1-3 build on compat types, Wave 4 must rewrite every path again. Bypassing from the start means each path is rewritten exactly once.

### 2.5 AD-05: Named Parameters for EntryRecord SQL

**Decision**: All SQL statements involving the 24-column entries table MUST use rusqlite `named_params!{}` macro instead of positional `?` placeholders.

**Rationale (SR-02)**: With 24 columns, positional bind parameter errors are near-certain. Named parameters (`:id`, `:title`, `:content`, etc.) make column-order bugs impossible and are self-documenting.

### 2.6 AD-06: Foreign Keys Enabled

**Decision**: Change `PRAGMA foreign_keys = OFF` to `PRAGMA foreign_keys = ON` in `Store::open_with_config`. Add `FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE` to entry_tags.

**Rationale (SR-08)**: entry_tags junction table requires CASCADE delete to prevent orphan rows. Foreign key enforcement also benefits INJECTION_LOG (entry_id FK) for data integrity.

### 2.7 AD-07: Migration Preserves Deserializers

**Decision**: Bincode deserialize functions (`deserialize_entry`, `deserialize_session`, `deserialize_injection_log`, `deserialize_signal`, `deserialize_co_access`, `deserialize_agent`, `deserialize_audit_event`) are retained in a `migration_compat` module gated behind `#[cfg(feature = "migration")]` or kept as dead code with `#[allow(dead_code)]` annotation until v5 databases no longer exist in the wild.

**Rationale (SR-01)**: The v5-to-v6 migration must deserialize every existing bincode blob. Removing deserializers before migration code is complete would make migration impossible. Keeping them in a dedicated module prevents accidental use in runtime paths.

---

## 3. Domain Models

### 3.1 `entries` Table (24 columns)

Replaces: `entries (id INTEGER PK, data BLOB)` + 5 index tables.

```sql
CREATE TABLE entries (
    id                INTEGER PRIMARY KEY,
    title             TEXT    NOT NULL,
    content           TEXT    NOT NULL,
    topic             TEXT    NOT NULL,
    category          TEXT    NOT NULL,
    source            TEXT    NOT NULL,
    status            INTEGER NOT NULL DEFAULT 0,
    confidence        REAL    NOT NULL DEFAULT 0.0,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    last_accessed_at  INTEGER NOT NULL DEFAULT 0,
    access_count      INTEGER NOT NULL DEFAULT 0,
    supersedes        INTEGER,
    superseded_by     INTEGER,
    correction_count  INTEGER NOT NULL DEFAULT 0,
    embedding_dim     INTEGER NOT NULL DEFAULT 0,
    created_by        TEXT    NOT NULL DEFAULT '',
    modified_by       TEXT    NOT NULL DEFAULT '',
    content_hash      TEXT    NOT NULL DEFAULT '',
    previous_hash     TEXT    NOT NULL DEFAULT '',
    version           INTEGER NOT NULL DEFAULT 0,
    feature_cycle     TEXT    NOT NULL DEFAULT '',
    trust_source      TEXT    NOT NULL DEFAULT '',
    helpful_count     INTEGER NOT NULL DEFAULT 0,
    unhelpful_count   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_entries_topic      ON entries(topic);
CREATE INDEX idx_entries_category   ON entries(category);
CREATE INDEX idx_entries_status     ON entries(status);
CREATE INDEX idx_entries_created_at ON entries(created_at);
```

**Column-to-Rust mapping**:

| Column | Rust Type | SQL Type | Nullable | Default | Notes |
|--------|-----------|----------|----------|---------|-------|
| id | u64 | INTEGER PK | No | — | Stored as i64, cast on read |
| title | String | TEXT | No | — | |
| content | String | TEXT | No | — | |
| topic | String | TEXT | No | — | Indexed |
| category | String | TEXT | No | — | Indexed |
| source | String | TEXT | No | — | |
| status | Status (u8) | INTEGER | No | 0 | Indexed, `#[repr(u8)]`: Active=0, Deprecated=1, Proposed=2, Quarantined=3 |
| confidence | f64 | REAL | No | 0.0 | |
| created_at | u64 | INTEGER | No | — | Indexed, Unix epoch seconds |
| updated_at | u64 | INTEGER | No | — | |
| last_accessed_at | u64 | INTEGER | No | 0 | |
| access_count | u32 | INTEGER | No | 0 | |
| supersedes | Option\<u64\> | INTEGER | Yes | NULL | FK-like to entries(id) |
| superseded_by | Option\<u64\> | INTEGER | Yes | NULL | FK-like to entries(id) |
| correction_count | u32 | INTEGER | No | 0 | |
| embedding_dim | u16 | INTEGER | No | 0 | |
| created_by | String | TEXT | No | '' | |
| modified_by | String | TEXT | No | '' | |
| content_hash | String | TEXT | No | '' | |
| previous_hash | String | TEXT | No | '' | |
| version | u32 | INTEGER | No | 0 | |
| feature_cycle | String | TEXT | No | '' | |
| trust_source | String | TEXT | No | '' | |
| helpful_count | u32 | INTEGER | No | 0 | |
| unhelpful_count | u32 | INTEGER | No | 0 | |

**Future JOIN-ability**: `entries.id` is the primary key for JOINs with `injection_log.entry_id`, `entry_tags.entry_id`, `feature_entries.entry_id`, `outcome_index.entry_id`, `vector_map.entry_id`, and `co_access.entry_id_a`/`entry_id_b`. All indexed columns support WHERE-clause filtering in JOINed queries.

### 3.2 `entry_tags` Junction Table

Replaces: TAG_INDEX multimap table + `EntryRecord.tags: Vec<String>`.

```sql
CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL,
    tag      TEXT    NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX idx_entry_tags_tag ON entry_tags(tag);
```

| Column | Rust Type | SQL Type | Nullable | Notes |
|--------|-----------|----------|----------|-------|
| entry_id | u64 | INTEGER | No | FK to entries(id), CASCADE delete |
| tag | String | TEXT | No | Indexed for tag-based queries |

**Query semantics**: Tag filtering uses AND semantics (intersection). An entry must have ALL specified tags to match. SQL: `SELECT entry_id FROM entry_tags WHERE tag IN (...) GROUP BY entry_id HAVING COUNT(DISTINCT tag) = ?`.

**Tag loading**: Tags are loaded via a separate batch query: `SELECT entry_id, tag FROM entry_tags WHERE entry_id IN (?,?,?)`, then grouped by entry_id in Rust to populate `EntryRecord.tags`.

### 3.3 `co_access` Table

Replaces: `co_access (entry_id_a, entry_id_b, data BLOB)`.

```sql
CREATE TABLE co_access (
    entry_id_a    INTEGER NOT NULL,
    entry_id_b    INTEGER NOT NULL,
    count         INTEGER NOT NULL DEFAULT 0,
    last_updated  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (entry_id_a, entry_id_b),
    CHECK (entry_id_a < entry_id_b)
);

CREATE INDEX idx_co_access_b ON co_access(entry_id_b);
```

| Column | Rust Type | SQL Type | Nullable | Default | Notes |
|--------|-----------|----------|----------|---------|-------|
| entry_id_a | u64 | INTEGER | No | — | Composite PK, CHECK enforces a < b |
| entry_id_b | u64 | INTEGER | No | — | Composite PK, indexed for reverse lookup |
| count | u32 | INTEGER | No | 0 | Co-retrieval count |
| last_updated | u64 | INTEGER | No | 0 | Unix epoch, used for staleness filtering |

**Future JOIN-ability**: `co_access.last_updated` as a SQL column enables `WHERE last_updated >= ?` without blob deserialization, replacing the current full-table-scan + deserialize pattern in `co_access_stats` and `top_co_access_pairs`.

### 3.4 `sessions` Table

Replaces: `sessions (session_id TEXT PK, data BLOB)`.

```sql
CREATE TABLE sessions (
    session_id       TEXT    PRIMARY KEY,
    feature_cycle    TEXT,
    agent_role       TEXT,
    started_at       INTEGER NOT NULL,
    ended_at         INTEGER,
    status           INTEGER NOT NULL DEFAULT 0,
    compaction_count INTEGER NOT NULL DEFAULT 0,
    outcome          TEXT,
    total_injections INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle);
CREATE INDEX idx_sessions_started_at    ON sessions(started_at);
```

| Column | Rust Type | SQL Type | Nullable | Default | Notes |
|--------|-----------|----------|----------|---------|-------|
| session_id | String | TEXT PK | No | — | UUID-format session identifier |
| feature_cycle | Option\<String\> | TEXT | Yes | NULL | Indexed for feature-scoped queries |
| agent_role | Option\<String\> | TEXT | Yes | NULL | |
| started_at | u64 | INTEGER | No | — | Indexed for GC age queries |
| ended_at | Option\<u64\> | INTEGER | Yes | NULL | Set on SessionClose |
| status | SessionLifecycleStatus | INTEGER | No | 0 | Active=0, Completed=1, TimedOut=2, Abandoned=3 |
| compaction_count | u32 | INTEGER | No | 0 | |
| outcome | Option\<String\> | TEXT | Yes | NULL | "success" / "rework" / "abandoned" |
| total_injections | u32 | INTEGER | No | 0 | |

**Future JOIN-ability**: `sessions.session_id` joins with `injection_log.session_id`. `sessions.feature_cycle` joins with `entries` via `feature_entries` for entry effectiveness analysis (ASS-016 Tier 1-A). The `feature_cycle` and `started_at` indexes replace the current full-table-scan + deserialize-and-filter pattern in `scan_sessions_by_feature` and `gc_sessions`.

### 3.5 `injection_log` Table

Replaces: `injection_log (log_id INTEGER PK, data BLOB)`.

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

| Column | Rust Type | SQL Type | Nullable | Notes |
|--------|-----------|----------|----------|-------|
| log_id | u64 | INTEGER PK | No | Monotonic, from next_log_id counter |
| session_id | String | TEXT | No | Indexed for session cascade GC |
| entry_id | u64 | INTEGER | No | Indexed for entry effectiveness queries |
| confidence | f64 | REAL | No | Reranked score at injection time |
| timestamp | u64 | INTEGER | No | Unix epoch seconds |

**Future JOIN-ability**: This is the **critical JOIN table** for ASS-016 entry effectiveness analysis:
```sql
SELECT e.id, e.title,
  COUNT(CASE WHEN s.outcome = 'success' THEN 1 END) as success_count,
  COUNT(CASE WHEN s.outcome = 'rework' THEN 1 END) as rework_count
FROM injection_log il
JOIN sessions s ON il.session_id = s.session_id
JOIN entries e ON il.entry_id = e.id
WHERE s.feature_cycle = ?
GROUP BY e.id
```
The `session_id` index replaces the current full-table-scan pattern in `gc_sessions` cascade deletion. The `entry_id` index enables future per-entry injection frequency queries.

### 3.6 `signal_queue` Table

Replaces: `signal_queue (signal_id INTEGER PK, data BLOB)`.

```sql
CREATE TABLE signal_queue (
    signal_id     INTEGER PRIMARY KEY,
    session_id    TEXT    NOT NULL,
    created_at    INTEGER NOT NULL,
    entry_ids     TEXT    NOT NULL DEFAULT '[]',
    signal_type   INTEGER NOT NULL,
    signal_source INTEGER NOT NULL
);
```

| Column | Rust Type | SQL Type | Nullable | Default | Notes |
|--------|-----------|----------|----------|---------|-------|
| signal_id | u64 | INTEGER PK | No | — | Monotonic, from next_signal_id counter |
| session_id | String | TEXT | No | — | |
| created_at | u64 | INTEGER | No | — | Unix epoch seconds |
| entry_ids | Vec\<u64\> | TEXT (JSON) | No | '[]' | JSON array, e.g. `[10,20,30]` |
| signal_type | SignalType | INTEGER | No | — | Helpful=0, Flagged=1 |
| signal_source | SignalSource | INTEGER | No | — | ImplicitOutcome=0, ImplicitRework=1 |

**JSON column justification (SR-06)**: `entry_ids` is only read as a complete list during signal drain — never queried by individual element. JSON array is adequate. Signal queue is a work queue with bounded size (10,000 cap) and records are deleted after consumption.

**Serialization**: `entry_ids` serialized via `serde_json::to_string(&record.entry_ids)`, deserialized via `serde_json::from_str::<Vec<u64>>`.

### 3.7 `agent_registry` Table

Replaces: `agent_registry (agent_id TEXT PK, data BLOB)`.

```sql
CREATE TABLE agent_registry (
    agent_id           TEXT    PRIMARY KEY,
    trust_level        INTEGER NOT NULL,
    capabilities       TEXT    NOT NULL DEFAULT '[]',
    allowed_topics     TEXT,
    allowed_categories TEXT,
    enrolled_at        INTEGER NOT NULL,
    last_seen_at       INTEGER NOT NULL,
    active             INTEGER NOT NULL DEFAULT 1
);
```

| Column | Rust Type | SQL Type | Nullable | Default | Notes |
|--------|-----------|----------|----------|---------|-------|
| agent_id | String | TEXT PK | No | — | |
| trust_level | TrustLevel | INTEGER | No | — | System=0, Privileged=1, Internal=2, Restricted=3 |
| capabilities | Vec\<Capability\> | TEXT (JSON) | No | '[]' | JSON array of integer discriminants |
| allowed_topics | Option\<Vec\<String\>\> | TEXT (JSON) | Yes | NULL | NULL = all topics allowed |
| allowed_categories | Option\<Vec\<String\>\> | TEXT (JSON) | Yes | NULL | NULL = all categories allowed |
| enrolled_at | u64 | INTEGER | No | — | Unix epoch seconds |
| last_seen_at | u64 | INTEGER | No | — | Unix epoch seconds |
| active | bool | INTEGER | No | 1 | 0=false, 1=true |

**JSON column justification (SR-06)**: `capabilities` (5 enum variants max), `allowed_topics`, and `allowed_categories` are loaded as complete records into memory — never queried by individual element. JSON array is adequate.

**Enum discriminants for JSON**: Capability values stored as integers in JSON: `[0,1,2]` for `[Read, Write, Search]`. Deserialization uses `serde_json` with integer-to-enum mapping.

### 3.8 `audit_log` Table

Replaces: `audit_log (event_id INTEGER PK, data BLOB)`.

```sql
CREATE TABLE audit_log (
    event_id   INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    session_id TEXT    NOT NULL,
    agent_id   TEXT    NOT NULL,
    operation  TEXT    NOT NULL,
    target_ids TEXT    NOT NULL DEFAULT '[]',
    outcome    INTEGER NOT NULL,
    detail     TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX idx_audit_log_agent_id  ON audit_log(agent_id);
CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
```

| Column | Rust Type | SQL Type | Nullable | Default | Notes |
|--------|-----------|----------|----------|---------|-------|
| event_id | u64 | INTEGER PK | No | — | Monotonic, from next_audit_id counter |
| timestamp | u64 | INTEGER | No | — | Indexed for time-range queries |
| session_id | String | TEXT | No | — | |
| agent_id | String | TEXT | No | — | Indexed for `write_count_since` |
| operation | String | TEXT | No | — | Tool name |
| target_ids | Vec\<u64\> | TEXT (JSON) | No | '[]' | JSON array, append-only |
| outcome | Outcome | INTEGER | No | — | Success=0, Denied=1, Error=2, NotImplemented=3 |
| detail | String | TEXT | No | '' | |

**Indexes**: `agent_id` index replaces the current full-table-scan in `write_count_since`. `timestamp` index enables future time-range audit queries.

**JSON column justification (SR-06)**: `target_ids` is write-only in an append-only log. If future analytics need "which agents modified entry X?" queries, `json_each(target_ids)` is functional. If performance degrades at scale, a junction table can be added in a future migration.

---

## 4. Tables Unchanged

| Table | Schema | Reason |
|-------|--------|--------|
| vector_map | `(entry_id INTEGER PK, hnsw_data_id INTEGER)` | Simple KV, already normalized |
| counters | `(name TEXT PK, value INTEGER)` | Simple KV, already normalized |
| feature_entries | `(feature_id TEXT, entry_id INTEGER, PK)` | Simple composite, already normalized |
| outcome_index | `(feature_cycle TEXT, entry_id INTEGER, PK)` | Simple composite, already normalized |
| observation_metrics | `(feature_cycle TEXT PK, data BLOB)` | Excluded per ADR #354 |

---

## 5. Tables Eliminated

| Table | Replaced By | Index Sync Code Eliminated |
|-------|-------------|---------------------------|
| topic_index | `CREATE INDEX idx_entries_topic ON entries(topic)` | ~14 lines per write path |
| category_index | `CREATE INDEX idx_entries_category ON entries(category)` | ~14 lines per write path |
| tag_index | `entry_tags` junction table + `idx_entry_tags_tag` | ~24 lines (multimap loop) |
| time_index | `CREATE INDEX idx_entries_created_at ON entries(created_at)` | ~14 lines per write path |
| status_index | `CREATE INDEX idx_entries_status ON entries(status)` | ~14 lines per write path |

---

## 6. Query Semantics (SR-03 Mitigation)

### 6.1 Current Query Semantics (Must Be Preserved)

The following semantics are documented from `read.rs` and must be preserved exactly:

1. **Tag filtering is AND (intersection)**: `collect_ids_by_tags` intersects per-tag ID sets. An entry must have ALL specified tags. (read.rs:68-92)
2. **Empty filter → Active status**: When all QueryFilter fields are None, `query()` defaults to `status = Active`. (read.rs:208-218)
3. **Empty tags list → skip tag filter**: When `filter.tags = Some(vec![])`, the tag filter is skipped (not applied). (read.rs:228-231)
4. **Invalid time range → empty result**: When `range.start > range.end`, `query_by_time_range` returns empty. (read.rs:189-191)
5. **Multiple filters → intersection**: All non-None filters are intersected (AND semantics across dimensions). (read.rs:247-249)
6. **Option fields**: `supersedes` and `superseded_by` are stored as NULL in SQL when None. No queries filter on these fields.
7. **Status as integer**: Status filtering compares `status as u8` values. (read.rs:121)

### 6.2 New Query Implementation

**Single-query entry fetch** (replaces HashSet intersection + N+1):

```sql
-- Dynamic WHERE clause built in Rust
SELECT id, title, content, topic, category, source, status, confidence,
       created_at, updated_at, last_accessed_at, access_count,
       supersedes, superseded_by, correction_count, embedding_dim,
       created_by, modified_by, content_hash, previous_hash,
       version, feature_cycle, trust_source, helpful_count, unhelpful_count
FROM entries
WHERE topic = :topic           -- if filter.topic is Some
  AND category = :category     -- if filter.category is Some
  AND status = :status         -- if filter.status is Some (or default Active)
  AND created_at BETWEEN :start AND :end  -- if filter.time_range is Some
  AND id IN (
    SELECT entry_id FROM entry_tags
    WHERE tag IN (:tag1, :tag2)
    GROUP BY entry_id
    HAVING COUNT(DISTINCT tag) = :tag_count
  )                            -- if filter.tags is Some and non-empty
```

**Tag loading** (separate batch query):

```sql
SELECT entry_id, tag FROM entry_tags
WHERE entry_id IN (:id1, :id2, :id3)
ORDER BY entry_id, tag
```

Tags are grouped by `entry_id` in Rust and assigned to each `EntryRecord.tags`.

### 6.3 `load_tags_for_entries` Helper (SR-08)

A single helper function is mandated for tag loading:

```rust
fn load_tags_for_entries(
    conn: &Connection,
    entry_ids: &[u64],
) -> Result<HashMap<u64, Vec<String>>>
```

Every code path that constructs `EntryRecord` from SQL rows MUST call this helper to populate the `tags` field. This prevents the silent-empty-tags bug identified in SR-08.

---

## 7. Write Path Specification

### 7.1 Insert Path

**Before** (write.rs): 7+ SQL statements (serialize blob, insert entry, insert into 5 index tables, update counter).

**After**: 2+ SQL statements:

1. `INSERT INTO entries (...24 columns...) VALUES (:id, :title, ...)` — using `named_params!{}` (AD-05)
2. `INSERT INTO entry_tags (entry_id, tag) VALUES (:entry_id, :tag)` — per tag, in loop
3. `UPDATE counters SET value = :val WHERE name = :key` — status counter increment

All within a single `BEGIN IMMEDIATE` / `COMMIT` transaction.

### 7.2 Update Path

**Before** (write.rs): Read blob, deserialize, serialize updated blob, diff-based index sync (2-6+ SQL statements).

**After**: 2+ SQL statements:

1. `UPDATE entries SET title=:title, content=:content, ... WHERE id=:id` — using `named_params!{}`
2. `DELETE FROM entry_tags WHERE entry_id = :id` — clear all tags
3. `INSERT INTO entry_tags (entry_id, tag) VALUES (:entry_id, :tag)` — per tag, in loop
4. Status counter adjustment (if status changed)

No diff-based index sync needed — SQL indexes update automatically.

### 7.3 Delete Path

**Before** (write.rs): Read blob, deserialize (to get field values for index deletion), delete from 6 tables.

**After**: 1 SQL statement:

1. `DELETE FROM entries WHERE id = :id` — CASCADE deletes entry_tags rows (AD-06)
2. `DELETE FROM vector_map WHERE entry_id = :id` — no FK, manual delete
3. Status counter decrement

The old entry must still be read before deletion to determine its status (for counter adjustment) and to verify it exists (for EntryNotFound error).

### 7.4 Update Status Path

**Before** (write.rs): Read blob, deserialize, modify status, serialize, write blob, update status_index.

**After**:

1. `SELECT status FROM entries WHERE id = :id` — get old status (no blob deserialize)
2. `UPDATE entries SET status = :new_status, updated_at = :now WHERE id = :id`
3. Status counter adjustment

---

## 8. Migration Specification (v5 to v6)

### 8.1 Strategy

The migration follows a create-new-then-swap pattern for safety (SR-01):

1. Create `entries_v6` table with 24 columns
2. Create `entry_tags` table
3. For each row in old `entries`: deserialize bincode blob, INSERT into `entries_v6` (columns), INSERT tags into `entry_tags`
4. Create normalized versions of operational tables (co_access_v6, sessions_v6, injection_log_v6, signal_queue_v6)
5. For each row in old operational tables: deserialize bincode blob, INSERT into new table (columns)
6. Create normalized server tables (agent_registry_v6, audit_log_v6)
7. For each row in old server tables: deserialize bincode blob, INSERT into new table (columns)
8. Drop old tables (entries, topic_index, category_index, tag_index, time_index, status_index, co_access, sessions, injection_log, signal_queue, agent_registry, audit_log)
9. Rename new tables (entries_v6 → entries, etc.)
10. Create SQL indexes
11. Update schema_version counter to 6

### 8.2 Ordering Constraint (SR-01)

Migration code MUST be written and tested BEFORE bincode infrastructure is removed from runtime paths. The implementation ordering is:

1. Write migration.rs v5→v6 function (uses existing deserialize_* functions)
2. Write migration tests (round-trip tests with synthetic v5 data)
3. Rewrite runtime read/write paths to use SQL columns
4. Move deserialize_* functions to migration_compat module (AD-07)
5. Remove remaining bincode infrastructure from runtime paths

### 8.3 Database Backup

Before starting migration, copy the database file:

```rust
std::fs::copy(&db_path, db_path.with_extension("db.v5-backup"))?;
```

This provides a rollback point for the one-way door migration.

### 8.4 Historical Schema Compatibility

Entries written at schema versions v0-v5 will have accumulated different `serde(default)` field sets. The migration deserializes using the current `EntryRecord` struct, which handles all historical versions via `#[serde(default)]` annotations. The resulting SQL columns will have correct defaults for any fields that were absent in the original blob.

**Test requirement**: Synthetic entries from each historical schema version (v0, v1, v2, v3, v5) must round-trip through the migration path and produce correct column values.

---

## 9. Compat Layer Removal

### 9.1 Files Removed

| File | Lines | Purpose | Removal Condition |
|------|-------|---------|-------------------|
| handles.rs | ~428 | Typed table handle wrappers (TableU64Blob, TableStrU64, etc.) | All server call sites rewritten to direct SQL |
| dispatch.rs | ~134 | TableSpec/MultimapSpec traits, open_table dispatch | All server call sites rewritten |

### 9.2 Files Simplified

| File | Current | After | Changes |
|------|---------|-------|---------|
| tables.rs | ~182 lines (table constants, marker types, counter helpers, guard types, RangeResult) | Removed entirely | Table constants, guard types, marker types, counter helpers — all dead code after normalization |
| txn.rs | ~90 lines | ~52 lines | Remove `SqliteReadTransaction`, remove `primary_key_column`/`data_column` mapping functions; keep `SqliteWriteTransaction` (AD-02) |
| lib.rs | Re-exports compat types | Remove all compat re-exports | Remove `handles`, `dispatch`, `tables` module declarations and re-exports |

### 9.3 Server Code Changes

| Server File | Current Pattern | After Pattern |
|-------------|-----------------|---------------|
| infra/audit.rs | `txn.open_table(AUDIT_LOG)`, `txn.open_table(COUNTERS)`, bincode serialize | Direct SQL on `txn.guard`: `INSERT INTO audit_log (event_id, timestamp, ...) VALUES (...)` |
| infra/registry.rs | `txn.open_table(AGENT_REGISTRY)`, bincode serialize/deserialize | Direct SQL on `txn.guard`: `SELECT/INSERT/UPDATE agent_registry` with columns |
| services/store_ops.rs | Imports table constants, `serialize_entry`, writes index tables | SQL INSERT with named params, no index writes |
| services/store_correct.rs | Manual deprecate + create across 8 tables | SQL UPDATE + INSERT, no index table manipulation |
| services/status.rs | Scans index tables for counts | `SELECT COUNT(*) FROM entries WHERE status = ?` |
| infra/contradiction.rs | Scans STATUS_INDEX + deserializes entry blobs | `SELECT ... FROM entries WHERE status = ?` |

---

## 10. Acceptance Criteria

### Wave 1: ENTRIES Decomposition + Index Elimination

**AC-01**: ENTRIES table has 24 SQL columns instead of a bincode blob.
- Verified by: `PRAGMA table_info(entries)` returns 24 columns with correct names and types.
- Named parameters (`:id`, `:title`, etc.) used in all INSERT/UPDATE statements (AD-05).

**AC-02**: `entry_tags` junction table exists with `(entry_id INTEGER, tag TEXT, PRIMARY KEY(entry_id, tag))`.
- Verified by: `PRAGMA table_info(entry_tags)` returns 2 columns.
- Foreign key: `entry_id REFERENCES entries(id) ON DELETE CASCADE`.
- Index: `idx_entry_tags_tag` on `entry_tags(tag)`.

**AC-03**: TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX tables eliminated.
- Verified by: `SELECT name FROM sqlite_master WHERE type='table'` does not include any of the 5 index tables.

**AC-04**: SQL indexes exist on `entries(topic)`, `entries(category)`, `entries(status)`, `entries(created_at)`, `entry_tags(tag)`.
- Verified by: `SELECT name FROM sqlite_master WHERE type='index'` includes all 5 index names.

**AC-11**: read.rs query path uses SQL WHERE clauses, not HashSet intersection.
- Verified by: Code review — no `HashSet<u64>` intersection in read.rs. Single SQL query with dynamic WHERE clause.
- Query semantics preserved: tag AND, empty-filter defaults to Active, empty tags skipped, time range validation.

**AC-12**: N+1 entry fetch pattern eliminated — queries return entries directly.
- Verified by: Code review — `fetch_entries` loop replaced by batch SELECT. `load_tags_for_entries` helper used.

### Wave 2: Store-Crate Operational Tables

**AC-05**: CO_ACCESS table has SQL columns: `entry_id_a`, `entry_id_b`, `count`, `last_updated`.
- Verified by: `PRAGMA table_info(co_access)` returns 4 columns, no `data BLOB`.
- CHECK constraint: `entry_id_a < entry_id_b`.
- Index: `idx_co_access_b` on `co_access(entry_id_b)`.

**AC-06**: SESSIONS table has SQL columns for all 9 SessionRecord fields.
- Verified by: `PRAGMA table_info(sessions)` returns 9 columns, no `data BLOB`.
- Indexes: `idx_sessions_feature_cycle`, `idx_sessions_started_at`.
- `scan_sessions_by_feature` uses `WHERE feature_cycle = ?` instead of full table scan.
- `gc_sessions` uses `WHERE started_at < ?` instead of full table scan + deserialize.

**AC-07**: INJECTION_LOG table has SQL columns for all 5 fields, with indexed `session_id`.
- Verified by: `PRAGMA table_info(injection_log)` returns 5 columns, no `data BLOB`.
- Indexes: `idx_injection_log_session`, `idx_injection_log_entry`.
- GC cascade uses `DELETE FROM injection_log WHERE session_id IN (...)` instead of full table scan.

**AC-08**: SIGNAL_QUEUE table has SQL columns with `entry_ids` as JSON array.
- Verified by: `PRAGMA table_info(signal_queue)` returns 6 columns, no `data BLOB`.
- `entry_ids` stored as JSON TEXT, parsed via `serde_json`.
- `drain_signals` uses `WHERE signal_type = ?` instead of full scan + deserialize.

### Wave 3: Server-Crate Tables

**AC-09**: AGENT_REGISTRY table has SQL columns with `capabilities`/`allowed_topics`/`allowed_categories` as JSON arrays.
- Verified by: `PRAGMA table_info(agent_registry)` returns 8 columns, no `data BLOB`.
- JSON fields parsed via `serde_json`.
- `resolve_or_enroll` and `enroll_agent` use SQL columns, not bincode.

**AC-10**: AUDIT_LOG table has SQL columns with `target_ids` as JSON array.
- Verified by: `PRAGMA table_info(audit_log)` returns 8 columns, no `data BLOB`.
- Indexes: `idx_audit_log_agent_id`, `idx_audit_log_timestamp`.
- `write_count_since` uses `WHERE agent_id = ? AND timestamp >= ? AND operation IN (...)` instead of full scan.

### Wave 4: Compat Layer Removal + Cleanup

**AC-13**: `handles.rs` and `dispatch.rs` removed.
- Verified by: Files do not exist in `crates/unimatrix-store/src/`.
- No compilation errors referencing removed types.
- `tables.rs` removed entirely.

**AC-15**: No bincode serialize/deserialize for any normalized table (OBSERVATION_METRICS excluded).
- Verified by: `grep -r 'bincode' crates/` returns hits only in:
  - `observation_metrics` paths
  - `migration_compat` module (gated/dead code)
  - `Cargo.toml` dependency declaration
- Runtime paths for all 7 normalized tables use SQL columns, not bincode.

### Wave 5: Verification

**AC-14**: Schema version is 6; migration from v5 databases works correctly.
- Verified by: `SELECT value FROM counters WHERE name = 'schema_version'` returns 6.
- Integration test: create v5 database with known data, open with new code, verify all data accessible via Store API.
- Round-trip test: every field of every record type survives migration with exact values (SR-01).
- Historical schema test: entries from v0, v1, v2, v3, v5 migrate correctly with proper defaults.

**AC-16**: `cargo build` succeeds, `cargo test --workspace` passes.
- Verified by: CI gate — workspace compilation and all tests pass.
- No regressions in existing test count (~1025 unit + ~174 integration).

**AC-17**: All 12 MCP tools produce identical results (behavioral parity).
- Verified by: Integration tests that exercise each MCP tool with known data and assert identical outputs.
- Specific coverage: `context_search`, `context_lookup`, `context_get`, `context_store`, `context_correct`, `context_deprecate`, `context_status`, `context_briefing`, `context_quarantine`, `context_enroll`, `context_retrospective`, `context_search` re-ranking.

**AC-18**: Future EntryRecord field additions use `ALTER TABLE ADD COLUMN`, not scan-and-rewrite.
- Verified by: Documentation in migration.rs explaining the new field addition pattern.
- No bincode positional encoding constraints remain for the entries table.
- New fields added via `ALTER TABLE entries ADD COLUMN new_field TYPE DEFAULT value` — instant, zero-downtime.

---

## 11. Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | nxs-007 must be merged before implementation begins | SCOPE prerequisite |
| C-02 | Store public API unchanged — `EntryRecord`, `Store` method signatures identical | SCOPE non-goal |
| C-03 | No behavioral changes — all 12 MCP tools return identical results | AC-17 |
| C-04 | Test infrastructure is cumulative — extend existing fixtures | CLAUDE.md rule |
| C-05 | One-way migration — v5→v6 creates new tables before dropping old ones | SR-01 mitigation |
| C-06 | Migration code written before bincode removal | SR-01 ordering |
| C-07 | Named parameters (`named_params!{}`) for all 24-column entries SQL | SR-02 mitigation, AD-05 |
| C-08 | `PRAGMA foreign_keys = ON` enabled in Store::open | AD-06 |
| C-09 | entry_tags has ON DELETE CASCADE | SR-08 mitigation |
| C-10 | `load_tags_for_entries` helper used in every EntryRecord construction path | SR-08 mitigation |
| C-11 | Each wave includes both store and server crate changes for the tables it normalizes | SR-05 mitigation |
| C-12 | Compilation gate after each wave: `cargo build --workspace` | SR-05 mitigation |
| C-13 | Round-trip integration test with all 24 fields set to distinct non-default values | SR-02 mitigation |
| C-14 | Tag query AND semantics preserved exactly | SR-03, Section 6.1 |
| C-15 | Empty QueryFilter defaults to Active status | SR-03, Section 6.1 |
| C-16 | Database file backup before v5→v6 migration | SR-01 mitigation |
| C-17 | `drain_signals` uses `WHERE signal_type = ?` SQL filter | AC-08 optimization |
| C-18 | INJECTION_LOG, SESSIONS, ENTRIES have indexed columns for ASS-016 JOINs | SR-06 mitigation |

---

## 12. Risk Mitigations Summary

| Risk | Severity | Mitigation | Acceptance Criteria |
|------|----------|-----------|---------------------|
| SR-01: Migration Data Fidelity | HIGH | Backup before migration; create-new-then-swap; deserializers preserved in migration_compat; round-trip tests for all historical schema versions | AC-14 |
| SR-02: 24-Column Bind Parameters | HIGH | Named parameters mandatory (AD-05); round-trip test with all 24 fields distinct | AC-01, C-07, C-13 |
| SR-03: SQL Query Semantic Equivalence | HIGH | Document current semantics (Section 6.1); before/after integration tests; tag AND preserved | AC-11, AC-17 |
| SR-04: Compat Layer Open Questions | MEDIUM | All 3 questions resolved in AD-01/02/03; Waves 1-3 bypass compat (AD-04) | AC-13 |
| SR-05: Cross-Crate Coupling | MEDIUM | Each wave includes both crates; workspace compilation gate | C-11, C-12 |
| SR-06: JSON Array Constraints | MEDIUM | Critical JOIN path (injection_log.session_id, entries.id) fully indexed; JSON only for non-queried Vec fields | C-18, AC-07 |
| SR-07: Enum-to-Integer Stability | MEDIUM | Migration uses full deserialization, not raw bytes; unit tests for each enum's discriminant stability | AD-01 |
| SR-08: entry_tags Consistency | LOW | FK CASCADE; `load_tags_for_entries` helper; integration tests | C-09, C-10, AC-02 |

---

## 13. Wave Execution Plan

| Wave | Tables Affected | Files Changed (Store) | Files Changed (Server) | Gate |
|------|----------------|----------------------|----------------------|------|
| 1 | entries, entry_tags, 5 index tables | db.rs, write.rs, read.rs, migration.rs, schema.rs | store_ops.rs, store_correct.rs, status.rs, contradiction.rs | `cargo build --workspace` + `cargo test --workspace` |
| 2 | co_access, sessions, injection_log, signal_queue | db.rs, write_ext.rs, sessions.rs, injection_log.rs, signal.rs, migration.rs | — | `cargo build --workspace` + `cargo test --workspace` |
| 3 | agent_registry, audit_log | migration.rs | registry.rs, audit.rs | `cargo build --workspace` + `cargo test --workspace` |
| 4 | — (cleanup) | Remove handles.rs, dispatch.rs, tables.rs; simplify txn.rs, lib.rs | Remove compat imports | `cargo build --workspace` + `cargo test --workspace` |
| 5 | — (verification) | — | — | Full AC-01 through AC-18 verification |

---

## 14. Test Strategy

### 14.1 Migration Tests (SR-01)

- Round-trip test: create v5 database, insert entries at each historical schema version, migrate, read back via column path, assert field-by-field equality
- Empty database migration: v5 with no data → v6 succeeds
- Large batch: 200 entries with diverse field values → all survive migration
- Backup verification: `.db.v5-backup` file exists after migration

### 14.2 Query Parity Tests (SR-03)

- Each `query_by_*` method: topic, category, tags, time_range, status
- Combined `query()` with all filter combinations: single filter, two filters, all filters, no filters
- Tag AND semantics: entry with tags [A, B], query for [A, B] matches, query for [A, C] does not
- Empty tags: `filter.tags = Some(vec![])` returns results (tag filter skipped)
- Default status: empty filter returns only Active entries

### 14.3 Write Path Tests (SR-02)

- Insert with all 24 fields set to distinct non-default values → read back → assert equality
- Update: change every field → read back → assert equality
- Delete: verify entry_tags CASCADE, vector_map cleanup, counter adjustment
- Update status: verify counter adjustment, updated_at change

### 14.4 Operational Table Tests

- CO_ACCESS: insert, query partners, stats with staleness filter, top pairs
- SESSIONS: insert, update, scan by feature, scan with status filter, GC (timed out + delete + cascade)
- INJECTION_LOG: batch insert, scan by session
- SIGNAL_QUEUE: insert, drain by type, queue length, cap enforcement
- AGENT_REGISTRY: bootstrap, resolve_or_enroll, enroll_agent, capability checks
- AUDIT_LOG: log_event, write_in_txn, write_count_since, monotonic IDs

### 14.5 MCP Tool Parity Tests (AC-17)

Integration tests exercising each of the 12 MCP tools with known data, asserting identical JSON output structure and values.
