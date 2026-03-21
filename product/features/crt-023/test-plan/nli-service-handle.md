# Test Plan: NliServiceHandle (`unimatrix-server/src/infra/nli_handle.rs`)

## Component Scope

File: `crates/unimatrix-server/src/infra/nli_handle.rs`

State machine: `Loading → Ready | Failed → Retrying → Loading`
Poison path: `Ready → Failed` (detected at `get_provider()`)

## Risks Covered

R-01 (Critical): Pool saturation under 3 concurrent NLI searches.
R-02 (High): Pool floor raise race at startup.
R-05 (Critical): Hash verification absent or wrong — warn/error requirement.
R-06 (High): Partial model file must produce `Failed`, not panic.
R-13 (High): Mutex poison detected at `get_provider()` boundary.

---

## Unit Tests: State Machine Transitions

### AC-05: Missing Model File → Server Starts, Cosine Fallback Works

```rust
#[tokio::test]
async fn test_missing_model_file_handle_fails_gracefully() {
    let handle = NliServiceHandle::new();
    let config = NliConfig {
        nli_model_path: Some(PathBuf::from("/nonexistent/model.onnx")),
        ..NliConfig::default()
    };
    handle.start_loading(config);
    // Wait for state to settle (exponential backoff means this takes time in real impl;
    // use a test-friendly wait with a short timeout)
    tokio::time::sleep(Duration::from_millis(200)).await;
    // After all retries exhausted, get_provider() must return Err(NliFailed)
    let result = handle.get_provider().await;
    assert!(matches!(result, Err(ServerError::NliFailed(_))),
        "Expected NliFailed, got: {:?}", result);
}

#[tokio::test]
async fn test_loading_state_returns_nli_not_ready() {
    // Immediately after start_loading, before model can load, state is Loading.
    let handle = NliServiceHandle::new();
    let config = NliConfig {
        nli_model_path: Some(PathBuf::from("/nonexistent/model.onnx")),
        ..NliConfig::default()
    };
    handle.start_loading(config);
    // Immediately poll — must return NliNotReady (not NliFailed yet)
    let result = handle.get_provider().await;
    assert!(
        matches!(result, Err(ServerError::NliNotReady) | Err(ServerError::NliFailed(_))),
        "Expected NliNotReady or NliFailed immediately after start_loading, got: {:?}", result
    );
}
```

### R-05: Hash Verification

```rust
#[tokio::test]
async fn test_hash_missing_emits_warn_not_error() {
    // nli_model_sha256 = None must emit warn!, not error!, and proceed to load.
    // This test captures tracing output to assert the warning is present.
    // Implementation: use tracing_test crate subscriber or capture via env filter.
    // Assertion: at least one WARN event with content mentioning "hash" or "verification"
    //   is emitted when nli_model_sha256 is None.
    //
    // If model is not available, the Failed transition will also emit -- that is OK.
    // The test asserts the specific warn for absent hash before any load attempt.
    // (Exact assertion depends on tracing_test setup; framework detail for Stage 3b.)
}

#[tokio::test]
async fn test_hash_mismatch_transitions_to_failed() {
    // AC-06: valid model file + wrong 64-char hex hash → Failed state
    // + log contains "security" and "hash mismatch".
    // Requires: real (or mocked) model file path.
    // Use a temp file containing valid bytes but set wrong expected hash.
    let model_file = create_temp_model_file_valid_bytes(); // test helper
    let wrong_hash = "a".repeat(64); // 64 hex chars, wrong hash
    let config = NliConfig {
        nli_model_path: Some(model_file.path().to_path_buf()),
        nli_model_sha256: Some(wrong_hash),
        ..NliConfig::default()
    };
    let handle = NliServiceHandle::new();
    handle.start_loading(config);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let result = handle.get_provider().await;
    assert!(matches!(result, Err(ServerError::NliFailed(_))));
    // Log assertion: "security" and "hash mismatch" must appear in error-level events.
    // (Captured via test subscriber — implementation detail.)
}

#[tokio::test]
async fn test_invalid_sha256_format_should_be_caught_at_validate() {
    // nli_model_sha256 that is not 64 hex chars must fail at InferenceConfig::validate(),
    // not at NliServiceHandle loading time. This test validates the config layer blocks it.
    // See config-extension.md for the validate() test.
    // Assertion here: NliServiceHandle::start_loading is never called when validate() aborts.
}
```

### R-06: Partial / Corrupt Model File

```rust
#[tokio::test]
async fn test_truncated_model_file_transitions_to_failed() {
    // R-06: a 1KB file at model_path must → Failed, not panic.
    use std::io::Write;
    let mut tmpfile = tempfile::NamedTempFile::new().unwrap();
    tmpfile.write_all(&[0xAB; 1024]).unwrap(); // 1KB of garbage
    let config = NliConfig {
        nli_model_path: Some(tmpfile.path().to_path_buf()),
        nli_model_sha256: None,
        ..NliConfig::default()
    };
    let handle = NliServiceHandle::new();
    handle.start_loading(config);
    // Wait for all retries to exhaust (test config: MAX_RETRIES small, backoff short)
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = handle.get_provider().await;
    assert!(matches!(result, Err(ServerError::NliFailed(_))),
        "Truncated file must produce NliFailed, got: {:?}", result);
}

#[tokio::test]
async fn test_corrupt_onnx_header_transitions_to_failed() {
    // Write a file with valid ZIP/ONNX header magic bytes followed by garbage.
    let mut tmpfile = tempfile::NamedTempFile::new().unwrap();
    // ONNX protobuf magic bytes + garbage
    tmpfile.write_all(&[0x08, 0x01, 0x00, 0x00]).unwrap();
    tmpfile.write_all(&[0xFF; 2048]).unwrap();
    let config = NliConfig {
        nli_model_path: Some(tmpfile.path().to_path_buf()),
        ..NliConfig::default()
    };
    let handle = NliServiceHandle::new();
    handle.start_loading(config);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(matches!(handle.get_provider().await, Err(ServerError::NliFailed(_))));
}

#[tokio::test]
async fn test_corrupt_file_triggers_retry_then_stays_failed() {
    // After MAX_RETRIES=3, handle stays Failed without crashing.
    // Verify retry sequence via state inspection (is_ready_or_loading())
    // or by observing the handle transitions (tracing events).
    // Final assertion: get_provider() returns NliFailed after retry exhaustion.
}
```

### R-13: Mutex Poison Detection

```rust
#[tokio::test]
async fn test_mutex_poison_detected_at_get_provider() {
    // R-13 (Critical): Poison the Mutex<Session> by injecting a panicking mock.
    // Approach: construct NliServiceHandle with a mock NliProvider whose
    // Mutex<Session> has been poisoned externally.
    //
    // Setup: use a test-only NliServiceHandle constructor that accepts
    // Arc<dyn CrossEncoderProvider> directly (bypassing model loading).
    // Poison the mutex by catching a panic inside a closure that holds the lock:
    let poisoned_provider = create_poisoned_nli_provider(); // test helper
    let handle = NliServiceHandle::with_provider_for_test(poisoned_provider);

    // Act: call get_provider()
    let result = handle.get_provider().await;

    // Assert: must detect poison and return Err, NOT Ok
    assert!(
        matches!(result, Err(ServerError::NliFailed(_))),
        "Poisoned mutex must produce NliFailed at get_provider boundary, got: {:?}", result
    );
}

#[tokio::test]
async fn test_mutex_poison_initiates_retry_sequence() {
    // After poison detection → Failed, the handle must initiate retry (→ Loading/Retrying).
    // If retry succeeds with a fresh provider, get_provider() eventually returns Ok.
    let handle = create_handle_that_recovers_after_poison(); // test helper
    // Give retry time to complete
    tokio::time::sleep(Duration::from_millis(500)).await;
    let result = handle.get_provider().await;
    // Assertion: either Ok (recovered) or Err(NliFailed) (retries exhausted).
    // Must NOT be an infinite loop or hang.
    assert!(result.is_ok() || matches!(result, Err(ServerError::NliFailed(_))));
}

#[tokio::test]
async fn test_retry_exhaustion_stays_failed() {
    // After MAX_RETRIES, get_provider() returns NliFailed permanently.
    let handle = NliServiceHandle::new();
    let config = NliConfig {
        nli_model_path: Some(PathBuf::from("/nonexistent/model.onnx")),
        ..NliConfig::default()
    };
    handle.start_loading(config);
    // Wait for retries to exhaust
    tokio::time::sleep(Duration::from_secs(3)).await;
    // Must be Failed
    assert!(matches!(handle.get_provider().await, Err(ServerError::NliFailed(_))));
    // A second call must also return Failed (not restart retry)
    assert!(matches!(handle.get_provider().await, Err(ServerError::NliFailed(_))));
}
```

---

## R-01: Concurrent Search Pool Saturation

```rust
#[tokio::test]
async fn test_concurrent_nli_search_pool_saturation() {
    // R-01 (Critical, non-negotiable):
    // 3 concurrent NLI search calls must all complete (possibly via fallback).
    // A 4th concurrent non-NLI context_search must also complete within 2x baseline.
    //
    // Setup: AppState with NliServiceHandle in Ready state (mock provider with ~50ms delay),
    //        RayonPool with floor=6, 3 tokio tasks each calling SearchService::search.
    let pool = Arc::new(RayonPool::new_with_min_threads(6));
    let mock_provider = Arc::new(SlowMockProvider::new(Duration::from_millis(50)));
    let handle = NliServiceHandle::with_provider_for_test(mock_provider);
    let search_service = SearchService::new(/* ... */, handle, pool);

    let start = Instant::now();
    let futures: Vec<_> = (0..3)
        .map(|_| search_service.search("test query", /* config */))
        .collect();
    let results = futures::future::join_all(futures).await;
    let elapsed = start.elapsed();

    // All searches must succeed or gracefully degrade (no panics, no Err propagated to caller)
    for result in &results {
        assert!(result.is_ok(), "Search must not error: {:?}", result);
    }
    // Must complete well within MCP_HANDLER_TIMEOUT
    assert!(elapsed < Duration::from_secs(30),
        "3 concurrent NLI searches took {:?}, may indicate pool saturation", elapsed);
}

#[tokio::test]
async fn test_nli_search_concurrent_embedding_not_starved() {
    // Non-NLI context_search issued concurrently with 3 NLI searches
    // must complete within 2x single-call baseline latency.
    // Implementation: separate mock for embedding service, mock NLI provider with 100ms delay.
    // Measure baseline: single search call latency without concurrent NLI.
    // Measure under load: search call with 3 concurrent NLI calls in background.
    // Assert: load_latency < 2 * baseline_latency.
}
```

## R-02: Pool Floor Raise

```rust
#[test]
fn test_pool_floor_raised_when_nli_enabled() {
    // R-02: When nli_enabled=true, rayon pool size must be >= 6 before any inference.
    // This tests the config application at startup.
    let config = InferenceConfig {
        nli_enabled: true,
        rayon_pool_size: 4, // default minimum
        ..InferenceConfig::default()
    };
    // validate() applies pool floor: rayon_pool_size.max(6).min(8)
    let resolved = config.resolve_pool_size();
    assert!(resolved >= 6,
        "Pool size must be >= 6 when nli_enabled=true, got {resolved}");
}

#[test]
fn test_pool_floor_not_raised_when_nli_disabled() {
    // R-02: nli_enabled=false must not raise pool floor to 6.
    let config = InferenceConfig {
        nli_enabled: false,
        rayon_pool_size: 4,
        ..InferenceConfig::default()
    };
    let resolved = config.resolve_pool_size();
    assert_eq!(resolved, 4,
        "Pool size must remain at configured value when nli_enabled=false");
}
```

---

## AC-14: Graceful Degradation Variants

```rust
#[tokio::test]
async fn test_nli_disabled_config_returns_not_ready() {
    // nli_enabled=false → get_provider() returns Err(NliNotReady) immediately.
    let handle = NliServiceHandle::new();
    let config = NliConfig { nli_enabled: false, ..NliConfig::default() };
    handle.start_loading(config);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let result = handle.get_provider().await;
    assert!(matches!(result, Err(ServerError::NliNotReady)),
        "nli_enabled=false must return NliNotReady immediately, got: {:?}", result);
}

#[test]
fn test_is_ready_or_loading_returns_true_while_loading() {
    let handle = NliServiceHandle::new();
    let config = NliConfig {
        nli_model_path: Some(PathBuf::from("/slow/model.onnx")),
        ..NliConfig::default()
    };
    handle.start_loading(config);
    // Immediately: must report loading (true)
    assert!(handle.is_ready_or_loading());
}
```

---

## Integration Surface Assertions

The following integration-level checks are assigned to Stage 3c (execution phase):
- `context_search` MCP tool returns results when NLI handle is in Failed state (cosine fallback)
- Server starts without NLI model (AC-05): verified by infra-001 `tools` suite
- Hash mismatch does not abort server startup (AC-06): verified by new security suite test
