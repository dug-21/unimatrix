# Pseudocode: embed_handle.rs (C8 — Embed Service Handle)

## Purpose

Lazy-loading wrapper around the embedding service. Implements a state machine (Loading -> Ready | Failed) that allows the MCP server to start immediately without blocking on model download.

## Types

```
struct EmbedServiceHandle {
    state: RwLock<EmbedState>,
}

enum EmbedState {
    Loading,
    Ready(Arc<EmbedAdapter>),
    Failed(String),
}
```

Uses `tokio::sync::RwLock` for async-compatible read/write locking. Most calls are reads (checking state); writes happen once (state transition).

## Functions

### EmbedServiceHandle::new() -> Arc<Self>

```
Arc::new(EmbedServiceHandle {
    state: RwLock::new(EmbedState::Loading),
})
```

### EmbedServiceHandle::start_loading(self: &Arc<Self>, config: EmbedConfig)

```
let handle = Arc::clone(self)

tokio::spawn(async move {
    // OnnxProvider::new is blocking (downloads model) — must use spawn_blocking
    let result = tokio::task::spawn_blocking(move || {
        OnnxProvider::new(config)
    }).await;

    let mut state = handle.state.write().await;
    MATCH result:
        Ok(Ok(provider)) => {
            let provider_arc: Arc<dyn EmbeddingProvider> = Arc::new(provider);
            let adapter = EmbedAdapter::new(provider_arc);
            *state = EmbedState::Ready(Arc::new(adapter));
            tracing::info!("embedding model loaded successfully");
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            *state = EmbedState::Failed(msg.clone());
            tracing::error!(error = %msg, "embedding model failed to load");
        }
        Err(join_err) => {
            let msg = join_err.to_string();
            *state = EmbedState::Failed(msg.clone());
            tracing::error!(error = %msg, "embedding model load task panicked");
        }
});
```

Key: `OnnxProvider::new` is called inside `spawn_blocking` because it's a blocking operation (file I/O, possible network download). The outer `tokio::spawn` allows the server to continue starting while the model loads in the background.

### EmbedServiceHandle::get_adapter(&self) -> Result<Arc<EmbedAdapter>, ServerError>

```
let state = self.state.read().await;
MATCH &*state:
    EmbedState::Ready(adapter) => Ok(Arc::clone(adapter))
    EmbedState::Loading => Err(ServerError::EmbedNotReady)
    EmbedState::Failed(msg) => Err(ServerError::EmbedFailed(msg.clone()))
```

This is the primary access method. Tool handlers that need embeddings call this.

### EmbedServiceHandle::is_ready(&self) -> bool

```
// Blocking check for non-async contexts (e.g., logging)
// Use try_read() to avoid blocking
MATCH self.state.try_read():
    Ok(guard) => matches!(&*guard, EmbedState::Ready(_))
    Err(_) => false  // lock contended, conservatively say not ready
```

Note: Since we use `tokio::sync::RwLock`, `is_ready()` needs to be either async or use `try_read()`. For a synchronous check, `try_read()` is appropriate.

## State Machine

```
Loading --[OnnxProvider::new succeeds]--> Ready(adapter)
Loading --[OnnxProvider::new fails]----> Failed(message)
Loading --[spawn_blocking panics]------> Failed(message)
```

Transitions are one-way. Once Failed, the handle stays Failed for the session lifetime. Restart the server to retry.

## Error Handling

- `get_adapter()` returns `ServerError::EmbedNotReady` when Loading, `ServerError::EmbedFailed` when Failed
- The background task catches both `OnnxProvider::new` errors and `spawn_blocking` join errors
- State transitions use write lock; reads use read lock (concurrent reads are fine)

## Key Test Scenarios

1. New handle starts in Loading state
2. After successful load, handle is in Ready state and get_adapter returns Ok
3. After failed load, handle is in Failed state and get_adapter returns EmbedFailed
4. is_ready returns false when Loading, true when Ready, false when Failed
5. get_adapter returns EmbedNotReady when called before model loads
6. State transitions are one-way (Ready doesn't go back to Loading)
