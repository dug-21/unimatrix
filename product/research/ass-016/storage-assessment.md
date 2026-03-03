# ASS-016: Storage Architecture Assessment

**Status**: Research Complete
**Date**: 2026-03-03
**Scope**: Evaluate redb as Unimatrix's storage backend given project growth; assess alternatives; make objective recommendations
**Decision Required**: None immediate — this is a standing reference for when/if the storage tier needs evolution

---

## Executive Summary

redb is a sound choice at Unimatrix's current scale (~53 active entries, 17 tables, sub-100 MiB database). The project has grown from 7 tables (nxs-001) to 17 tables across 5 schema versions, with increasing sophistication in confidence scoring, co-access tracking, session management, and vector operations. This growth has introduced architectural patterns that work around redb's limitations (manual secondary indexes, full table scans for reverse lookups, build-new-then-swap for HNSW compaction).

**The honest assessment**: redb is adequate today. It would start showing stress at ~10K+ entries or if write frequency increases significantly. The two realistic migration targets are **SQLite** (if we want richer query capabilities) or **LMDB via heed** (if we want the same architecture but faster reads). No migration is urgent.

---

## Part 1: Current Architecture Inventory

### 17 Tables (Schema v5)

| # | Table | Key | Value | Purpose | Access Pattern |
|---|-------|-----|-------|---------|---------------|
| 1 | ENTRIES | `u64` | `&[u8]` (bincode) | Primary entry storage | Point lookup, batch update |
| 2 | TOPIC_INDEX | `(&str, u64)` | `()` | Topic filtering | Prefix scan |
| 3 | CATEGORY_INDEX | `(&str, u64)` | `()` | Category filtering | Prefix scan |
| 4 | TAG_INDEX | `&str` → `u64` | (multimap) | Tag intersection | Multimap get + HashSet intersect |
| 5 | TIME_INDEX | `(u64, u64)` | `()` | Temporal range queries | Range scan |
| 6 | STATUS_INDEX | `(u8, u64)` | `()` | Status filtering | Prefix scan |
| 7 | VECTOR_MAP | `u64` | `u64` | entry_id → HNSW data_id bridge | Point lookup, full scan on load |
| 8 | COUNTERS | `&str` | `u64` | ID generation, stats | Point read/write (hot path) |
| 9 | AGENT_REGISTRY | `&str` | `&[u8]` | Agent trust records | Point lookup on every request |
| 10 | AUDIT_LOG | `u64` | `&[u8]` | Append-only compliance trail | Append-only write |
| 11 | FEATURE_ENTRIES | `&str` → `u64` | (multimap) | Feature→entry mapping | Multimap get |
| 12 | CO_ACCESS | `(u64, u64)` | `&[u8]` | Co-retrieval pair tracking | Prefix scan + **full table scan** |
| 13 | OUTCOME_INDEX | `(&str, u64)` | `()` | Feature cycle outcomes | Prefix scan |
| 14 | OBSERVATION_METRICS | `&str` | `&[u8]` | Retrospective metrics | Point lookup, full scan |
| 15 | SIGNAL_QUEUE | `u64` | `&[u8]` | Confidence signal work queue | Append + drain (transient) |
| 16 | SESSIONS | `&str` | `&[u8]` | Session lifecycle | Point lookup, full scan for GC |
| 17 | INJECTION_LOG | `u64` | `&[u8]` | Entry injection events | Batch append, cascade delete |

### Serialization: bincode v2 (serde path)

- Positional encoding (not field-name-based)
- Schema evolution via append-only fields + scan-and-rewrite migration
- No backwards compatibility across versions — readers must match writer schema

### Concurrency: Single Writer / Multi Reader (MVCC)

- `Arc<Store>` shared across async handlers
- Write transactions serialize at redb boundary
- Read transactions are non-blocking snapshots
- Fire-and-forget usage recording via `spawn_blocking`

### Vector Integration: Hybrid Architecture

- HNSW graph lives in-memory (hnsw_rs, f32 vectors, 384 dimensions)
- VECTOR_MAP table is crash-safe source of truth for entry_id↔data_id mappings
- IdMap rebuilt from VECTOR_MAP on startup
- Stale HNSW nodes accumulate; require full graph rebuild (compact) to reclaim

---

## Part 2: Architectural Friction Points

### 2.1 Manual Secondary Indexes (5 of 17 tables)

Tables 2-6 (TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX) exist solely because redb has no secondary index support. Every entry write must maintain all 5 indexes in the same transaction. Every entry delete must clean up all 5. This is:

- **Code maintenance cost**: ~300 lines of index synchronization in `write.rs`
- **Write amplification**: A single entry insert touches 7+ tables (ENTRIES + 5 indexes + COUNTERS)
- **Correctness risk**: Index desynchronization bugs are possible (though none observed)

With SQLite, all 5 would be `CREATE INDEX` statements — zero application code.

### 2.2 Co-Access Reverse Lookup (Full Table Scan)

`get_co_access_partners(entry_id)` must find all pairs containing `entry_id`. Keys are ordered `(min_id, max_id)`. When `entry_id` is `min`, prefix scan works. When `entry_id` is `max`, a **full table scan** is required.

- Currently 368 pairs — no problem
- At 10K+ pairs: O(n) scan on every co-access query
- Fix within redb: add a reverse index table (CO_ACCESS_REVERSE)
- Fix with SQL: `SELECT * FROM co_access WHERE id_a = ? OR id_b = ?` with indexes on both columns

### 2.3 HNSW Compaction (Build-New-Then-Swap)

The HNSW graph cannot incrementally delete nodes. When stale ratio exceeds 10%, the entire graph must be rebuilt:

1. Build new HNSW from all active embeddings
2. Write new VECTOR_MAP to redb (crash-safe checkpoint)
3. Atomic in-memory swap of graph + IdMap
4. Reset next_data_id counter

This is a fundamental limitation of the HNSW algorithm, not redb. However, the VECTOR_MAP bridge pattern adds complexity that wouldn't exist if vectors were in a database with native vector search.

### 2.4 Schema Migrations (Scan-and-Rewrite)

Each schema version bump that touches EntryRecord requires deserializing and re-serializing every entry:

```
v0→v1: Add 7 security fields (full rewrite)
v1→v2: Add usage tracking (full rewrite)
v2→v3: confidence f32→f64 (full rewrite)
v3→v4: Add SIGNAL_QUEUE table (table creation only)
v4→v5: Add SESSIONS + INJECTION_LOG (table creation only)
```

At current scale (dozens of entries), this is instant. At 100K+ entries, a full rewrite migration could take seconds to minutes.

With SQL: `ALTER TABLE entries ADD COLUMN helpful_count INTEGER DEFAULT 0` — no rewrite needed.

### 2.5 Session GC Cascade

`gc_sessions()` scans all SESSIONS for stale entries, then scans all of INJECTION_LOG to find matching session_ids for cascade delete. No secondary index on INJECTION_LOG by session_id.

### 2.6 No Built-in Compression

redb stores data uncompressed. The benchmarks show redb has the worst space efficiency of all tested engines:

| Engine | Uncompacted | Compacted |
|--------|-------------|-----------|
| redb | 4.00 GiB | 1.69 GiB |
| RocksDB | 893 MiB | 455 MiB |
| SQLite | 1.09 GiB | 557 MiB |
| LMDB | 2.61 GiB | 1.26 GiB |

At Unimatrix's scale this doesn't matter. At multi-GiB scale, it would.

---

## Part 3: redb Assessment

### Strengths (for Unimatrix)

1. **Pure Rust, memory-safe** — no C/C++ FFI, no segfault risk, auditable
2. **Best individual write performance** — 920ms vs 1,598ms (LMDB) for fsync'd writes
3. **ACID transactions** — clean, typed API with compile-time table definitions
4. **Active maintainer** — responsive to issues, regular releases
5. **v3 improvements** — smaller minimum file size (~50 KiB), better non-durable tx handling
6. **Simple deployment** — single file, no configuration knobs

### Weaknesses (for Unimatrix)

1. **No secondary indexes** — 5 of 17 tables exist solely as manual indexes
2. **No compression** — worst space efficiency among peers
3. **3.3x slower reads than LMDB** — due to user-space cache vs mmap
4. **Worst removal performance** — 23s vs 6-11s for competitors
5. **Deferred page reuse** — freed pages unavailable until transaction N+2 (#829)
6. **No async API** — requires spawn_blocking wrappers in tokio context
7. **Space bloat** — 2.4x uncompacted:compacted ratio
8. **Minor version regressions** — documented performance regressions between minor versions

### Known Issues at Scale (Not Currently Relevant)

- Performance cliff at ~50 GiB (ord project, #344)
- Slow database open for large files — minutes for 50 GiB (#1055, #386)
- Compaction busy loop after large deletions (#852, fixed)
- quick_repair causing 5x size increase (#934)

### Risk Assessment for Unimatrix

| Risk | Likelihood | Impact | Notes |
|------|-----------|--------|-------|
| Outgrow current scale | Low (1-2 years) | Medium | Knowledge bases rarely exceed 10K entries |
| Space bloat without compaction | Medium | Low | Need periodic compact() calls |
| Minor version regression | Medium | Medium | Pin versions carefully, test upgrades |
| Need for richer queries | Medium | High | As cortical/learning systems grow |
| Performance bottleneck | Low | Low | Current workload is light |

---

## Part 4: Alternative Engines Evaluated

### Tier 1: Realistic Candidates

#### SQLite (via rusqlite) — Strongest Alternative

| Dimension | Assessment |
|-----------|-----------|
| Architecture | B-tree + WAL, the most deployed database engine ever |
| Rust bindings | rusqlite 0.38 — gold-standard, bundled SQLite 3.49.2 |
| Concurrency | Single writer / multi reader (WAL mode) — same as redb |
| ACID | Full, with savepoints and nested transactions |
| Max size | 281 TB theoretical, hundreds of GB practical |
| Vector search | sqlite-vec (brute-force, ~57ms for 100K×384d; sub-ms for our scale) |
| Schema | Full SQL DDL — ALTER TABLE, CREATE INDEX, JOINs |
| Compression | Via optional extensions |
| Production readiness | Trillions of deployments. Extraordinary test suite (100% MC/DC) |
| Disk footprint | Compact, WAL overhead during writes, VACUUM available |
| Limitation | C dependency (bundled); sqlite-vec is brute-force only (no ANN) |

**What changes if we migrate to SQLite:**
- Eliminate 5 manual index tables → `CREATE INDEX` statements
- Co-access reverse lookup becomes a simple `WHERE` clause
- Schema migrations become `ALTER TABLE` — no scan-and-rewrite
- Session GC cascade becomes `DELETE FROM injection_log WHERE session_id IN (...)`
- Tag intersection becomes SQL JOIN
- Could replace custom HNSW with sqlite-vec at current scale (brute-force is sub-ms for ~500 entries)
- Lose: typed compile-time table definitions, pure Rust guarantee

**Migration complexity**: **High**. Would require rewriting Store trait implementations, all read/write paths, and migration logic. The abstraction boundary at `StoreAdapter` helps but doesn't eliminate the work.

#### LMDB (via heed) — Closest Architectural Match

| Dimension | Assessment |
|-----------|-----------|
| Architecture | Memory-mapped B+ tree (copy-on-write) — redb was inspired by LMDB |
| Rust bindings | heed 0.20 (Meilisearch) — typed, minimal overhead, Serde support |
| Concurrency | Single writer / unlimited readers — wait-free reads |
| ACID | Full, with nested transactions |
| Max size | 128 TB on 64-bit |
| Vector search | None — same custom HNSW approach needed |
| Schema | Named databases (≈tables), key-value — same model as redb |
| Compression | None built-in |
| Production readiness | Extremely mature — Meilisearch, OpenLDAP, Postfix, Monero |
| Disk footprint | Excellent — no WAL. Must pre-size map. No auto-shrink |
| Limitation | C dependency; must pre-configure max DB size; same KV model (no secondary indexes) |

**What changes if we migrate to LMDB:**
- **1.5-3x faster reads** (zero-copy mmap vs user-space cache)
- Same 17-table structure, same manual indexes, same co-access pattern
- Must pre-size database (e.g., 1 GiB map, grow if needed)
- Lose: pure Rust, compile-time typed tables

**Migration complexity**: **Medium**. heed's API is similar to redb's. Table definitions, serialization, and access patterns would be nearly identical. The main work is adapting to heed's API differences and map sizing.

### Tier 2: Not Recommended

| Engine | Why Not |
|--------|---------|
| **RocksDB** | C++ dependency, slow compilation, overkill tuning complexity, LSM overhead not justified at our scale |
| **DuckDB** | OLAP architecture — optimized for scans/aggregates, not point lookups. Wrong access pattern match |
| **SurrealDB** | Massive dependency footprint, API instability, full multi-model DB for a KV use case |
| **fjall** | Pure Rust LSM, but entering maintenance mode in 2026. Risk of abandonment |
| **sled** | Effectively abandoned. Alpha for years. Known data loss bugs. Do not consider |

### Notable Mention

**native_db** — a higher-level database built on top of redb that adds secondary indexes, migrations, and subscriptions. Conceptually interesting but has "huge overhead" for single-transaction operations and the API is not stable. Worth watching but not production-ready.

---

## Part 5: Scenario Analysis

### Scenario A: Stay with redb (Status Quo)

**When this is right**: Knowledge base stays under ~5K entries, query patterns don't get more complex, the current 17-table structure remains manageable.

**Actions to take now**:
- Ensure `compact()` runs periodically (already called on shutdown)
- Pin redb versions carefully; test upgrades in CI before adopting
- Add CO_ACCESS_REVERSE table if co-access pair count exceeds ~1K
- Monitor database file size vs entry count ratio

**Cost**: Zero migration cost. Ongoing cost of maintaining manual indexes.

### Scenario B: Migrate to SQLite

**When this becomes right**:
- Need for more complex queries (JOINs across entries, aggregations for analytics)
- Co-access or injection_log tables grow to sizes where full scans are unacceptable
- Want to eliminate manual index maintenance code
- Considering replacing custom HNSW with database-native vector search
- Schema evolution frequency increases and scan-and-rewrite becomes costly

**Trigger signals**:
- Adding a 6th+ manual index table
- Co-access pairs exceed 5K and reverse lookup latency is noticeable
- New feature requires a query that would be trivial in SQL but painful in KV
- Database file exceeds 500 MiB

**Migration path**:
1. Define SQLite schema mirroring current data model
2. Implement `StoreAdapter` trait against rusqlite
3. Write export/import tooling (redb → SQLite)
4. Run dual-backend in tests to verify parity
5. Cut over

**Estimated effort**: 2-3 feature cycles of focused work.

### Scenario C: Migrate to LMDB (heed)

**When this becomes right**:
- Read latency becomes critical (3x improvement available)
- Want to stay in KV paradigm but need better performance characteristics
- Database size grows and redb's space inefficiency becomes problematic

**Trigger signals**:
- context_search p99 latency exceeds acceptable thresholds
- Database file bloat ratio consistently > 2x after compaction
- Need concurrent multi-process read access (LMDB supports this natively)

**Migration path**:
1. Replace redb table definitions with heed database definitions
2. Adapt serialization (heed supports Serde natively)
3. Map redb transaction patterns to heed equivalents
4. Test with identical data sets

**Estimated effort**: 1-2 feature cycles (smaller surface area than SQLite migration).

---

## Part 6: Recommendations

### R1: No Migration Needed Now

redb is performing adequately at current scale. The friction points are real but manageable. A premature migration would cost significant effort with minimal immediate benefit.

### R2: SQLite is the Strategic Long-Term Target

If/when a migration becomes necessary, SQLite is the strongest choice because:
- It eliminates the most architectural friction (5 manual index tables, complex queries, schema migrations)
- sqlite-vec handles our vector search needs at current scale
- The ecosystem maturity is unmatched
- It opens doors for future capabilities (FTS5 for content search, JSON functions, window functions for analytics)

### R3: Establish Migration Trigger Criteria

Monitor these metrics and revisit this assessment when any trigger fires:

| Metric | Threshold | Action |
|--------|-----------|--------|
| Active entry count | > 5,000 | Reassess query performance |
| Manual index table count | > 6 | Strong signal for SQLite |
| Co-access pair count | > 5,000 | Add reverse index or migrate |
| Database file size | > 500 MiB | Assess space efficiency |
| New query type needed | Requires JOIN/aggregate | Strong signal for SQLite |
| Schema migration frequency | > 2 per quarter | Reassess migration approach |

### R4: Incremental Preparation (Low Cost)

Without committing to migration, these steps reduce future migration cost:
- The existing `StoreAdapter` trait abstraction is the right boundary. Keep it clean.
- Avoid leaking redb-specific types beyond the store crate
- Keep serialization logic (bincode) isolated in `schema.rs`
- If adding new tables, ask: "would this be an index in SQL?" If yes, it's a signal.

### R5: Track redb Health

- Subscribe to redb releases for regression notices
- Monitor GitHub issues for patterns at scale
- redb v3.x is a meaningful improvement — ensure we're on latest stable

### R6: Server Refactoring Before Storage Migration

If both efforts are pursued, the server refactoring ([server-refactoring-architecture.md](../optimizations/server-refactoring-architecture.md)) should be completed first. The dependency is asymmetric — server refactoring makes storage migration significantly easier, but not vice versa.

#### Surface Area Reduction

Storage is currently consumed from ~12 call sites across 3 large modules (`tools.rs`, `uds_listener.rs`, `server.rs`), with duplicated search/ranking/write pipelines between MCP and UDS paths. The server refactoring reduces consumers to ~5 service modules, each with a single codepath. Migration effort scales with `(changes per consumer) × (number of consumers)` — refactoring first cuts the multiplier by more than half.

#### Dependency Direction

| | Server refactoring first | Storage migration first |
|--|--------------------------|------------------------|
| Helps the other? | Yes — clean service boundary becomes the migration surface. `StoreAdapter` trait tightened. Swap implementation behind trait. | Marginally — eliminates 5 index tables so server has fewer store calls, but duplicated pipelines remain. |
| Dual-run testing | Services enable running both backends behind the trait, verifying parity per-service. | No clean boundary to inject a second backend. Testing requires parallel changes across 3 modules. |
| Rollback risk | Low — behavioral clones, module moves. | High — data persistence change with subtle serialization risks. |

#### Five Reasons for This Ordering

1. **Duplicated search pipelines are the worst-case migration target.** Search/ranking/boost logic exists in both `tools.rs` and `uds_listener.rs` with subtle divergences (MCP has metadata filtering, UDS has similarity floors). Migrating storage means updating both copies and keeping them in sync. After SearchService extraction, there's one pipeline to migrate.

2. **The `StoreAdapter` trait boundary is the migration seam.** The server refactoring cleans this boundary — services call the trait, not raw redb. Storage migration becomes: implement `StoreAdapter` for SQLite, swap, verify. Without the service layer, raw store calls are scattered across the codebase.

3. **Security gaps are present-tense risk; storage performance is not.** The UDS path has zero content scanning (F-25), zero authorization (F-26), zero query validation (F-27), and zero audit trail (F-28). These affect the running system today. Storage performance at ~53 entries is fine. The server refactoring's Security Gateway closes these gaps in Wave 1.

4. **The storage migration might not be needed.** The server refactoring reorganizes how indexes are consumed. The SearchService centralizes queries and may reduce pressure on the index pattern. If the trigger doesn't fire, the high-risk migration is avoided entirely. If it does fire, the clean seam is ready.

5. **SQLite's index elimination changes the Store API.** `CREATE INDEX` replaces 5 manual index tables, meaning `query_filtered()` replaces 5 separate range scans. The service layer absorbs this API change — transport code and service callers remain untouched.

#### Counterargument Addressed

> "Server refactoring moves code that will change again during storage migration — double work."

The service layer is both transport-agnostic and storage-agnostic. `SearchService::search()` takes params and returns results. Whether results come from redb range scans or SQLite queries is invisible to the service's callers. Service internals change during storage migration, but the API and all transport code remain untouched. The abstraction isolates blast radius — that's its purpose.

#### Recommended Sequence

```
Phase 1:  Server refactoring (Waves 1-3)
          └─ Closes security gaps, deduplicates pipelines, cleans StoreAdapter boundary
          └─ Lower risk (organizational + behavioral cloning)

Phase 2:  Storage migration (if/when trigger fires)
          └─ Implement SQLite StoreAdapter behind clean trait boundary
          └─ Dual-run testing via services
          └─ Cut over with isolated blast radius
```

---

## Appendix A: Benchmark Context

All benchmarks from redb's official suite (Ryzen 9950X3D, Samsung 9100 PRO NVMe, 1M entries):

| Operation | redb | LMDB | RocksDB | SQLite |
|-----------|------|------|---------|--------|
| Individual writes (fsync) | **920ms** | 1,598ms | 2,432ms | 7,040ms |
| Batch writes | 1,595ms | 942ms | **451ms** | 2,625ms |
| Random reads | 1,138ms | **637ms** | 2,911ms | 4,283ms |
| Random reads (32 threads) | 410ms | **125ms** | 1,100ms | 26,536ms |
| Removals | 23,297ms | 10,435ms | 6,900ms | 10,323ms |
| Compacted size | 1.69 GiB | 1.26 GiB | **455 MiB** | 557 MiB |

## Appendix B: redb GitHub Issues Referenced

| Issue | Title | Relevance |
|-------|-------|-----------|
| #344 | Performance cliff with large database | Scale ceiling |
| #829 | Freed pages not reused promptly | Space efficiency |
| #1055 | Slow database open on large files | Startup time |
| #852 | Busy loop during compaction | Reliability |
| #934 | quick_repair increasing database size | Operational risk |
| #603 | Slowdown after mmap removal | Architecture tradeoff |
| #879 | Performance regression v2.1.3→v2.1.4 | Upgrade risk |
| #30 | No async read interface | API gap |

## Appendix C: Data Model Complexity Growth

```
nxs-001 (initial):  7 tables,  ~15 EntryRecord fields, schema v0
nxs-004:            9 tables,  24 EntryRecord fields, schema v1  (+security, agent registry, audit)
crt-001:            9 tables,  24 fields + usage,     schema v2  (+helpful/unhelpful counts)
crt-004:           12 tables,  24 fields,             schema v2  (+co-access, feature_entries, outcome)
crt-005:           12 tables,  24 fields (f64),       schema v3  (+confidence precision)
col-009:           13 tables,  24 fields,             schema v4  (+signal queue)
col-010:           17 tables,  24 fields,             schema v5  (+sessions, injection log, metrics, signals)
```

Growth rate: ~2.5 tables per milestone, 1 schema version per milestone.
