//! Timeout utilities for spawn_blocking calls (#236).
//!
//! Provides `spawn_blocking_with_timeout` to prevent MCP tool handlers from
//! blocking indefinitely when the Store mutex is held by background tasks.

use std::time::Duration;

use unimatrix_core::CoreError;

use crate::error::ServerError;

/// Default timeout for MCP tool handler spawn_blocking calls (#236).
///
/// 30 seconds is generous enough for normal operations but short enough to
/// prevent indefinite client hangs when background maintenance holds the mutex.
pub const MCP_HANDLER_TIMEOUT: Duration = Duration::from_secs(30);

/// Run a blocking closure on the spawn_blocking pool with a timeout.
///
/// Returns `ServerError` if the task panics or the timeout expires.
/// Use this for MCP tool handler business logic that acquires the Store mutex.
///
/// Do NOT use this for fire-and-forget background writes (usage recording,
/// confidence seeding, etc.) where timeouts would cause data loss.
pub async fn spawn_blocking_with_timeout<F, T>(
    timeout_duration: Duration,
    f: F,
) -> Result<T, ServerError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(join_err)) => Err(ServerError::Core(CoreError::JoinError(format!(
            "task panicked: {join_err}"
        )))),
        Err(_) => Err(ServerError::Core(CoreError::JoinError(
            "operation timed out".to_string(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_blocking_with_timeout_returns_result() {
        let result = spawn_blocking_with_timeout(Duration::from_secs(5), || 42).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_spawn_blocking_with_timeout_on_timeout() {
        let result = spawn_blocking_with_timeout(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_secs(5));
            42
        })
        .await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("timed out"),
            "error should mention timeout: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_spawn_blocking_with_timeout_on_panic() {
        let result =
            spawn_blocking_with_timeout(Duration::from_secs(5), || panic!("test panic")).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("panicked"),
            "error should mention panic: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_spawn_blocking_with_timeout_string_result() {
        let result =
            spawn_blocking_with_timeout(Duration::from_secs(5), || "hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_mcp_handler_timeout_is_30s() {
        assert_eq!(MCP_HANDLER_TIMEOUT.as_secs(), 30);
    }
}
