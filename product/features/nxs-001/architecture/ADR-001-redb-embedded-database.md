# ADR-001: redb as Embedded Database

## Status

Accepted

## Context

Unimatrix needs a persistent storage engine for its knowledge entries, indexes, and metadata. The storage must be:

- **Embedded** — no separate server process; runs in-process alongside the MCP server
- **ACID** — crash-safe writes with fsync; no data loss on power failure or process crash
- **Multi-table** — multiple named tables with independent key/value types for primary storage and secondary indexes
- **Atomic cross-table writes** — a single write transaction must span all tables (ENTRIES + all index tables) to prevent index inconsistency
- **Pure Rust** — no C/C++ FFI, no system library dependencies, builds cleanly on all platforms
- **Synchronous API** — appropriate for wrapping with `spawn_blocking` in async contexts
- **Sub-millisecond latency** — at Unimatrix's target scale of 1K-100K entries

Alternatives considered:

- **SQLite (via rusqlite)** — Mature, but requires C compilation (libsqlite3), adds FFI complexity, and its SQL interface is unnecessary overhead for a fixed schema accessed programmatically.
- **sled** — Pure Rust, but development stalled (last release 2021), known durability issues, and the author recommends against production use pending the unreleased marble rewrite.
- **RocksDB (via rust-rocksdb)** — Excellent performance but requires C++ compilation, large binary size, complex configuration, and is over-engineered for single-user local data at our scale.
- **fjall** — Pure Rust LSM-tree. Strong batch write performance but less mature than redb (fewer dependents, less production validation). Better suited for write-heavy workloads; Unimatrix is read-heavy.
- **redb** — Pure Rust, ACID, COW B-tree, zero required dependencies, sub-millisecond at our scale, `Send + Sync` for `Arc<Database>` sharing, actively maintained (v3.1.0, 303 dependents, production use in Iroh).

## Decision

Use **redb v3.1.x** as the embedded database engine.

redb provides the exact combination of properties Unimatrix requires: pure Rust, ACID transactions spanning multiple typed tables, no external dependencies, and a synchronous API that maps cleanly to our architecture. The ASS-003 spike (D2 deliverable) validated all critical patterns at high confidence — compound tuple keys, MultimapTable, `Arc<Database>` + `spawn_blocking`, and manual compaction.

At Unimatrix's scale (1K-100K entries, ~50-200 MB), redb's known weaknesses (slower batch writes and larger file size vs LSM-tree engines) are irrelevant. Read latency is sub-millisecond with a B-tree depth of 2-3.

## Consequences

**Positive:**
- Zero external dependencies — builds on any Rust target without system libraries
- ACID guarantees with fsync-on-commit — no data loss scenarios at the storage layer
- Single-file database — simple deployment, backup, and migration
- Typed table definitions catch key/value type mismatches at compile time
- MVCC (readers never block writers) maps directly to the MCP server's concurrent access pattern
- Production-proven `Arc<Database>` + `spawn_blocking` pattern (Iroh)

**Negative:**
- No secondary index mechanism — all indexes must be manually maintained as separate tables, updated atomically in write transactions (see ADR-003)
- No built-in compaction — must call `db.compact()` manually on shutdown
- Synchronous-only API — async consumers must use `spawn_blocking` (see ADR-004)
- Single-writer model — write throughput is limited to one transaction at a time (acceptable at our scale; writes are sub-millisecond)
- Requires Rust edition 2024 and MSRV 1.89 (redb v3.1.0 requirement)

**Risks:**
- redb is a younger project than SQLite or RocksDB. Mitigated by: active maintenance, growing adoption (303 dependents, 3M downloads), production use in Iroh, and our sub-100K entry scale which avoids edge cases that stress larger deployments.
