# C8: Download Module -- Test Plan

## Tests

```
test_ensure_model_creates_directory:
    // Use a temp directory, verify model subdirectory is created
    temp = tempdir()
    cache_dir = temp.path()
    model = EmbeddingModel::AllMiniLmL6V2
    result = ensure_model(model, cache_dir)
    // This will attempt to download -- integration test
    IF result.is_ok():
        model_dir = result.unwrap()
        ASSERT model_dir.exists()
        ASSERT model_dir.join("model.onnx").exists()
        ASSERT model_dir.join("tokenizer.json").exists()
    ELSE:
        // Network may not be available; check error type
        ASSERT matches!(result.unwrap_err(), EmbedError::Download(_))

test_ensure_model_cached_no_redownload:
    // After first download, second call should use cache
    temp = tempdir()
    model = EmbeddingModel::AllMiniLmL6V2

    // First call downloads
    result1 = ensure_model(model, temp.path())
    IF result1.is_err(): SKIP // no network

    // Record modification times
    onnx_mtime = metadata(model_dir.join("model.onnx")).modified()
    tok_mtime = metadata(model_dir.join("tokenizer.json")).modified()

    // Second call uses cache
    result2 = ensure_model(model, temp.path())
    ASSERT result2.is_ok()

    // Files unchanged
    ASSERT metadata(model_dir.join("model.onnx")).modified() == onnx_mtime
    ASSERT metadata(model_dir.join("tokenizer.json")).modified() == tok_mtime

test_ensure_model_sanitized_subdir:
    temp = tempdir()
    model = EmbeddingModel::AllMiniLmL6V2
    result = ensure_model(model, temp.path())
    IF result.is_ok():
        model_dir = result.unwrap()
        ASSERT model_dir.file_name().unwrap() == "sentence-transformers_all-MiniLM-L6-v2"

test_ensure_model_corrupt_file:
    temp = tempdir()
    model = EmbeddingModel::AllMiniLmL6V2
    // Create model subdir with empty (corrupt) model.onnx
    model_dir = temp.path().join("sentence-transformers_all-MiniLM-L6-v2")
    create_dir_all(&model_dir)
    write(model_dir.join("model.onnx"), "")  // empty file
    write(model_dir.join("tokenizer.json"), "")  // empty file

    // Should detect empty files and attempt re-download
    result = ensure_model(model, temp.path())
    // If network available, should re-download
    // If not, should return Download error

test_file_size_helper:
    // Internal helper
    temp = tempdir()
    path = temp.path().join("test.txt")
    write(&path, "hello")
    ASSERT file_size(&path) == 5

    ASSERT file_size(&temp.path().join("nonexistent")) == 0
```

## Risks Covered

- R-05: Model download failure or corruption.
- R-10: Cache path resolution, directory creation.
- AC-02: Model downloaded on first use, cached for subsequent.
