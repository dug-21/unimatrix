//! Embed model readiness wait loop for eval runner (nan-007).
//!
//! After `EvalServiceLayer::from_profile()` starts the embed model loading in
//! the background, the runner must poll until the model is ready before
//! replaying scenarios. Without this guard, the first scenario's `search()`
//! call will fail with `EmbedNotReady` if the ONNX model has not yet loaded.
//!
//! Pseudocode: eval-runner.md lines 148–158.

use std::sync::Arc;
use std::time::Duration;

use crate::infra::embed_handle::EmbedServiceHandle;

/// Maximum number of 100 ms poll attempts before giving up (eval-runner.md line 153).
const MAX_EMBED_WAIT_ATTEMPTS: u32 = 30;

/// Poll interval for embed model readiness (eval-runner.md line 154).
const EMBED_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Wait for the embedding model to finish loading before scenario replay.
///
/// Polls `handle.get_adapter()` up to `MAX_EMBED_WAIT_ATTEMPTS` times with
/// `EMBED_POLL_INTERVAL` between each attempt. Returns `Ok(())` when the model
/// is ready, or `Err` if all attempts are exhausted or the handle reports a
/// permanent failure (`EmbedFailed`).
///
/// This mirrors the `TestHarness` readiness pattern used in integration tests.
pub(super) async fn wait_for_embed_model(
    handle: &Arc<EmbedServiceHandle>,
    profile_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut attempts: u32 = 0;
    loop {
        match handle.get_adapter().await {
            Ok(_) => {
                tracing::debug!(profile = profile_name, attempts, "embed model ready");
                return Ok(());
            }
            Err(e) if attempts < MAX_EMBED_WAIT_ATTEMPTS => {
                tracing::debug!(
                    profile = profile_name,
                    attempt = attempts + 1,
                    max = MAX_EMBED_WAIT_ATTEMPTS,
                    "embed model not yet ready, polling"
                );
                tokio::time::sleep(EMBED_POLL_INTERVAL).await;
                attempts += 1;
            }
            Err(e) => {
                return Err(format!(
                    "embed model failed to load for profile '{}' after {} attempts: {e}",
                    profile_name, attempts
                )
                .into());
            }
        }
    }
}
