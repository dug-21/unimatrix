use std::path::Path;
use std::sync::Arc;

use anndists::dist::DistDot;
use hnsw_rs::api::AnnT;
use hnsw_rs::hnswio;
use unimatrix_store::Store;

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
    pub fn dump(&self, dir: &Path) -> Result<()> {
        // Create directory if needed
        std::fs::create_dir_all(dir).map_err(|e| {
            VectorError::Persistence(format!(
                "failed to create directory {}: {e}",
                dir.display()
            ))
        })?;

        // Dump hnsw_rs index (read lock)
        let point_count;
        let actual_basename;
        {
            let hnsw = self.hnsw_read();
            point_count = hnsw.get_nb_point();

            if point_count > 0 {
                actual_basename = hnsw.file_dump(dir, DUMP_BASENAME).map_err(|e| {
                    VectorError::Persistence(format!(
                        "failed to dump hnsw index to {}: {e}",
                        dir.display()
                    ))
                })?;
            } else {
                // hnsw_rs cannot dump an empty index (no entry point).
                // Write empty placeholder files so load detects "empty" cleanly.
                actual_basename = DUMP_BASENAME.to_string();
            }
        }

        // Write metadata file with the actual basename used by file_dump.
        // hnsw_rs may generate a unique basename when datamap_opt is set
        // (true after load_hnsw, even without mmap).
        let next = self.next_data_id_value();
        let meta_path = dir.join(METADATA_FILENAME);
        let meta_content = format!(
            "basename={actual_basename}\npoint_count={point_count}\ndimension={}\nnext_data_id={next}\n",
            self.config().dimension,
        );

        std::fs::write(&meta_path, meta_content).map_err(|e| {
            VectorError::Persistence(format!(
                "failed to write metadata to {}: {e}",
                meta_path.display()
            ))
        })?;

        Ok(())
    }

    /// Load a previously dumped index from disk.
    ///
    /// Reads the metadata file, loads hnsw_rs graph and data files,
    /// and rebuilds the IdMap from VECTOR_MAP in unimatrix-store.
    pub fn load(
        store: Arc<Store>,
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

        let (basename, point_count, dimension, next_data_id) =
            parse_metadata(&meta_content)?;

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

        // Rebuild IdMap from VECTOR_MAP
        let mappings = store.iter_vector_mappings()?;

        Ok(VectorIndex::from_parts(
            hnsw,
            store,
            config,
            next_data_id,
            mappings,
        ))
    }
}

/// Parse the metadata file into (basename, point_count, dimension, next_data_id).
fn parse_metadata(
    contents: &str,
) -> Result<(String, Option<usize>, Option<usize>, u64)> {
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
    let next_data_id = next_data_id.ok_or_else(|| {
        VectorError::Persistence("missing 'next_data_id' in metadata".into())
    })?;

    Ok((basename, point_count, dimension, next_data_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        random_normalized_embedding, seed_vectors, TestVectorIndex,
    };

    // -- AC-09: Dump Produces Index Files --

    #[test]
    fn test_dump_creates_files() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 50);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        assert!(dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(dump_dir.join("unimatrix.hnsw.data").exists());
        assert!(dump_dir.join("unimatrix-vector.meta").exists());
    }

    #[test]
    fn test_dump_metadata_content() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 10);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let meta =
            std::fs::read_to_string(dump_dir.join("unimatrix-vector.meta")).unwrap();
        assert!(meta.contains("basename=unimatrix"));
        assert!(meta.contains("point_count=10"));
        assert!(meta.contains("dimension=384"));
        assert!(meta.contains("next_data_id=10"));
    }

    #[test]
    fn test_dump_empty_index() {
        let tvi = TestVectorIndex::new();
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();
        assert!(dump_dir.join("unimatrix-vector.meta").exists());
    }

    #[test]
    fn test_load_after_empty_dump() {
        let tvi = TestVectorIndex::new();
        let dump_dir = tvi.dir().join("index");

        // Dump an empty index (writes .meta but no graph/data files)
        tvi.vi().dump(&dump_dir).unwrap();
        assert!(dump_dir.join("unimatrix-vector.meta").exists());
        assert!(!dump_dir.join("unimatrix.hnsw.graph").exists());
        assert!(!dump_dir.join("unimatrix.hnsw.data").exists());

        // Load should succeed — returns a fresh empty index
        let loaded = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
        .unwrap();
        assert_eq!(loaded.point_count(), 0);
    }

    // -- AC-10: Load Restores Index --

    #[test]
    fn test_load_round_trip() {
        let tvi = TestVectorIndex::new();
        let _ids = seed_vectors(tvi.vi(), tvi.store(), 50);

        // Record search results
        let queries: Vec<Vec<f32>> =
            (0..5).map(|_| random_normalized_embedding(384)).collect();
        let original_results: Vec<Vec<_>> = queries
            .iter()
            .map(|q| tvi.vi().search(q, 10, 32).unwrap())
            .collect();

        // Dump
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        // Load
        let loaded = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
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

    #[test]
    fn test_load_point_count_matches() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 100);
        let original_count = tvi.vi().point_count();

        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let loaded = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
        .unwrap();
        assert_eq!(loaded.point_count(), original_count);
    }

    #[test]
    fn test_load_idmap_consistent() {
        let tvi = TestVectorIndex::new();
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let loaded = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
        .unwrap();

        for id in &ids {
            assert!(loaded.contains(*id));
            assert!(tvi.store().get_vector_mapping(*id).unwrap().is_some());
        }
    }

    // -- R-04: Additional Persistence Scenarios --

    #[test]
    fn test_load_missing_meta_file() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 10);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        std::fs::remove_file(dump_dir.join("unimatrix-vector.meta")).unwrap();

        let result = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        );
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[test]
    fn test_load_missing_graph_file() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 10);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        std::fs::remove_file(dump_dir.join("unimatrix.hnsw.graph")).unwrap();

        let result = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        );
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[test]
    fn test_load_missing_data_file() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 10);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        std::fs::remove_file(dump_dir.join("unimatrix.hnsw.data")).unwrap();

        let result = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        );
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[test]
    fn test_load_nonexistent_directory() {
        let tvi = TestVectorIndex::new();
        let dump_dir = tvi.dir().join("does_not_exist");

        let result = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        );
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[test]
    fn test_load_empty_directory() {
        let tvi = TestVectorIndex::new();
        let dump_dir = tvi.dir().join("empty_index");
        std::fs::create_dir_all(&dump_dir).unwrap();

        let result = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        );
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[test]
    fn test_load_dimension_mismatch() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 5);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();

        let wrong_config = VectorConfig {
            dimension: 768,
            ..VectorConfig::default()
        };
        let result = VectorIndex::load(tvi.store().clone(), wrong_config, &dump_dir);
        assert!(matches!(result, Err(VectorError::Persistence(_))));
    }

    #[test]
    fn test_multi_cycle_dump_load() {
        let tvi = TestVectorIndex::new();
        // Cycle 1: insert + dump + load
        seed_vectors(tvi.vi(), tvi.store(), 10);
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();
        let loaded = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
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
            let eid = tvi.store().insert(entry).unwrap();
            loaded
                .insert(eid, &random_normalized_embedding(384))
                .unwrap();
        }

        loaded.dump(&dump_dir).unwrap();
        let loaded2 = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
        .unwrap();

        assert_eq!(loaded2.point_count(), 20);
    }

    // -- AC-18: IdMap Consistent After Full Lifecycle --

    #[test]
    fn test_idmap_consistency_full_lifecycle() {
        let tvi = TestVectorIndex::new();
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100);

        // Verify before dump
        for id in &ids {
            assert!(tvi.vi().contains(*id));
            assert!(tvi.store().get_vector_mapping(*id).unwrap().is_some());
        }

        // Dump and load
        let dump_dir = tvi.dir().join("index");
        tvi.vi().dump(&dump_dir).unwrap();
        let loaded = VectorIndex::load(
            tvi.store().clone(),
            VectorConfig::default(),
            &dump_dir,
        )
        .unwrap();

        // Verify after load
        for id in &ids {
            assert!(loaded.contains(*id));
        }

        // Re-embed 10 entries
        for i in 0..10 {
            loaded
                .insert(ids[i], &random_normalized_embedding(384))
                .unwrap();
        }

        // Verify after re-embed
        for id in &ids {
            assert!(loaded.contains(*id));
        }
    }

    // -- IR-03: New Index with Existing VECTOR_MAP --

    #[test]
    fn test_new_index_with_existing_vector_map() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 10);

        // Create fresh index with same store
        let new_vi =
            VectorIndex::new(tvi.store().clone(), VectorConfig::default()).unwrap();
        assert_eq!(new_vi.point_count(), 0);
        assert!(!new_vi.contains(1));

        // VECTOR_MAP still has old entries
        assert!(tvi.store().get_vector_mapping(1).unwrap().is_some());
    }
}
