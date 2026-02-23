# C1: Error Module -- Test Plan

## Tests

```
test_onnx_runtime_error_display:
    err = EmbedError::OnnxRuntime(some_ort_error)
    msg = format!("{err}")
    ASSERT msg contains "onnx runtime error"

test_tokenizer_error_display:
    err = EmbedError::Tokenizer("bad token".to_string())
    msg = format!("{err}")
    ASSERT msg contains "tokenizer error"
    ASSERT msg contains "bad token"

test_download_error_display:
    err = EmbedError::Download("network timeout".to_string())
    msg = format!("{err}")
    ASSERT msg contains "model download failed"
    ASSERT msg contains "network timeout"

test_model_not_found_error_display:
    err = EmbedError::ModelNotFound { path: PathBuf::from("/tmp/missing.onnx") }
    msg = format!("{err}")
    ASSERT msg contains "model not found"
    ASSERT msg contains "/tmp/missing.onnx"

test_io_error_from:
    io_err = std::io::Error::new(ErrorKind::NotFound, "file missing")
    embed_err: EmbedError = io_err.into()
    ASSERT matches!(embed_err, EmbedError::Io(_))

test_dimension_mismatch_display:
    err = EmbedError::DimensionMismatch { expected: 384, got: 768 }
    msg = format!("{err}")
    ASSERT msg contains "expected 384"
    ASSERT msg contains "got 768"

test_error_is_send_sync:
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<EmbedError>()
    // Note: EmbedError may not be Sync if ort::Error is not Sync.
    // Verify at implementation time.

test_result_alias:
    // Verify type alias works
    result: crate::Result<Vec<f32>> = Ok(vec![1.0])
    ASSERT result.is_ok()
```

## Risks Covered

- AC-14: Error types cover model loading, inference, tokenization, download failures.
- Each variant produces descriptive Display output.
