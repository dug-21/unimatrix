## ADR-004: Bidirectional In-Memory ID Map

### Context

Entry IDs (u64, assigned by unimatrix-store) and hnsw data IDs (usize, assigned by unimatrix-vector) are independent numbering schemes. We need:

1. **Forward lookup** (entry_id -> data_id): For building `FilterT` allow-lists from metadata-filtered entry IDs.
2. **Reverse lookup** (data_id -> entry_id): For translating hnsw_rs `Neighbour.d_id` results back to entry IDs.

The VECTOR_MAP table in redb stores `entry_id -> hnsw_data_id` (forward direction only). Reverse lookup from VECTOR_MAP would require a table scan, which is O(n).

Alternatives:
1. **VECTOR_MAP only, scan for reverse**: Simple, no extra memory, but O(n) per search result mapping.
2. **Reverse VECTOR_MAP table in redb**: Crash-safe, but adds write overhead (two redb writes per insert instead of one).
3. **In-memory HashMap, one direction**: Forward or reverse only. The other direction requires VECTOR_MAP scan.
4. **In-memory bidirectional HashMap**: O(1) both directions. Rebuilt from VECTOR_MAP on load. Extra memory.

### Decision

Maintain an in-memory bidirectional `IdMap`:

```rust
struct IdMap {
    data_to_entry: HashMap<u64, u64>,
    entry_to_data: HashMap<u64, u64>,
}
```

Memory overhead: ~16 bytes per entry per direction = ~32 bytes per entry. At 100K entries = ~3.2 MB. Negligible compared to hnsw_rs memory (~183 MB at 100K/384d).

On `insert`: add to both maps atomically (within IdMap write lock).
On `load`: iterate VECTOR_MAP from redb, populate both maps.

VECTOR_MAP in redb remains the crash-safe source of truth. The in-memory IdMap is a cache rebuilt on startup.

### Consequences

- **Easier**: O(1) result mapping (search results -> entry IDs). Critical for search latency.
- **Easier**: O(1) filter construction (entry IDs -> data IDs for FilterT). Critical for filtered search.
- **Easier**: No additional redb writes for reverse mapping.
- **Harder**: Additional memory (~32 bytes/entry). Acceptable at Unimatrix scale.
- **Harder**: IdMap must be kept in sync with VECTOR_MAP. Maintained atomically via write lock.
- **Harder**: On load, requires iterating all VECTOR_MAP entries. O(n) startup cost. Acceptable.
- **Harder**: Re-embedding (same entry_id, new data_id) must update `entry_to_data` to point to the new data_id. The old data_id remains in `data_to_entry` (stale entry), which is acceptable -- the old point still exists in hnsw_rs but will map to the correct entry_id on search.

Note on re-embedding: When entry_id E is re-embedded, the old mapping `(data_id_old -> E)` remains in `data_to_entry`. The new mapping `(data_id_new -> E)` is added. `entry_to_data` is updated to `(E -> data_id_new)`. Both old and new hnsw_rs points map back to E, which is correct -- if the old point appears in search results, it still maps to the right entry. The old point is "stale" (less accurate embedding) but not incorrect.
