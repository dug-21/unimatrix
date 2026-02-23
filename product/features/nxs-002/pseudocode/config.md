# C3: Config Module -- Pseudocode

## Purpose

Define `VectorConfig` with defaults matching the product specification.

## File: `crates/unimatrix-vector/src/config.rs`

```
#[derive(Debug, Clone)]
STRUCT VectorConfig:
    dimension: usize          // Embedding vector dimension
    max_nb_connection: usize  // Max edges per HNSW node (M parameter)
    ef_construction: usize    // Construction beam width
    max_elements: usize       // Pre-allocation hint (grows dynamically)
    max_layer: usize          // Max HNSW graph layers
    default_ef_search: usize  // Default search beam width

IMPL Default for VectorConfig:
    fn default() -> Self:
        VectorConfig {
            dimension: 384,           // all-MiniLM-L6-v2
            max_nb_connection: 16,    // standard M for 384d
            ef_construction: 200,     // high quality construction
            max_elements: 10_000,     // pre-allocation hint
            max_layer: 16,            // standard for this M
            default_ef_search: 32,    // reasonable default
        }
```

## Design Notes

- All fields are `pub` for direct construction.
- `max_elements` is a hint, not a hard cap. hnsw_rs grows dynamically.
- `default_ef_search` is used when callers don't specify ef_search explicitly (future convenience method).
- No validation in the config struct itself -- VectorIndex::new validates constraints.
