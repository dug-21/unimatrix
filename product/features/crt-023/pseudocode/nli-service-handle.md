# NliServiceHandle — Pseudocode

**File**: `crates/unimatrix-server/src/infra/nli_handle.rs` (new)
**Exports via**: `infra/mod.rs` (add `pub mod nli_handle`)

**Purpose**: Lazy-loading state machine managing the lifecycle of one `NliProvider` instance.
Mirrors `EmbedServiceHandle` exactly in structure and behavior. States: Loading → Ready | Failed
→ Retrying. Provides `get_provider()` for consumers (SearchService, StoreService,
nli_detection.rs). Performs SHA-256 hash verification before ONNX session construction (ADR-003).
Detects `Mutex<Session>` poisoning at the `get_provider()` boundary (ADR-001, R-13).

---

## State Machine

```
States:
  Loading    - Model load in progress (initial state after start_loading)
  Ready      - Arc<NliProvider> available; get_provider() returns Ok
  Failed     - Load failed or retries exhausted; get_provider() returns Err(NliFailed)
  Retrying   - Backoff before re-attempt; get_provider() returns Err(NliNotReady)

Transitions:
  Loading  --load_ok--> Ready
  Loading  --load_err-> Failed (attempts < MAX_RETRIES: retry monitor triggers)
  Failed   --monitor--> Retrying --backoff--> Loading (if attempts < MAX_RETRIES)
  Ready    --get_provider() detects poison--> Failed (retry monitor triggers)

get_provider() return by state:
  Loading   -> Err(ServerError::NliNotReady)
  Ready     -> try_lock() check:
                  lock OK but provider is poisoned -> transition to Failed, Err(NliFailed)
                  lock OK and session healthy      -> Ok(Arc<NliProvider>)
  Failed    -> if attempts < MAX_RETRIES: Err(NliNotReady)  [retry in flight or pending]
               else:                      Err(NliFailed)    [retries exhausted]
  Retrying  -> Err(NliNotReady)

Special: nli_enabled = false -> get_provider() returns Err(NliNotReady) immediately without
transitioning state (the handle is initialized but never calls start_loading; stays Loading).
```

---

## `NliConfig` (new struct, defined in nli_handle.rs or re-exported from config.rs)

```
/// Subset of InferenceConfig fields needed by NliServiceHandle.
/// Extracted to avoid importing the full InferenceConfig into nli_handle.rs.
struct NliConfig {
    nli_enabled:       bool,
    nli_model_name:    Option<String>,   // resolved to NliModel by validate()
    nli_model_path:    Option<PathBuf>,  // explicit ONNX file path override
    nli_model_sha256:  Option<String>,   // 64-char hex SHA-256 or None
    cache_dir:         PathBuf,          // resolved at startup from InferenceConfig
}
```

---

## `NliState` enum (private)

```
enum NliState {
    Loading,
    Ready(Arc<NliProvider>),
    Failed { message: String, attempts: u32 },
    Retrying { attempt: u32 },
}
```

---

## `NliServiceHandle` struct

```
struct NliServiceHandle {
    state:  RwLock<NliState>,         // current lifecycle state
    config: RwLock<Option<NliConfig>>, // stored for retries
}
```

---

## `NliServiceHandle::new`

```
fn new() -> Arc<Self>
    Arc::new(NliServiceHandle {
        state:  RwLock::new(NliState::Loading),
        config: RwLock::new(None),
    })
```

---

## `NliServiceHandle::start_loading`

```
fn start_loading(self: &Arc<Self>, config: NliConfig)
    // Mirrors EmbedServiceHandle::start_loading exactly.
    // Called once at server startup after InferenceConfig::validate() passes.
    // If nli_enabled = false: do not call this function (caller guards).

    // Store config for retries
    {
        let mut cfg = match self.config.try_write():
            Ok(g)  -> g
            Err(_) -> log error "NLI config lock contended at startup"; return
        *cfg = Some(config.clone())
    }

    self.spawn_load_task(config, attempt=1)
    self.spawn_retry_monitor()
```

---

## `NliServiceHandle::spawn_load_task` (private)

```
fn spawn_load_task(self: &Arc<Self>, config: NliConfig, attempt: u32)
    handle = Arc::clone(self)

    tokio::spawn(async move {
        // Step 1: Resolve model path
        let model = match resolve_nli_model(&config):
            Ok(m)  -> m
            Err(e) -> {
                let mut state = handle.state.write().await
                *state = NliState::Failed { message: e.to_string(), attempts: attempt }
                tracing::error!(error=%e, attempt, "NLI model resolution failed")
                return
            }

        let model_path = match resolve_model_path(&model, &config):
            Ok(p)  -> p
            Err(e) -> {
                // Model file not found or ensure_nli_model download failed
                let mut state = handle.state.write().await
                *state = NliState::Failed { message: e.to_string(), attempts: attempt }
                tracing::warn!(error=%e, "NLI model not available; continuing on cosine fallback")
                return
            }

        // Step 2: SHA-256 hash verification (ADR-003, NFR-09, R-05)
        // Performed BEFORE Session::builder() to detect tampered/corrupt files.
        if let Some(ref expected_hash) = config.nli_model_sha256 {
            match verify_sha256(&model_path, expected_hash):
                Ok(())  -> // hash matches, proceed
                Err(e)  -> {
                    let msg = format!("security: hash mismatch for NLI model at {}: {e}", model_path.display())
                    let mut state = handle.state.write().await
                    *state = NliState::Failed { message: msg.clone(), attempts: attempt }
                    // REQUIRED: log must contain "security" AND "hash mismatch" (AC-06, R-05)
                    tracing::error!("NLI model security: hash mismatch — expected {expected_hash}, model file integrity check failed")
                    return
                }
        } else {
            // No hash configured: warn that integrity verification is disabled (R-05 scenario 1)
            tracing::warn!("nli_model_sha256 not set; NLI model integrity verification is disabled")
        }

        // Step 3: Construct NliProvider in spawn_blocking (I/O + one-time CPU, not rayon)
        // NOTE: Model loading is spawn_blocking (not rayon pool). Rayon pool is for
        // repeated inference only. Loading is a one-time blocking I/O + init operation.
        let model_path_clone = model_path.clone()
        let result = tokio::task::spawn_blocking(move || {
            NliProvider::new(model, &model_path_clone)
        }).await

        let mut state = handle.state.write().await
        match result:
            Ok(Ok(provider)) ->
                *state = NliState::Ready(Arc::new(provider))
                if attempt > 1:
                    tracing::info!(attempt, "NLI model loaded successfully (retry)")
                else:
                    tracing::info!("NLI model loaded successfully; NLI re-ranking active")

            Ok(Err(embed_err)) ->
                let msg = embed_err.to_string()
                *state = NliState::Failed { message: msg.clone(), attempts: attempt }
                tracing::error!(error=%msg, attempt, "NLI model failed to load")

            Err(join_err) ->
                // spawn_blocking task panicked (e.g. Session::builder panicked on corrupt file)
                let msg = join_err.to_string()
                *state = NliState::Failed { message: msg.clone(), attempts: attempt }
                tracing::error!(error=%msg, attempt, "NLI model load task panicked")
    })
```

---

## `resolve_nli_model` (private helper)

```
fn resolve_nli_model(config: &NliConfig) -> Result<NliModel, String>
    match &config.nli_model_name:
        None ->
            // Default: NliMiniLM2L6H768
            Ok(NliModel::NliMiniLM2L6H768)
        Some(name) ->
            NliModel::from_config_name(name)
                .ok_or_else(|| format!("unknown nli_model_name: '{}'; valid: minilm2, deberta", name))
            // Note: InferenceConfig::validate() catches this at startup (R-15).
            // This is a defensive fallback in case validate() was not called.
```

---

## `resolve_model_path` (private helper)

```
fn resolve_model_path(model: &NliModel, config: &NliConfig) -> Result<PathBuf, EmbedError>
    if let Some(ref explicit_path) = config.nli_model_path:
        // Operator-provided path; trust it but verify it exists
        if explicit_path.exists() AND file_size(explicit_path) > 0:
            // Return the directory containing the file, not the file itself
            // (NliProvider::new expects a directory with model.onnx inside)
            Ok(explicit_path.parent().unwrap_or(explicit_path).to_path_buf())
        else:
            Err(EmbedError::ModelNotFound { path: explicit_path.clone() })
    else:
        // Auto-resolve from cache dir
        ensure_nli_model(*model, &config.cache_dir)
            .map_err(|e| e)  // propagate EmbedError::Download or ModelNotFound
```

---

## `verify_sha256` (private helper)

```
fn verify_sha256(model_path: &Path, expected_hex: &str) -> Result<(), String>
    // Read the model file and compute SHA-256 hash.
    // model_path here is the DIRECTORY returned by ensure_nli_model.
    // The actual ONNX file is model_path.join("model.onnx").

    let onnx_file = model_path.join("model.onnx")
    let bytes = std::fs::read(&onnx_file)
                    .map_err(|e| format!("failed to read model file for hash check: {e}"))?

    use sha2::{Sha256, Digest}
    let mut hasher = Sha256::new()
    hasher.update(&bytes)
    let actual_hex = hex::encode(hasher.finalize())  // or format!("{:x}", hasher.finalize())

    if actual_hex.eq_ignore_ascii_case(expected_hex):
        Ok(())
    else:
        Err(format!("expected {}, got {}", expected_hex, actual_hex))

    // NOTE: sha2 crate must be in unimatrix-server/Cargo.toml (R-22).
    // Implementation should verify sha2 is present before writing this code.
```

---

## `NliServiceHandle::get_provider`

```
async fn get_provider(&self) -> Result<Arc<NliProvider>, ServerError>
    let state = self.state.read().await
    match &*state:
        NliState::Ready(provider) ->
            // Poison check (ADR-001, R-13): try_lock() detects if session Mutex<Session>
            // was poisoned by a rayon worker panic. This is the boundary where we detect
            // that the NliProvider is no longer usable.
            //
            // Implementation note: NliProvider does not expose try_lock() directly.
            // The poison detection requires either:
            // (a) A method on NliProvider that tries the mutex and returns an error, or
            // (b) Checking if the Arc<NliProvider> has a is_session_healthy() method.
            // Design choice: NliProvider exposes `fn is_session_healthy(&self) -> bool`
            // that calls `self.session.try_lock().is_ok()` without holding the lock.

            if provider.is_session_healthy():
                Ok(Arc::clone(provider))
            else:
                // Mutex is poisoned. Transition to Failed and initiate retry.
                // Must drop read lock before acquiring write lock.
                drop(state)
                let mut write_state = self.state.write().await
                // Re-check — another caller may have already transitioned
                if matches!(&*write_state, NliState::Ready(_)):
                    *write_state = NliState::Failed {
                        message: "NLI session mutex poisoned by rayon panic".to_string(),
                        attempts: 1,
                    }
                    tracing::error!("NLI Mutex<Session> poisoned; transitioning to Failed; retry will start")
                Err(ServerError::NliFailed("NLI session mutex poisoned".to_string()))

        NliState::Loading | NliState::Retrying { .. } ->
            Err(ServerError::NliNotReady)

        NliState::Failed { message, attempts } ->
            if *attempts < MAX_RETRIES:
                // Retry in flight (retry monitor handles it); report as not-ready
                Err(ServerError::NliNotReady)
            else:
                Err(ServerError::NliFailed(message.clone()))
```

### `NliProvider::is_session_healthy` (added to NliProvider in cross_encoder.rs)

```
/// Non-blocking check: returns true if Mutex<Session> is not poisoned.
/// Used by NliServiceHandle::get_provider() for poison detection (R-13).
pub fn is_session_healthy(&self) -> bool
    self.session.try_lock().is_ok()
    // If poisoned: try_lock() returns Err(PoisonError); we return false.
    // If locked (busy): try_lock() returns Err(WouldBlock); we return false too,
    // which is conservative (may cause a spurious NliNotReady for a brief moment
    // while inference is in progress). Acceptable tradeoff: no false "healthy" on poison.
    //
    // Better implementation: distinguish poison from busy:
    // match self.session.try_lock():
    //     Ok(_)                    -> true   (healthy)
    //     Err(TryLockError::Poisoned(_)) -> false  (poisoned)
    //     Err(TryLockError::WouldBlock)  -> true   (busy but healthy)
```

---

## `NliServiceHandle::spawn_retry_monitor` (private)

```
fn spawn_retry_monitor(self: &Arc<Self>)
    handle = Arc::clone(self)
    tokio::spawn(async move {
        base_delay = Duration::from_secs(10)

        loop:
            tokio::time::sleep(base_delay).await

            let mut state = handle.state.write().await
            match &*state:
                NliState::Ready(_) -> return  // done

                NliState::Loading | NliState::Retrying { .. } ->
                    drop(state)
                    continue

                NliState::Failed { attempts, .. } if *attempts >= MAX_RETRIES ->
                    tracing::warn!(attempts=*attempts, "NLI model retries exhausted; NLI permanently disabled for this session")
                    return

                NliState::Failed { attempts, .. } ->
                    let next_attempt = *attempts + 1
                    let config = handle.config.read().await
                    if let Some(cfg) = config.as_ref():
                        let delay = base_delay * 2u32.saturating_pow(next_attempt - 1)
                        tracing::info!(attempt=next_attempt, max=MAX_RETRIES, delay_secs=delay.as_secs(),
                                       "retrying NLI model load")
                        let cfg_clone = cfg.clone()
                        *state = NliState::Retrying { attempt: next_attempt }
                        drop(config)
                        drop(state)
                        tokio::time::sleep(delay).await
                        handle.spawn_load_task(cfg_clone, next_attempt)
                    else:
                        return  // no config, cannot retry
    })
```

---

## `NliServiceHandle::is_ready_or_loading`

```
/// Non-blocking check used by StoreService to decide whether to spawn post-store NLI task.
/// Returns true if handle is Ready or Loading (worth spawning the task).
/// Returns false if Failed (retries exhausted) to avoid unnecessary task overhead.
fn is_ready_or_loading(&self) -> bool
    match self.state.try_read():
        Ok(guard) -> matches!(&*guard, NliState::Ready(_) | NliState::Loading | NliState::Retrying { .. })
        Err(_)    -> false  // lock contended; conservative false
```

---

## `NliServiceHandle::wait_for_nli_ready` (for eval path, ADR-006)

```
/// Poll get_provider() until Ready or timeout.
/// Used by EvalServiceLayer::from_profile() (ADR-006).
/// Not used on MCP handler path (MCP path uses get_provider() directly with fallback).
async fn wait_for_nli_ready(&self, timeout: Duration) -> Result<(), NliNotReadyError>
    let deadline = Instant::now() + timeout
    let poll_interval = Duration::from_millis(500)

    loop:
        match self.get_provider().await:
            Ok(_) -> return Ok(())  // Ready
            Err(ServerError::NliFailed(msg)) ->
                return Err(NliNotReadyError::Failed(msg))  // retries exhausted
            Err(ServerError::NliNotReady) ->
                // Still loading; poll again
                if Instant::now() >= deadline:
                    return Err(NliNotReadyError::Timeout)
                tokio::time::sleep(poll_interval.min(deadline - Instant::now())).await
            Err(other) ->
                return Err(NliNotReadyError::Failed(other.to_string()))

/// Error type for wait_for_nli_ready (used only by eval path).
enum NliNotReadyError {
    Timeout,
    Failed(String),
}
```

---

## Constants

```
const MAX_RETRIES: u32 = 3
// Matches EmbedServiceHandle::MAX_RETRIES for consistency.
```

---

## error.rs Extension

Add two new variants to the existing `ServerError` enum:

```
// In ServerError enum:
/// NLI model is still loading or retrying.
NliNotReady,
/// NLI model failed to load (retries exhausted or mutex poisoned).
NliFailed(String),
```

Add to `fmt::Display` impl:
```
ServerError::NliNotReady    -> "NLI model is initializing"
ServerError::NliFailed(msg) -> "NLI model failed: {msg}"
```

Add to `From<ServerError> for ErrorData`:
```
ServerError::NliNotReady     -> ErrorData::new(ERROR_EMBED_NOT_READY, "NLI model is initializing. Search uses cosine fallback.", None)
ServerError::NliFailed(msg)  -> ErrorData::new(ERROR_EMBED_NOT_READY, format!("NLI model failed to load: {msg}. Server continues on cosine fallback."), None)
// Use ERROR_EMBED_NOT_READY (-32004) — NLI failure is not user-visible as an error in normal operation.
// NliNotReady and NliFailed are internal signals; they cause fallback, not MCP errors.
```

---

## infra/mod.rs Extension

```
// Add to infra/mod.rs:
pub mod nli_handle;
```

---

## Error Handling Summary

| Failure Mode | Transition | Client Visible? |
|-------------|-----------|----------------|
| Model file not found | Loading → Failed; retry | No (cosine fallback) |
| Hash mismatch | Loading → Failed (no retry on hash mismatch — security) | No (warn log) |
| `Session::builder()` panics | Loading → Failed; retry | No |
| `spawn_blocking` JoinError | Loading → Failed; retry | No |
| Rayon panic poisons Mutex | Ready → Failed (detected at next get_provider()) | No |
| Retries exhausted | Failed permanently | No (cosine fallback) |
| `nli_enabled = false` | Never loads; get_provider() returns NliNotReady | No |

**Hash mismatch retry behavior**: Hash mismatch is a security event. The implementation may
choose to retry (the file may have been partially downloaded and will be corrected) or to not
retry (treat as permanent failure). Recommend: do not retry on hash mismatch after 3 consecutive
hash mismatches — this prevents infinite retry loops on a permanently tampered file. A single
mismatch should allow retry (the file may have been interrupted mid-download).

---

## Key Test Scenarios

1. **AC-05 / Loading → NliNotReady**: `NliServiceHandle::new()` starts in Loading; `get_provider()` returns `Err(NliNotReady)`.
2. **AC-05 / server starts without model**: Start server with missing NLI model file; assert `context_search` returns results (cosine fallback).
3. **AC-06 / hash mismatch**: Set `nli_model_sha256` to wrong 64-char hex; assert state → Failed, error log contains "security" and "hash mismatch", server continues (R-05).
4. **R-05 / no hash configured**: When `nli_model_sha256 = None`, assert a warn-level log noting hash verification disabled.
5. **R-13 / poison detection**: Poison `Mutex<Session>` via rayon panic; call `get_provider()`; assert `Err(NliFailed)`.
6. **R-13 / retry after poison**: After poison detection → Failed, retry monitor triggers; eventually reaches Ready (if model load succeeds).
7. **AC-05 / retries exhausted**: Fail 3 times; assert `get_provider()` returns `Err(NliFailed)`, not `Err(NliNotReady)`.
8. **R-02 / pool floor**: After `start_loading` with `nli_enabled=true`, assert `pool_size() >= 6`.
9. **R-06 / partial model file**: Write 1KB truncated ONNX to model path; assert state → Failed (not panic, not hang).
