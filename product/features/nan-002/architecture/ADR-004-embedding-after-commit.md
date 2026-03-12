## ADR-004: Re-Embedding After DB Commit, Not Inside Transaction

### Context

Import must re-embed all entries using the current ONNX model (AC-10). Re-embedding 500 entries takes up to 60 seconds of CPU-bound work (AC-17). Two placement options:

1. **Inside the transaction**: Embed before COMMIT. If embedding fails, the transaction rolls back and no partial state exists. But the write lock is held for the entire embedding duration, blocking any concurrent database access.

2. **After the transaction**: COMMIT the data first, then embed. If embedding fails, the database has all entries but no vector index. The server can start but semantic search is unavailable until re-import or manual re-embedding.

### Decision

Re-embed after committing the database transaction. The pipeline is:

1. BEGIN IMMEDIATE transaction
2. Insert all JSONL data into tables
3. Validate hash chains
4. COMMIT transaction
5. Initialize OnnxProvider (may download model)
6. Read entries from committed database
7. Batch embed and build VectorIndex
8. VectorIndex::dump() to persist HNSW index
9. Record import provenance in audit log

If step 5-8 fails, the database is fully restored and usable for non-search operations (lookup by ID, status queries, audit log queries). The user can re-run `import --force` to retry embedding, or start the server which will re-embed entries on startup via the background embedding task.

### Consequences

- **Easier**: Transaction duration is bounded by I/O (JSONL parsing + SQL inserts), not CPU (ONNX inference). Write lock is held for seconds, not minutes.
- **Easier**: Partial success is useful -- a fully restored database without search is better than nothing.
- **Easier**: ONNX model download failure (SR-01, air-gapped environments) does not lose the imported data.
- **Harder**: If embedding fails, the database exists without a vector index. The server will attempt background re-embedding on startup, but this is not guaranteed to cover all entries immediately.
- **Harder**: The import is no longer fully atomic -- database commit and vector index are separate success/failure points. The summary output must clearly indicate which phase succeeded and which failed.
