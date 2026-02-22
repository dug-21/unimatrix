# C6: Persistence Module -- Pseudocode

## Purpose

Dump and load the hnsw_rs index with metadata tracking. Rebuild IdMap from VECTOR_MAP on load.

## File: `crates/unimatrix-vector/src/persistence.rs`

### Dump

```
use std::path::Path;
use std::fs;
use std::io::Write;
use hnsw_rs::prelude::*;
use crate::index::VectorIndex;
use crate::error::{VectorError, Result};

IMPL VectorIndex:
    pub fn dump(&self, dir: &Path) -> Result<()>:
        // Step 1: Create directory if it doesn't exist
        fs::create_dir_all(dir)
            .map_err(|e| VectorError::Persistence(
                format!("failed to create directory {}: {}", dir.display(), e)
            ))?

        // Step 2: Dump hnsw_rs index (read lock)
        let point_count;
        {
            hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner())
            point_count = hnsw.get_nb_point()
            hnsw.file_dump(dir, "unimatrix")
                .map_err(|e| VectorError::Persistence(
                    format!("hnsw_rs file_dump failed: {}", e)
                ))?
        }

        // Step 3: Write metadata file
        next = self.next_data_id.load(Ordering::Relaxed)
        meta_path = dir.join("unimatrix-vector.meta")

        meta_content = format!(
            "basename=unimatrix\npoint_count={}\ndimension={}\nnext_data_id={}\n",
            point_count, self.config.dimension, next
        )

        fs::write(&meta_path, meta_content)
            .map_err(|e| VectorError::Persistence(
                format!("failed to write metadata to {}: {}", meta_path.display(), e)
            ))?

        Ok(())
```

### Load

```
    pub fn load(
        store: Arc<Store>,
        config: VectorConfig,
        dir: &Path,
    ) -> Result<VectorIndex>:
        // Step 1: Read and parse metadata file
        meta_path = dir.join("unimatrix-vector.meta")
        meta_content = fs::read_to_string(&meta_path)
            .map_err(|e| VectorError::Persistence(
                format!("failed to read metadata from {}: {}", meta_path.display(), e)
            ))?

        // Parse key=value lines
        let mut basename = None
        let mut next_data_id_val = None
        let mut dimension = None

        for line in meta_content.lines():
            if let Some((key, value)) = line.split_once('='):
                match key.trim():
                    "basename" => basename = Some(value.trim().to_string())
                    "next_data_id" => next_data_id_val = Some(
                        value.trim().parse::<u64>()
                            .map_err(|e| VectorError::Persistence(
                                format!("invalid next_data_id: {}", e)
                            ))?
                    )
                    "dimension" => dimension = Some(
                        value.trim().parse::<usize>()
                            .map_err(|e| VectorError::Persistence(
                                format!("invalid dimension: {}", e)
                            ))?
                    )
                    _ => {}  // ignore unknown keys (forward compat)

        let basename = basename.ok_or_else(||
            VectorError::Persistence("missing 'basename' in metadata".into()))?
        let next_data_id_val = next_data_id_val.ok_or_else(||
            VectorError::Persistence("missing 'next_data_id' in metadata".into()))?

        // Step 2: Validate dimension matches config
        if let Some(dim) = dimension:
            if dim != config.dimension:
                return Err(VectorError::Persistence(
                    format!("dimension mismatch: metadata says {}, config says {}",
                        dim, config.dimension)
                ))

        // Step 3: Load hnsw_rs index from files
        //   hnsw_rs expects: {dir}/{basename}.hnsw.graph and {dir}/{basename}.hnsw.data
        let description = hnsw_rs::hnswio::load_description(dir, &basename)
            .map_err(|e| VectorError::Persistence(
                format!("failed to load hnsw description from {}: {}",
                    dir.display(), e)
            ))?

        let hnsw: Hnsw<f32, DistDot> = hnsw_rs::hnswio::load_hnsw_with_dist(
            dir, &basename, DistDot
        ).map_err(|e| VectorError::Persistence(
            format!("failed to load hnsw index from {}: {}", dir.display(), e)
        ))?

        // Step 4: Rebuild IdMap from VECTOR_MAP
        let mappings = store.iter_vector_mappings()?
        let mut id_map = IdMap::new()
        for (entry_id, data_id) in mappings:
            id_map.entry_to_data.insert(entry_id, data_id)
            id_map.data_to_entry.insert(data_id, entry_id)

        // Step 5: Construct VectorIndex
        Ok(VectorIndex {
            hnsw: RwLock::new(hnsw),
            store,
            config,
            next_data_id: AtomicU64::new(next_data_id_val),
            id_map: RwLock::new(id_map),
        })
```

## Design Notes

- **hnsw_rs file_dump API**: `file_dump(&self, dirpath: &Path, fname: &str)` creates `{fname}.hnsw.graph` and `{fname}.hnsw.data`. We always use "unimatrix" as the basename.
- **hnsw_rs load API**: Need to check the actual hnsw_rs v0.3 API for loading. It may use `HnswIo` struct or direct functions. The pseudocode uses `load_description` + `load_hnsw_with_dist` which are the common patterns. Will verify during implementation.
- **Metadata file format**: Simple `key=value` text. Forward-compatible (unknown keys are ignored). No need for JSON/TOML for 4 fields.
- **Dimension validation on load**: Prevents loading an index with wrong dimension into a mis-configured VectorConfig.
- **VECTOR_MAP as source of truth**: On load, the IdMap is rebuilt entirely from VECTOR_MAP. This means even if some re-embeddings happened, the IdMap reflects the latest mappings. Stale points in hnsw_rs (from re-embedding) will have data_ids not present in the reverse map and will be silently skipped during search.
- **Error wrapping**: All I/O errors wrapped in `VectorError::Persistence` with path context.

## Implementation Concerns

- The hnsw_rs load API needs to be verified during implementation. The `file_dump`/`HnswIo` API may have changed across minor versions.
- The VectorIndex struct fields may need `pub(crate)` visibility or constructor functions to allow `persistence.rs` to construct a VectorIndex directly. The approach depends on the chosen module organization (methods on VectorIndex vs standalone functions).
- Alternative: implement dump/load as methods on VectorIndex (same file or via `impl VectorIndex` in persistence.rs). The Rust module system allows splitting `impl` blocks across files within the same crate.
