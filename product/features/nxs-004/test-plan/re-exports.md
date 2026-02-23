# Test Plan: re-exports

## Scope

Verify all domain types are accessible through unimatrix-core without importing lower crates.

## Compilation Tests (in crates/unimatrix-core tests/)

### test_all_reexported_types
- Write a test function that imports and uses all re-exported types from `unimatrix_core`:
  - From store: `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `DatabaseConfig`, `Store`, `StoreError`
  - From vector: `SearchResult`, `VectorConfig`, `VectorIndex`, `VectorError`
  - From embed: `EmbeddingProvider`, `EmbedConfig`, `OnnxProvider`, `EmbedError`
- Each type must be referenced (e.g., `let _: Option<EntryRecord> = None;`) to force the compiler to resolve the path.
- This is a compilation test: if it compiles, re-exports are correct.

### test_no_direct_lower_crate_import_needed
- Write a test in unimatrix-core that constructs a `NewEntry`, creates a `Status::Active`, and references a `QueryFilter` -- all via `unimatrix_core::*` paths.
- Verify the test does NOT contain `use unimatrix_store::`, `use unimatrix_vector::`, or `use unimatrix_embed::`.

### test_error_types_reexported
- Assert `unimatrix_core::StoreError`, `unimatrix_core::VectorError`, `unimatrix_core::EmbedError` are importable.
- This enables downstream error matching without adding lower crate dependencies.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-11 | test_all_reexported_types, test_no_direct_lower_crate_import_needed, test_error_types_reexported |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-01 (partial) | Types accessible through unimatrix-core |
