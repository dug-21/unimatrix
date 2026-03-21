//! Embed model and NLI model readiness wait loops for eval runner (nan-007, crt-023).
//!
//! After `EvalServiceLayer::from_profile()` starts the embed model loading in
//! the background, the runner must poll until the model is ready before
//! replaying scenarios. Without this guard, the first scenario's `search()`
//! call will fail with `EmbedNotReady` if the ONNX model has not yet loaded.
//!
//! crt-023 adds a second wait loop for NLI model readiness (ADR-006).
//! NLI-enabled profiles call `wait_for_nli_ready` after embed model is ready.
//! Profiles that fail NLI readiness are handled as SKIPPED by `run_eval_async`.
//!
//! Pseudocode: eval-runner.md lines 148–158.

use std::sync::Arc;
use std::time::Duration;

use crate::error::ServerError;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::nli_handle::NliServiceHandle;

/// Maximum number of 100 ms poll attempts before giving up (eval-runner.md line 153).
const MAX_EMBED_WAIT_ATTEMPTS: u32 = 30;

/// Poll interval for embed model readiness (eval-runner.md line 154).
const EMBED_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Maximum time to wait for NLI model readiness in eval (ADR-006).
/// 60 seconds covers worst-case download time on a slow network connection.
const MAX_NLI_WAIT_SECS: u64 = 60;

/// Poll interval for NLI model readiness.
const NLI_POLL_INTERVAL: Duration = Duration::from_millis(500);

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
            Err(_e) if attempts < MAX_EMBED_WAIT_ATTEMPTS => {
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

// ---------------------------------------------------------------------------
// NLI model readiness (crt-023, ADR-006)
// ---------------------------------------------------------------------------

/// Reason why an NLI-enabled eval profile was skipped.
#[derive(Debug)]
pub(super) enum NliNotReadyForEval {
    /// NLI model failed to load (missing file, hash mismatch, retries exhausted).
    Failed { profile_name: String },
    /// NLI model did not become ready within `MAX_NLI_WAIT_SECS`.
    Timeout { profile_name: String },
}

impl NliNotReadyForEval {
    pub fn profile_name(&self) -> &str {
        match self {
            NliNotReadyForEval::Failed { profile_name }
            | NliNotReadyForEval::Timeout { profile_name } => profile_name.as_str(),
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            NliNotReadyForEval::Failed { .. } => {
                "NLI model failed to load (missing or hash mismatch)"
            }
            NliNotReadyForEval::Timeout { .. } => "NLI model not ready within 60s timeout",
        }
    }
}

/// Wait for the NLI model to finish loading before scenario replay (ADR-006).
///
/// Polls `handle.get_provider()` until `Ready` or until `MAX_NLI_WAIT_SECS` elapses.
/// Returns `Ok(())` when ready.
/// Returns `Err(NliNotReadyForEval)` when the model is unavailable or fails to load.
///
/// Called from `run_eval_async` ONLY when `nli_handle` is `Some` (NLI-enabled profile).
/// Never called for baseline profiles (`nli_handle = None`).
pub(super) async fn wait_for_nli_ready(
    handle: &Arc<NliServiceHandle>,
    profile_name: &str,
) -> Result<(), NliNotReadyForEval> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(MAX_NLI_WAIT_SECS);
    let mut attempt: u32 = 0;

    loop {
        match handle.get_provider().await {
            Ok(_) => {
                tracing::debug!(profile = profile_name, attempt, "NLI model ready for eval");
                return Ok(());
            }
            Err(ServerError::NliFailed(_)) => {
                // Permanent failure: model not found or hash mismatch or retries exhausted.
                // ADR-006: trigger SKIPPED profile handling.
                tracing::warn!(
                    profile = profile_name,
                    "eval: NLI model failed to load; profile will be SKIPPED"
                );
                return Err(NliNotReadyForEval::Failed {
                    profile_name: profile_name.to_string(),
                });
            }
            Err(ServerError::NliNotReady) => {
                // Still loading — poll again if within deadline.
                if tokio::time::Instant::now() >= deadline {
                    tracing::warn!(
                        profile = profile_name,
                        timeout_secs = MAX_NLI_WAIT_SECS,
                        "eval: NLI model not ready within timeout; profile will be SKIPPED"
                    );
                    return Err(NliNotReadyForEval::Timeout {
                        profile_name: profile_name.to_string(),
                    });
                }
                tracing::debug!(
                    profile = profile_name,
                    attempt = attempt + 1,
                    "NLI model loading, polling"
                );
                tokio::time::sleep(NLI_POLL_INTERVAL).await;
                attempt += 1;
            }
            Err(_other) => {
                // Unexpected error — treat as Failed for eval purposes.
                return Err(NliNotReadyForEval::Failed {
                    profile_name: profile_name.to_string(),
                });
            }
        }
    }
}
