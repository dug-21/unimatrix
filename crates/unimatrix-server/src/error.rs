//! Server-specific error types and mapping to rmcp's `ErrorData`.

use std::fmt;
use std::path::PathBuf;

use rmcp::model::{ErrorCode, ErrorData};
use unimatrix_core::CoreError;
use unimatrix_store::StoreError;

use crate::infra::registry::Capability;

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

/// MCP error code: content scan rejected.
pub const ERROR_CONTENT_SCAN_REJECTED: ErrorCode = ErrorCode(-32006);

/// MCP error code: invalid category.
pub const ERROR_INVALID_CATEGORY: ErrorCode = ErrorCode(-32007);

/// MCP error code: protected bootstrap agent cannot be modified.
pub const ERROR_PROTECTED_AGENT: ErrorCode = ErrorCode(-32008);

/// MCP error code: caller cannot remove own Admin capability.
pub const ERROR_SELF_LOCKOUT: ErrorCode = ErrorCode(-32009);

/// MCP error code: no observation data available.
pub const ERROR_NO_OBSERVATION_DATA: ErrorCode = ErrorCode(-32010);

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
    /// Input validation failure.
    InvalidInput {
        /// The field that failed validation.
        field: String,
        /// Why validation failed.
        reason: String,
    },
    /// Content scan detected prohibited pattern.
    ContentScanRejected {
        /// The pattern category that matched.
        category: String,
        /// Human-readable description of the match.
        description: String,
    },
    /// Database is locked by another process after exhausting retries.
    DatabaseLocked(PathBuf),
    /// Category not in allowlist.
    InvalidCategory {
        /// The category that was rejected.
        category: String,
        /// Valid categories for guidance.
        valid_categories: Vec<String>,
    },
    /// Attempt to modify a protected bootstrap agent.
    ProtectedAgent {
        /// The agent ID that is protected.
        agent_id: String,
    },
    /// Caller attempted to remove own Admin capability.
    SelfLockout,
    /// Observation analysis failed.
    ObservationError(String),
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
            ServerError::InvalidInput { field, reason } => {
                write!(f, "invalid parameter '{field}': {reason}")
            }
            ServerError::ContentScanRejected {
                category,
                description,
            } => write!(f, "content rejected: {description} ({category} detected)"),
            ServerError::DatabaseLocked(path) => {
                write!(f, "database is locked by another process: {}", path.display())
            }
            ServerError::InvalidCategory {
                category,
                valid_categories,
            } => {
                let list = valid_categories.join(", ");
                write!(f, "unknown category '{category}'. Valid: {list}")
            }
            ServerError::ProtectedAgent { agent_id } => {
                write!(
                    f,
                    "agent '{agent_id}' is a protected bootstrap agent and cannot be modified via enrollment"
                )
            }
            ServerError::SelfLockout => {
                write!(f, "cannot remove Admin capability from the calling agent")
            }
            ServerError::ObservationError(msg) => {
                write!(f, "observation analysis error: {msg}")
            }
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
            ServerError::InvalidInput { field, reason } => ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("Invalid parameter '{field}': {reason}"),
                None,
            ),
            ServerError::ContentScanRejected {
                category,
                description,
            } => ErrorData::new(
                ERROR_CONTENT_SCAN_REJECTED,
                format!(
                    "Content rejected: {description} ({category} detected). Remove the flagged content and retry."
                ),
                None,
            ),
            ServerError::DatabaseLocked(path) => ErrorData::new(
                ERROR_INTERNAL,
                format!(
                    "Database is locked by another process at {}. Kill the other unimatrix-server process, or run: lsof {}",
                    path.display(),
                    path.display()
                ),
                None,
            ),
            ServerError::InvalidCategory {
                category,
                valid_categories,
            } => {
                let mut sorted = valid_categories;
                sorted.sort();
                let list = sorted.join(", ");
                ErrorData::new(
                    ERROR_INVALID_CATEGORY,
                    format!("Unknown category '{category}'. Valid categories: {list}."),
                    None,
                )
            }
            ServerError::ProtectedAgent { agent_id } => ErrorData::new(
                ERROR_PROTECTED_AGENT,
                format!(
                    "Agent '{agent_id}' is a protected bootstrap agent and cannot be modified via enrollment."
                ),
                None,
            ),
            ServerError::SelfLockout => ErrorData::new(
                ERROR_SELF_LOCKOUT,
                "Cannot remove Admin capability from the calling agent. This would cause lockout.",
                None,
            ),
            ServerError::ObservationError(msg) => ErrorData::new(
                ERROR_NO_OBSERVATION_DATA,
                format!("Observation analysis error: {msg}"),
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

    #[test]
    fn test_invalid_input_maps_to_32602() {
        let err = ServerError::InvalidInput {
            field: "title".to_string(),
            reason: "exceeds 200 characters".to_string(),
        };
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_INVALID_PARAMS);
        assert!(data.message.contains("title"));
        assert!(data.message.contains("200"));
    }

    #[test]
    fn test_content_scan_rejected_maps_to_32006() {
        let err = ServerError::ContentScanRejected {
            category: "InstructionOverride".to_string(),
            description: "instruction override attempt detected".to_string(),
        };
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_CONTENT_SCAN_REJECTED);
        assert!(data.message.contains("InstructionOverride"));
        assert!(data.message.contains("Remove the flagged content"));
    }

    #[test]
    fn test_invalid_category_maps_to_32007() {
        let err = ServerError::InvalidCategory {
            category: "unknown".to_string(),
            valid_categories: vec![
                "convention".to_string(),
                "decision".to_string(),
                "outcome".to_string(),
            ],
        };
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_INVALID_CATEGORY);
        assert!(data.message.contains("unknown"));
        assert!(data.message.contains("convention"));
    }

    #[test]
    fn test_display_invalid_input() {
        let err = ServerError::InvalidInput {
            field: "query".to_string(),
            reason: "too long".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("query"));
        assert!(msg.contains("too long"));
        assert!(!msg.contains("ServerError"));
    }

    #[test]
    fn test_display_content_scan_rejected() {
        let err = ServerError::ContentScanRejected {
            category: "EmailAddress".to_string(),
            description: "email detected".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("EmailAddress"));
        assert!(msg.contains("email detected"));
    }

    #[test]
    fn test_display_invalid_category() {
        let err = ServerError::InvalidCategory {
            category: "bogus".to_string(),
            valid_categories: vec!["a".to_string(), "b".to_string()],
        };
        let msg = format!("{err}");
        assert!(msg.contains("bogus"));
        assert!(msg.contains("a, b"));
    }

    #[test]
    fn test_database_locked_display() {
        let err = ServerError::DatabaseLocked(PathBuf::from("/tmp/test.db"));
        let msg = format!("{err}");
        assert!(msg.contains("locked"), "should mention locked: {msg}");
        assert!(
            msg.contains("/tmp/test.db"),
            "should contain path: {msg}"
        );
        assert!(
            !msg.contains("ServerError"),
            "should not leak Rust type: {msg}"
        );
    }

    #[test]
    fn test_database_locked_error_data_code() {
        let err = ServerError::DatabaseLocked(PathBuf::from("/data/unimatrix.db"));
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_INTERNAL);
    }

    #[test]
    fn test_database_locked_error_data_message() {
        let err = ServerError::DatabaseLocked(PathBuf::from("/data/unimatrix.db"));
        let data: ErrorData = err.into();
        assert!(
            data.message.contains("/data/unimatrix.db"),
            "message should contain path: {}",
            data.message
        );
        assert!(
            data.message.contains("lsof"),
            "message should contain lsof hint: {}",
            data.message
        );
    }

    // -- alc-002: ProtectedAgent and SelfLockout --

    #[test]
    fn test_protected_agent_display() {
        let err = ServerError::ProtectedAgent {
            agent_id: "system".to_string(),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("system"),
            "should contain agent_id: {msg}"
        );
        assert!(
            msg.contains("protected bootstrap agent"),
            "should describe protection: {msg}"
        );
        assert!(
            !msg.contains("ServerError"),
            "should not leak Rust type: {msg}"
        );
    }

    #[test]
    fn test_self_lockout_display() {
        let err = ServerError::SelfLockout;
        let msg = format!("{err}");
        assert!(
            msg.contains("Admin"),
            "should mention Admin: {msg}"
        );
        assert!(
            !msg.contains("ServerError"),
            "should not leak Rust type: {msg}"
        );
    }

    #[test]
    fn test_protected_agent_maps_to_32008() {
        let err = ServerError::ProtectedAgent {
            agent_id: "system".to_string(),
        };
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_PROTECTED_AGENT);
    }

    #[test]
    fn test_self_lockout_maps_to_32009() {
        let err = ServerError::SelfLockout;
        let data: ErrorData = err.into();
        assert_eq!(data.code, ERROR_SELF_LOCKOUT);
    }

    #[test]
    fn test_protected_agent_error_message_contains_agent_id() {
        let err = ServerError::ProtectedAgent {
            agent_id: "test-agent".to_string(),
        };
        let data: ErrorData = err.into();
        assert!(
            data.message.contains("test-agent"),
            "message should contain agent_id: {}",
            data.message
        );
    }

    #[test]
    fn test_self_lockout_error_message_actionable() {
        let err = ServerError::SelfLockout;
        let data: ErrorData = err.into();
        assert!(
            data.message.contains("Admin"),
            "message should mention Admin: {}",
            data.message
        );
        assert!(
            data.message.contains("lockout"),
            "message should mention lockout: {}",
            data.message
        );
    }
}
