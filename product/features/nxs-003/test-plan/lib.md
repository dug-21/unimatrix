# C11: Lib Module -- Test Plan

## Tests

```
test_crate_builds:
    // AC-01: cargo build --workspace succeeds
    // Verified by build system, not by a Rust test

test_forbid_unsafe_code_ac15:
    // AC-15: Verified by compiler -- #![forbid(unsafe_code)] at crate root
    // Any unsafe block causes compile error

test_public_reexports:
    // Verify all expected types are accessible from crate root
    USE unimatrix_embed::EmbedConfig
    USE unimatrix_embed::EmbedError
    USE unimatrix_embed::EmbeddingModel
    USE unimatrix_embed::EmbeddingProvider
    USE unimatrix_embed::OnnxProvider
    USE unimatrix_embed::l2_normalize
    USE unimatrix_embed::l2_normalized
    USE unimatrix_embed::prepare_text
    USE unimatrix_embed::embed_entry
    USE unimatrix_embed::embed_entries
    USE unimatrix_embed::Result

    // All imports compile = re-exports working

test_test_support_feature:
    // Under test-support feature, test_helpers is accessible
    #[cfg(feature = "test-support")]
    USE unimatrix_embed::test_helpers::MockProvider
    USE unimatrix_embed::test_helpers::cosine_similarity
    USE unimatrix_embed::test_helpers::assert_dimension
    USE unimatrix_embed::test_helpers::assert_normalized
```

## Risks Covered

- R-15: ort RC build verification.
- AC-01: Workspace compilation.
- AC-15: forbid(unsafe_code) enforced.
