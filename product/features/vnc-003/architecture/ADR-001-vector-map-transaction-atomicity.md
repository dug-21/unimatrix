## ADR-001: VECTOR_MAP Transaction Atomicity (GH #14 Fix)

### Context

In vnc-002, `insert_with_audit()` writes the entry + indexes + audit event in a single redb write transaction, but the VECTOR_MAP write happens separately inside `VectorIndex::insert()`, which opens its own write transaction via `Store::put_vector_mapping()`. This creates a crash-safety gap: if the process crashes after the combined transaction commits but before `VectorIndex::insert()` completes, the entry exists without a vector mapping. The entry is stored but not semantically discoverable.

GH #14 identified this gap. vnc-003 adds two mutating tools (`context_correct`, `context_store` already uses this path) that also need VECTOR_MAP writes, making the fix more urgent.

### Decision

Decouple `VectorIndex::insert()` into two operations:

1. `allocate_data_id() -> u64`: Atomically increments the internal `next_data_id` counter and returns the allocated ID. Called before the write transaction.

2. `insert_hnsw_only(entry_id, data_id, embedding) -> Result<()>`: Inserts into the HNSW in-memory index and updates the IdMap, but skips the VECTOR_MAP write. Called after the transaction commits.

The server writes `VECTOR_MAP.insert(entry_id, data_id)` inside the combined write transaction, between the entry/index writes and the audit write. This makes entry + vector mapping + audit fully atomic.

The existing `VectorIndex::insert()` method is preserved unchanged for backward compatibility (used by tests and any code path that doesn't need combined transactions).

### Consequences

**Easier:**
- Crash after commit guarantees VECTOR_MAP mapping is present -- entry is recoverable via HNSW rebuild from VECTOR_MAP
- All three mutating operations (store, correct) use the same atomic pattern
- No changes to the `VectorIndex` public API for non-server callers

**Harder:**
- Server must import `VECTOR_MAP` table definition from `unimatrix-store`
- Server needs direct `Arc<VectorIndex>` reference (not just `AsyncVectorStore`)
- `allocate_data_id()` consumes a data_id even if the transaction rolls back (acceptable: sparse IDs are harmless)
- If HNSW insert fails after commit, the VECTOR_MAP entry exists but the vector is not searchable until server restart (HNSW rebuild from VECTOR_MAP handles this)
