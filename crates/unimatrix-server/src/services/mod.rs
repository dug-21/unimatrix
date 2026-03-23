//! Transport-agnostic service layer for vnc-006.
//!
//! Provides SearchService, StoreService, ConfidenceService unified behind
//! ServiceLayer, with SecurityGateway enforcing S1/S2/S3/S4/S5 invariants.

use std::fmt;
use std::sync::Arc;

use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{CoreError, Store, VectorAdapter, VectorIndex};
use unimatrix_store::StoreError;

use unimatrix_adapt::AdaptationService;

use unimatrix_observe::domain::DomainPackRegistry;

use crate::error::ServerError;
use crate::infra::audit::AuditLog;
use crate::infra::config::InferenceConfig;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;
use crate::infra::registry::TrustLevel;
use crate::infra::usage_dedup::UsageDedup;
use crate::services::store_ops::NliStoreConfig;

pub(crate) mod briefing;
pub(crate) mod confidence;
pub(crate) mod contradiction_cache;
pub(crate) mod effectiveness;
pub(crate) mod gateway;
pub(crate) mod nli_detection;
pub(crate) mod observation;
pub(crate) mod search;
pub(crate) mod status;
pub(crate) mod store_correct;
pub(crate) mod store_ops;
pub(crate) mod typed_graph;
pub(crate) mod usage;

pub(crate) use briefing::BriefingService;
pub(crate) use confidence::ConfidenceService;
pub use confidence::{ConfidenceState, ConfidenceStateHandle};
pub use contradiction_cache::{
    ContradictionScanCacheHandle, ContradictionScanResult, new_contradiction_cache_handle,
};
pub use effectiveness::{EffectivenessState, EffectivenessStateHandle};
pub(crate) use gateway::{RateLimitConfig, SecurityGateway};
pub(crate) use search::{FusionWeights, RetrievalMode, SearchService, ServiceSearchParams};
pub(crate) use status::StatusService;
pub(crate) use store_ops::StoreService;
pub use typed_graph::{TypedGraphState, TypedGraphStateHandle};
pub(crate) use usage::UsageService;

// ---------------------------------------------------------------------------
// CallerId
// ---------------------------------------------------------------------------

/// Type-safe caller identity for rate limiting and audit.
///
/// Prevents cross-transport key collisions structurally. MCP constructs
/// `Agent`, UDS constructs `UdsSession`. Services never construct CallerIds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CallerId {
    /// MCP caller identified by resolved agent_id.
    Agent(String),
    /// UDS caller identified by session_id.
    UdsSession(String),
}

// ---------------------------------------------------------------------------
// Session ID helpers (ADR-004)
// ---------------------------------------------------------------------------

/// Prefix a raw session ID with transport identifier.
pub(crate) fn prefix_session_id(transport: &str, raw: &str) -> String {
    format!("{transport}::{raw}")
}

/// Strip transport prefix from a prefixed session ID.
///
/// Returns the raw ID after the first `::` delimiter.
/// If no prefix found, returns the input unchanged.
#[allow(dead_code)]
pub(crate) fn strip_session_prefix(prefixed: &str) -> &str {
    match prefixed.find("::") {
        Some(pos) => &prefixed[pos + 2..],
        None => prefixed,
    }
}

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
    ContentRejected {
        category: String,
        description: String,
    },
    /// S2: Rate limit exceeded.
    RateLimited {
        limit: u32,
        window_secs: u64,
        retry_after_secs: u64,
    },
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
            ServiceError::RateLimited {
                limit,
                window_secs,
                retry_after_secs,
            } => {
                write!(
                    f,
                    "rate limited: {limit} requests per {window_secs}s, retry after {retry_after_secs}s"
                )
            }
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
            ServiceError::RateLimited {
                limit,
                window_secs,
                retry_after_secs,
            } => ServerError::InvalidInput {
                field: "rate_limit".to_string(),
                reason: format!(
                    "rate limited: {limit} per {window_secs}s, retry after {retry_after_secs}s"
                ),
            },
            ServiceError::ValidationFailed(msg) => ServerError::InvalidInput {
                field: "input".to_string(),
                reason: msg,
            },
            ServiceError::Core(e) => ServerError::Core(e),
            ServiceError::EmbeddingFailed(msg) => ServerError::EmbedFailed(msg),
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
    pub(crate) briefing: BriefingService,
    pub(crate) status: StatusService,
    pub(crate) usage: UsageService,
    /// crt-018b: effectiveness classification cache shared with SearchService,
    /// BriefingService, and the background tick. Held here for external access
    /// via `effectiveness_state_handle()` (mirrors `confidence_state_handle()`).
    effectiveness_state: EffectivenessStateHandle,
    /// crt-021: typed graph state cache shared with SearchService and the background
    /// tick. Pre-built TypedRelationGraph + entry snapshot. Held here for external
    /// access via `typed_graph_handle()` (mirrors `effectiveness_state_handle()`).
    typed_graph_state: TypedGraphStateHandle,
    /// GH #278 fix: contradiction scan result cache shared with StatusService
    /// and the background tick. Eliminates O(N) ONNX inference from every
    /// `context_status` call. Held here for external access via
    /// `contradiction_cache_handle()` (mirrors `supersession_state_handle()`).
    contradiction_cache: ContradictionScanCacheHandle,
    /// crt-022 (ADR-004): shared rayon thread pool for ML inference (ONNX embedding,
    /// future NLI and GNN). All consumers receive this via `Arc::clone` from `ServiceLayer`.
    pub(crate) ml_inference_pool: Arc<RayonPool>,
    // TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here
}

impl ServiceLayer {
    /// Return a clone of the `ConfidenceStateHandle` owned by this layer.
    ///
    /// Used by the binary crate (`main.rs`) to pass the shared handle to
    /// `spawn_background_tick` so the background tick loop's `StatusService`
    /// shares the same `Arc<RwLock<ConfidenceState>>` as the search path.
    pub fn confidence_state_handle(&self) -> ConfidenceStateHandle {
        self.confidence.state_handle()
    }

    /// Return a clone of the `EffectivenessStateHandle` owned by this layer.
    ///
    /// Used by the binary crate (`main.rs`) to pass the shared handle to
    /// `spawn_background_tick` so the background tick shares the same
    /// `Arc<RwLock<EffectivenessState>>` as the search and briefing paths.
    /// Mirrors `confidence_state_handle()` (crt-018b).
    pub fn effectiveness_state_handle(&self) -> EffectivenessStateHandle {
        Arc::clone(&self.effectiveness_state)
    }

    /// Return a clone of the `TypedGraphStateHandle` owned by this layer.
    ///
    /// Used by the binary crate (`main.rs`) to pass the shared handle to
    /// `spawn_background_tick` so the background tick rebuilds the same
    /// `Arc<RwLock<TypedGraphState>>` that `SearchService` reads from.
    /// Mirrors `effectiveness_state_handle()` (crt-021).
    pub fn typed_graph_handle(&self) -> TypedGraphStateHandle {
        Arc::clone(&self.typed_graph_state)
    }

    /// Return a clone of the `ContradictionScanCacheHandle` owned by this layer.
    ///
    /// Used by the binary crate (`main.rs`) to pass the shared handle to
    /// `spawn_background_tick` so the background tick writes the same
    /// `Arc<RwLock<Option<ContradictionScanResult>>>` that `StatusService` reads from.
    /// Mirrors `supersession_state_handle()` (GH #278 fix).
    pub fn contradiction_cache_handle(&self) -> ContradictionScanCacheHandle {
        Arc::clone(&self.contradiction_cache)
    }

    pub fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<Store>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        audit: Arc<AuditLog>,
        usage_dedup: Arc<UsageDedup>,
        boosted_categories: std::collections::HashSet<String>,
        ml_inference_pool: Arc<RayonPool>,
        nli_handle: Arc<NliServiceHandle>,
        nli_top_k: usize,
        nli_enabled: bool,
        inference_config: Arc<InferenceConfig>,
        observation_registry: Arc<DomainPackRegistry>,
        confidence_params: Arc<unimatrix_engine::confidence::ConfidenceParams>,
    ) -> Self {
        Self::with_rate_config(
            store,
            vector_index,
            vector_store,
            entry_store,
            embed_service,
            adapt_service,
            audit,
            usage_dedup,
            RateLimitConfig::default(),
            boosted_categories,
            ml_inference_pool,
            nli_handle,
            nli_top_k,
            nli_enabled,
            inference_config,
            observation_registry,
            confidence_params,
        )
    }

    pub(crate) fn with_rate_config(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<Store>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        audit: Arc<AuditLog>,
        usage_dedup: Arc<UsageDedup>,
        rate_config: RateLimitConfig,
        boosted_categories: std::collections::HashSet<String>,
        ml_inference_pool: Arc<RayonPool>,
        nli_handle: Arc<NliServiceHandle>,
        nli_top_k: usize,
        nli_enabled: bool,
        inference_config: Arc<InferenceConfig>,
        observation_registry: Arc<DomainPackRegistry>,
        confidence_params: Arc<unimatrix_engine::confidence::ConfidenceParams>,
    ) -> Self {
        let gateway = Arc::new(SecurityGateway::with_rate_config(
            Arc::clone(&audit),
            rate_config,
        ));

        let confidence = ConfidenceService::new(Arc::clone(&store), Arc::clone(&confidence_params));
        // crt-019 (ADR-001): obtain handle before constructing search/status
        // so both services share the same Arc<RwLock<ConfidenceState>>.
        let confidence_state_handle = confidence.state_handle();

        // crt-018b (ADR-001): create effectiveness state handle once; clone into
        // SearchService, BriefingService, and the background tick so all components
        // share the same Arc<RwLock<EffectivenessState>> (mirrors confidence pattern).
        let effectiveness_state = EffectivenessState::new_handle();

        // crt-021: create typed graph state handle once; clone into SearchService
        // and the background tick so the tick rebuilds the pre-built graph SearchService reads.
        let typed_graph_state = TypedGraphState::new_handle();

        // GH #278 fix: create contradiction cache handle once; clone into StatusService
        // (read path) and the background tick (write path) so they share the same
        // Arc<RwLock<Option<ContradictionScanResult>>>.
        let contradiction_cache = new_contradiction_cache_handle();

        let search = SearchService::new(
            Arc::clone(&store),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&gateway),
            Arc::clone(&confidence_state_handle),
            Arc::clone(&effectiveness_state),
            Arc::clone(&typed_graph_state),
            boosted_categories,
            Arc::clone(&ml_inference_pool),
            Arc::clone(&nli_handle),
            nli_top_k,
            nli_enabled,
            FusionWeights::from_config(&inference_config),
        );

        let nli_store_cfg = NliStoreConfig {
            enabled: inference_config.nli_enabled,
            nli_post_store_k: inference_config.nli_post_store_k,
            nli_entailment_threshold: inference_config.nli_entailment_threshold,
            nli_contradiction_threshold: inference_config.nli_contradiction_threshold,
            max_contradicts_per_tick: inference_config.max_contradicts_per_tick,
        };

        let store_ops = StoreService::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&gateway),
            Arc::clone(&audit),
            Arc::clone(&ml_inference_pool),
            Arc::clone(&nli_handle),
            nli_store_cfg,
        );

        let semantic_k = briefing::parse_semantic_k();
        let briefing = BriefingService::new(
            Arc::clone(&entry_store),
            search.clone(),
            Arc::clone(&gateway),
            semantic_k,
            Arc::clone(&effectiveness_state), // crt-018b (ADR-004): required, non-optional
        );

        let status = StatusService::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&confidence_state_handle),
            Arc::clone(&confidence_params),
            Arc::clone(&contradiction_cache),
            Arc::clone(&ml_inference_pool),
            Arc::clone(&observation_registry),
        );

        let usage = UsageService::new(
            Arc::clone(&store),
            usage_dedup,
            Arc::clone(&confidence_state_handle),
            Arc::clone(&confidence_params),
        );

        ServiceLayer {
            search,
            store_ops,
            confidence,
            briefing,
            status,
            usage,
            effectiveness_state, // crt-018b: held for external access via effectiveness_state_handle()
            typed_graph_state,   // crt-021: held for external access via typed_graph_handle()
            contradiction_cache, // GH #278: held for external access via contradiction_cache_handle()
            ml_inference_pool,   // crt-022 (ADR-004): shared ML inference pool
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
        assert!(matches!(
            server_err,
            ServerError::ContentScanRejected { .. }
        ));
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
        assert!(matches!(
            server_err,
            ServerError::Core(CoreError::Store(StoreError::EntryNotFound(99)))
        ));
    }
}
