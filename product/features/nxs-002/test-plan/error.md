# C2: Error Module -- Test Plan

## AC-14: Typed Result Errors (No Panics)

### Tests

```
test_dimension_mismatch_display:
    err = VectorError::DimensionMismatch { expected: 384, got: 128 }
    msg = err.to_string()
    ASSERT msg contains "384"
    ASSERT msg contains "128"
    ASSERT msg contains "dimension mismatch"

test_store_error_display:
    store_err = StoreError::EntryNotFound(42)
    err = VectorError::Store(store_err)
    msg = err.to_string()
    ASSERT msg contains "store error"

test_persistence_error_display:
    err = VectorError::Persistence("file not found: /tmp/test".into())
    msg = err.to_string()
    ASSERT msg contains "persistence error"
    ASSERT msg contains "file not found"

test_empty_index_display:
    err = VectorError::EmptyIndex
    msg = err.to_string()
    ASSERT msg contains "empty"

test_entry_not_in_index_display:
    err = VectorError::EntryNotInIndex(42)
    msg = err.to_string()
    ASSERT msg contains "42"

test_index_error_display:
    err = VectorError::Index("hnsw internal error".into())
    msg = err.to_string()
    ASSERT msg contains "index error"

test_invalid_embedding_display:
    err = VectorError::InvalidEmbedding("NaN at index 5".into())
    msg = err.to_string()
    ASSERT msg contains "invalid embedding"
    ASSERT msg contains "NaN"

test_from_store_error:
    store_err = StoreError::EntryNotFound(1)
    err: VectorError = store_err.into()
    ASSERT matches!(err, VectorError::Store(_))

test_is_std_error:
    fn assert_error<T: std::error::Error>() {}
    assert_error::<VectorError>()

test_error_source_store_variant:
    store_err = StoreError::EntryNotFound(1)
    err = VectorError::Store(store_err)
    ASSERT err.source().is_some()

test_error_source_other_variants:
    ASSERT VectorError::EmptyIndex.source().is_none()
    ASSERT VectorError::Persistence("x".into()).source().is_none()
    ASSERT VectorError::DimensionMismatch { expected: 1, got: 2 }.source().is_none()
```

## Risks Covered
- R-11 (partial): Error types are constructible and have meaningful messages.
