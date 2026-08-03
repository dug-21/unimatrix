use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anndists::dist::DistDot;
use hnsw_rs::api::AnnT;
use hnsw_rs::hnswio;
use tracing::warn;
use unimatrix_store::SqlxStore;

use crate::config::VectorConfig;
use crate::error::{Result, VectorError};
use crate::index::VectorIndex;

const METADATA_FILENAME: &str = "unimatrix-vector.meta";
const DUMP_BASENAME: &str = "unimatrix";

impl VectorIndex {
    /// Persist the hnsw_rs index and metadata to disk.
    ///
    /// Creates `.hnsw.graph`, `.hnsw.data`, and `.meta` files in `dir`.
    /// The directory is created if it does not exist.
    ///
    /// The publish is atomic against a SIGKILL mid-dump: the graph/data files
    /// are written to sibling temp files inside `dir` and then `rename`d into
    /// place, with the `.meta` (which claims `point_count`) renamed LAST. A
    /// crash at any point therefore never leaves a `.meta` overclaiming a
    /// missing/torn graph — an observer sees either the prior dump or the new
    /// complete one.
    ///
    /// NOTE: power-loss durability (fsync of files + directory before/after
    /// rename) is intentionally OUT OF SCOPE. `rename(2)` is atomic for
    /// observers but not guaranteed durable across a power-loss without fsync;
    /// the bug this guards (GH-824, SIGKILL mid-dump) is fully covered by
    /// temp+rename alone.
    pub fn dump(&self, dir: &Path) -> Result<()> {
        // Create directory if needed
        std::fs::create_dir_all(dir).map_err(|e| {
            VectorError::Persistence(format!("failed to create directory {}: {e}", dir.display()))
        })?;

        // Dump hnsw_rs index to SIBLING temp files inside `dir` (read lock).
        // Temp files MUST live in `dir` so the rename below is intra-filesystem
        // (a cross-device rename fails with EXDEV).
        let point_count;
        let actual_basename;
        let temp_graph = dir.join(format!(".{DUMP_BASENAME}.hnsw.graph.tmp"));
        let temp_data = dir.join(format!(".{DUMP_BASENAME}.hnsw.data.tmp"));
        {
            let hnsw = self.hnsw_read();
            point_count = hnsw.get_nb_point();

            if point_count > 0 {
                // hnsw_rs writes `{basename}.hnsw.graph` / `.hnsw.data` directly;
                // it cannot target arbitrary temp names, so dump under a temp
                // basename and rename the produced files into their final names.
                let temp_basename = format!(".{DUMP_BASENAME}.tmp");
                let produced = hnsw.file_dump(dir, &temp_basename).map_err(|e| {
                    VectorError::Persistence(format!(
                        "failed to dump hnsw index to {}: {e}",
                        dir.display()
                    ))
                })?;
                // hnsw_rs may return a basename distinct from the one requested
                // (datamap_opt path); the produced files use `{produced}.hnsw.*`.
                let produced_graph = dir.join(format!("{produced}.hnsw.graph"));
                let produced_data = dir.join(format!("{produced}.hnsw.data"));
                std::fs::rename(&produced_graph, &temp_graph).map_err(|e| {
                    VectorError::Persistence(format!(
                        "failed to stage graph temp {}: {e}",
                        temp_graph.display()
                    ))
                })?;
                std::fs::rename(&produced_data, &temp_data).map_err(|e| {
                    VectorError::Persistence(format!(
                        "failed to stage data temp {}: {e}",
                        temp_data.display()
                    ))
                })?;
                // Final published basename is the stable DUMP_BASENAME.
                actual_basename = DUMP_BASENAME.to_string();
            } else {
                // hnsw_rs cannot dump an empty index (no entry point).
                // The published set is meta-only.
                actual_basename = DUMP_BASENAME.to_string();
            }
        }

        let next = self.next_data_id_value();
        let final_graph = dir.join(format!("{actual_basename}.hnsw.graph"));
        let final_data = dir.join(format!("{actual_basename}.hnsw.data"));
        let meta_path = dir.join(METADATA_FILENAME);
        let temp_meta = dir.join(format!(".{METADATA_FILENAME}.tmp"));

        let meta_content = format!(
            "basename={actual_basename}\npoint_count={point_count}\ndimension={}\nnext_data_id={next}\n",
            self.config().dimension,
        );
        std::fs::write(&temp_meta, meta_content).map_err(|e| {
            VectorError::Persistence(format!(
                "failed to write metadata temp {}: {e}",
                temp_meta.display()
            ))
        })?;

        // Publish. Graph/data first, meta LAST: a crash before the meta rename
        // leaves the prior (consistent) meta in place over the prior graph.
        if point_count > 0 {
            std::fs::rename(&temp_graph, &final_graph).map_err(|e| {
                VectorError::Persistence(format!(
                    "failed to publish graph {}: {e}",
                    final_graph.display()
                ))
            })?;
            std::fs::rename(&temp_data, &final_data).map_err(|e| {
                VectorError::Persistence(format!(
                    "failed to publish data {}: {e}",
                    final_data.display()
                ))
            })?;
        } else {
            // Empty dump (meta-only): remove any stale graph/data left by a
            // prior non-empty dump so the published set is self-consistent —
            // a `point_count=0` meta must never sit beside an orphan graph (F3).
            remove_if_exists(&final_graph)?;
            remove_if_exists(&final_data)?;
        }

        std::fs::rename(&temp_meta, &meta_path).map_err(|e| {
            VectorError::Persistence(format!(
                "failed to publish metadata {}: {e}",
                meta_path.display()
            ))
        })?;

        Ok(())
    }

    /// Load a previously dumped index from disk.
    ///
    /// Reads the metadata file, loads hnsw_rs graph and data files,
    /// and rebuilds the IdMap from VECTOR_MAP in unimatrix-store.
    pub async fn load(
        store: Arc<SqlxStore>,
        config: VectorConfig,
        dir: &Path,
    ) -> Result<VectorIndex> {
        // Read and parse metadata file
        let meta_path = dir.join(METADATA_FILENAME);
        let meta_content = std::fs::read_to_string(&meta_path).map_err(|e| {
            VectorError::Persistence(format!(
                "failed to read metadata from {}: {e}",
                meta_path.display()
            ))
        })?;

        let (basename, point_count, dimension, next_data_id) = parse_metadata(&meta_content)?;

        // Empty index: meta exists but no graph/data files were written.
        // Return a fresh index instead of failing on missing files.
        if point_count == Some(0) {
            return VectorIndex::new(store, config);
        }

        // Validate dimension
        if let Some(dim) = dimension.filter(|&d| d != config.dimension) {
            return Err(VectorError::Persistence(format!(
                "dimension mismatch: metadata says {dim}, config says {}",
                config.dimension
            )));
        }

        // Load hnsw_rs index
        let graph_path = dir.join(format!("{basename}.hnsw.graph"));
        let data_path = dir.join(format!("{basename}.hnsw.data"));

        if !graph_path.exists() {
            return Err(VectorError::Persistence(format!(
                "graph file not found: {}",
                graph_path.display()
            )));
        }
        if !data_path.exists() {
            return Err(VectorError::Persistence(format!(
                "data file not found: {}",
                data_path.display()
            )));
        }

        // Box::leak the HnswIo so the loaded Hnsw can be 'static.
        // hnsw_rs requires load_hnsw's lifetime to be tied to HnswIo ('a: 'b).
        // With default ReloadOptions (no mmap), the Hnsw doesn't actually
        // reference the HnswIo data, but the constraint is enforced statically.
        // The leaked memory is small (paths + metadata only).
        let reloader = Box::leak(Box::new(hnswio::HnswIo::new(dir, &basename)));
        let hnsw = reloader.load_hnsw::<f32, DistDot>().map_err(|e| {
            VectorError::Persistence(format!(
                "failed to load hnsw index from {}: {e}",
                dir.display()
            ))
        })?;

        // Rebuild IdMap from VECTOR_MAP — but ONLY for entries whose data_id is
        // actually present in the graph we just loaded. `vector_map` and the HNSW
        // graph are separate on-disk artifacts; a DB-only copy (unimatrix.db without
        // the HNSW dir) leaves `vector_map` recording N vectors while the loaded
        // graph holds fewer. Rebuilding the IdMap blind (GH#972) makes the
        // IdMap-based `contains()` lie — it returns true for entries with no graph
        // point, which both suppresses the capped self-heal (status.rs Sub-case B
        // keys off `!contains`) and saturates `stale_count()` to 0. Filtering to
        // graph-present data_ids makes `contains()` truthful, so the existing heal
        // repopulates the absent entries while retained mappings stay searchable.
        let mappings = store.iter_vector_mappings().await?;
        let recorded = mappings.len();

        // Enumerate origin_ids present in the loaded graph across ALL LAYERS.
        // hnsw_rs assigns each point to a single, probabilistically chosen level;
        // ~6% live at level >= 1 and appear ONLY in points_by_layer[L], not layer 0.
        // Iterate via IterPoint (IntoIterator for &PointIndexation) exactly as
        // `get_embedding` does — a `get_layer_iterator(0)` scan would wrongly drop
        // the level>=1 points and heal them needlessly. (GH#286, lesson #1712)
        let present_origin_ids: HashSet<u64> = {
            let point_indexation = hnsw.get_point_indexation();
            let mut present = HashSet::new();
            for point in point_indexation {
                present.insert(point.get_origin_id() as u64);
            }
            present
        };

        let filtered: Vec<(u64, u64)> = mappings
            .into_iter()
            .filter(|(_entry_id, data_id)| present_origin_ids.contains(data_id))
            .collect();

        let dropped = recorded - filtered.len();
        if dropped > 0 {
            // Previously invisible divergence (#5718): surface it loudly so a
            // DB-only copy is diagnosable instead of silently degrading retrieval.
            warn!(
                recorded,
                actual = filtered.len(),
                dropped,
                "vector_map records more mappings than the loaded HNSW graph holds \
                 (DB-only copy without the HNSW index dir?); dropping graph-absent \
                 mappings so contains() is truthful and the capped self-heal can repopulate"
            );
        }

        Ok(VectorIndex::from_parts(
            hnsw,
            store,
            config,
            next_data_id,
            filtered,
        ))
    }
}

/// Remove a file if present; absence is not an error. ENOENT is tolerated so
/// the atomic empty-dump cleanup is idempotent across repeated empty dumps.
fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(VectorError::Persistence(format!(
            "failed to remove stale file {}: {e}",
            path.display()
        ))),
    }
}

/// Parse the metadata file into (basename, point_count, dimension, next_data_id).
fn parse_metadata(contents: &str) -> Result<(String, Option<usize>, Option<usize>, u64)> {
    let mut basename = None;
    let mut point_count = None;
    let mut dimension = None;
    let mut next_data_id = None;

    for line in contents.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "basename" => basename = Some(value.trim().to_string()),
                "point_count" => {
                    point_count = Some(value.trim().parse::<usize>().map_err(|e| {
                        VectorError::Persistence(format!("invalid point_count: {e}"))
                    })?);
                }
                "dimension" => {
                    dimension = Some(value.trim().parse::<usize>().map_err(|e| {
                        VectorError::Persistence(format!("invalid dimension: {e}"))
                    })?);
                }
                "next_data_id" => {
                    next_data_id = Some(value.trim().parse::<u64>().map_err(|e| {
                        VectorError::Persistence(format!("invalid next_data_id: {e}"))
                    })?);
                }
                _ => {} // ignore unknown keys for forward compat
            }
        }
    }

    let basename = basename
        .ok_or_else(|| VectorError::Persistence("missing 'basename' in metadata".into()))?;
    let next_data_id = next_data_id
        .ok_or_else(|| VectorError::Persistence("missing 'next_data_id' in metadata".into()))?;

    Ok((basename, point_count, dimension, next_data_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{TestVectorIndex, random_normalized_embedding, seed_vectors};

    // -- AC-09: Dump Produces Index Files --

    #[tokio::test]
    async fn test_dump_creates_files() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 50).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        assert!(dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(dump_dir.join("unimatrix.hnsw.data").exists());
        assert!(dump_dir.join("unimatrix-vector.meta").exists());
    }

    #[tokio::test]
    async fn test_dump_metadata_content() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let meta = std::fs::read_to_string(dump_dir.join("unimatrix-vector.meta")).unwrap();
        assert!(meta.contains("basename=unimatrix"));
        assert!(meta.contains("point_count=10"));
        assert!(meta.contains("dimension=384"));
        assert!(meta.contains("next_data_id=10"));
    }

    #[tokio::test]
    async fn test_dump_empty_index() {
        let tvi = TestVectorIndex::new().await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();
        assert!(dump_dir.join("unimatrix-vector.meta").exists());
    }

    #[tokio::test]
    async fn test_load_after_empty_dump() {
        let tvi = TestVectorIndex::new().await;
        let dump_dir = tvi.dir().join("index");

        // Dump an empty index (writes .meta but no graph/data files)
        tvi.vi().dump(&dump_dir).unwrap();
        assert!(dump_dir.join("unimatrix-vector.meta").exists());
        assert!(!dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(!dump_dir.join("unimatrix.hnsw.data").exists());

        // Load should succeed — returns a fresh empty index
        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();
        assert_eq!(loaded.point_count(), 0);
    }

    // -- GH-824: Atomic dump crash-safety --

    /// After a normal non-empty dump, the published set uses the EXACT final
    /// filenames and leaves no sibling `.tmp` artifacts — the published dir is
    /// internally self-consistent (meta + graph + data, no stragglers).
    #[tokio::test]
    async fn test_dump_atomic_no_temp_artifacts() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 20).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        // Exact final names present.
        assert!(dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(dump_dir.join("unimatrix.hnsw.data").exists());
        assert!(dump_dir.join("unimatrix-vector.meta").exists());

        // No leftover temp siblings.
        for entry in std::fs::read_dir(&dump_dir).unwrap() {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.ends_with(".tmp"),
                "atomic dump left a temp artifact: {name}"
            );
        }
    }

    /// An empty (meta-only) dump that follows a prior NON-EMPTY dump must remove
    /// the stale `.hnsw.graph` / `.hnsw.data` so a `point_count=0` meta never
    /// sits beside an orphan graph (F3). The published set must always be
    /// self-consistent: a non-zero meta implies present graph+data; a zero meta
    /// implies neither.
    #[tokio::test]
    async fn test_empty_dump_removes_stale_graph_data() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 15).await;
        let dump_dir = tvi.dir().join("index");

        // First: a non-empty dump leaves graph + data on disk.
        tvi.vi().dump(&dump_dir).unwrap();
        assert!(dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(dump_dir.join("unimatrix.hnsw.data").exists());

        // Now dump a fresh EMPTY index into the same dir.
        let empty = VectorIndex::new(tvi.store().clone(), VectorConfig::default()).unwrap();
        empty.dump(&dump_dir).unwrap();

        // Meta now claims zero points...
        let meta = std::fs::read_to_string(dump_dir.join("unimatrix-vector.meta")).unwrap();
        assert!(meta.contains("point_count=0"));
        // ...and the stale graph/data are gone — no orphan beside the empty meta.
        assert!(!dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(!dump_dir.join("unimatrix.hnsw.data").exists());

        // Self-consistency: load must succeed and return an empty index, not Err.
        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();
        assert_eq!(loaded.point_count(), 0);
    }

    /// Crash-safety contract: an interrupted dump (we simulate the failure
    /// window by aborting BEFORE the meta is published) never leaves a meta
    /// overclaiming a missing/torn graph. With temp+rename the meta is the LAST
    /// thing published, so a crash before it leaves the prior good meta in place
    /// — the on-disk set load() observes is always self-consistent.
    #[tokio::test]
    async fn test_interrupted_dump_keeps_prior_consistent_set() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 12).await;
        let dump_dir = tvi.dir().join("index");

        // A first, complete dump — the "prior good" published set.
        tvi.vi().dump(&dump_dir).unwrap();
        let prior_meta = std::fs::read_to_string(dump_dir.join("unimatrix-vector.meta")).unwrap();
        assert!(prior_meta.contains("point_count=12"));

        // Simulate a crash mid-dump: a NEW staged graph temp exists but the meta
        // was never republished. The published meta still describes the prior set.
        std::fs::write(dump_dir.join(".unimatrix.hnsw.graph.tmp"), b"torn").unwrap();

        // load() reads only the PUBLISHED names and must see the prior consistent
        // set — it must NOT pick up the torn temp, and must NOT Err.
        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();
        assert_eq!(loaded.point_count(), 12);

        // The published meta still matches the prior good dump.
        let meta_now = std::fs::read_to_string(dump_dir.join("unimatrix-vector.meta")).unwrap();
        assert_eq!(meta_now, prior_meta);
    }

    // -- AC-10: Load Restores Index --

    #[tokio::test]
    async fn test_load_round_trip() {
        let tvi = TestVectorIndex::new().await;
        let _ids = seed_vectors(tvi.vi(), tvi.store(), 50).await;

        // Record search results
        let queries: Vec<Vec<f32>> = (0..5).map(|_| random_normalized_embedding(384)).collect();
        let original_results: Vec<Vec<_>> = queries
            .iter()
            .map(|q| tvi.vi().search(q, 10, 32).unwrap())
            .collect();

        // Dump
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        // Load
        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        // Verify same results
        for (query, original) in queries.iter().zip(original_results.iter()) {
            let loaded_results = loaded.search(query, 10, 32).unwrap();
            assert_eq!(loaded_results.len(), original.len());
            for (o, l) in original.iter().zip(loaded_results.iter()) {
                assert_eq!(o.entry_id, l.entry_id);
                assert!(
                    (o.similarity - l.similarity).abs() < 0.01,
                    "similarity mismatch: {} vs {}",
                    o.similarity,
                    l.similarity
                );
            }
        }
    }

    #[tokio::test]
    async fn test_load_point_count_matches() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 100).await;
        let original_count = tvi.vi().point_count();

        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();
        assert_eq!(loaded.point_count(), original_count);
    }

    #[tokio::test]
    async fn test_load_idmap_consistent() {
        let tvi = TestVectorIndex::new().await;
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        for id in &ids {
            assert!(loaded.contains(*id));
            assert!(tvi.store().get_vector_mapping(*id).await.unwrap().is_some());
        }
    }

    // -- R-04: Additional Persistence Scenarios --

    #[tokio::test]
    async fn test_load_missing_meta_file() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        std::fs::remove_file(dump_dir.join("unimatrix-vector.meta")).unwrap();

        let result =
            VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir).await;
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[tokio::test]
    async fn test_load_missing_graph_file() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        std::fs::remove_file(dump_dir.join("unimatrix.hnsw.graph")).unwrap();

        let result =
            VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir).await;
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[tokio::test]
    async fn test_load_missing_data_file() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        std::fs::remove_file(dump_dir.join("unimatrix.hnsw.data")).unwrap();

        let result =
            VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir).await;
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[tokio::test]
    async fn test_load_nonexistent_directory() {
        let tvi = TestVectorIndex::new().await;
        let dump_dir = tvi.dir().join("does_not_exist");

        let result =
            VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir).await;
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[tokio::test]
    async fn test_load_empty_directory() {
        let tvi = TestVectorIndex::new().await;
        let dump_dir = tvi.dir().join("empty_index");
        std::fs::create_dir_all(&dump_dir).unwrap();

        let result =
            VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir).await;
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[tokio::test]
    async fn test_load_dimension_mismatch() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 5).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let wrong_config = VectorConfig {
            dimension: 768,
            ..VectorConfig::default()
        };
        let result = VectorIndex::load(tvi.store().clone(), wrong_config, &dump_dir).await;
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[tokio::test]
    async fn test_multi_cycle_dump_load() {
        let tvi = TestVectorIndex::new().await;
        // Cycle 1: insert + dump + load
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();
        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        // Cycle 2: insert more + dump + load
        for i in 0..10 {
            let entry = unimatrix_store::NewEntry {
                title: format!("Cycle2 {i}"),
                content: format!("Content {i}"),
                topic: "test".to_string(),
                category: "cycle2".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: unimatrix_store::Status::Active,
                created_by: String::new(),
                feature_cycle: String::new(),
                trust_source: String::new(),
            };
            let eid = tvi.store().insert(entry).await.unwrap();
            loaded
                .insert(eid, &random_normalized_embedding(384))
                .await
                .unwrap();
        }

        loaded.dump(&dump_dir).unwrap();
        let loaded2 = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        assert_eq!(loaded2.point_count(), 20);
    }

    // -- AC-18: IdMap Consistent After Full Lifecycle --

    #[tokio::test]
    async fn test_idmap_consistency_full_lifecycle() {
        let tvi = TestVectorIndex::new().await;
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100).await;

        // Verify before dump
        for id in &ids {
            assert!(tvi.vi().contains(*id));
            assert!(tvi.store().get_vector_mapping(*id).await.unwrap().is_some());
        }

        // Dump and load
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();
        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        // Verify after load
        for id in &ids {
            assert!(loaded.contains(*id));
        }

        // Re-embed 10 entries
        for &id in ids.iter().take(10) {
            loaded
                .insert(id, &random_normalized_embedding(384))
                .await
                .unwrap();
        }

        // Verify after re-embed
        for id in &ids {
            assert!(loaded.contains(*id));
        }
    }

    // -- GH#972: Graph under-counts VECTOR_MAP (DB-only copy) --

    /// A DB-only copy (unimatrix.db without the HNSW index dir) leaves `vector_map`
    /// recording more mappings than the loaded graph holds. `load` must filter the
    /// rebuilt IdMap to graph-present data_ids so `contains()` is truthful: FALSE for
    /// graph-absent entries (letting the capped self-heal repopulate them) and TRUE
    /// for retained ones. This is the missing "graph-under-counts-DB" test.
    ///
    /// N1 GUARD: 200 seeded points make it near-certain (~1-(15/16)^200) that several
    /// land at level >= 1. Those points live ONLY in points_by_layer[L], not layer 0.
    /// Asserting ALL 200 are retained proves the load enumeration walks every layer —
    /// a layer-0-only scan would drop the level>=1 points and wrongly report them absent.
    #[tokio::test]
    async fn test_load_graph_undercounts_vector_map_filters_absent_entries() {
        let tvi = TestVectorIndex::new().await;

        // Seed 200 vectors: writes HNSW points (data_ids 0..199) AND vector_map rows.
        let present_ids = seed_vectors(tvi.vi(), tvi.store(), 200).await;

        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        // Simulate the DB-only copy: vector_map gains rows whose data_ids were never
        // written to the dumped graph (graph holds only the 200 seeded points).
        // point_count in meta is 200 (>= 1) — NOT the empty-first-boot short-circuit.
        let absent_ids: Vec<u64> = (10_001..=10_005).collect();
        for (i, &eid) in absent_ids.iter().enumerate() {
            // data_ids 1000+ are guaranteed absent from the 0..199 graph.
            tvi.store()
                .put_vector_mapping(eid, 1000 + i as u64)
                .await
                .unwrap();
        }

        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        // Graph-present entries (including any assigned level >= 1) are RETAINED.
        for &id in &present_ids {
            assert!(
                loaded.contains(id),
                "graph-present entry {id} must be retained (N1: layer-0-only scan would drop level>=1 points)"
            );
        }

        // Graph-absent entries are DROPPED — contains() is now truthfully false, so
        // the capped self-heal (status.rs Sub-case B, keyed off !contains) can repopulate.
        for &id in &absent_ids {
            assert!(
                !loaded.contains(id),
                "graph-absent entry {id} must be dropped so contains() no longer lies"
            );
        }
    }

    /// Retained (graph-present) points stay searchable after a load that dropped
    /// graph-absent mappings, and a graph-absent entry can never surface in search.
    #[tokio::test]
    async fn test_load_graph_undercounts_retained_points_searchable() {
        let tvi = TestVectorIndex::new().await;

        // Manual seed so embeddings are retained for self-search verification.
        let mut embeddings: Vec<Vec<f32>> = Vec::new();
        let mut present_ids: Vec<u64> = Vec::new();
        for i in 0..50 {
            let entry = unimatrix_store::NewEntry {
                title: format!("Undercount entry {i}"),
                content: format!("Content {i}"),
                topic: "test".to_string(),
                category: "vector".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: unimatrix_store::Status::Active,
                created_by: String::new(),
                feature_cycle: String::new(),
                trust_source: String::new(),
            };
            let eid = tvi.store().insert(entry).await.unwrap();
            let emb = random_normalized_embedding(384);
            tvi.vi().insert(eid, &emb).await.unwrap();
            embeddings.push(emb);
            present_ids.push(eid);
        }

        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        // Inject a graph-absent mapping (DB-only-copy divergence).
        let absent_id = 20_000u64;
        tvi.store()
            .put_vector_mapping(absent_id, 5000)
            .await
            .unwrap();

        let loaded = VectorIndex::load(tvi.store().clone(), VectorConfig::default(), &dump_dir)
            .await
            .unwrap();

        // Every retained point is still searchable — self-search returns its entry,
        // and the graph-absent entry never surfaces (it has no graph point).
        for (emb, &id) in embeddings.iter().zip(present_ids.iter()) {
            let results = loaded.search(emb, 1, 32).unwrap();
            assert_eq!(
                results[0].entry_id, id,
                "retained entry {id} must remain searchable after load"
            );
            assert!(
                results.iter().all(|r| r.entry_id != absent_id),
                "graph-absent entry {absent_id} must never surface in search"
            );
        }

        assert!(!loaded.contains(absent_id));
    }

    // -- IR-03: New Index with Existing VECTOR_MAP --

    #[tokio::test]
    async fn test_new_index_with_existing_vector_map() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 10).await;

        // Create fresh index with same store
        let new_vi = VectorIndex::new(tvi.store().clone(), VectorConfig::default()).unwrap();
        assert_eq!(new_vi.point_count(), 0);
        assert!(!new_vi.contains(1));

        // VECTOR_MAP still has old entries
        assert!(tvi.store().get_vector_mapping(1).await.unwrap().is_some());
    }
}
