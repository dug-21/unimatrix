//! Lazy-loading wrapper around the embedding service.
//!
//! Implements a state machine (Loading -> Ready | Failed) that allows
//! the MCP server to start immediately without blocking on model download.

use std::sync::Arc;

use tokio::sync::RwLock;
use unimatrix_core::EmbedAdapter;
use unimatrix_embed::{EmbedConfig, EmbeddingProvider, OnnxProvider};

use crate::error::ServerError;

/// State of the embedding service.
enum EmbedState {
    /// Model is being downloaded/loaded in the background.
    Loading,
    /// Model loaded successfully.
    Ready(Arc<EmbedAdapter>),
    /// Model failed to load.
    Failed(String),
}

/// Lazy-loading handle for the embedding service.
///
/// Created in `Loading` state. Transitions to `Ready` or `Failed`
/// when the background loading task completes.
pub struct EmbedServiceHandle {
    state: RwLock<EmbedState>,
}

impl EmbedServiceHandle {
    /// Create a new handle in Loading state.
    pub fn new() -> Arc<Self> {
        Arc::new(EmbedServiceHandle {
            state: RwLock::new(EmbedState::Loading),
        })
    }

    /// Start loading the embedding model in a background task.
    ///
    /// The task downloads the model if not cached, then transitions
    /// the handle to Ready or Failed.
    pub fn start_loading(self: &Arc<Self>, config: EmbedConfig) {
        let handle = Arc::clone(self);

        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || OnnxProvider::new(config)).await;

            let mut state = handle.state.write().await;
            match result {
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
            }
        });
    }

    /// Get the adapter if the model is ready.
    ///
    /// Returns `EmbedNotReady` if still loading, `EmbedFailed` if failed.
    pub async fn get_adapter(&self) -> Result<Arc<EmbedAdapter>, ServerError> {
        let state = self.state.read().await;
        match &*state {
            EmbedState::Ready(adapter) => Ok(Arc::clone(adapter)),
            EmbedState::Loading => Err(ServerError::EmbedNotReady),
            EmbedState::Failed(msg) => Err(ServerError::EmbedFailed(msg.clone())),
        }
    }

    /// Check if the model is ready (non-blocking).
    pub fn is_ready(&self) -> bool {
        match self.state.try_read() {
            Ok(guard) => matches!(&*guard, EmbedState::Ready(_)),
            Err(_) => false,
        }
    }

    /// Set state directly for testing.
    #[cfg(test)]
    async fn set_failed_for_test(&self, msg: String) {
        let mut state = self.state.write().await;
        *state = EmbedState::Failed(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_starts_loading() {
        let handle = EmbedServiceHandle::new();
        assert!(!handle.is_ready());
    }

    #[tokio::test]
    async fn test_get_adapter_loading_returns_not_ready() {
        let handle = EmbedServiceHandle::new();
        let result = handle.get_adapter().await;
        assert!(matches!(result, Err(ServerError::EmbedNotReady)));
    }

    #[tokio::test]
    async fn test_failed_state() {
        let handle = EmbedServiceHandle::new();
        handle
            .set_failed_for_test("test error".to_string())
            .await;

        assert!(!handle.is_ready());
        let result = handle.get_adapter().await;
        assert!(matches!(result, Err(ServerError::EmbedFailed(_))));
        if let Err(ServerError::EmbedFailed(msg)) = result {
            assert_eq!(msg, "test error");
        }
    }

    #[tokio::test]
    async fn test_is_ready_false_when_loading() {
        let handle = EmbedServiceHandle::new();
        assert!(!handle.is_ready());
    }

    #[tokio::test]
    async fn test_is_ready_false_when_failed() {
        let handle = EmbedServiceHandle::new();
        handle
            .set_failed_for_test("error".to_string())
            .await;
        assert!(!handle.is_ready());
    }
}
