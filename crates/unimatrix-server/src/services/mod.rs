//! Transport-agnostic service layer for vnc-006.
//!
//! Provides SearchService, StoreService, ConfidenceService unified behind
//! ServiceLayer, with SecurityGateway enforcing S1/S3/S4/S5 invariants.

use std::fmt;
use std::sync::Arc;

use unimatrix_core::{
    CoreError, Store, StoreAdapter, VectorAdapter, VectorIndex,
};
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_store::StoreError;

use unimatrix_adapt::AdaptationService;

use crate::audit::AuditLog;
use crate::embed_handle::EmbedServiceHandle;
use crate::error::ServerError;
use crate::registry::TrustLevel;

pub(crate) mod confidence;
pub(crate) mod gateway;
pub(crate) mod search;
pub(crate) mod store_correct;
pub(crate) mod store_ops;

pub(crate) use confidence::ConfidenceService;
pub(crate) use gateway::SecurityGateway;
pub(crate) use search::{SearchService, ServiceSearchParams};
pub(crate) use store_ops::StoreService;

// ---------------------------------------------------------------------------
// AuditContext
// ---------------------------------------------------------------------------

/// Transport-provided context for audit and retrospective compatibility.
///
/// Fields are part of the service contract; some are consumed by audit emission
/// which will be fully migrated to services in a follow-up.
#[allow(dead_code)]
pub(crate) struct AuditContext {
    pub source: AuditSource,
    pub caller_id: String,
    pub session_id: Option<String>,
    pub feature_cycle: Option<String>,
}

/// Identifies the caller's transport origin.
#[allow(dead_code)]
pub(crate) enum AuditSource {
    Mcp {
        agent_id: String,
        trust_level: TrustLevel,
    },
    Uds {
        uid: u32,
        pid: Option<u32>,
        session_id: String,
    },
    Internal {
        service: String,
    },
}

// ---------------------------------------------------------------------------
// ServiceError
// ---------------------------------------------------------------------------

/// Service-specific error type that maps to both MCP ErrorData and UDS HookResponse::Error.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum ServiceError {
    /// S1: Content scan rejection (writes only).
    ContentRejected { category: String, description: String },
    /// S3: Input validation failure.
    ValidationFailed(String),
    /// Core/store error.
    Core(CoreError),
    /// Embedding error.
    EmbeddingFailed(String),
    /// Entry not found.
    NotFound(u64),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::ContentRejected {
                category,
                description,
            } => write!(f, "content rejected ({category}): {description}"),
            ServiceError::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            ServiceError::Core(e) => write!(f, "core error: {e}"),
            ServiceError::EmbeddingFailed(msg) => write!(f, "embedding failed: {msg}"),
            ServiceError::NotFound(id) => write!(f, "entry not found: {id}"),
        }
    }
}

impl From<CoreError> for ServiceError {
    fn from(e: CoreError) -> Self {
        ServiceError::Core(e)
    }
}

impl From<ServiceError> for ServerError {
    fn from(e: ServiceError) -> Self {
        match e {
            ServiceError::ContentRejected {
                category,
                description,
            } => ServerError::ContentScanRejected {
                category,
                description,
            },
            ServiceError::ValidationFailed(msg) => ServerError::InvalidInput {
                field: "service".to_string(),
                reason: msg,
            },
            ServiceError::Core(e) => ServerError::Core(e),
            ServiceError::EmbeddingFailed(msg) => {
                ServerError::EmbedFailed(msg)
            }
            ServiceError::NotFound(id) => {
                ServerError::Core(CoreError::Store(StoreError::EntryNotFound(id)))
            }
        }
    }
}

impl From<ServiceError> for rmcp::ErrorData {
    fn from(e: ServiceError) -> Self {
        let server_err: ServerError = e.into();
        rmcp::ErrorData::from(server_err)
    }
}

// ---------------------------------------------------------------------------
// ServiceLayer
// ---------------------------------------------------------------------------

/// Aggregate struct providing access to all services.
///
/// Public for main.rs to construct and pass to both MCP and UDS transports.
/// Internal service types remain pub(crate).
#[derive(Clone)]
pub struct ServiceLayer {
    pub(crate) search: SearchService,
    pub(crate) store_ops: StoreService,
    pub(crate) confidence: ConfidenceService,
}

impl ServiceLayer {
    pub fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        audit: Arc<AuditLog>,
    ) -> Self {
        let gateway = Arc::new(SecurityGateway::new(Arc::clone(&audit)));

        let search = SearchService::new(
            Arc::clone(&store),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&gateway),
        );

        let store_ops = StoreService::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&gateway),
            Arc::clone(&audit),
        );

        let confidence = ConfidenceService::new(Arc::clone(&store));

        ServiceLayer {
            search,
            store_ops,
            confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_error_display_content_rejected() {
        let err = ServiceError::ContentRejected {
            category: "InstructionOverride".to_string(),
            description: "injection detected".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("InstructionOverride"));
        assert!(msg.contains("injection detected"));
    }

    #[test]
    fn service_error_display_validation_failed() {
        let err = ServiceError::ValidationFailed("query too long".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("query too long"));
    }

    #[test]
    fn service_error_display_not_found() {
        let err = ServiceError::NotFound(42);
        let msg = format!("{err}");
        assert!(msg.contains("42"));
    }

    #[test]
    fn service_error_display_embedding_failed() {
        let err = ServiceError::EmbeddingFailed("model not loaded".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("model not loaded"));
    }

    #[test]
    fn service_error_to_server_error_content_rejected() {
        let err = ServiceError::ContentRejected {
            category: "EmailAddress".to_string(),
            description: "email detected".to_string(),
        };
        let server_err: ServerError = err.into();
        assert!(matches!(server_err, ServerError::ContentScanRejected { .. }));
    }

    #[test]
    fn service_error_to_server_error_validation() {
        let err = ServiceError::ValidationFailed("bad input".to_string());
        let server_err: ServerError = err.into();
        assert!(matches!(server_err, ServerError::InvalidInput { .. }));
    }

    #[test]
    fn service_error_to_server_error_not_found() {
        let err = ServiceError::NotFound(99);
        let server_err: ServerError = err.into();
        assert!(matches!(server_err, ServerError::Core(CoreError::Store(StoreError::EntryNotFound(99)))));
    }
}
