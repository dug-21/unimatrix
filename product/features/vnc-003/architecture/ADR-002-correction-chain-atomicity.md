## ADR-002: Correction Chain Atomicity

### Context

`context_correct` performs a two-entry operation: deprecate the original entry (set status=Deprecated, superseded_by=new_id, increment correction_count) and insert a new correction entry (set supersedes=original_id). These two changes must be atomic -- if only one side commits, the correction chain is broken (dangling superseded_by pointer or orphaned correction entry).

vnc-002 established `insert_with_audit()` as the pattern for single-entry atomic mutations. `context_correct` needs to extend this to a two-entry atomic operation.

### Decision

Add `correct_with_audit()` to `UnimatrixServer` that performs both operations in a single redb write transaction:

1. Read the original entry from ENTRIES
2. Deprecate the original: update status, set superseded_by, increment correction_count, update STATUS_INDEX + counters, serialize and overwrite
3. Insert the correction: generate ID, build record with supersedes=original_id, write ENTRIES + 5 indexes + VECTOR_MAP
4. Write audit event with target_ids=[original_id, new_id]
5. Commit

HNSW insert for the correction happens after commit (same as `insert_with_audit`).

The original entry is NOT removed from the HNSW index -- it remains as a stale point that will be filtered by the IdMap (since its data_id mapping is not removed). Over time, stale HNSW points accumulate but are harmless and invisible to search results.

### Consequences

**Easier:**
- Correction chains are always consistent: both sides or neither
- Single audit event captures the full operation
- Reuses established patterns from insert_with_audit

**Harder:**
- Longer write transaction (two entry serializations + index updates)
- Must handle the case where original_id is not found inside the transaction (return error, roll back)
- The deprecated original's embedding remains in HNSW as a stale point (acceptable: stale points are filtered by IdMap)
