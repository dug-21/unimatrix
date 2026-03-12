//! Post-commit embedding reconstruction for import pipeline (nan-002).
//!
//! After the database transaction is committed, re-embeds all imported entries
//! using the current ONNX model and builds a fresh HNSW vector index.
//! This is Phase 2 of the two-phase import (ADR-004).

use std::path::Path;
use std::sync::Arc;

use unimatrix_embed::{EmbedConfig, OnnxProvider, embed_entries};
use unimatrix_store::Store;
use unimatrix_vector::{VectorConfig, VectorIndex};

/// Batch size for embedding entries. Each batch produces
/// batch_size * 384 * 4 bytes (~98KB) of f32 vectors.
const EMBED_BATCH_SIZE: usize = 64;

/// An entry triple: (entry_id, title, content).
type EntryTriple = (u64, String, String);

/// Re-embed all entries in the database and build a fresh HNSW vector index.
///
/// Reads all entries from the committed database, embeds them in batches of 64
/// using the ONNX model, inserts each embedding into a new `VectorIndex`, and
/// persists the index to the `vector_dir` on disk.
///
/// # Errors
///
/// Returns an error if:
/// - The ONNX model cannot be initialized (model unavailable)
/// - Embedding fails for any batch
/// - Vector index insertion fails
/// - Vector index persistence (dump) fails
///
/// All errors occur after the DB commit. The database is fully restored and
/// usable for non-search operations regardless of embedding outcome (ADR-004).
pub fn reconstruct_embeddings(
    store: &Arc<Store>,
    vector_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Initialize ONNX provider
    eprintln!("Initializing embedding model...");
    let embed_config = EmbedConfig::default();
    let provider = OnnxProvider::new(embed_config).map_err(|e| {
        format!(
            "failed to initialize ONNX embedding model: {e}. \
             Ensure the all-MiniLM-L6-v2 model is available. \
             The database has been restored but vector search will not work \
             until re-embedding succeeds."
        )
    })?;

    // Step 2: Read all entries from committed DB
    let entries = read_entries(store)?;

    let total = entries.len();
    if total == 0 {
        eprintln!("No entries to embed.");
        return Ok(());
    }

    // Step 3: Build VectorIndex
    let vector_config = VectorConfig::default();
    let vector_index = VectorIndex::new(Arc::clone(store), vector_config)
        .map_err(|e| format!("failed to create vector index: {e}"))?;

    // Step 4: Batch embed (64 entries per batch)
    let num_batches = total.div_ceil(EMBED_BATCH_SIZE);

    for (batch_idx, chunk) in entries.chunks(EMBED_BATCH_SIZE).enumerate() {
        let batch_num = batch_idx + 1;
        let processed = (batch_idx * EMBED_BATCH_SIZE) + chunk.len();
        eprintln!(
            "  Embedding batch {batch_num}/{num_batches} \
             ({processed}/{total} entries)"
        );

        // Prepare batch for embed_entries: Vec<(String, String)> of (title, content)
        let batch_input: Vec<(String, String)> = chunk
            .iter()
            .map(|(_, title, content)| (title.clone(), content.clone()))
            .collect();

        // Call embed_entries with separator ": " (matches server embedding)
        let embeddings = embed_entries(&provider, &batch_input, ": ")
            .map_err(|e| format!("embedding failed on batch {batch_num}/{num_batches}: {e}"))?;

        // Insert each embedding into VectorIndex
        for (i, embedding) in embeddings.iter().enumerate() {
            let entry_id = chunk[i].0;
            vector_index
                .insert(entry_id, embedding)
                .map_err(|e| format!("vector index insert failed for entry {entry_id}: {e}"))?;
        }
    }

    // Step 5: Persist HNSW index to disk
    eprintln!("  Persisting vector index to {}...", vector_dir.display());

    std::fs::create_dir_all(vector_dir)?;

    vector_index
        .dump(vector_dir)
        .map_err(|e| format!("failed to persist vector index: {e}"))?;

    eprintln!("  Embedded and indexed {total} entries.");

    Ok(())
}

/// Read all entry (id, title, content) triples from the database.
///
/// Releases the connection lock before returning so that the caller
/// can proceed with CPU-bound embedding work without holding the mutex.
fn read_entries(store: &Arc<Store>) -> Result<Vec<EntryTriple>, Box<dyn std::error::Error>> {
    let conn = store.lock_conn();
    let mut stmt = conn.prepare("SELECT id, title, content FROM entries ORDER BY id")?;
    let mut entries: Vec<EntryTriple> = Vec::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let title: String = row.get(1)?;
        let content: String = row.get(2)?;
        entries.push((id as u64, title, content));
    }
    // rows, stmt, conn drop here -- releasing the lock before CPU-bound work
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_batch_size_constant() {
        assert_eq!(EMBED_BATCH_SIZE, 64);
    }

    #[test]
    fn test_batch_count_calculation() {
        // Verify the div_ceil behavior used in reconstruct_embeddings
        let batch_size = EMBED_BATCH_SIZE;

        // 0 entries -> 0 batches
        assert_eq!(0usize.div_ceil(batch_size), 0);

        // 1 entry -> 1 batch
        assert_eq!(1usize.div_ceil(batch_size), 1);

        // 64 entries -> 1 batch (exact boundary)
        assert_eq!(64usize.div_ceil(batch_size), 1);

        // 65 entries -> 2 batches (one overflow)
        assert_eq!(65usize.div_ceil(batch_size), 2);

        // 128 entries -> 2 batches (exact double)
        assert_eq!(128usize.div_ceil(batch_size), 2);

        // 129 entries -> 3 batches
        assert_eq!(129usize.div_ceil(batch_size), 3);
    }

    #[test]
    fn test_read_entries_empty_db() {
        // Create an in-memory Store with empty entries table
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("unimatrix.db");
        let store = Arc::new(Store::open(&db_path).unwrap());

        let entries = read_entries(&store).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_entries_returns_id_title_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("unimatrix.db");
        let store = Arc::new(Store::open(&db_path).unwrap());

        // Insert a test entry via direct SQL
        {
            let conn = store.lock_conn();
            conn.execute(
                "INSERT INTO entries (id, title, content, topic, category, source, \
                 status, confidence, created_at, updated_at, last_accessed_at, \
                 access_count, correction_count, embedding_dim, created_by, \
                 modified_by, content_hash, previous_hash, version, \
                 feature_cycle, trust_source, helpful_count, unhelpful_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                 ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                unimatrix_store::rusqlite::params![
                    1i64,
                    "Test Title",
                    "Test Content",
                    "test-topic",
                    "convention",
                    "human",
                    0i64, // Active
                    0.5f64,
                    1000i64,
                    1000i64,
                    1000i64,
                    0i64,
                    0i64,
                    384i64,
                    "system",
                    "system",
                    "hash123",
                    "",
                    1i64,
                    "",
                    "direct",
                    0i64,
                    0i64,
                ],
            )
            .unwrap();
        }

        let entries = read_entries(&store).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 1u64);
        assert_eq!(entries[0].1, "Test Title");
        assert_eq!(entries[0].2, "Test Content");
    }

    #[test]
    fn test_read_entries_ordered_by_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("unimatrix.db");
        let store = Arc::new(Store::open(&db_path).unwrap());

        // Insert entries out of order
        {
            let conn = store.lock_conn();
            for id in [3i64, 1, 2] {
                conn.execute(
                    "INSERT INTO entries (id, title, content, topic, category, source, \
                     status, confidence, created_at, updated_at, last_accessed_at, \
                     access_count, correction_count, embedding_dim, created_by, \
                     modified_by, content_hash, previous_hash, version, \
                     feature_cycle, trust_source, helpful_count, unhelpful_count) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                     ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                    unimatrix_store::rusqlite::params![
                        id,
                        format!("Title {id}"),
                        format!("Content {id}"),
                        "topic",
                        "convention",
                        "human",
                        0i64,
                        0.5f64,
                        1000i64,
                        1000i64,
                        1000i64,
                        0i64,
                        0i64,
                        384i64,
                        "system",
                        "system",
                        format!("hash{id}"),
                        "",
                        1i64,
                        "",
                        "direct",
                        0i64,
                        0i64,
                    ],
                )
                .unwrap();
            }
        }

        let entries = read_entries(&store).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, 1);
        assert_eq!(entries[1].0, 2);
        assert_eq!(entries[2].0, 3);
    }

    #[test]
    fn test_read_entries_multiple_entries() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("unimatrix.db");
        let store = Arc::new(Store::open(&db_path).unwrap());

        // Insert multiple entries to verify all fields are read correctly
        {
            let conn = store.lock_conn();
            for id in 1..=5i64 {
                conn.execute(
                    "INSERT INTO entries (id, title, content, topic, category, source, \
                     status, confidence, created_at, updated_at, last_accessed_at, \
                     access_count, correction_count, embedding_dim, created_by, \
                     modified_by, content_hash, previous_hash, version, \
                     feature_cycle, trust_source, helpful_count, unhelpful_count) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                     ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                    unimatrix_store::rusqlite::params![
                        id,
                        format!("Entry {id}"),
                        format!("Content for entry {id}"),
                        "topic",
                        "convention",
                        "human",
                        0i64,
                        0.5f64,
                        1000i64,
                        1000i64,
                        1000i64,
                        0i64,
                        0i64,
                        384i64,
                        "system",
                        "system",
                        format!("hash{id}"),
                        "",
                        1i64,
                        "",
                        "direct",
                        0i64,
                        0i64,
                    ],
                )
                .unwrap();
            }
        }

        let entries = read_entries(&store).unwrap();
        assert_eq!(entries.len(), 5);

        // Verify each entry has correct id, title, and content
        for (i, (id, title, content)) in entries.iter().enumerate() {
            let expected_id = (i + 1) as u64;
            assert_eq!(*id, expected_id);
            assert_eq!(*title, format!("Entry {expected_id}"));
            assert_eq!(*content, format!("Content for entry {expected_id}"));
        }
    }
}
