# Pseudocode: ServiceLayer + Types (services/mod.rs)

## Module Structure

```
// services/mod.rs -- re-exports submodules

pub(crate) mod gateway;
pub(crate) mod search;
pub(crate) mod store_ops;
pub(crate) mod confidence;

pub(crate) use gateway::{SecurityGateway, ScanWarning};
pub(crate) use search::{SearchService, ServiceSearchParams, SearchResults, ScoredEntry};
pub(crate) use store_ops::{StoreService, InsertResult, CorrectResult};
pub(crate) use confidence::ConfidenceService;
```

## AuditContext

```
pub(crate) struct AuditContext {
    pub source: AuditSource,
    pub caller_id: String,
    pub session_id: Option<String>,
    pub feature_cycle: Option<String>,
}
```

## AuditSource

```
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
```

## ServiceError

```
pub(crate) enum ServiceError {
    /// S1: Content scan rejection (writes only)
    ContentRejected { category: String, description: String },
    /// S3: Input validation failure
    ValidationFailed(String),
    /// Core/store error
    Core(CoreError),
    /// Embedding error
    EmbeddingFailed(String),
    /// Entry not found
    NotFound(u64),
}

impl fmt::Display for ServiceError:
    match self:
        ContentRejected { category, description } =>
            write!("content rejected ({category}): {description}")
        ValidationFailed(msg) =>
            write!("validation failed: {msg}")
        Core(e) =>
            write!("core error: {e}")
        EmbeddingFailed(msg) =>
            write!("embedding failed: {msg}")
        NotFound(id) =>
            write!("entry not found: {id}")

impl From<CoreError> for ServiceError:
    fn from(e: CoreError) -> Self:
        ServiceError::Core(e)

impl From<ServiceError> for rmcp::ErrorData:
    fn from(e: ServiceError) -> Self:
        match e:
            ServiceError::ContentRejected { category, description } =>
                ErrorData { code: -32001, message: format!("content rejected ({category}): {description}"), data: None }
            ServiceError::ValidationFailed(msg) =>
                ErrorData { code: -32602, message: format!("validation failed: {msg}"), data: None }
            ServiceError::Core(e) =>
                ErrorData::from(ServerError::Core(e))
            ServiceError::EmbeddingFailed(msg) =>
                ErrorData { code: -32603, message: format!("embedding failed: {msg}"), data: None }
            ServiceError::NotFound(id) =>
                ErrorData { code: -32602, message: format!("entry not found: {id}"), data: None }

impl From<ServiceError> for ServerError:
    fn from(e: ServiceError) -> Self:
        match e:
            ServiceError::ContentRejected { category, description } =>
                ServerError::ContentScanRejected { category, description }
            ServiceError::ValidationFailed(msg) =>
                ServerError::Validation(msg)
            ServiceError::Core(e) =>
                ServerError::Core(e)
            ServiceError::EmbeddingFailed(msg) =>
                ServerError::Core(CoreError::EmbedError(msg))
            ServiceError::NotFound(id) =>
                ServerError::Core(CoreError::Store(StoreError::EntryNotFound(id)))
```

## ServiceLayer

```
pub(crate) struct ServiceLayer {
    pub search: SearchService,
    pub store_ops: StoreService,
    pub confidence: ConfidenceService,
}

impl ServiceLayer:
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        audit: Arc<AuditLog>,
    ) -> Self:
        let gateway = Arc::new(SecurityGateway::new(Arc::clone(&audit)))

        let search = SearchService::new(
            Arc::clone(&store),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&gateway),
        )

        let store_ops = StoreService::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&gateway),
        )

        let confidence = ConfidenceService::new(Arc::clone(&store))

        ServiceLayer { search, store_ops, confidence }
```

## Notes

- All types are pub(crate) -- not exposed outside the server crate.
- ServiceError conversions to rmcp::ErrorData preserve error codes: -32001 for content rejection (matches existing ContentScanRejected), -32602 for validation (standard JSON-RPC invalid params), -32603 for internal errors.
- AuditContext is constructed by the transport layer (tools.rs or uds_listener.rs) before calling service methods. It carries audit metadata through the service call.
