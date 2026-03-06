//! Lazy-loading wrapper around the embedding service.
//!
//! Implements a state machine (Loading -> Ready | Failed -> Retrying) that allows
//! the MCP server to start immediately without blocking on model download.
//! On failure, the next `get_adapter()` call triggers an automatic retry (#52).

use std::sync::Arc;

use tokio::sync::RwLock;
use unimatrix_core::EmbedAdapter;
use unimatrix_embed::{EmbedConfig, EmbeddingProvider, OnnxProvider};

use crate::error::ServerError;

/// Maximum number of automatic retry attempts before giving up permanently.
const MAX_RETRIES: u32 = 3;

/// State of the embedding service.
enum EmbedState {
    /// Model is being downloaded/loaded in the background.
    Loading,
    /// Model loaded successfully.
    Ready(Arc<EmbedAdapter>),
    /// Model failed to load. Retryable if attempts < MAX_RETRIES.
    Failed { message: String, attempts: u32 },
    /// Retry in progress (loading after a previous failure).
    Retrying { attempt: u32 },
}

/// Lazy-loading handle for the embedding service.
///
/// Created in `Loading` state. Transitions to `Ready` or `Failed`
/// when the background loading task completes. On failure, the next
/// `get_adapter()` call automatically triggers a retry up to `MAX_RETRIES`
/// times (#52).
pub struct EmbedServiceHandle {
    state: RwLock<EmbedState>,
    config: RwLock<Option<EmbedConfig>>,
}

impl EmbedServiceHandle {
    /// Create a new handle in Loading state.
    pub fn new() -> Arc<Self> {
        Arc::new(EmbedServiceHandle {
            state: RwLock::new(EmbedState::Loading),
            config: RwLock::new(None),
        })
    }

    /// Start loading the embedding model in a background task.
    ///
    /// The task downloads the model if not cached, then transitions
    /// the handle to Ready or Failed. If loading fails, a retry monitor
    /// automatically re-attempts up to `MAX_RETRIES` times (#52).
    pub fn start_loading(self: &Arc<Self>, config: EmbedConfig) {
        // Store config for potential retries.
        {
            let mut cfg = self.config.try_write().expect("config lock uncontended at startup");
            *cfg = Some(config.clone());
        }
        self.spawn_load_task(config, 1);
        self.spawn_retry_monitor();
    }

    /// Spawn the background model loading task.
    fn spawn_load_task(self: &Arc<Self>, config: EmbedConfig, attempt: u32) {
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
                    if attempt > 1 {
                        tracing::info!(attempt, "embedding model loaded successfully (retry)");
                    } else {
                        tracing::info!("embedding model loaded successfully");
                    }
                }
                Ok(Err(e)) => {
                    let msg = e.to_string();
                    *state = EmbedState::Failed { message: msg.clone(), attempts: attempt };
                    tracing::error!(error = %msg, attempt, "embedding model failed to load");
                }
                Err(join_err) => {
                    let msg = join_err.to_string();
                    *state = EmbedState::Failed { message: msg.clone(), attempts: attempt };
                    tracing::error!(error = %msg, attempt, "embedding model load task panicked");
                }
            }
        });
    }

    /// Get the adapter if the model is ready.
    ///
    /// Returns `EmbedNotReady` if still loading or retrying.
    /// Returns `EmbedFailed` if all retry attempts are exhausted.
    /// The retry monitor (spawned by `start_loading`) automatically handles
    /// re-attempts on failure (#52).
    pub async fn get_adapter(&self) -> Result<Arc<EmbedAdapter>, ServerError> {
        let state = self.state.read().await;
        match &*state {
            EmbedState::Ready(adapter) => Ok(Arc::clone(adapter)),
            EmbedState::Loading | EmbedState::Retrying { .. } => Err(ServerError::EmbedNotReady),
            EmbedState::Failed { message, attempts } => {
                if *attempts < MAX_RETRIES {
                    // Retry monitor will handle this; report as not-ready so callers
                    // know the model may become available.
                    Err(ServerError::EmbedNotReady)
                } else {
                    Err(ServerError::EmbedFailed(message.clone()))
                }
            }
        }
    }

    /// Background retry monitor: watches for `Failed` state and automatically
    /// re-attempts loading up to `MAX_RETRIES` times with exponential backoff (#52).
    ///
    /// Runs until the state reaches `Ready` or retries are exhausted.
    fn spawn_retry_monitor(self: &Arc<Self>) {
        let handle = Arc::clone(self);
        tokio::spawn(async move {
            // Base delay: 10 seconds, doubles each retry (10s, 20s, 40s).
            let base_delay = std::time::Duration::from_secs(10);

            loop {
                // Wait before checking — gives the initial load time to complete.
                tokio::time::sleep(base_delay).await;

                let mut state = handle.state.write().await;
                match &*state {
                    EmbedState::Ready(_) => {
                        // Model loaded, monitor done.
                        return;
                    }
                    EmbedState::Loading | EmbedState::Retrying { .. } => {
                        // Load in progress, wait and re-check.
                        drop(state);
                        continue;
                    }
                    EmbedState::Failed { attempts, .. } if *attempts >= MAX_RETRIES => {
                        // Retries exhausted, monitor done.
                        tracing::warn!(
                            attempts = *attempts,
                            "embedding model retries exhausted, giving up"
                        );
                        return;
                    }
                    EmbedState::Failed { attempts, .. } => {
                        let next_attempt = *attempts + 1;
                        let config = handle.config.read().await;
                        if let Some(cfg) = config.as_ref() {
                            let delay = base_delay * 2u32.saturating_pow(next_attempt - 1);
                            tracing::info!(
                                attempt = next_attempt,
                                max = MAX_RETRIES,
                                delay_secs = delay.as_secs(),
                                "retrying embedding model load"
                            );
                            let cfg_clone = cfg.clone();
                            *state = EmbedState::Retrying { attempt: next_attempt };
                            drop(config);
                            drop(state);

                            // Backoff before spawning retry.
                            tokio::time::sleep(delay).await;
                            handle.spawn_load_task(cfg_clone, next_attempt);
                        } else {
                            // No config available, cannot retry.
                            return;
                        }
                    }
                }
            }
        });
    }

    /// Check if the model is ready (non-blocking).
    pub fn is_ready(&self) -> bool {
        match self.state.try_read() {
            Ok(guard) => matches!(&*guard, EmbedState::Ready(_)),
            Err(_) => false,
        }
    }

    /// Try to get the adapter synchronously (non-blocking).
    ///
    /// Returns `None` if the model is not ready or the lock is contended.
    /// Used by adaptation training to get embeddings in a blocking context.
    /// Does NOT trigger retry (retry requires async context).
    pub fn try_get_adapter_sync(&self) -> Option<Arc<EmbedAdapter>> {
        match self.state.try_read() {
            Ok(guard) => match &*guard {
                EmbedState::Ready(adapter) => Some(Arc::clone(adapter)),
                _ => None,
            },
            Err(_) => None,
        }
    }

    /// Set state directly for testing.
    #[cfg(test)]
    async fn set_failed_for_test(&self, msg: String, attempts: u32) {
        let mut state = self.state.write().await;
        *state = EmbedState::Failed { message: msg, attempts };
    }

    /// Check current attempt count for testing.
    #[cfg(test)]
    async fn current_attempts(&self) -> u32 {
        let state = self.state.read().await;
        match &*state {
            EmbedState::Failed { attempts, .. } => *attempts,
            EmbedState::Retrying { attempt } => *attempt,
            _ => 0,
        }
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
    async fn test_failed_state_with_retries_exhausted() {
        let handle = EmbedServiceHandle::new();
        // Set attempts = MAX_RETRIES so no retry is triggered.
        handle
            .set_failed_for_test("test error".to_string(), MAX_RETRIES)
            .await;

        assert!(!handle.is_ready());
        let result = handle.get_adapter().await;
        assert!(matches!(result, Err(ServerError::EmbedFailed(_))));
        if let Err(ServerError::EmbedFailed(msg)) = result {
            assert_eq!(msg, "test error");
        }
    }

    #[tokio::test]
    async fn test_failed_with_retries_remaining_returns_not_ready() {
        let handle = EmbedServiceHandle::new();
        // Set attempts = 1 (< MAX_RETRIES), so get_adapter reports as not-ready
        // (the retry monitor would handle the actual retry in production).
        handle
            .set_failed_for_test("transient error".to_string(), 1)
            .await;

        let result = handle.get_adapter().await;
        assert!(
            matches!(result, Err(ServerError::EmbedNotReady)),
            "should return EmbedNotReady when retries remain"
        );
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
            .set_failed_for_test("error".to_string(), MAX_RETRIES)
            .await;
        assert!(!handle.is_ready());
    }

    #[tokio::test]
    async fn test_max_retries_constant() {
        // Verify the retry limit is reasonable.
        assert!(MAX_RETRIES >= 2, "should allow at least 2 retries");
        assert!(MAX_RETRIES <= 10, "should not retry excessively");
    }

    #[tokio::test]
    async fn test_current_attempts_tracking() {
        let handle = EmbedServiceHandle::new();
        assert_eq!(handle.current_attempts().await, 0);

        handle.set_failed_for_test("err".to_string(), 2).await;
        assert_eq!(handle.current_attempts().await, 2);
    }
}
