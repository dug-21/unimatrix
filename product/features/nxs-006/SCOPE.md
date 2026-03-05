# nxs-006: redb Removal & SQLite Schema Normalization

## Problem Statement

After nxs-005 completes, Unimatrix has two storage backends: redb (default) and SQLite (behind `backend-sqlite` feature flag). The redb code is dead weight — ~6,200 lines of implementation, 17 `TableDefinition` constants, feature flag machinery, and a one-time migration tool that will never run again. More importantly, the SQLite backend created by nxs-005 carries forward redb's architectural limitations: entries stored as opaque bincode blobs with 5 application-managed index tables. This defeats the primary benefit of migrating to SQLite — the ability to use SQL indexes, JOINs, and column-level queries.

The 5 index tables (TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX) exist because redb has no secondary index support. Each entry write must manually maintain all 5 in the same transaction (~300 lines of synchronization code in write.rs). In SQLite, these become `CREATE INDEX` statements — zero application code. But `CREATE INDEX` requires actual SQL columns, not bincode blobs. This means completing the index elimination requires decomposing EntryRecord into SQL columns.

## Goals

1. Remove redb as a dependency and delete all redb-specific code
2. Remove the `backend-sqlite` feature flag — SQLite becomes the sole, unconditional backend
3. Flatten the `sqlite/` module into the main store crate (no more submodule indirection)
4. Decompose ENTRIES bincode blob into SQL columns, enabling native SQL indexes
5. Replace 5 application-managed index tables with `CREATE INDEX` statements
6. Decompose operational tables (CO_ACCESS, SESSIONS, INJECTION_LOG, SIGNAL_QUEUE) into SQL columns
7. Add `session_id` column to INJECTION_LOG for efficient cascade deletes and session joins
8. Remove the redb→SQLite migration tooling (`migrate_redb_to_sqlite`)
9. Preserve all observable behavior — zero functional change across 10 MCP tools
10. Migrate existing SQLite databases from nxs-005 blob schema to normalized schema

## Non-Goals

- **Do not change the public Store API surface** — all public method signatures remain identical. Internal implementation changes only.
- **Do not change EntryRecord, NewEntry, or QueryFilter structs** — these are the Rust-side data model. Only how they map to SQL changes.
- **Do not change code outside `crates/unimatrix-store/`** — the EntryStore trait boundary (unimatrix-core) holds completely.
- **Do not replace HNSW with sqlite-vec** — HNSW stays in-memory with VECTOR_MAP bridge table. This decision was made in nxs-005 and remains valid.
- **Do not normalize AUDIT_LOG or OBSERVATION_METRICS** — these are append-only/key-lookup tables where bincode blobs are appropriate.
- **Do not change AGENT_REGISTRY serialization** — bincode is fine for point-lookup-only tables.
- **Do not add new MCP tools or change tool signatures** — this is a storage-internal refactor.
- **Do not implement multi-table JOINs for retrospective analytics** — that's a future feature that benefits from the schema normalization but is out of scope here.

## Background

### Prior Work

- **nxs-005** (in implementation): SQLite backend behind feature flag. Preserves redb's blob+index-table architecture in SQLite. Explicitly deferred: redb removal, index elimination, injection_log session_id, schema normalization.
- **ASS-016 storage-assessment.md**: Identified 7 friction points with redb, recommended SQLite as strategic target. Section 2.1 quantified the index maintenance cost (~300 lines of write.rs synchronization).
- **ASS-016 retrospective-data-architecture.md**: Identified that multi-table JOINs (entry effectiveness scoring) require SQL columns, not blobs.

### Why Full Column Decomposition (Not Hybrid)

There are two approaches to enabling `CREATE INDEX`:

**Option A — Hybrid columns + blob**: Add SQL columns for indexed fields (topic, category, status, created_at) alongside the existing bincode blob. CREATE INDEX on the new columns. Drop the 5 index tables.
- Pro: Minimal change to read paths
- Con: Data duplication (indexed fields stored twice — in columns AND in blob)
- Con: Must keep columns in sync with blob on every write
- Con: Non-indexed fields still inaccessible to SQL queries and JOINs

**Option B — Full decomposition**: All EntryRecord fields become SQL columns. No blob.
- Pro: Clean relational schema, zero data duplication
- Pro: All fields queryable via SQL, enables future analytics JOINs
- Pro: Eliminates bincode serialization overhead on every read/write
- Pro: Tags become a proper junction table with foreign keys
- Con: Larger initial change (more write.rs / read.rs rewrite)
- Con: Must handle Option<u64> mapping carefully (NULL vs 0)

**Recommendation: Option B.** The entire motivation for the redb→SQLite migration was to leverage SQL's strengths. Keeping bincode blobs in SQLite is an anti-pattern that preserves the friction we set out to eliminate. EntryRecord has 24 flat fields (no nested structures) — they map cleanly to SQL columns. The rewrite of read.rs/write.rs is one-time work that permanently simplifies the codebase.

### Codebase Analysis

**Files to delete entirely (redb implementation):**

| File | Lines | Purpose |
|------|-------|---------|
| `src/db.rs` | 532 | redb Store struct, open, compact |
| `src/read.rs` | 924 | redb read operations |
| `src/write.rs` | 1,939 | redb write operations + index sync |
| `src/counter.rs` | 56 | redb counter helpers |
| `src/query.rs` | 318 | redb multi-filter query logic |
| `src/migration.rs` | 1,421 | redb schema migration chain (v0→v5) |
| `src/sessions.rs` | 674 | redb session lifecycle |
| `src/injection_log.rs` | 253 | redb injection log |
| `src/signal.rs` | 142 | redb signal queue |
| **Total** | **6,259** | |

**Files to modify:**

| File | Change |
|------|--------|
| `src/schema.rs` | Remove 17 `TableDefinition` / `MultimapTableDefinition` constants, remove `use redb::*` import. Keep all Rust structs and helpers. |
| `src/lib.rs` | Remove cfg gates, flatten sqlite module re-exports |
| `src/error.rs` | Remove cfg-gated redb error variant, make SQLite variant unconditional |
| `src/test_helpers.rs` | Remove backend-selection logic |
| `Cargo.toml` | Remove `redb` dependency, make `rusqlite` unconditional, remove `backend-sqlite` feature |

**Files from nxs-005 to delete:**

| File | Purpose |
|------|---------|
| `src/migrate_redb_to_sqlite.rs` | One-time redb→SQLite export tool |

**Server crate changes:**

| File | Change |
|------|--------|
| `Cargo.toml` | Remove `redb` dependency |
| 7+ source files | Remove `use redb::*` imports, use re-exported types from unimatrix-store |

**Workspace changes:**

| File | Change |
|------|--------|
| `Cargo.toml` | Remove `redb = "3.1"` from workspace dependencies |

### Target SQLite Schema

**ENTRIES table (normalized):**
```sql
CREATE TABLE entries (
    id              INTEGER PRIMARY KEY,
    title           TEXT NOT NULL DEFAULT '',
    content         TEXT NOT NULL DEFAULT '',
    topic           TEXT NOT NULL DEFAULT '',
    category        TEXT NOT NULL DEFAULT '',
    source          TEXT NOT NULL DEFAULT '',
    status          INTEGER NOT NULL DEFAULT 0,
    confidence      REAL NOT NULL DEFAULT 0.0,
    created_at      INTEGER NOT NULL DEFAULT 0,
    updated_at      INTEGER NOT NULL DEFAULT 0,
    last_accessed_at INTEGER NOT NULL DEFAULT 0,
    access_count    INTEGER NOT NULL DEFAULT 0,
    supersedes      INTEGER,           -- NULL maps to Option::None
    superseded_by   INTEGER,           -- NULL maps to Option::None
    correction_count INTEGER NOT NULL DEFAULT 0,
    embedding_dim   INTEGER NOT NULL DEFAULT 0,
    created_by      TEXT NOT NULL DEFAULT '',
    modified_by     TEXT NOT NULL DEFAULT '',
    content_hash    TEXT NOT NULL DEFAULT '',
    previous_hash   TEXT NOT NULL DEFAULT '',
    version         INTEGER NOT NULL DEFAULT 0,
    feature_cycle   TEXT NOT NULL DEFAULT '',
    trust_source    TEXT NOT NULL DEFAULT '',
    helpful_count   INTEGER NOT NULL DEFAULT 0,
    unhelpful_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_entries_topic ON entries(topic);
CREATE INDEX idx_entries_category ON entries(category);
CREATE INDEX idx_entries_status ON entries(status);
CREATE INDEX idx_entries_created_at ON entries(created_at);
CREATE INDEX idx_entries_feature_cycle ON entries(feature_cycle);
```

**ENTRY_TAGS junction table (replaces TAG_INDEX):**
```sql
CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL,
    tag      TEXT NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX idx_entry_tags_tag ON entry_tags(tag);
```

**Tables dropped (replaced by CREATE INDEX):**
- TOPIC_INDEX
- CATEGORY_INDEX
- TIME_INDEX
- STATUS_INDEX
- TAG_INDEX (replaced by entry_tags junction table)

**INJECTION_LOG (normalized + session_id column):**
```sql
CREATE TABLE injection_log (
    log_id     INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL DEFAULT '',
    entry_id   INTEGER NOT NULL DEFAULT 0,
    confidence REAL NOT NULL DEFAULT 0.0,
    timestamp  INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_injection_log_session ON injection_log(session_id);
```

**CO_ACCESS (normalized):**
```sql
CREATE TABLE co_access (
    entry_id_a   INTEGER NOT NULL,
    entry_id_b   INTEGER NOT NULL,
    count        INTEGER NOT NULL DEFAULT 0,
    last_updated INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (entry_id_a, entry_id_b),
    CHECK (entry_id_a < entry_id_b)
);
```

**SESSIONS (normalized):**
```sql
CREATE TABLE sessions (
    session_id        TEXT PRIMARY KEY,
    feature_cycle     TEXT NOT NULL DEFAULT '',
    agent_role        TEXT NOT NULL DEFAULT '',
    started_at        INTEGER NOT NULL DEFAULT 0,
    ended_at          INTEGER,
    status            TEXT NOT NULL DEFAULT 'Active',
    compaction_count  INTEGER NOT NULL DEFAULT 0,
    outcome           TEXT NOT NULL DEFAULT '',
    total_injections  INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sessions_feature ON sessions(feature_cycle);
CREATE INDEX idx_sessions_status ON sessions(status);
```

**SIGNAL_QUEUE (normalized):**
```sql
CREATE TABLE signal_queue (
    signal_id    INTEGER PRIMARY KEY,
    session_id   TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL DEFAULT 0,
    entry_ids    BLOB NOT NULL,          -- bincode Vec<u64>, small and rarely queried by field
    signal_type  TEXT NOT NULL DEFAULT '',
    signal_source TEXT NOT NULL DEFAULT ''
);
```

**Tables unchanged (remain as-is from nxs-005):**
- COUNTERS (`TEXT PRIMARY KEY, INTEGER`)
- VECTOR_MAP (`INTEGER PRIMARY KEY, INTEGER`)
- AGENT_REGISTRY (`TEXT PRIMARY KEY, BLOB`)
- AUDIT_LOG (`INTEGER PRIMARY KEY, BLOB`)
- FEATURE_ENTRIES (multimap: `TEXT, INTEGER`)
- OUTCOME_INDEX (multimap: `TEXT, INTEGER`)
- OBSERVATION_METRICS (`TEXT PRIMARY KEY, BLOB`)

## Proposed Approach

### Phase 1: redb Removal & Module Flattening

Delete all redb implementation files. Remove the `backend-sqlite` feature flag. Make `rusqlite` an unconditional dependency. Flatten the `sqlite/` submodule into the main `src/` directory. Remove `migrate_redb_to_sqlite`. Clean up workspace and server crate dependencies.

This is mechanical cleanup with zero behavioral change. All existing SQLite tests continue to pass.

### Phase 2: ENTRIES Column Decomposition

Rewrite the ENTRIES table from a bincode blob column to 24 individual SQL columns. Replace `serialize_entry`/`deserialize_entry` in the write and read paths with SQL column mapping. Add the `entry_tags` junction table. Create SQL indexes. Drop the 5 index tables. This eliminates ~300 lines of index synchronization code from write operations.

Add a schema migration (v5→v6) that:
1. Creates the new `entries` table with columns
2. Reads each row from the old `entries` table, deserializes the blob, inserts into the new table
3. Populates `entry_tags` from each entry's tags
4. Drops the 5 index tables
5. Drops the old `entries` table, renames the new one

### Phase 3: Operational Table Normalization

Decompose CO_ACCESS, SESSIONS, INJECTION_LOG, and SIGNAL_QUEUE from bincode blobs to SQL columns. Add `session_id` column to INJECTION_LOG with index. Add indexes to SESSIONS.

This is a continuation of the Phase 2 pattern — blob→columns migration for each table.

### Phase 4: Validation

Run all existing store tests (should be ~234+ from nxs-005). Run full infra-001 integration harness. Verify zero behavioral change.

## Acceptance Criteria

- AC-01: `redb` does not appear in any Cargo.toml in the workspace
- AC-02: No `cfg(feature = "backend-sqlite")` gates remain in any source file
- AC-03: No `sqlite/` submodule — all store implementation files are in `src/` directly
- AC-04: ENTRIES table has 24 SQL columns (no bincode blob column)
- AC-05: 5 former index tables (TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX) do not exist in the database
- AC-06: `entry_tags` junction table exists with `(entry_id, tag)` composite primary key
- AC-07: SQL indexes exist on entries(topic), entries(category), entries(status), entries(created_at), entry_tags(tag)
- AC-08: INJECTION_LOG has a `session_id` TEXT column with index
- AC-09: CO_ACCESS, SESSIONS, SIGNAL_QUEUE have SQL columns (no bincode blob for primary fields)
- AC-10: Schema migration v5→v6 correctly migrates existing nxs-005 databases (blob→columns with data preservation)
- AC-11: `serialize_entry`/`deserialize_entry` are removed or deprecated (no longer used in store operations)
- AC-12: All existing store unit tests pass (count should match nxs-005 final count)
- AC-13: All 10 MCP tools return identical results (infra-001 integration harness passes)
- AC-14: No code changes outside `crates/unimatrix-store/` and dependency cleanup in `crates/unimatrix-server/Cargo.toml` and workspace `Cargo.toml`
- AC-15: `Option<u64>` fields (supersedes, superseded_by) map to SQL NULL correctly (not 0)

## Constraints

- **Prerequisite**: nxs-005 must be complete (SQLite backend passing all tests) before nxs-006 implementation begins
- **Migration**: Schema migration must handle existing nxs-005 databases. Users should not lose data.
- **No new public API**: Store's public method signatures must not change.
- **EntryRecord struct unchanged**: The Rust struct remains identical — only the storage mapping changes.
- **Test infrastructure is cumulative**: Extend existing test helpers, don't create isolated scaffolding.
- **Tags are multi-valued**: TAG_INDEX cannot become a simple CREATE INDEX. It must be a junction table (entry_tags).
- **signal_queue.entry_ids stays as blob**: Vec<u64> is a variable-length array with no SQL-native representation. Bincode is appropriate here.
- **Transaction boundaries preserved**: All write operations that were atomic under redb/nxs-005 remain atomic.

## Open Questions

1. **Should `serialize_entry`/`deserialize_entry` be fully removed or kept for backward compatibility?** They're currently pub exports from unimatrix-store. If external code (tests, migration tools) uses them, removal is a breaking change. Leaning toward removal since no code outside the store crate should be serializing entries.

2. **Schema version: v6 or reset?** nxs-005 creates the SQLite schema at some version (likely v5 to match redb). nxs-006 migration bumps to v6. Alternatively, since this is a major normalization, we could reset to v1-sqlite. Leaning toward v6 (continuous version chain is simpler).

3. **FEATURE_ENTRIES and OUTCOME_INDEX**: These are multimap tables with `(text, integer)` keys. They could become junction tables with foreign keys like entry_tags. Should we normalize them too, or leave them as-is? Leaning toward leaving them (they work fine as simple tables and aren't causing friction).

## Estimated Impact

- **Lines deleted**: ~6,500 (redb implementation + migration tooling + TableDefinition constants + cfg gates)
- **Lines rewritten**: ~2,500 (sqlite read.rs/write.rs rewritten for column access instead of blob serde)
- **Net line change**: Likely -3,000 to -4,000 (index sync elimination, serde removal)
- **Tables dropped**: 5 (index tables) + old blob-based entries table
- **Tables added**: 1 (entry_tags)
- **Indexes added**: ~8 (replacing 5 application-managed tables + new operational indexes)

## Tracking

GitHub Issue will be created during Session 1 synthesis phase.
