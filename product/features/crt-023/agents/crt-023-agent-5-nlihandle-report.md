# Agent Report: crt-023-agent-5-nlihandle

## Task

Implement `NliServiceHandle` — the loading state machine managing `NliProvider` lifecycle.

## Files Modified

- `crates/unimatrix-server/src/infra/nli_handle.rs` — full implementation (was Wave 1 placeholder)
- `crates/unimatrix-embed/src/cross_encoder.rs` — added `NliProvider::is_session_healthy()` for poison detection

## Implementation Summary

### NliServiceHandle

State machine: `Loading → Ready | Failed → Retrying`, mirroring `EmbedServiceHandle` exactly.

Key behaviors implemented per pseudocode spec:

1. **`new()`** — constructs in `Loading` state, does not load.
2. **`start_loading(config: NliConfig)`** — guarded by `nli_enabled` check; emits single `warn!` and returns if disabled; stores config, spawns load task + retry monitor.
3. **`get_provider()`** — two-phase read/write lock pattern: fast path under read lock; slow path (poison transition) under write lock. Distinguishes `WouldBlock` from `Poisoned` via `NliProvider::is_session_healthy()`.
4. **`spawn_load_task()`** — resolves model, verifies SHA-256 before ONNX session construction, calls `NliProvider::new` in `spawn_blocking`.
5. **`spawn_retry_monitor()`** — exponential backoff (10s base, doubles per attempt), exits on `Ready` or exhausted retries.
6. **`is_ready_or_loading()`** — non-blocking `try_read` for `StoreService` guard.
7. **`wait_for_nli_ready(timeout)`** — poll-based eval path (ADR-006), 500ms interval.

### NliConfig

New struct (defined in `nli_handle.rs`) decoupling config passing from `InferenceConfig`. Fields: `nli_enabled`, `nli_model_name`, `nli_model_path`, `nli_model_sha256`, `cache_dir`.

### SHA-256 verification

`verify_sha256(model_dir, expected_hex)` reads `model_dir/model.onnx`, computes hash via `sha2`, compares case-insensitively. On mismatch: `tracing::error!` containing "security" and "hash mismatch" (R-05, AC-06). `sha2 = "0.10"` was already present in `unimatrix-server/Cargo.toml`.

### Poison detection (R-13, ADR-001)

Added `NliProvider::is_session_healthy()` to `cross_encoder.rs`. Uses `std::sync::TryLockError` to distinguish `WouldBlock` (healthy/busy → `true`) from `Poisoned(_)` (broken → `false`). Called in `get_provider()` before returning the `Arc<NliProvider>`.

### `resolve_model_dir`

Handles both explicit-path and cache-dir cases. When `nli_model_path` points to a file (not directory), uses `parent()` as the directory. Verifies `model.onnx` exists and is non-empty before returning.

## Tests

25 unit tests, all passing.

Coverage:
- `test_new_starts_in_loading_state`
- `test_failed_retries_exhausted_returns_nli_failed`
- `test_failed_retries_remaining_returns_not_ready`
- `test_is_ready_or_loading_true_while_loading`
- `test_is_ready_or_loading_false_when_retries_exhausted`
- `test_current_attempts_tracking`
- `test_nli_disabled_returns_not_ready` (AC-14)
- `test_nli_disabled_is_ready_or_loading_true`
- `test_missing_model_file_transitions_to_failed` (AC-05)
- `test_loading_state_immediately_after_start`
- `test_hash_mismatch_transitions_to_failed` (AC-06 / R-05)
- `test_verify_sha256_correct_hash`
- `test_verify_sha256_wrong_hash_returns_err`
- `test_verify_sha256_missing_file_returns_err`
- `test_mutex_poison_detected_at_get_provider` (R-13)
- `test_concurrent_get_provider_while_loading_no_deadlock` (R-01)
- `test_resolve_nli_model_none_returns_minilm2`
- `test_resolve_nli_model_minilm2_name`
- `test_resolve_nli_model_deberta_name`
- `test_resolve_nli_model_unknown_name_returns_err`
- `test_pool_floor_raised_when_nli_enabled` (R-02)
- `test_pool_floor_not_raised_when_nli_disabled` (R-02)
- `test_nli_config_default_enabled`
- `test_retry_exhaustion_stays_failed`
- `test_max_retries_value`

## Test Results

```
cargo test -p unimatrix-server nli_handle
test result: ok. 25 passed; 0 failed
```

Full workspace build: zero errors. Pre-existing doctest failure in `config.rs` line 21 (file path in module doc comment) confirmed pre-existing before these changes.

## Deviations from Pseudocode

None. Implementation follows the pseudocode spec exactly, with one clarification:

- **Poison detection via method not trait**: Pseudocode spec shows `NliProviderExt` trait in `nli_handle.rs`. Since `NliProvider::session` is private, the health check was added directly to `NliProvider` as `is_session_healthy()` — semantically equivalent, cleaner boundary.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found ADR-005 briefing degradation pattern and ADR-006 lazy embed init, confirming EmbedServiceHandle is the correct mirror target. crt-023 ADRs confirmed via search.
- Stored: entry #2731 "NliProviderExt: add health check to wrapped type, not as trait in consuming crate" via `/uni-store-pattern` — captures the private-field boundary issue that is invisible until you try to compile the trait extension approach.
