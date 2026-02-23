//! Server-specific error types and mapping to rmcp's `ErrorData`.

use std::fmt;

use rmcp::model::{ErrorCode, ErrorData};
use unimatrix_core::CoreError;
use unimatrix_store::StoreError;

use crate::registry::Capability;

/// MCP error code: entry not found.
pub const ERROR_ENTRY_NOT_FOUND: ErrorCode = ErrorCode(-32001);

/// MCP error code: invalid parameters (standard JSON-RPC).
pub const ERROR_INVALID_PARAMS: ErrorCode = ErrorCode(-32602);

/// MCP error code: capability denied.
pub const ERROR_CAPABILITY_DENIED: ErrorCode = ErrorCode(-32003);

/// MCP error code: embedding model not ready.
pub const ERROR_EMBED_NOT_READY: ErrorCode = ErrorCode(-32004);

/// MCP error code: tool not yet implemented.
pub const ERROR_NOT_IMPLEMENTED: ErrorCode = ErrorCode(-32005);

/// MCP error code: internal server error (standard JSON-RPC).
pub const ERROR_INTERNAL: ErrorCode = ErrorCode(-32603);

/// Server-specific error type covering all failure modes.
#[derive(Debug)]
pub enum ServerError {
    /// Error from unimatrix-core (store, vector, embed).
    Core(CoreError),
    /// Agent registry operation failed.
    Registry(String),
    /// Audit log operation failed.
    Audit(String),
    /// Project initialization failed.
    ProjectInit(String),
    /// Embedding model is still loading.
    EmbedNotReady,
    /// Embedding model failed to load.
    EmbedFailed(String),
    /// Agent lacks required capability.
    CapabilityDenied {
        /// The agent that was denied.
        agent_id: String,
        /// The capability that was required.
        capability: Capability,
    },
    /// Tool not yet implemented (vnc-001 stubs).
    NotImplemented(String),
    /// Shutdown sequence error.
    Shutdown(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::Core(e) => write!(f, "internal error: {e}"),
            ServerError::Registry(msg) => write!(f, "registry error: {msg}"),
            ServerError::Audit(msg) => write!(f, "audit error: {msg}"),
            ServerError::ProjectInit(msg) => write!(f, "project initialization failed: {msg}"),
            ServerError::EmbedNotReady => write!(f, "embedding model is initializing"),
            ServerError::EmbedFailed(msg) => write!(f, "embedding model failed: {msg}"),
            ServerError::CapabilityDenied {
                agent_id,
                capability,
            } => write!(f, "agent '{agent_id}' lacks {capability:?} capability"),
            ServerError::NotImplemented(tool) => {
                write!(f, "tool '{tool}' is not yet implemented")
            }
            ServerError::Shutdown(msg) => write!(f, "shutdown error: {msg}"),
        }
    }
}

impl std::error::Error for ServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ServerError::Core(e) => Some(e),
            _ => None,
        }
    }
}

impl From<CoreError> for ServerError {
    fn from(e: CoreError) -> Self {
        ServerError::Core(e)
    }
}

impl From<ServerError> for ErrorData {
    fn from(err: ServerError) -> Self {
        match err {
            ServerError::Core(CoreError::Store(StoreError::EntryNotFound(id))) => ErrorData::new(
                ERROR_ENTRY_NOT_FOUND,
                format!("Entry {id} not found. Verify the ID from a previous search result."),
                None,
            ),
            ServerError::Core(_) => ErrorData::new(
                ERROR_INTERNAL,
                "Internal storage error. The operation was not completed.",
                None,
            ),
            ServerError::CapabilityDenied {
                agent_id,
                capability,
            } => ErrorData::new(
                ERROR_CAPABILITY_DENIED,
                format!("Agent '{agent_id}' lacks {capability:?} capability. Contact project admin."),
                None,
            ),
            ServerError::EmbedNotReady => ErrorData::new(
                ERROR_EMBED_NOT_READY,
                "Embedding model is initializing. Try again in a few seconds, or use context_lookup which does not require embeddings.",
                None,
            ),
            ServerError::EmbedFailed(msg) => ErrorData::new(
                ERROR_EMBED_NOT_READY,
                format!("Embedding model failed to load: {msg}. Restart the server to retry."),
                None,
            ),
            ServerError::NotImplemented(tool) => ErrorData::new(
                ERROR_NOT_IMPLEMENTED,
                format!("Tool '{tool}' is registered but not yet implemented. Full implementation ships in vnc-002."),
                None,
            ),
            ServerError::Registry(msg) => ErrorData::new(
                ERROR_INTERNAL,
                format!("Agent registry error: {msg}"),
                None,
            ),
            ServerError::Audit(msg) => ErrorData::new(
                ERROR_INTERNAL,
                format!("Audit log error: {msg}"),
                None,
            ),
            ServerError::ProjectInit(msg) => ErrorData::new(
                ERROR_INTERNAL,
                format!("Project initialization failed: {msg}"),
                None,
            ),
            ServerError::Shutdown(msg) => ErrorData::new(
                ERROR_INTERNAL,
                format!("Shutdown error: {msg}"),
                None,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_not_found_maps_to_32001() {
        let err = ServerError::Core(CoreError::Store(StoreError::EntryNotFound(42)));
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_ENTRY_NOT_FOUND);
        let msg = &data.message;
        assert!(msg.contains("42"), "message should contain entry id: {msg}");
        assert!(
            msg.contains("Verify"),
            "message should be actionable: {msg}"
        );
    }

    #[test]
    fn test_core_error_maps_to_32603() {
        let err =
            ServerError::Core(CoreError::Store(StoreError::Serialization("test".to_string())));
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_INTERNAL);
        assert!(data.message.contains("Internal storage error"));
    }

    #[test]
    fn test_capability_denied_maps_to_32003() {
        let err = ServerError::CapabilityDenied {
            agent_id: "test-agent".to_string(),
            capability: Capability::Write,
        };
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_CAPABILITY_DENIED);
        assert!(data.message.contains("test-agent"));
        assert!(data.message.contains("Write"));
    }

    #[test]
    fn test_embed_not_ready_maps_to_32004() {
        let err = ServerError::EmbedNotReady;
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_EMBED_NOT_READY);
        assert!(data.message.contains("context_lookup"));
    }

    #[test]
    fn test_embed_failed_maps_to_32004() {
        let err = ServerError::EmbedFailed("download error".to_string());
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_EMBED_NOT_READY);
        assert!(data.message.contains("download error"));
    }

    #[test]
    fn test_not_implemented_maps_to_32005() {
        let err = ServerError::NotImplemented("context_search".to_string());
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_NOT_IMPLEMENTED);
        assert!(data.message.contains("vnc-002"));
    }

    #[test]
    fn test_registry_error_maps_to_32603() {
        let err = ServerError::Registry("table corrupted".to_string());
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_INTERNAL);
    }

    #[test]
    fn test_display_no_rust_types() {
        let errors: Vec<ServerError> = vec![
            ServerError::Core(CoreError::Store(StoreError::EntryNotFound(1))),
            ServerError::Registry("test".to_string()),
            ServerError::EmbedNotReady,
            ServerError::NotImplemented("test".to_string()),
        ];
        for err in errors {
            let msg = format!("{err}");
            assert!(!msg.contains("StoreError"), "leaked StoreError: {msg}");
            assert!(!msg.contains("CoreError"), "leaked CoreError: {msg}");
        }
    }

    #[test]
    fn test_from_core_error() {
        let core_err = CoreError::Store(StoreError::EntryNotFound(1));
        let server_err: ServerError = core_err.into();
        assert!(matches!(server_err, ServerError::Core(_)));
    }
}
