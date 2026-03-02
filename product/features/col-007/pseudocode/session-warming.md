# Pseudocode: session-warming

## Purpose

Extend the `SessionRegister` handler to pre-warm the ONNX embedding model. When a new Claude Code session starts, the server blocks until the embedding model is loaded, then runs a no-op warmup embedding to force ONNX runtime initialization. This ensures the first `UserPromptSubmit` ContextSearch completes within the 50ms latency budget.

## Modified: SessionRegister Handler

The warming logic is extracted into a helper function for clarity.

```
async fn handle_session_register(
    embed_service: &Arc<EmbedServiceHandle>,
    session_id: String,
    cwd: String,
    agent_role: Option<String>,
    feature: Option<String>,
) -> HookResponse:
    tracing::info!(
        session_id,
        cwd,
        agent_role = ?agent_role,
        feature = ?feature,
        "UDS: session registered"
    )

    // Pre-warm embedding model (FR-04)
    warm_embedding_model(embed_service).await

    HookResponse::Ack
```

## New: warm_embedding_model()

```
async fn warm_embedding_model(embed_service: &Arc<EmbedServiceHandle>):
    // Step 1: Wait for adapter to be ready
    match embed_service.get_adapter().await:
        Ok(adapter) =>
            // Step 2: Run warmup embedding via spawn_blocking
            // This forces ONNX runtime to fully initialize (load model weights,
            // allocate inference buffers). The empty string is a no-op that
            // exercises the full inference path.
            match tokio::task::spawn_blocking(move || {
                adapter.embed_entry("", "warmup")
            }).await:
                Ok(Ok(_)) =>
                    tracing::info!("ONNX embedding model pre-warmed")
                Ok(Err(e)) =>
                    // Warmup embedding failed, but adapter is Ready.
                    // Subsequent ContextSearch calls will work because they
                    // call embed_entry themselves (FR-04.4 note).
                    tracing::warn!("warmup embedding failed: {e}")
                Err(e) =>
                    tracing::warn!("warmup spawn_blocking failed: {e}")

        Err(ServerError::EmbedNotReady) =>
            // Model still loading -- this means get_adapter is blocking
            // but returned NotReady. In practice get_adapter waits until
            // Ready or Failed, so this path means the state changed to
            // Loading after we started waiting (unlikely race).
            tracing::warn!("embed service not ready during session warming")

        Err(ServerError::EmbedFailed(msg)) =>
            // Model failed to load. Skip warming. ContextSearch will
            // return empty results (FR-02.6, FR-04.4).
            tracing::warn!("embed service failed: {msg}, skipping warmup")

        Err(e) =>
            tracing::warn!("unexpected embed error during warming: {e}")
```

## Warming Characteristics

- **Blocking from server perspective**: The `SessionRegister` handler awaits warming before returning `Ack`. This blocks the UDS connection handler for this specific connection.
- **Non-blocking from hook perspective**: The hook process fires `SessionRegister` as fire-and-forget and exits immediately. It never sees the `Ack`.
- **Idempotent**: Multiple `SessionStart` events trigger multiple `get_adapter()` calls, but `get_adapter()` returns immediately if the model is already loaded. The warmup `embed_entry("", "warmup")` is also idempotent (cheap no-op when already warm).
- **Concurrency safe**: The `EmbedServiceHandle` uses `RwLock<EmbedState>`. Multiple concurrent `get_adapter()` calls are safe. The `spawn_blocking` for warmup embedding is isolated.

## Error Handling

- EmbedNotReady: log warning, return Ack anyway (warmup is best-effort)
- EmbedFailed: log warning, return Ack anyway (model cannot be used)
- spawn_blocking failure: log warning, return Ack anyway
- Warmup embed_entry failure: log warning, return Ack anyway (adapter is still Ready)

In all error cases, the handler returns `Ack` because:
1. The hook process already disconnected (fire-and-forget)
2. Warming failure doesn't prevent session registration
3. ContextSearch has its own fallback for EmbedNotReady/EmbedFailed

## Key Test Scenarios

1. SessionRegister with healthy embed service: warming completes, Ack returned
2. SessionRegister with EmbedFailed: warning logged, Ack returned, no panic
3. SessionRegister with EmbedNotReady: warning logged, Ack returned
4. Multiple SessionRegister calls: idempotent warming
5. ContextSearch immediately after warming: returns results (not empty)
6. ContextSearch without prior SessionRegister: returns empty (EmbedNotReady)
