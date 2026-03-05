# nxs-008: Schema Normalization

## Problem Statement

After nxs-007 completes the redb removal and SQLite becomes the sole backend, the storage layer retains redb-era data patterns that prevent SQLite from being used effectively. The ENTRIES table stores a 24-field `EntryRecord` as a bincode blob — opaque to SQL. Five separate index tables (TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX) are manually synchronized on every write, reproducing what `CREATE INDEX` provides natively. Four operational tables (CO_ACCESS, SESSIONS, INJECTION_LOG, SIGNAL_QUEUE) and two server-owned tables (AGENT_REGISTRY, AUDIT_LOG) also store structured records as bincode blobs despite being SQL-native tables created post-SQLite.

The result: every query is a multi-step process of HashSet intersection in Rust, every write is 7+ SQL statements with manual index synchronization, and the multi-table JOINs needed for retrospective analytics (entry effectiveness = `INJECTION_LOG JOIN SESSIONS JOIN ENTRIES`) are impossible because the data is locked inside opaque blobs.

nxs-008 normalizes the schema: decompose blobs into SQL columns, eliminate manual index tables, and replace client-side filtering with SQL WHERE clauses. The storage API (`Store` methods, `EntryRecord` struct) remains unchanged — this is an internal restructuring.

## Goals

1. Decompose the ENTRIES bincode blob into 24 SQL columns on the `entries` table
2. Create an `entry_tags` junction table for the many-to-many tag relationship (replaces TAG_INDEX)
3. Eliminate 5 manual index tables (TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX) — replaced by SQL `CREATE INDEX` statements on entries columns and the entry_tags junction table
4. Decompose CO_ACCESS bincode blob into SQL columns (2 data fields + composite key)
5. Decompose SESSIONS bincode blob into SQL columns (9 fields)
6. Decompose INJECTION_LOG bincode blob into SQL columns (5 fields), adding indexed `session_id` column for efficient session cascade GC
7. Decompose SIGNAL_QUEUE bincode blob into SQL columns (6 fields), with `entry_ids` stored as JSON array
8. Decompose AGENT_REGISTRY bincode blob into SQL columns (8 fields), with `capabilities`, `allowed_topics`, `allowed_categories` stored as JSON arrays
9. Decompose AUDIT_LOG bincode blob into SQL columns (8 fields), with `target_ids` stored as JSON array
10. Replace client-side HashSet intersection query logic with SQL WHERE clauses and JOINs
11. Eliminate N+1 entry fetch pattern — single query returns all matching entries
12. Remove bincode serialization/deserialization infrastructure for all normalized tables
13. Remove the redb-pattern compat layer (tables.rs, handles.rs, dispatch.rs) that wraps SQLite in redb-shaped typed table abstractions
14. Migrate existing data from schema v5 (bincode blobs) to schema v6 (SQL columns)

## Non-Goals

- **No server decoupling** — the server's direct table access is intentional and accepted (ADR #352). Server code updates mechanically as part of schema changes.
- **No OBSERVATION_METRICS normalization** — dynamic-shape MetricVector stays as bincode blob (ADR #354, GH #103)
- **No HNSW/vector changes** — VECTOR_MAP bridge table and in-memory HNSW index are untouched
- **No Store public API changes** — `EntryRecord`, `Store::insert()`, `Store::get()`, `Store::query()` signatures remain identical. Callers are unaffected.
- **No new MCP tools or behavioral changes** — all 12 MCP tools produce identical results
- **No serialization format change for OBSERVATION_METRICS** — stays bincode
- **No new tables** beyond entry_tags junction (which replaces TAG_INDEX)

## Background Research

### Current Schema (v5) — Tables with Bincode Blobs

| Table | Record Type | Fields | Crate | Blob Complexity |
|-------|-------------|--------|-------|-----------------|
| ENTRIES | EntryRecord | 24 fields (14 non-Option, 10 serde(default)) | store/schema.rs | Heavy — core entity |
| CO_ACCESS | CoAccessRecord | 2 fields (count, last_updated) | store/schema.rs | Trivial |
| SESSIONS | SessionRecord | 9 fields (5 non-Option, 4 Option) | store/sessions.rs | Medium |
| INJECTION_LOG | InjectionLogRecord | 5 fields (all non-Option) | store/injection_log.rs | Light |
| SIGNAL_QUEUE | SignalRecord | 6 fields (all non-Option, entry_ids is Vec) | store/signal.rs | Medium — Vec field |
| AGENT_REGISTRY | AgentRecord | 8 fields (6 non-Option, 2 Option Vec) | server/infra/registry.rs | Medium — Vec fields |
| AUDIT_LOG | AuditEvent | 8 fields (all non-Option, target_ids is Vec) | server/infra/audit.rs | Medium — Vec field |

### Tables Already Normalized (No Change)

| Table | Schema | Notes |
|-------|--------|-------|
| VECTOR_MAP | `(entry_id INTEGER PK, hnsw_data_id INTEGER)` | Simple KV |
| COUNTERS | `(name TEXT PK, value INTEGER)` | Simple KV |
| FEATURE_ENTRIES | `(feature_id TEXT, entry_id INTEGER, PK)` | Simple composite |
| OUTCOME_INDEX | `(feature_cycle TEXT, entry_id INTEGER, PK)` | Simple composite |
| OBSERVATION_METRICS | `(feature_cycle TEXT PK, data BLOB)` | Excluded (ADR #354) |

### Tables Eliminated (Become SQL Indexes)

| Current Table | Replacement | Lines of Index Sync Code Eliminated |
|---------------|-------------|-------------------------------------|
| TOPIC_INDEX | `CREATE INDEX idx_entries_topic ON entries(topic)` | ~14 (insert + update paths) |
| CATEGORY_INDEX | `CREATE INDEX idx_entries_category ON entries(category)` | ~14 |
| TAG_INDEX | `entry_tags` junction table + `CREATE INDEX idx_entry_tags_tag ON entry_tags(tag)` | ~24 (multimap loop) |
| TIME_INDEX | `CREATE INDEX idx_entries_created_at ON entries(created_at)` | ~14 |
| STATUS_INDEX | `CREATE INDEX idx_entries_status ON entries(status)` | ~14 |

### EntryRecord Fields (24 total)

| Field | SQL Type | Nullable | Notes |
|-------|----------|----------|-------|
| id | INTEGER PRIMARY KEY | No | |
| title | TEXT | No | |
| content | TEXT | No | |
| topic | TEXT | No | Indexed |
| category | TEXT | No | Indexed |
| source | TEXT | No | |
| status | INTEGER | No | Indexed, maps to Status enum (0-3) |
| confidence | REAL | No | Default 0.0 |
| created_at | INTEGER | No | Indexed, Unix epoch |
| updated_at | INTEGER | No | |
| last_accessed_at | INTEGER | No | Default 0 |
| access_count | INTEGER | No | Default 0 |
| supersedes | INTEGER | Yes | FK-like to entries(id) |
| superseded_by | INTEGER | Yes | FK-like to entries(id) |
| correction_count | INTEGER | No | Default 0 |
| embedding_dim | INTEGER | No | Default 0 |
| created_by | TEXT | No | Default '' |
| modified_by | TEXT | No | Default '' |
| content_hash | TEXT | No | Default '' |
| previous_hash | TEXT | No | Default '' |
| version | INTEGER | No | Default 0 |
| feature_cycle | TEXT | No | Default '' |
| trust_source | TEXT | No | Default '' |
| helpful_count | INTEGER | No | Default 0 |
| unhelpful_count | INTEGER | No | Default 0 |

Tags are stored separately in `entry_tags(entry_id INTEGER, tag TEXT, PRIMARY KEY(entry_id, tag))`.

### Vec Fields Strategy

Fields with `Vec<T>` types need special handling since SQL has no native array type:

| Record | Field | Query by Element? | Strategy | Rationale |
|--------|-------|-------------------|----------|-----------|
| EntryRecord | tags | Yes (filter by tag) | Junction table (`entry_tags`) | Queried in WHERE clauses, replaces TAG_INDEX |
| SignalRecord | entry_ids | No (bulk read only) | JSON array column | Only read as complete list during signal drain |
| AgentRecord | capabilities | No (checked in-memory) | JSON array column | Small cardinality (5 enum variants), loaded as complete record |
| AgentRecord | allowed_topics | No (checked in-memory) | JSON array column | Loaded as complete record, nullable |
| AgentRecord | allowed_categories | No (checked in-memory) | JSON array column | Loaded as complete record, nullable |
| AuditEvent | target_ids | No (write-only, append log) | JSON array column | Append-only audit trail, rarely queried |

### Write Path Impact

**Current insert path** (write.rs, 107 lines):
1. Serialize EntryRecord → bincode blob
2. INSERT into entries table
3. INSERT into TOPIC_INDEX
4. INSERT into CATEGORY_INDEX
5. Loop: INSERT into TAG_INDEX per tag
6. INSERT into TIME_INDEX
7. INSERT into STATUS_INDEX
8. Update counter
**Total: 7+ SQL statements per insert**

**After normalization**:
1. INSERT INTO entries (24 columns) VALUES (...)
2. Loop: INSERT INTO entry_tags (entry_id, tag) per tag
3. Update counter
**Total: 2+ SQL statements per insert** (index maintenance is automatic)

**Current update path** (write.rs, 121 lines):
- Diff-based: check each dimension for changes, DELETE old + INSERT new index rows
- 2-6+ SQL statements per update

**After normalization**:
1. UPDATE entries SET col1=?, col2=?, ... WHERE id=?
2. DELETE FROM entry_tags WHERE entry_id=?
3. Loop: INSERT INTO entry_tags per tag
**Total: 2+ SQL statements per update** (simpler, no diff logic needed for indexes)

### Read Path Impact

**Current query path** (read.rs, ~200 lines):
1. For each filter dimension: SELECT from index table → build HashSet in Rust
2. Intersect all HashSets in Rust
3. For each result ID: SELECT blob FROM entries WHERE id=? (N+1 pattern)
4. Deserialize each bincode blob → EntryRecord

**After normalization**:
1. Single SQL: `SELECT * FROM entries WHERE topic=? AND category=? AND status=? AND id IN (SELECT entry_id FROM entry_tags WHERE tag IN (?,?))`
2. Construct EntryRecord from row columns
3. Separate query for tags: `SELECT tag FROM entry_tags WHERE entry_id IN (?,?,?)`

**~200 lines of HashSet intersection + N+1 fetch → ~40 lines of SQL query building**

### Compat Layer Removal

nxs-007 created these files to relocate redb-pattern abstractions:

| File | Lines | Purpose | nxs-008 Action |
|------|-------|---------|----------------|
| tables.rs | ~60 | Table name constants, guard types, counter helpers | Remove table constants (direct SQL), keep counter helpers if needed |
| handles.rs | ~200 | Typed table handle wrappers (TableU64Blob, TableStrU64, etc.) | Remove entirely — direct SQL replaces typed handles |
| dispatch.rs | ~150 | TableSpec/MultimapSpec traits for open_table dispatch | Remove entirely — no more open_table pattern |
| txn.rs | ~90 | SqliteReadTransaction, SqliteWriteTransaction wrappers | Simplify — may keep thin connection wrappers |

### Server Code Updates

Only `store_ops.rs` imports table constants directly. Other server files use Store API methods. Server changes are mechanical:

| Server File | Current Pattern | After Normalization |
|-------------|-----------------|---------------------|
| services/store_ops.rs | Imports table constants, calls serialize_entry, writes all index tables | SQL INSERT with columns, no index writes |
| services/store_correct.rs | Manual deprecate + create in one transaction across 8 tables | SQL UPDATE + INSERT, no index table manipulation |
| infra/audit.rs | serialize AuditEvent → bincode, INSERT blob | SQL INSERT with columns |
| infra/registry.rs | serialize AgentRecord → bincode, INSERT blob | SQL INSERT with columns, JSON for Vec fields |
| services/status.rs | Scans index tables for counts | SQL COUNT queries on entries table directly |
| infra/contradiction.rs | Scans STATUS_INDEX + deserializes entry blobs | SQL SELECT with WHERE status=? |

### Migration Strategy (v5 → v6)

1. Create new table schemas (entries_new with columns, entry_tags, etc.)
2. For each table with blobs: SELECT all rows → deserialize bincode → INSERT into new schema
3. Drop old tables
4. Rename new tables (entries_new → entries)
5. Create SQL indexes
6. Update schema_version counter to 6

At current scale (~50-200 entries, ~400 operational records), migration completes in milliseconds.

**One-way door**: No rollback to bincode without backup. The migration code handles this by creating new tables first, only dropping old tables after successful migration.

### Future Schema Evolution

With SQL columns, adding new fields to EntryRecord changes from scan-and-rewrite (current) to `ALTER TABLE entries ADD COLUMN new_field TYPE DEFAULT value` — instant, zero-downtime. This eliminates the bincode positional encoding constraint documented in schema.rs lines 234-247.

## Proposed Approach

### Wave 1: ENTRIES Decomposition + Index Elimination

The core transformation. Decompose the entries bincode blob into 24 SQL columns. Create entry_tags junction table. Replace 5 index tables with SQL CREATE INDEX. Rewrite write.rs insert/update paths to use SQL columns. Rewrite read.rs query path to use SQL WHERE clauses instead of HashSet intersection.

**Files changed**: db.rs, write.rs, read.rs, migration.rs, schema.rs, lib.rs
**Files removed/gutted**: tables.rs (index table constants), handles.rs (typed handles for index tables)

### Wave 2: Store-Crate Operational Tables

Decompose CO_ACCESS, SESSIONS, INJECTION_LOG, SIGNAL_QUEUE from bincode blobs to SQL columns. Each table's implementation file (sessions.rs, injection_log.rs, signal.rs, write_ext.rs) is updated to read/write SQL columns instead of serializing/deserializing bincode.

**Files changed**: db.rs (schema), sessions.rs, injection_log.rs, signal.rs, write_ext.rs, migration.rs

### Wave 3: Server-Crate Tables

Decompose AGENT_REGISTRY and AUDIT_LOG from bincode blobs to SQL columns. These record types live in the server crate, so the serialization changes are localized there.

**Files changed**: server/infra/registry.rs, server/infra/audit.rs, server/src/server.rs (if schema init touches these)

### Wave 4: Compat Layer Removal + Cleanup

Remove the redb-pattern compat abstractions (handles.rs, dispatch.rs, remnants of tables.ts). Simplify txn.rs. Remove all bincode serialize/deserialize helper functions for normalized tables. Clean up unused imports across both crates.

**Files removed**: handles.rs, dispatch.rs
**Files simplified**: tables.rs, txn.rs
**Files cleaned**: lib.rs, all files with removed bincode imports

### Wave 5: Verification

- `cargo build` succeeds
- `cargo test --workspace` passes
- All 12 MCP tools produce identical results
- No bincode serialization remains for normalized tables (OBSERVATION_METRICS excluded)
- No manual index table references remain
- Schema version is 6
- Migration from v5 database works correctly

## Acceptance Criteria

- AC-01: ENTRIES table has 24 SQL columns instead of a bincode blob
- AC-02: entry_tags junction table exists with `(entry_id INTEGER, tag TEXT, PRIMARY KEY(entry_id, tag))`
- AC-03: TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX tables eliminated
- AC-04: SQL indexes exist on entries(topic), entries(category), entries(status), entries(created_at), entry_tags(tag)
- AC-05: CO_ACCESS table has SQL columns: entry_id_a, entry_id_b, count, last_updated
- AC-06: SESSIONS table has SQL columns for all 9 SessionRecord fields
- AC-07: INJECTION_LOG table has SQL columns for all 5 fields, with indexed session_id
- AC-08: SIGNAL_QUEUE table has SQL columns with entry_ids as JSON array
- AC-09: AGENT_REGISTRY table has SQL columns with capabilities/allowed_topics/allowed_categories as JSON arrays
- AC-10: AUDIT_LOG table has SQL columns with target_ids as JSON array
- AC-11: read.rs query path uses SQL WHERE clauses, not HashSet intersection
- AC-12: N+1 entry fetch pattern eliminated — queries return entries directly
- AC-13: handles.rs and dispatch.rs removed
- AC-14: Schema version is 6; migration from v5 databases works correctly
- AC-15: No bincode serialize/deserialize for any normalized table (OBSERVATION_METRICS excluded)
- AC-16: `cargo build` succeeds, `cargo test --workspace` passes
- AC-17: All 12 MCP tools produce identical results (behavioral parity)
- AC-18: Future EntryRecord field additions use ALTER TABLE ADD COLUMN, not scan-and-rewrite

## Constraints

- **Prerequisite: nxs-007 must be complete** — redb must be fully removed and SQLite must be the sole backend
- **No behavioral changes** — subtractive/structural only. All MCP tools return identical results.
- **Store public API unchanged** — EntryRecord struct, Store methods signatures stay the same. Internal implementation changes only.
- **Test infrastructure is cumulative** — extend existing fixtures and helpers
- **One-way migration** — v5→v6 migration creates new tables before dropping old ones for safety, but there is no v6→v5 rollback path
- **Server code updates are mechanical** — per ADR #352, the server's direct table access is accepted. Updates follow from schema changes.

## Open Questions

1. **txn.rs fate** — SqliteReadTransaction/SqliteWriteTransaction wrappers may still be useful as thin connection wrappers even after compat layer removal. Keep simplified versions or remove entirely?
2. **Counter table** — COUNTERS remains as-is (simple KV, no blob). Some counter helpers in tables.rs may need to survive into a utility module. Where do they live?
3. **Enum storage** — Status, SessionLifecycleStatus, SignalType, SignalSource, Outcome, TrustLevel, Capability are stored as integers. Should we use TEXT for readability or INTEGER for compactness? Current code uses `#[repr(u8)]` which suggests INTEGER.

## Tracking

GitHub Issue: TBD
Unimatrix ADRs: #352 (server decoupling rejected), #354 (OBSERVATION_METRICS excluded)
Predecessor: nxs-007 (redb removal), ASS-016 (retrospective data architecture research)
