# ADR-002: Re-embed from Text for Contradiction Scanning

## Status

Accepted

## Context

Contradiction detection needs each entry's embedding to search HNSW for similar entries. Two approaches:

**Option A: Retrieve stored embeddings from hnsw_rs**
- `hnsw_rs::get_point_data(PointId)` can retrieve stored embeddings
- But our `VectorIndex` maps entry_id -> data_id, not data_id -> PointId
- PointId is `(layer: u8, position: i32)` -- assigned internally by hnsw_rs during insertion
- We would need to track PointIds during insertion or iterate all points to find the mapping
- Adding PointId tracking would modify the VectorIndex insert path and require persisting the mapping

**Option B: Re-embed from entry text**
- Call `EmbedService::embed_entry(title, content)` for each active entry
- This is the same call used during `context_store` and `context_correct`
- The re-computed embedding should match the stored embedding (deterministic model)
- As a side effect, this validates embedding consistency (if the re-computed embedding does NOT match, we detect it)

## Decision

Option B: Re-embed from text for all scanning operations.

## Rationale

1. **Simpler architecture**: No changes to VectorIndex or its ID mapping. No new persistence requirements.
2. **Dual purpose**: Re-embedding simultaneously serves contradiction detection (find similar entries) AND embedding consistency checking (verify stored embedding matches re-computed one). Two features for the price of one operation.
3. **Deterministic model guarantee**: The ONNX embedding model is deterministic given the same input. Re-embedding should produce the same vector. If it does not, that itself is a finding worth surfacing.
4. **Acceptable cost**: At 100-2000 active entries, embedding generation at ~5ms/entry = 0.5-10 seconds. This runs during `context_status`, not on every request.
5. **No VectorIndex API changes**: Avoids modifying a core component that multiple features depend on.

## Consequences

- Contradiction scanning requires the embed service to be ready (not just the HNSW index)
- Scanning is slower than retrieving stored embeddings (embed generation vs memory read)
- The embedding consistency check becomes a natural byproduct of the scanning process
- If the embedding model is changed (different model loaded), all entries would show as "inconsistent" -- this is actually correct behavior (the stored embeddings no longer match the current model)
