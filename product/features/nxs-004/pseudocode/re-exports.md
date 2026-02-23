# Pseudocode: re-exports

## Purpose
Re-export domain types from lower crates so consumers only need unimatrix-core.

## Modified File: crates/unimatrix-core/src/lib.rs

This is handled in the crate-setup component. The re-exports are:

From unimatrix-store:
- EntryRecord, NewEntry, QueryFilter, Status, TimeRange, DatabaseConfig
- Store (concrete type, for direct use in initialization and shutdown/compact)
- StoreError (for error matching)

From unimatrix-vector:
- SearchResult, VectorConfig, VectorIndex (concrete type, for initialization)
- VectorError (for error matching)

From unimatrix-embed:
- EmbeddingProvider (trait), EmbedConfig, OnnxProvider (concrete type)
- EmbedError (for error matching)

## Key Test Scenarios
- Test file in unimatrix-core uses all re-exported types
- No import from unimatrix-store needed by test (everything via unimatrix-core)
