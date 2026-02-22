# redb Research Notes

**Date**: 2026-02-20
**Purpose**: Raw findings and notable details from Track 1B research

---

## Architecture: COW B-Tree

redb uses a **copy-on-write B-tree** — the same fundamental architecture as LMDB. When a page is modified:

1. A new page is allocated with the modification
2. Parent pages are updated with the new child pointer (also COW)
3. The old page is added to a freed list
4. Freed pages are recycled only after all readers that might reference them complete (epoch-based reclamation)

This gives ACID without a WAL but means:
- Every write amplifies through the tree depth (modifying a leaf rewrites leaf + all ancestors)
- File size grows with writes until compaction
- No WAL recovery — instead uses checksummed double-buffered commit slots

### Double-Buffered Commit ("God Byte")

redb maintains two commit slots. On commit, it writes to the inactive slot and flips a "god byte" to make it active. On crash, it reads the god byte to find the last valid commit. This is elegant but means recovery may need to scan checksums (unless 2PC mode is enabled).

---

## Notable API Patterns

### AccessGuard — Zero-Copy Reads

All read operations return `AccessGuard<V>` rather than owned values:
```rust
let guard = table.get(key)?.unwrap();
let value = guard.value();  // borrows from the DB page
// value is valid only while guard is alive
```

This is a significant design choice — reads are zero-copy but you can't hold values across transaction boundaries. For our MCP server, this means: read data, transform into owned response types, then drop the guard/transaction.

### Mutation Helpers

redb 2.0+ replaced `drain()` with safer alternatives:
- `extract_if(predicate)` — lazy conditional removal
- `extract_from_if(range, predicate)` — ranged conditional removal
- `retain(predicate)` / `retain_in(range, predicate)` — keep matching entries

These are useful for lifecycle management: e.g., "deprecate all entries older than X with low confidence."

### In-Place Mutation (v3.0+)

```rust
let mut guard = table.get_mut(key)?.unwrap();
// modify in-place without read + remove + insert cycle
```

---

## Tokio Integration Deep Dive

redb is synchronous. The proven pattern (used by Iroh in production):

```rust
let db = Arc::new(Database::create(path)?);
let db_clone = db.clone();
tokio::task::spawn_blocking(move || {
    // redb operations here
}).await??;
```

Key considerations:
- `Database` is `Send + Sync` — safe to share via Arc
- Read transactions are non-blocking (MVCC) — many can run concurrently on the blocking pool
- Write transactions serialize (single writer) — but complete fast at our scale
- `spawn_blocking` tasks can't be cancelled — writes always complete
- Tokio's blocking pool default: 512 threads (more than enough)

Iroh blog post confirms this works well: https://www.iroh.computer/blog/async-rust-challenges-in-iroh

---

## Comparison with Alternatives

### redb vs sled

sled was the previous go-to pure-Rust embedded DB but has serious problems:
- Never reached 1.0
- Semi-abandoned (sporadic maintenance)
- Full storage subsystem rewrite in progress (komora/marble)
- On-disk format will change before 1.0
- **redb is the clear successor for new projects**

### redb vs LMDB

Both use COW B-trees and MVCC. Key differences:
- **LMDB**: C library via FFI. Faster reads (~2x). Memory-unsafe if misused. Very mature.
- **redb**: Pure Rust. Slower reads but fastest individual writes. Memory-safe. Simpler builds.

For Unimatrix, the pure Rust advantage (no C toolchain, no FFI unsafety) outweighs the ~2x read penalty, especially since reads are sub-millisecond at our scale regardless.

### redb vs SQLite

SQLite is overkill for our key-value metadata store. redb advantages:
- 7x faster individual writes (920ms vs 7,040ms in benchmarks)
- No SQL parsing overhead
- Pure Rust, no C dependency
- Simpler API for KV workloads

---

## Production Users

| Project | Scale | Notes |
|---------|-------|-------|
| **ord** (Bitcoin Ordinals) | Large — indexes the Bitcoin blockchain | Uses in-memory cache in front of redb |
| **Iroh** (n0.computer) | Production p2p data sync | Both blob store and document store |
| **Cuprate** | Monero node in Rust | One of multiple storage backends |
| **OpenDAL** | Apache data access layer | Optional backend |

The ord project is particularly telling — it handles a dataset far larger than anything Unimatrix will see.

---

## File Size Concerns

redb has the worst file size efficiency of all tested databases:
- Uncompacted: 4.00 GiB (vs 893 MiB for RocksDB)
- Compacted: 1.69 GiB (vs 455 MiB for RocksDB)

At Unimatrix scale (~50 MB raw data), this means:
- Expect 50-200 MB file size with fragmentation
- Compaction reduces by ~58%
- **This is completely acceptable** — even 200 MB is trivial for a local-first app

Mitigation: `db.compact()` on clean shutdown.

---

## Optional Features Worth Noting

redb has optional crate features:
- `logging` — enables log crate integration for debug output
- `chrono_v0_4` — adds `Key`/`Value` impls for chrono DateTime types
- `uuid` — adds `Key`/`Value` impls for uuid::Uuid

The `chrono` feature could be useful if we want to use `DateTime<Utc>` directly as keys instead of raw `u64` timestamps. However, raw `u64` is simpler and avoids the dependency.

---

## Schema Evolution Considerations

redb has **no built-in schema migration**. If the metadata struct changes:

1. `type_name()` validation will catch mismatches on open
2. Migration requires: create new table, read old, transform, write new, delete old
3. All within a single write transaction (atomic)

For Unimatrix, using serde + bincode with `&[u8]` values gives us the most flexibility:
- Add optional fields with defaults (bincode handles this)
- Version field in the metadata struct for explicit migration logic
- Avoids redb's type_name validation (since we're just storing bytes)

---

## Gaps Requiring Future Investigation

| Gap | How to Resolve | Priority |
|-----|---------------|----------|
| Actual filter-build latency (redb read → Vec for hnsw_rs FilterT) | Build test harness measuring end-to-end | Medium |
| Optimal bincode vs postcard for metadata serialization | Benchmark both with representative metadata structs | Low |
| `redb_derive` vs serde for struct storage tradeoffs | Test both approaches with schema evolution scenarios | Low |
| Compaction frequency impact on write performance | Long-running test with periodic compaction | Low |
| Memory usage with 64 MB vs 128 MB cache at 100K entries | Benchmark with cache_stats() | Low |

These are implementation-level details, not blockers for interface design.
