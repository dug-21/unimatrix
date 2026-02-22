# nxs-001: Embedded Storage Engine

## Problem Statement

Unimatrix has no persistent storage layer. The product vision defines a self-learning context engine that accumulates knowledge across feature cycles, but there is currently no Rust codebase, no database, no schema, and no way to store or retrieve entries. Without a storage engine, no downstream feature (vector index, embedding pipeline, MCP server, or learning system) can be built.

This is Milestone 1's foundational feature. Every other feature in the roadmap depends on it.

## Goals

1. Implement a redb-backed entry store with 8 named tables: ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, and COUNTERS.
2. Define the EntryRecord schema using serde + bincode serialization with `#[serde(default)]` on all non-essential fields to enable zero-migration schema evolution.
3. Provide atomic multi-table write transactions that keep all indexes consistent with the ENTRIES table.
4. Provide efficient read paths: point lookup by entry ID, range queries on time index, prefix scans on topic/category indexes, set intersection on tag index, and status filtering.
5. Implement a monotonically increasing entry ID generator via the COUNTERS table.
6. Expose a synchronous Rust API suitable for wrapping with `tokio::task::spawn_blocking` and `Arc<Database>` in downstream async code.
7. Support the VECTOR_MAP bridge table that maps entry IDs to hnsw_rs data IDs (written by the future nxs-002 vector index feature).
8. Implement database lifecycle operations: open-or-create, compaction on shutdown, and cache size configuration.

## Non-Goals

- **No vector index integration.** The VECTOR_MAP table schema is defined and created, but hnsw_rs insertion/search is nxs-002's responsibility. This feature only stores the entry_id-to-data_id mapping.
- **No embedding pipeline.** Content embedding is nxs-003. This feature stores raw content and metadata only.
- **No MCP server or tool interface.** MCP exposure is vnc-001/vnc-002. This feature provides a Rust library API only.
- **No async API.** The storage engine is synchronous (matching redb's API). Async wrapping with spawn_blocking is the responsibility of the MCP server layer (vnc-001).
- **No confidence computation.** The `confidence` field exists on EntryRecord as a cached f32, but the computation formula (Wilson score, time decay, correction penalty) is a higher-level concern for vnc-002 or crt-002.
- **No usage tracking tables.** USAGE_LOG, FEATURE_ENTRIES, and OUTCOME_INDEX are Proposal C additions (crt-001, col-001). The schema uses `#[serde(default)]` so these fields can be added later without migration.
- **No CLI.** CLI commands (init, status, export) are nan-001.
- **No project isolation or multi-project support.** Project-scoped data directories are dsn-001/dsn-002. This feature operates on a single database path.
- **No near-duplicate detection.** Dedup requires vector similarity search (nxs-002 + nxs-003).

## Background Research

### Prior Spike Research (Heavily Leveraged)

Three completed spikes provide high-confidence design decisions for this feature:

**ASS-003 (redb Storage Patterns)** -- D2 deliverable at `product/research/ass-003/research/D2-redb-storage-pattern-guide.md`:
- Confirmed single-DB-file, multi-table layout as the correct pattern.
- Validated compound tuple keys `(u64, u64)` for time/status/topic/category indexes with B-tree range scans.
- Confirmed MultimapTable for tag index (one-to-many).
- Established serde + bincode as `&[u8]` for EntryRecord (most flexible for schema evolution).
- Documented `Arc<Database>` + `spawn_blocking` as the proven async pattern (Iroh uses this in production).
- Confirmed sub-millisecond read/write latency at Unimatrix scale (1K-100K entries).
- Established compaction strategy: manual `db.compact()` on clean shutdown.
- Recommended 64-128 MB cache (reduced from 1 GiB default).

**ASS-001 (hnsw_rs Capability)** -- D1 deliverable at `product/research/ass-001/research/D1-hnsw-rs-capability-matrix.md`:
- Defined VECTOR_MAP bridge table requirement: entry_id (u64) to hnsw_rs data_id (usize).
- Confirmed hnsw_rs has no deletion API -- deprecated entries must be filtered via redb metadata during search.
- Established FilterT pattern: build sorted `Vec<usize>` from redb index, pass to hnsw_rs `search_filter`.

**ASS-007 (Interface Specification)** -- Proposal A DATABASE.md at `product/research/ass-007/proposals/a-knowledge-oracle/DATABASE.md`:
- Defined the 8-table layout (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS).
- Designed the EntryRecord schema with all fields.
- Mapped query patterns to table access patterns (context_lookup uses index scans + intersection; context_search uses VECTOR_MAP + hnsw_rs).
- Defined Status enum (Active=0, Deprecated=1, Proposed=2) and lifecycle transitions.
- Specified COUNTERS keys: "next_entry_id", "total_active", "total_deprecated".

### Existing Codebase State

No Rust codebase exists yet. This feature creates the initial Cargo workspace, crate structure, and first library crate. The workspace layout must anticipate future crates (vector index, embedding pipeline, MCP server, core traits).

### Technical Constraints Discovered

- **redb is synchronous-only.** All operations must be wrapped in `spawn_blocking` for async contexts. This feature exposes a sync API; async wrapping is deferred to downstream consumers.
- **redb has no secondary index mechanism.** All indexes are manually maintained as separate tables, updated atomically within a single write transaction.
- **redb v3.1.0** requires Rust edition 2024 and MSRV 1.89. The workspace must target this edition.
- **bincode serialization** must handle schema evolution gracefully. Using `#[serde(default)]` on all Option fields and future-added fields ensures older serialized data deserializes correctly with new fields defaulting to None/0/false.
- **VECTOR_MAP value type**: The spike research uses both `u64` and `usize` for hnsw_rs data IDs. hnsw_rs uses `usize` internally, but redb requires fixed-width types. Store as `u64` in redb, cast to `usize` at the boundary.
- **TOPIC_INDEX and CATEGORY_INDEX** use `(&str, u64)` compound keys (string prefix + entry_id), NOT hash-based keys. This preserves human-readable scan capability and avoids hash collision concerns.

## Proposed Approach

### Crate Structure

Create a `unimatrix-store` library crate within a Cargo workspace. The workspace root is at the repository root. The crate provides:

1. **Schema module** -- EntryRecord struct, Status enum, all table definitions as typed constants.
2. **Database module** -- Open/create database, cache configuration, compaction, shutdown.
3. **Write operations module** -- Insert entry (atomic multi-table write), update entry, delete indexes for an entry, update status (with index migration).
4. **Read operations module** -- Get by ID, query by topic, query by category, query by tags (intersection), query by time range, query by status, combined multi-index queries.
5. **Counter module** -- Atomic next-ID generation, counter reads for stats.
6. **Error types** -- Typed errors wrapping redb errors, serialization errors, and constraint violations.

### Table Schema

Eight tables as defined in the roadmap and spike research:

| Table | Type | Key | Value | Purpose |
|-------|------|-----|-------|---------|
| ENTRIES | Table | u64 (entry_id) | &[u8] (bincode EntryRecord) | Primary entry storage |
| TOPIC_INDEX | Table | (&str, u64) (topic, entry_id) | () | Topic prefix scan |
| CATEGORY_INDEX | Table | (&str, u64) (category, entry_id) | () | Category prefix scan |
| TAG_INDEX | MultimapTable | &str (tag) | u64 (entry_id) | Tag set intersection |
| TIME_INDEX | Table | (u64, u64) (timestamp, entry_id) | () | Temporal range queries |
| STATUS_INDEX | Table | (u8, u64) (status, entry_id) | () | Status filtering |
| VECTOR_MAP | Table | u64 (entry_id) | u64 (hnsw_data_id) | Bridge to vector index |
| COUNTERS | Table | &str (counter_name) | u64 (value) | ID generation + stats |

### EntryRecord Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRecord {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
    #[serde(default)]
    pub confidence: f32,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_accessed_at: u64,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub supersedes: Option<u64>,
    #[serde(default)]
    pub superseded_by: Option<u64>,
    #[serde(default)]
    pub correction_count: u32,
    #[serde(default)]
    pub embedding_dim: u16,
    // Future Proposal C fields will be added with #[serde(default)]
    // and deserialize to their default values from older data.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Deprecated = 1,
    Proposed = 2,
}
```

### Write Path

All writes are atomic multi-table transactions:

1. Begin write transaction.
2. Generate next entry ID from COUNTERS.
3. Serialize EntryRecord via bincode, insert into ENTRIES.
4. Insert compound key into TOPIC_INDEX, CATEGORY_INDEX, TIME_INDEX, STATUS_INDEX.
5. Insert each tag into TAG_INDEX (multimap).
6. Update COUNTERS ("total_active" or "total_deprecated").
7. Commit transaction (fsync by default -- crash safe).

Status changes (e.g., Active to Deprecated) require removing the old status index entry and inserting the new one, within the same transaction.

### Read Paths

- **By ID**: Direct ENTRIES table get. O(log n).
- **By topic**: TOPIC_INDEX range scan on `(topic, 0)..=(topic, u64::MAX)`. Returns entry IDs, batch fetch from ENTRIES.
- **By category**: Same pattern as topic on CATEGORY_INDEX.
- **By tags**: For each tag, get entry IDs from TAG_INDEX. Intersect the sets. Batch fetch from ENTRIES.
- **By time range**: TIME_INDEX range scan on `(start_ts, 0)..=(end_ts, u64::MAX)`.
- **By status**: STATUS_INDEX range scan on `(status_byte, 0)..=(status_byte, u64::MAX)`.
- **Combined via QueryFilter**: A `QueryFilter` struct with optional fields (`topic`, `category`, `tags`, `status`, `time_range`). The engine executes individual index queries for each present field, intersects result sets, and batch-fetches from ENTRIES. Designed for extensibility — future milestones add fields (feature, project, usage thresholds) without changing callers.

## Acceptance Criteria

- AC-01: A Cargo workspace exists at the repository root with a `unimatrix-store` library crate that compiles with `cargo build`.
- AC-02: The EntryRecord struct is defined with all fields listed above, using `#[serde(default)]` on all fields that may be added in future milestones. Round-trip serialization via bincode succeeds (serialize then deserialize produces the same record).
- AC-03: All 8 redb tables (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS) are defined as typed constants and created on database open.
- AC-04: Inserting an entry atomically writes to ENTRIES and all relevant index tables in a single write transaction. If any table write fails, no tables are modified (transaction abort).
- AC-05: The COUNTERS table generates monotonically increasing entry IDs. Concurrent calls (sequential due to redb's single-writer model) never produce duplicate IDs.
- AC-06: Point lookup by entry ID retrieves and deserializes the correct EntryRecord from the ENTRIES table.
- AC-07: Topic index queries return all entry IDs matching a given topic string via range scan on TOPIC_INDEX.
- AC-08: Category index queries return all entry IDs matching a given category string via range scan on CATEGORY_INDEX.
- AC-09: Tag index queries return entry IDs matching ALL specified tags (set intersection across TAG_INDEX multimap lookups).
- AC-10: Time range queries return entry IDs within a given timestamp range via range scan on TIME_INDEX.
- AC-11: Status index queries return entry IDs matching a given Status variant via range scan on STATUS_INDEX.
- AC-12: Status updates (e.g., Active to Deprecated) atomically remove the old status index entry and insert the new one, along with updating the ENTRIES record and COUNTERS.
- AC-13: VECTOR_MAP supports insert and lookup operations (entry_id to u64 hnsw_data_id mapping) for use by the future nxs-002 feature.
- AC-14: Database opens an existing file or creates a new one. Cache size is configurable (default 64 MB). `compact()` is callable for shutdown cleanup.
- AC-15: All public API functions return typed Result errors (not panics). Error types distinguish between redb errors, serialization errors, and application-level constraint violations (e.g., entry not found, duplicate ID).
- AC-16: Schema evolution is verified: an EntryRecord serialized without future `#[serde(default)]` fields deserializes correctly when those fields are added to the struct (defaulting to None/0/false).
- AC-17: A `query(QueryFilter)` function accepts optional topic, category, tags, status, and time_range fields. It intersects results from whichever index queries are applicable and returns matching EntryRecords. An empty filter returns all active entries.
- AC-18: Updating an entry (e.g., changing topic from "auth" to "security") atomically removes stale index entries, inserts new index entries, and writes the updated EntryRecord — all in a single transaction. Callers do not manage index cleanup.
- AC-19: Test infrastructure (database fixtures, setup/teardown helpers, assertion utilities) is designed for reuse by downstream features (nxs-002, vnc-001, etc.), not as throwaway single-feature scaffolding.

## Constraints

- **Rust edition 2024** required by redb v3.1.0.
- **Dependencies**: redb (v3.1.x), serde (v1), bincode (v2.x). No other runtime dependencies for this crate.
- **No async runtime dependency.** The crate is synchronous. It must not depend on tokio or any async runtime.
- **No unsafe code.** redb and bincode are both safe Rust. The storage engine must not introduce any `unsafe` blocks.
- **Single-writer concurrency model.** redb enforces one write transaction at a time. The API must document this and not attempt to circumvent it.
- **File path is caller-provided.** The storage engine does not determine where the database file lives. That is the responsibility of the MCP server layer (vnc-001) or project management layer (dsn-001).

## Resolved Decisions

1. **bincode v2**: Adopt bincode v2 for better `#[serde(default)]` handling and forward compatibility. The spike research used v1 patterns but v2's API improvements justify the switch for a greenfield crate.
2. **Owned String keys**: TOPIC_INDEX and CATEGORY_INDEX use `(String, u64)` compound keys. Owned strings avoid lifetime complexity in the public API at negligible performance cost for our scale.
3. **Both individual + combined query API**: Expose individual index queries as public building blocks (independently testable, per AC-07 through AC-11), AND provide a combined `query(QueryFilter)` function that composes them internally with set intersection. The `QueryFilter` struct accepts optional fields (`topic`, `category`, `tags`, `status`); the engine intersects whichever are present. This design is confirmed by future milestones (M4-M7) which add more query dimensions — usage ranking (crt-001), feature scoping (col-004), project scoping (dsn-001) — all of which extend `QueryFilter` without changing callers.
4. **Engine-managed index updates**: The storage engine handles old-index-removal + new-index-insertion internally on entry updates. Callers pass the updated `EntryRecord`; the engine reads the old record, diffs indexed fields, and updates all indexes atomically. This prevents orphaned index entries and is foundational for future milestones where the engine becomes progressively smarter (confidence evolution, usage tracking, co-access boosting).

## Tracking

GH Issue will be created during Session 1 Phase 2c (synthesizer).
