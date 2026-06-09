//! Embed-at-load for the fixture corpus (nan-018, ADR-002 branch (b)).
//!
//! The Wave-1 loader ([`super::loader::load_fixture_corpus`]) materializes the
//! snapshot DB *only* — it writes no vectors, so `EvalServiceLayer::from_profile`
//! falls back to an empty vector index and search returns nothing. That keeps the
//! loader's own unit tests model-free, but it makes the AC-14 trust assertions
//! VACUOUS (every result set is empty).
//!
//! ADR-002 chose **branch (b): embed-at-load, NO frozen vector sidecar** — the
//! embedding model identity is a first-class input to the retrieval-shape hash, so
//! re-embedding from a live model is safe (drift trips the guard). This module is
//! that embed-at-load step: it embeds each materialized fixture entry with a
//! caller-supplied [`EmbeddingProvider`] and dumps an HNSW [`VectorIndex`] into the
//! snapshot's sibling `vector/` directory — exactly where `from_profile` Step 5
//! looks. After this runs over a loaded corpus, search returns a ranked,
//! NON-EMPTY result set so ≥1 trust assertion is evaluated meaningfully (R-15).
//!
//! ## Model-free path stays intact
//!
//! The provider is **injected**, never constructed here. Production passes the
//! real ONNX provider; unit/smoke tests pass a deterministic in-memory provider.
//! Callers that want the model-free loader keep calling
//! [`super::loader::load_fixture_corpus`] unchanged — embedding is strictly opt-in
//! via [`embed_and_write_vectors`] / [`load_fixture_corpus_with_embeddings`].

use std::path::Path;
use std::sync::Arc;

use unimatrix_core::{VectorConfig, VectorIndex};
use unimatrix_embed::{EmbeddingProvider, prepare_text};
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

use super::loader::{CorpusError, LoadedCorpus, load_fixture_corpus};

/// Title/content separator used when preparing entry text for embedding.
///
/// Mirrors the server's normal entry-embedding separator so fixture vectors are
/// produced the same way live entries are.
const EMBED_SEPARATOR: &str = ": ";

/// One materialized entry's id + searchable text, read back from the snapshot DB.
#[derive(Debug, Clone)]
struct EmbeddableEntry {
    id: u64,
    title: String,
    content: String,
}

/// Load the fixture corpus AND embed it so the snapshot is end-to-end searchable.
///
/// Convenience wrapper: runs the model-free [`load_fixture_corpus`] then
/// [`embed_and_write_vectors`] over the materialized DB. The returned
/// [`LoadedCorpus`] is unchanged in shape; the difference is on disk — a populated
/// `vector/` directory beside the snapshot DB that `from_profile` will load.
pub async fn load_fixture_corpus_with_embeddings(
    dir: &Path,
    target_db: &Path,
    provider: &dyn EmbeddingProvider,
) -> Result<LoadedCorpus, CorpusError> {
    let corpus = load_fixture_corpus(dir, target_db).await?;
    embed_and_write_vectors(&corpus.db_path, provider).await?;
    Ok(corpus)
}

/// Embed every entry in the materialized snapshot at `db_path` and dump an HNSW
/// index into the sibling `vector/` directory (`{db_parent}/vector/`).
///
/// This is the ADR-002 branch (b) step. It:
/// 1. reopens the snapshot store read/write (it is a throwaway snapshot, never the
///    live DB — the live-DB guard lives in `from_profile`, not here),
/// 2. reads back each entry's id + title + content,
/// 3. embeds `prepare_text(title, content)` with the injected `provider`,
/// 4. inserts each vector into a fresh [`VectorIndex`] (which also writes the
///    `VECTOR_MAP` rows the loaded index needs),
/// 5. dumps the index to `{db_parent}/vector/` so `from_profile` Step 5 loads it
///    instead of falling back to an empty index.
///
/// The provider's dimension must match [`VectorConfig::default`]'s `dimension`
/// (384 for every catalog model); a mismatch surfaces as a `Materialize` error
/// from `VectorIndex::insert` rather than a panic.
pub async fn embed_and_write_vectors(
    db_path: &Path,
    provider: &dyn EmbeddingProvider,
) -> Result<(), CorpusError> {
    let db_parent = db_path.parent().ok_or_else(|| CorpusError::Materialize {
        reason: "snapshot db path has no parent directory".to_string(),
    })?;
    let vector_dir = db_parent.join("vector");

    // 1. Reopen the snapshot store (read/write — same DB the loader wrote).
    let store = SqlxStore::open(db_path, PoolConfig::default())
        .await
        .map_err(|e| CorpusError::Materialize {
            reason: format!("reopen snapshot for embed: {e}"),
        })?;
    let store_arc = Arc::new(store);

    // 2. Read back the entries to embed.
    let entries = read_embeddable_entries(&store_arc).await?;
    if entries.is_empty() {
        // Nothing to embed — leave no vector dir; from_profile falls back to empty.
        return Ok(());
    }

    // 3-4. Build the index and insert one vector per entry.
    let config = VectorConfig::default();
    let index =
        VectorIndex::new(Arc::clone(&store_arc), config).map_err(|e| CorpusError::Materialize {
            reason: format!("build vector index: {e}"),
        })?;

    for entry in &entries {
        let text = prepare_text(&entry.title, &entry.content, EMBED_SEPARATOR);
        let embedding = provider
            .embed(&text)
            .map_err(|e| CorpusError::Materialize {
                reason: format!("embed entry {}: {e}", entry.id),
            })?;
        index
            .insert(entry.id, &embedding)
            .await
            .map_err(|e| CorpusError::Materialize {
                reason: format!("insert vector for entry {}: {e}", entry.id),
            })?;
    }

    // 5. Dump to the sibling vector dir so from_profile loads it (GH-323 path).
    index
        .dump(&vector_dir)
        .map_err(|e| CorpusError::Materialize {
            reason: format!("dump vector index: {e}"),
        })?;

    Ok(())
}

/// Read back `(id, title, content)` for every entry in the snapshot.
async fn read_embeddable_entries(
    store: &Arc<SqlxStore>,
) -> Result<Vec<EmbeddableEntry>, CorpusError> {
    let pool = store.write_pool_server();
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, title, content FROM entries ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| CorpusError::Materialize {
        reason: format!("read entries for embed: {e}"),
    })?;

    Ok(rows
        .into_iter()
        .map(|(id, title, content)| EmbeddableEntry {
            id: id as u64,
            title,
            content,
        })
        .collect())
}
