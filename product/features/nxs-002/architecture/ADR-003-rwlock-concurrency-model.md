## ADR-003: RwLock Concurrency Model for VectorIndex

### Context

hnsw_rs has a split concurrency model:
- `insert(&self, ...)` and `search(&self, ...)` take shared references (concurrent use safe via internal `parking_lot::RwLock`).
- `set_searching_mode(&mut self, bool)` takes exclusive reference (required before first search after inserts).

The MCP server (vnc-001) will wrap `VectorIndex` in `Arc` and call from multiple `spawn_blocking` tasks concurrently. We need a safe concurrency strategy.

Alternatives:
1. **No wrapper, rely on hnsw_rs internal locks**: Works for insert+search concurrency, but cannot call `set_searching_mode` without `&mut`.
2. **Mutex wrapper**: Simple, but serializes all operations. Searches cannot run concurrently.
3. **RwLock wrapper**: Concurrent reads (searches), exclusive writes (inserts + mode transitions).
4. **Phase-based lifecycle**: Batch all inserts, transition once, then search. No concurrent insert+search.

### Decision

Wrap `Hnsw` in `std::sync::RwLock`:

- **Read lock** for `search` and `search_filtered` (concurrent, non-blocking between readers).
- **Write lock** for `insert` (exclusive, ensures `set_searching_mode` can be called safely).

The `insert` method:
1. Acquires write lock.
2. Calls `hnsw.insert_slice(...)` (hnsw_rs handles internal synchronization, but we hold write lock to prevent concurrent mode transitions).
3. Releases write lock.
4. Writes VECTOR_MAP (outside hnsw lock to minimize lock hold time).

The `search` method:
1. Acquires read lock.
2. Calls `hnsw.search_filter(...)`.
3. Releases read lock.
4. Maps results via IdMap (separate read lock).

Mode transition (`set_searching_mode`) is called internally within `insert` or `search` when the current mode does not match the requested operation. The write lock on `Hnsw` ensures exclusive access for mode transitions.

### Consequences

- **Easier**: Concurrent searches (read locks do not block each other). This is the dominant access pattern.
- **Easier**: Thread-safe mode transitions without caller coordination.
- **Harder**: Inserts block searches (and vice versa) due to write lock. At Unimatrix scale (low write frequency, sub-millisecond inserts), this is acceptable.
- **Harder**: Potential for write starvation if searches are continuous. Mitigated by short lock hold times (hnsw_rs insert is fast).
- **Harder**: Two levels of locking (our RwLock + hnsw_rs internal locks). Risk of performance surprises. Benchmark test will measure actual overhead (OQ-3 resolution).
