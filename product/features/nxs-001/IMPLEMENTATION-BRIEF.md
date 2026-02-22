# nxs-001: Embedded Storage Engine — Implementation Brief

**Handoff**: Session 1 (Design) -> Session 2 (Delivery)
**Date**: 2026-02-22

---

## Feature Summary

nxs-001 implements the foundational persistence layer for Unimatrix: a synchronous Rust library crate (`unimatrix-store`) backed by redb v3.1.x. It provides an 8-table storage engine (ENTRIES + 5 secondary index tables + VECTOR_MAP bridge + COUNTERS) with bincode v2 serialization, atomic multi-table write transactions, composable read paths via `QueryFilter` set intersection, and zero-migration schema evolution via `#[serde(default)]`. This crate has zero async runtime dependencies and is the foundation for every downstream feature in the Unimatrix roadmap.

---

## Component Map

### C1: crate-setup

**Purpose**: Cargo workspace and crate scaffolding.

| Attribute | Value |
|-----------|-------|
| Creates | `/Cargo.toml` (workspace root), `/crates/unimatrix-store/Cargo.toml` |
| Modules | N/A (configuration only) |
| Dependencies | None |

**Details**:
- Workspace root: `members = ["crates/*"]`, `resolver = "3"`, `edition = "2024"`, `rust-version = "1.89"`
- Workspace dependencies: `redb = "3.1"`, `serde = { version = "1", features = ["derive"] }`, `bincode = "2"`
- Crate dev-dependencies: `tempfile = "3"`
- Crate-level `#![forbid(unsafe_code)]`

### C2: schema

**Purpose**: All data types and redb table definitions.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/schema.rs` |
| Modules | `EntryRecord`, `Status`, `NewEntry`, `QueryFilter`, `TimeRange`, `DatabaseConfig`, 8 table constants |
| Dependencies | serde, bincode (derives), redb (table type definitions) |

**Key types**:
- `EntryRecord` — primary data struct with `#[serde(default)]` on future-evolving fields
- `Status` — `#[repr(u8)]` enum: `Active(0)`, `Deprecated(1)`, `Proposed(2)`
- `NewEntry` — insert input (excludes engine-assigned `id`, `created_at`, `updated_at`)
- `QueryFilter` — optional-field struct for composable multi-index queries
- 8 table constants: `ENTRIES`, `TOPIC_INDEX`, `CATEGORY_INDEX`, `TAG_INDEX`, `TIME_INDEX`, `STATUS_INDEX`, `VECTOR_MAP`, `COUNTERS`

### C3: error

**Purpose**: Typed error enum wrapping all failure modes.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/error.rs` |
| Modules | `StoreError`, `Result<T>` type alias |
| Dependencies | redb (error types), bincode (error types), std::error::Error |

**Variants**: `EntryNotFound(u64)`, `Database(redb::DatabaseError)`, `Transaction(redb::TransactionError)`, `Table(redb::TableError)`, `Storage(redb::StorageError)`, `Compaction(redb::CompactionError)`, `Serialization(String)`, `Deserialization(String)`, `InvalidStatus(u8)`.

Implements `Display`, `Error`, and `From` impls for ergonomic `?` usage.

### C4: store

**Purpose**: `Store` wrapper struct, database lifecycle (open/create/compact).

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/db.rs` |
| Modules | `Store` struct, `open()`, `open_with_config()`, `compact()` |
| Dependencies | C2 (schema — table constants), C3 (error) |

**Details**:
- `Store` wraps `redb::Database`. Is `Send + Sync`, shareable via `Arc<Store>`.
- On open, all 8 tables are created (if absent) in an initial write transaction.
- Cache size configurable via `DatabaseConfig` (default 64 MiB).
- `compact()` calls `redb::Database::compact()` for shutdown cleanup.

### C5: counter

**Purpose**: Atomic ID generation and statistical counters.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/counter.rs` |
| Modules | `next_entry_id()`, `read_counter()`, `increment_counter()`, `decrement_counter()` |
| Dependencies | C2 (schema — COUNTERS table), C3 (error) |

**Details**:
- `next_entry_id` takes a `&WriteTransaction` (not `&Store`) — must execute within the same transaction as the insert to prevent ID gaps.
- First entry ID is `1` (not `0`). ID `0` is reserved as sentinel.
- Counter keys: `"next_entry_id"`, `"total_active"`, `"total_deprecated"`, `"total_proposed"`.
- Missing keys return `0`.

### C6: write

**Purpose**: All write operations — insert, update, status change, delete, vector map.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/write.rs` |
| Modules | `Store::insert()`, `Store::update()`, `Store::update_status()`, `Store::delete()`, `Store::put_vector_mapping()` |
| Dependencies | C2 (schema), C3 (error), C4 (store — `&self`), C5 (counter — ID gen within transactions) |

**Details**:
- All writes are atomic multi-table transactions (ENTRIES + index tables + COUNTERS).
- Insert: generate ID via counter, serialize via bincode, write ENTRIES + all 5 indexes + increment counter.
- Update: read old record, diff indexed fields, remove stale index entries, insert new ones, write updated record.
- Status change: specialized update for STATUS_INDEX migration + counter adjustment.
- Delete: remove from ENTRIES + all indexes + VECTOR_MAP + decrement counter.
- `put_vector_mapping`: simple VECTOR_MAP key-value write.

### C7: read

**Purpose**: All read operations — point lookup, individual index queries, batch fetch.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/read.rs` |
| Modules | `Store::get()`, `Store::exists()`, `Store::query_by_topic()`, `Store::query_by_category()`, `Store::query_by_tags()`, `Store::query_by_time_range()`, `Store::query_by_status()`, `Store::get_vector_mapping()`, `Store::read_counter()` |
| Dependencies | C2 (schema), C3 (error), C4 (store — `&self`) |

**Details**:
- All reads use redb `ReadTransaction` (MVCC snapshots, concurrent with writes).
- Individual index queries return `Vec<EntryRecord>` (fetch from ENTRIES after index scan).
- Batch fetch: individual `get()` calls per entry ID (optimize to range scan only if profiling warrants).

### C8: query

**Purpose**: Combined `QueryFilter` multi-index intersection.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/query.rs` |
| Modules | `Store::query()` (combined query) |
| Dependencies | C2 (schema — QueryFilter), C3 (error), C4 (store), C7 (read — individual index query internals) |

**Details**:
- Each present filter field produces a `HashSet<u64>` of matching entry IDs.
- Intersects all sets, then batch-fetches from ENTRIES.
- Empty filter (all `None`) defaults to all `Status::Active` entries.
- Internal: shares index scan logic with C7's individual queries (not duplicated).

### C9: test-infra

**Purpose**: Reusable test infrastructure for this and downstream features.

| Attribute | Value |
|-----------|-------|
| Creates | Test helpers module (accessible via `#[cfg(test)]` internally, `test-support` feature flag for downstream) |
| Modules | `TestDb`, `TestEntry` builder, `assert_index_consistent()`, `assert_index_absent()`, `seed_entries()` |
| Dependencies | All above components, `tempfile` |

**Details**:
- `TestDb`: creates temp directory + database, implements `Drop` for cleanup.
- `TestEntry` builder: `TestEntry::new(topic, category).with_tags(&[...]).with_status(...).build()` -> `NewEntry`.
- Assertion helpers verify index consistency across all 6 index tables after writes.
- No hardcoded paths. No test interdependence.

### C10: lib

**Purpose**: Crate root with public re-exports.

| Attribute | Value |
|-----------|-------|
| Creates | `crates/unimatrix-store/src/lib.rs` |
| Modules | Re-exports from all other modules |
| Dependencies | All above modules |

**Re-exports**: `Store`, `EntryRecord`, `Status`, `NewEntry`, `QueryFilter`, `TimeRange`, `DatabaseConfig`, `StoreError`, `Result`.

---

## Implementation Order

```
Phase 1: Foundation (no internal dependencies)
  C1: crate-setup     — workspace + Cargo.toml files
  C3: error           — StoreError enum (depends only on redb/bincode types)
  C2: schema          — types + table definitions

Phase 2: Core (depends on Phase 1)
  C4: store           — Store struct, open/compact (needs schema for tables, error for Result)
  C5: counter         — ID gen + counter ops (needs schema for COUNTERS table, error)

Phase 3: Operations (depends on Phase 2)
  C6: write           — all write paths (needs store, counter, schema, error)
  C7: read            — all individual read paths (needs store, schema, error)

Phase 4: Composition (depends on Phase 3)
  C8: query           — combined QueryFilter (needs read internals, schema)
  C10: lib            — re-exports (needs all modules)

Phase 5: Test Infrastructure (depends on all above)
  C9: test-infra      — helpers, builders, assertions
```

**Critical path**: C1 -> C2/C3 -> C4 -> C5 -> C6 -> C8. The write path is the longest dependency chain and contains the highest-risk code (R1, R2).

---

## Critical Decisions Constraining Implementation

### From ADRs

| ADR | Constraint | Implementation Impact |
|-----|-----------|----------------------|
| ADR-001 | redb v3.1.x, Rust edition 2024, MSRV 1.89 | Workspace must target edition 2024. No workarounds. |
| ADR-002 | bincode v2 with serde-compatible encoding | See W1 below. |
| ADR-003 | Manual secondary indexes as separate tables | Write operations must update 5-6 tables atomically. No shortcuts. |
| ADR-004 | Synchronous API only | No tokio dependency. No `async fn`. |
| ADR-005 | Compound tuple keys for indexes | `(&str, u64)` for topic/category, `(u64, u64)` for time, `(u8, u64)` for status. |

### From Alignment Warnings (MUST resolve)

**W1: bincode v2 serde-compatible API path (CRITICAL)**

The implementation MUST use `bincode::serde::encode_to_vec` / `bincode::serde::decode_from_slice` (serde-compatible functions), NOT `bincode::encode_to_vec` with bincode-native `Encode`/`Decode` derives. Only the serde-compatible path respects `#[serde(default)]`, which is foundational to the zero-migration schema evolution strategy for milestones M1-M9.

`EntryRecord` derives `Serialize` + `Deserialize` (serde), NOT `Encode` + `Decode` (bincode-native).

**Write the schema evolution test (R4/AC-16) FIRST** before persisting any data, to verify the configuration.

**W2: Store wrapper API pattern**

The implementation adopts the Specification's `Store` wrapper pattern (methods on `Store`), not the Architecture's free-function pattern (functions taking `&Database`). Downstream integration uses `Arc<Store>`, not `Arc<Database>`. The `Store` type must be `Send + Sync`.

### From Specification Decisions

- First entry ID is `1`, not `0`. ID `0` is sentinel.
- `NewEntry` struct for inserts (type-safe separation of caller-provided vs engine-assigned fields).
- Engine-managed index diff on update (callers provide full record; engine diffs and updates indexes).
- Empty `QueryFilter` defaults to all `Status::Active` entries.
- `#![forbid(unsafe_code)]` at crate level.

---

## Integration Constraints

| Consumer | Integration Surface | Constraint |
|----------|-------------------|------------|
| nxs-002 (Vector Index) | `put_vector_mapping`, `get_vector_mapping`, `query_by_status`, `batch_get` | VECTOR_MAP stores `u64`; nxs-002 casts to `usize` at boundary. |
| nxs-003 (Embedding Pipeline) | `EntryRecord.content`, `EntryRecord.title` | Read-only. No coupling. |
| nxs-004 (Core Traits) | Future `EntryStore` trait | Trait defined externally; `Store` will implement it. |
| vnc-001 (MCP Server) | Full API via `Arc<Store>` + `spawn_blocking` | `Store` must be `Send + Sync`. |
| crt-001 (Usage Tracking) | `last_accessed_at`, `access_count` fields | Already in schema with `#[serde(default)]`. |
| col-001 (Outcome Tracking) | New tables in same database file | redb supports adding tables to existing databases. |

---

## Risk Hotspots (Test First)

Ranked by severity x likelihood from RISK-TEST-STRATEGY.md:

| Priority | Risk | Component | What to Test First |
|----------|------|-----------|-------------------|
| 1 | R2: Update Path Stale Index Orphaning | C6 (write) | Topic/category/tag/status change: verify old index entries removed, new ones inserted. Multi-field simultaneous changes. |
| 2 | R1: Index-Entry Desynchronization | C6 (write) | Insert entry, verify all 6 index tables contain matching entries. 50-entry bulk verify. |
| 3 | R7: QueryFilter Intersection Correctness | C8 (query) | All filter field combinations. Empty filter default. Disjoint filters -> empty result. Property tests. |
| 4 | R4: Schema Evolution | C2 (schema) | Serialize reduced struct, deserialize as full EntryRecord. `#[serde(default)]` verification. **Write this test FIRST (W1).** |
| 5 | R8: Status Transition Atomicity | C6 (write) | Active->Deprecated: verify STATUS_INDEX, ENTRIES, COUNTERS all updated. Counter consistency. |

---

## Resolved Questions

| Question | Resolution | Source |
|----------|-----------|--------|
| `NewEntry` vs `EntryRecord` for insert | `NewEntry` (type safety) | Spec OQ-1 |
| Test infrastructure exposure | `test-support` feature flag | Spec OQ-2 |
| Batch fetch strategy | Individual gets (optimize later if needed) | Spec OQ-3 |
| VECTOR_MAP value type | `u64` in redb, cast to `usize` at nxs-002 boundary | SCOPE |
| Owned vs borrowed string keys | `(&str, u64)` in table def; redb handles storage internally | Architecture |
| API shape | `Store` wrapper pattern per Specification (not free functions) | Alignment W2 |
| bincode API path | `bincode::serde::*` functions (serde-compatible), not native `Encode`/`Decode` | Alignment W1 |
| First entry ID | `1` (not `0`; `0` is sentinel) | Specification |

---

## Open Questions Remaining

**OQ-1: Null bytes in string keys.**
redb's `&str` Key implementation may not handle strings containing null bytes. If topic/category/tag strings can contain null bytes from user-provided content, this needs testing during implementation. Recommendation: validate at API boundary — document and reject strings with null bytes.

**OQ-2: Tag removal semantics in MultimapTable.**
When removing the last entry_id from a tag in TAG_INDEX, does the tag key remain (with empty value set) or is redb cleaned up automatically? Affects future "list all known tags" functionality. Verify during C6 implementation.

**OQ-3: Counter initialization strategy.**
Should COUNTERS be eagerly initialized on database creation (in the table-creation write transaction) or lazily on first insert? Recommendation: eager initialization during `Store::open()` for simplicity.

**OQ-4: Forward-compatibility of update path.**
If an entry was inserted before a code change that adds a new index table, the update path's "read old, diff, remove stale" logic must handle missing old index entries gracefully. This is a future concern but should be considered during C6 design.

**OQ-5: bincode v2 configuration variant.**
Which `bincode::config::Configuration` to use with serde functions — `standard()` (variable-length integers) or `legacy()` (fixed-length, v1-compatible)? Recommendation: `standard()` for new data (no v1 legacy). Verify in R4 schema evolution test.
