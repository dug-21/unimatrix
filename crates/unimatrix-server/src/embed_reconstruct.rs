//! Post-commit embedding reconstruction for import pipeline (nan-002).
//!
//! After the database transaction is committed, re-embeds all imported entries
//! using the current ONNX model and builds a fresh HNSW vector index.
//! This is Phase 2 of the two-phase import (ADR-004).

use std::path::Path;
use std::sync::Arc;

use unimatrix_core::Store;
use unimatrix_embed::{EmbedConfig, OnnxProvider, embed_entries};
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
            block_sync_raw(vector_index.insert(entry_id, embedding))
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
/// Bridges async sqlx into this sync context. Works whether called from within
/// or outside an async runtime (nxs-011).
fn read_entries(store: &Arc<Store>) -> Result<Vec<EntryTriple>, Box<dyn std::error::Error>> {
    use sqlx::Row;
    let pool = store.write_pool_server();
    let rows = block_sync_raw(
        sqlx::query("SELECT id, title, content FROM entries ORDER BY id").fetch_all(pool),
    )
    .map_err(|e| format!("failed to read entries: {e}"))?;

    let entries = rows
        .into_iter()
        .map(|row| {
            let id: i64 = row.get::<i64, _>(0);
            let title: String = row.get::<String, _>(1);
            let content: String = row.get::<String, _>(2);
            (id as u64, title, content)
        })
        .collect();
    Ok(entries)
}

/// Bridge an async future to sync context (works inside or outside a tokio runtime).
fn block_sync_raw<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            // Must be multi_thread: block_in_place (used by callers of this
            // function in the Ok arm) requires a multi-thread runtime. If
            // block_sync_raw is ever called standalone without an ambient
            // runtime, new_current_thread would cause the same panic (GH#554).
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(fut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_core::NewEntry;
    use unimatrix_store::test_helpers::open_test_store;

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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_entries_empty_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(open_test_store(&tmp).await);

        let entries = read_entries(&store).unwrap();
        assert!(entries.is_empty());
    }

    fn make_test_entry(title: &str, content: &str) -> NewEntry {
        NewEntry {
            title: title.to_string(),
            content: content.to_string(),
            topic: "test-topic".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "human".to_string(),
            status: unimatrix_core::Status::Active,
            created_by: "system".to_string(),
            feature_cycle: String::new(),
            trust_source: "direct".to_string(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_entries_returns_id_title_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(open_test_store(&tmp).await);

        store
            .insert(make_test_entry("Test Title", "Test Content"))
            .await
            .unwrap();

        let entries = read_entries(&store).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, "Test Title");
        assert_eq!(entries[0].2, "Test Content");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_entries_ordered_by_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(open_test_store(&tmp).await);

        // Insert 3 entries — IDs will be assigned in insertion order (1, 2, 3)
        store
            .insert(make_test_entry("Title A", "Content A"))
            .await
            .unwrap();
        store
            .insert(make_test_entry("Title B", "Content B"))
            .await
            .unwrap();
        store
            .insert(make_test_entry("Title C", "Content C"))
            .await
            .unwrap();

        let entries = read_entries(&store).unwrap();
        assert_eq!(entries.len(), 3);
        // IDs must be in ascending order
        assert!(entries[0].0 < entries[1].0);
        assert!(entries[1].0 < entries[2].0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_entries_multiple_entries() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(open_test_store(&tmp).await);

        for i in 1..=5u64 {
            store
                .insert(make_test_entry(
                    &format!("Entry {i}"),
                    &format!("Content for entry {i}"),
                ))
                .await
                .unwrap();
        }

        let entries = read_entries(&store).unwrap();
        assert_eq!(entries.len(), 5);

        for (i, (_, title, content)) in entries.iter().enumerate() {
            let expected_id = (i + 1) as u64;
            assert_eq!(*title, format!("Entry {expected_id}"));
            assert_eq!(*content, format!("Content for entry {expected_id}"));
        }
    }
}
