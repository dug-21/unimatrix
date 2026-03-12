# nan-002: embedding-reconstruction -- Pseudocode

## Purpose

After the database transaction is committed, re-embed all imported entries using the current ONNX model and build a fresh HNSW vector index. This is Phase 2 of the two-phase import (ADR-004). Lives in `import.rs` as a private function called by `run_import()`.

## File

- `crates/unimatrix-server/src/import.rs` (same file as import-pipeline, private function)

## reconstruct_embeddings()

```
fn reconstruct_embeddings(
    store: &Arc<Store>,
    paths: &ProjectPaths,
) -> Result<(), Box<dyn std::error::Error>>

FUNCTION BODY:

    // Step 1: Initialize ONNX provider
    eprintln!("Initializing embedding model...")
    let embed_config = EmbedConfig::default()
    let provider = OnnxProvider::new(embed_config)
        .map_err(|e| format!(
            "failed to initialize ONNX embedding model: {e}. \
             Ensure the all-MiniLM-L6-v2 model is available. \
             The database has been restored but vector search will not work until re-embedding succeeds."
        ))?

    // Step 2: Read all entries from committed DB
    let conn = store.lock_conn()
    let mut stmt = conn.prepare(
        "SELECT id, title, content FROM entries ORDER BY id"
    )?
    let mut entries: Vec<(u64, String, String)> = Vec::new()
    let mut rows = stmt.query([])?
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?
        let title: String = row.get(1)?
        let content: String = row.get(2)?
        entries.push((id as u64, title, content))
    }
    drop(rows)
    drop(stmt)
    drop(conn)  // Release lock before CPU-bound embedding work

    let total = entries.len()
    if total == 0 {
        eprintln!("No entries to embed.")
        return Ok(())
    }

    // Step 3: Build VectorIndex
    let vector_config = VectorConfig::default()
    let vector_index = VectorIndex::new(Arc::clone(store), vector_config)
        .map_err(|e| format!("failed to create vector index: {e}"))?

    // Step 4: Batch embed (64 entries per batch)
    let batch_size: usize = 64
    let num_batches = (total + batch_size - 1) / batch_size  // ceiling division

    for (batch_idx, chunk) in entries.chunks(batch_size).enumerate() {
        let batch_num = batch_idx + 1
        let processed = (batch_idx * batch_size) + chunk.len()
        eprintln!("  Embedding batch {batch_num}/{num_batches} ({processed}/{total} entries)")

        // Prepare batch for embed_entries: Vec<(String, String)> of (title, content)
        let batch_input: Vec<(String, String)> = chunk.iter()
            .map(|(_, title, content)| (title.clone(), content.clone()))
            .collect()

        // Call embed_entries with separator ": " (matches server embedding)
        let embeddings = embed_entries(&provider, &batch_input, ": ")
            .map_err(|e| format!(
                "embedding failed on batch {batch_num}/{num_batches}: {e}"
            ))?

        // Insert each embedding into VectorIndex
        for (i, embedding) in embeddings.iter().enumerate() {
            let entry_id = chunk[i].0
            vector_index.insert(entry_id, embedding)
                .map_err(|e| format!(
                    "vector index insert failed for entry {entry_id}: {e}"
                ))?
        }
    }

    // Step 5: Persist HNSW index to disk
    eprintln!("  Persisting vector index to {}...", paths.vector_dir.display())

    // Ensure vector directory exists
    std::fs::create_dir_all(&paths.vector_dir)?

    vector_index.dump(&paths.vector_dir)
        .map_err(|e| format!("failed to persist vector index: {e}"))?

    eprintln!("  Embedded and indexed {} entries.", total)

    Ok(())
```

## Data Flow

```
Committed DB (entries table)
  |
  +-- SELECT id, title, content FROM entries
  |
  v
Vec<(u64, String, String)>    -- (id, title, content) triples
  |
  +-- chunk into batches of 64
  |
  v
embed_entries(&provider, batch, ": ")
  |
  v
Vec<Vec<f32>>                  -- 384-dim embeddings per entry
  |
  +-- VectorIndex::insert(entry_id, &embedding)
  |
  v
VectorIndex (in-memory HNSW)
  |
  +-- VectorIndex::dump(&vector_dir)
  |
  v
Persisted HNSW files in vector/ directory
```

## Error Handling

| Error | When | Impact | Message |
|---|---|---|---|
| OnnxProvider::new fails | Step 1 | DB committed, no vectors | "failed to initialize ONNX... database has been restored but vector search will not work" |
| embed_entries fails | Step 4 | DB committed, partial vectors (not persisted) | "embedding failed on batch N/M: {error}" |
| VectorIndex::insert fails | Step 4 | DB committed, partial vectors | "vector index insert failed for entry {id}: {error}" |
| VectorIndex::dump fails | Step 5 | DB committed, vectors in memory but not on disk | "failed to persist vector index: {error}" |

All errors occur after DB commit. The database is fully restored and usable for non-search operations regardless of embedding outcome. This is by design (ADR-004).

## Interaction with Existing APIs

| API | Crate | Signature | Notes |
|---|---|---|---|
| `OnnxProvider::new` | unimatrix-embed | `fn new(config: EmbedConfig) -> Result<Self>` | May download ~80MB model on first use |
| `embed_entries` | unimatrix-embed | `fn embed_entries(provider: &dyn EmbeddingProvider, entries: &[(String, String)], separator: &str) -> Result<Vec<Vec<f32>>>` | Separator ": " matches server usage |
| `VectorIndex::new` | unimatrix-vector | `fn new(store: Arc<Store>, config: VectorConfig) -> Result<Self>` | Creates empty index |
| `VectorIndex::insert` | unimatrix-vector | `fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()>` | Incremental HNSW construction |
| `VectorIndex::dump` | unimatrix-vector | `fn dump(&self, dir: &Path) -> Result<()>` | Persists HNSW to disk |

## Memory Considerations

- Entries are loaded into memory as `Vec<(u64, String, String)>`. For 500 entries with average 1KB content, this is ~500KB.
- Embeddings are processed one batch at a time (64 entries). Each batch produces 64 * 384 * 4 bytes = ~98KB of f32 vectors.
- HNSW index grows incrementally via `VectorIndex::insert`. Memory is proportional to total entries.
- The connection lock is released before embedding to avoid holding the mutex during CPU-bound work.

## Key Test Scenarios

1. After successful import, `VectorIndex::dump` creates files in `vector/` directory.
2. After successful import, semantic search via VectorIndex returns results for imported entries.
3. Batch boundary: exactly 64 entries produces 1 batch. 65 entries produces 2 batches.
4. Empty database (0 entries): embedding phase completes immediately with "No entries to embed" message.
5. ONNX model failure: DB is committed and entries are queryable by ID, but exit code is non-zero.
6. Progress messages to stderr show batch N/M format.
7. 500-entry import completes re-embedding in under 60 seconds (NFR-01).
8. Embeddings produce 384-dim vectors (matching the model output dimension).
