# Architecture: vnc-006 — Service Layer + Security Gateway

## System Overview

vnc-006 introduces a transport-agnostic service layer within `crates/unimatrix-server/` that unifies duplicated business logic between the MCP and UDS request paths. The service layer sits between transport handlers (tools.rs, uds_listener.rs) and the foundation layer (unimatrix-store, unimatrix-vector, unimatrix-embed). A `SecurityGateway` struct is injected into each service, enforcing content scanning, input validation, quarantine exclusion, and structured audit as service-level invariants that no transport can bypass.

```
┌───────────────────┐   ┌────────────────────┐
│   MCP Transport   │   │   UDS Transport    │
│   (tools.rs)      │   │ (uds_listener.rs)  │
│                   │   │                    │
│ T: identity/caps  │   │ T: UID auth        │
│ T: rmcp framing   │   │ T: wire framing    │
│ T: format select  │   │ T: injection log   │
│ T: usage record   │   │ T: session track   │
└────────┬──────────┘   └────────┬───────────┘
         │                       │
         └───────────┬───────────┘
                     │
       ┌─────────────┴──────────────┐
       │      services/ module      │
       │                            │
       │  SecurityGateway (injected)│
       │    S1: Content scan        │
       │    S3: Input validation    │
       │    S4: Quarantine check    │
       │    S5: Audit emission      │
       │                            │
       │  SearchService             │
       │  StoreService              │
       │  ConfidenceService         │
       └─────────────┬──────────────┘
                     │
       ┌─────────────┴──────────────┐
       │    Foundation Layer        │
       │                            │
       │  Store (unimatrix-store)   │
       │  VectorIndex (unimatrix-   │
       │    vector)                 │
       │  EmbedServiceHandle        │
       │    (unimatrix-embed)       │
       │  AdaptationService         │
       └────────────────────────────┘
```

The service layer does NOT replace `UnimatrixServer`. It is consumed by `UnimatrixServer` (which gains a `ServiceLayer` field). Transport handlers delegate business logic to services while retaining transport-specific concerns.

## Component Breakdown

### 1. SecurityGateway (`services/gateway.rs`)

A struct injected into each service. Holds references to existing infrastructure (ContentScanner, validation functions, AuditLog). Services call gateway methods internally at the appropriate points.

**Responsibilities:**
- S1: Content scanning (injection patterns on search queries in warn mode; injection+PII on writes in reject mode)
- S3: Input validation (query length, k range, control chars, title/content lengths)
- S4: Quarantine exclusion check (returns bool for a given entry status)
- S5: Audit event emission (fire-and-forget via AuditLog)

**Not responsible for (deferred):**
- S2: Rate limiting (vnc-009) — interface placeholder only

```rust
pub(crate) struct SecurityGateway {
    audit: Arc<AuditLog>,
}

impl SecurityGateway {
    pub(crate) fn new(audit: Arc<AuditLog>) -> Self { ... }

    /// S1+S3: Validate and scan a search query. Returns Ok(scan_warning) where
    /// scan_warning is Some(ScanResult) if injection pattern detected (warn, not reject).
    pub(crate) fn validate_search_query(
        &self,
        query: &str,
        k: usize,
        audit_ctx: &AuditContext,
    ) -> Result<Option<ScanWarning>, ServiceError> { ... }

    /// S1+S3: Validate and scan a store/correct operation. Hard-rejects on injection/PII match.
    /// Skips S1 content scan when audit_ctx.source is Internal.
    pub(crate) fn validate_write(
        &self,
        title: &str,
        content: &str,
        category: &str,
        tags: &[String],
        audit_ctx: &AuditContext,
    ) -> Result<(), ServiceError> { ... }

    /// S4: Returns true if the entry should be excluded from results.
    pub(crate) fn is_quarantined(status: &Status) -> bool { ... }

    /// S5: Emit an audit event (fire-and-forget).
    pub(crate) fn emit_audit(
        &self,
        event: AuditEvent,
    ) { ... }

    #[cfg(test)]
    pub(crate) fn new_permissive() -> Self { ... }
}
```

**Design decision (ADR-001):** Gateway is a struct injected into services (hybrid pattern). Services call `self.gateway.validate_search_query()` etc. internally. This gives testability (mock gateway in tests) without the boilerplate of a full decorator pattern. The gateway does not wrap services — services own the gateway reference.

**Design decision (ADR-002):** `AuditSource::Internal` skips S1 content scanning but applies S3 validation and S5 audit. The `validate_write` method checks `audit_ctx.source` to determine scanning behavior. `pub(crate)` on `AuditSource::Internal` prevents external callers from claiming internal status.

### 2. SearchService (`services/search.rs`)

Unified search pipeline replacing duplicated logic in tools.rs (~lines 270-500) and uds_listener.rs (~lines 586-780).

**Responsibilities:**
- Embed query via EmbedServiceHandle + AdaptationService
- HNSW vector search (filtered or unfiltered based on params)
- Batch entry fetch from Store
- Re-rank with composite score (0.85 * similarity + 0.15 * confidence)
- Provenance boost (+0.02 for self-authored entries)
- Co-access boost (from CO_ACCESS table)
- Feature boost (from tag match)
- Apply optional similarity/confidence floors
- Quarantine exclusion (S4 invariant)
- Return results with query embedding for reuse

```rust
pub(crate) struct SearchService {
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
}

/// Transport-agnostic search parameters.
pub(crate) struct ServiceSearchParams {
    pub query: String,
    pub k: usize,
    pub filters: Option<QueryFilter>,
    pub similarity_floor: Option<f64>,
    pub confidence_floor: Option<f64>,
    pub feature_tag: Option<String>,
    pub co_access_anchors: Option<Vec<u64>>,
    pub caller_agent_id: Option<String>,
}

/// Search results including query embedding for reuse.
pub(crate) struct SearchResults {
    pub entries: Vec<ScoredEntry>,
    pub query_embedding: Vec<f32>,
}

pub(crate) struct ScoredEntry {
    pub entry: EntryRecord,
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
}

impl SearchService {
    pub(crate) async fn search(
        &self,
        params: ServiceSearchParams,
        audit_ctx: &AuditContext,
    ) -> Result<SearchResults, ServiceError> { ... }
}
```

**Pipeline steps (preserving exact existing behavior):**

1. `gateway.validate_search_query(query, k, audit_ctx)` — S1 warn + S3 bounds
2. Embed query via `embed_service.get_adapter()` then `spawn_blocking(adapter.embed_entry("", &query))`
3. Apply MicroLoRA adaptation via `adapt_service`
4. Normalize embedding
5. HNSW search: `vector_store.search(embedding, k * 2, ef_search)` or `search_filtered` if filters present
6. Fetch entries in batch from store (replacing per-result fetch)
7. Filter quarantined entries (S4)
8. Re-rank: `0.85 * similarity + 0.15 * confidence`
9. Provenance boost: `+0.02` if `entry.created_by == caller_agent_id`
10. Co-access boost: `spawn_blocking(compute_search_boost(...))`
11. Feature boost: tag-match reranking
12. Apply similarity_floor and confidence_floor (if set)
13. Sort by final_score, take top k
14. `gateway.emit_audit(...)` — S5
15. Return `SearchResults { entries, query_embedding }`

### 3. StoreService (`services/store_ops.rs`)

Unified write operations with atomic audit. Requires `Store::insert_in_txn` (ADR-003).

**Responsibilities:**
- Content scanning (S1) with AuditSource-dependent behavior
- Input validation (S3)
- Atomic insert + audit in single transaction
- Embedding generation for new entries
- Correction chain management (deprecate original, link new)

```rust
pub(crate) struct StoreService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
}

impl StoreService {
    pub(crate) async fn insert(
        &self,
        entry: NewEntry,
        embedding: Option<Vec<f32>>,  // Pre-computed, or None to compute
        audit_ctx: &AuditContext,
    ) -> Result<InsertResult, ServiceError> { ... }

    pub(crate) async fn correct(
        &self,
        original_id: u64,
        corrected: NewEntry,
        reason: Option<String>,
        audit_ctx: &AuditContext,
    ) -> Result<CorrectResult, ServiceError> { ... }
}

pub(crate) struct InsertResult {
    pub entry: EntryRecord,
    pub duplicate_of: Option<u64>,
}

pub(crate) struct CorrectResult {
    pub corrected_entry: EntryRecord,
    pub deprecated_original: EntryRecord,
}
```

**Design decision (ADR-003):** `Store::insert_in_txn` accepts an external `WriteTransaction` reference and performs all index writes within it. The service layer opens the transaction, calls `insert_in_txn`, writes audit in the same transaction, and commits atomically. The `WriteTransaction` is from redb and is not exposed in any public API — it stays `pub(crate)` within unimatrix-store.

### 4. ConfidenceService (`services/confidence.rs`)

Batched fire-and-forget confidence recomputation replacing 8 scattered blocks.

```rust
pub(crate) struct ConfidenceService {
    store: Arc<Store>,
}

impl ConfidenceService {
    /// Recompute confidence for a batch of entries. Fire-and-forget via spawn_blocking.
    /// Single read txn + write txn for the batch. Skip-and-log per entry on failure.
    pub(crate) fn recompute(&self, entry_ids: &[u64]) {
        let store = Arc::clone(&self.store);
        let ids = entry_ids.to_vec();
        let _ = tokio::task::spawn_blocking(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            for id in ids {
                match store.get(id) {
                    Ok(entry) => {
                        let conf = unimatrix_core::compute_confidence(&entry, now);
                        if let Err(e) = store.update_confidence(id, conf) {
                            tracing::warn!("confidence recompute failed for {id}: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("confidence recompute: entry {id} not found: {e}");
                    }
                }
            }
        });
    }
}
```

**Design decision (ADR-004):** Batched recomputation in a single `spawn_blocking` call. Aligns with the crt-005 batch refresh pattern (context_status maintain=true path). Per-entry failure is logged and skipped, not propagated — consistent with the existing fire-and-forget contract (see Unimatrix ADR #53: Fire-and-Forget Usage Recording).

### 5. AuditContext (`services/mod.rs`)

Transport-provided context for structured audit records, carried through all service calls.

```rust
/// Transport-provided context for audit and retrospective compatibility.
pub(crate) struct AuditContext {
    pub source: AuditSource,
    pub caller_id: String,
    pub session_id: Option<String>,
    pub feature_cycle: Option<String>,
}

/// Identifies the caller's transport origin.
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

**Construction:** Each transport constructs `AuditContext` before calling service methods:
- MCP: from resolved identity (agent_id, trust_level) + MCP session
- UDS: from peer credentials (uid, pid) + hook session_id
- Internal: from service name (e.g., "auto-outcome")

### 6. ServiceLayer (`services/mod.rs`)

Aggregate struct providing access to all services. Constructed once during server startup and stored in `UnimatrixServer`.

```rust
pub(crate) struct ServiceLayer {
    pub search: SearchService,
    pub store_ops: StoreService,
    pub confidence: ConfidenceService,
}

impl ServiceLayer {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        audit: Arc<AuditLog>,
    ) -> Self { ... }
}
```

### 7. ServiceError (`services/mod.rs`)

Service-specific error type that maps to both MCP `ErrorData` and UDS `HookResponse::Error`.

```rust
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
```

Both transports convert `ServiceError` to their native error format in the transport layer.

## Component Interactions

### MCP Search Flow (After Refactoring)

```
tools.rs::context_search(params)
  1. Resolve identity (transport-specific)
  2. Check capability: Search (transport-specific)
  3. Construct AuditContext::Mcp { agent_id, trust_level }
  4. Convert MCP SearchParams → ServiceSearchParams
  5. self.services.search.search(service_params, audit_ctx).await
  6. Record usage (transport-specific: helpful/unhelpful, access_count)
  7. Format response (transport-specific: summary/markdown/json)
  8. Return CallToolResult
```

### UDS Search Flow (After Refactoring)

```
uds_listener.rs::handle_context_search(query, session_id, k, ...)
  1. Construct AuditContext::Uds { uid, pid, session_id }
  2. Convert UDS params → ServiceSearchParams (with similarity_floor=0.5, confidence_floor=0.3)
  3. self.services.search.search(service_params, audit_ctx).await
  4. Record injection log (transport-specific)
  5. Record co-access pairs (transport-specific, session-scoped)
  6. Format HookResponse::ContextEntries
  7. Return HookResponse
```

### Store/Correct Flow (After Refactoring)

```
tools.rs::context_store(params)
  1. Resolve identity, check Write capability
  2. Construct AuditContext::Mcp { ... }
  3. Validate MCP-specific params, build NewEntry
  4. self.services.store_ops.insert(new_entry, None, audit_ctx).await
  5. self.services.confidence.recompute(&[entry_id])
  6. Record usage, format response
  7. Return CallToolResult
```

### Auto-Outcome Internal Write Flow

```
uds_listener.rs::write_auto_outcome_entry(...)
  1. Construct AuditContext::Internal { service: "auto-outcome" }
  2. Build NewEntry with outcome data
  3. self.services.store_ops.insert(new_entry, None, audit_ctx).await
     → StoreService checks AuditSource::Internal → skips S1 scan
     → S3 validation still applies
     → S5 audit still emitted
  4. self.services.confidence.recompute(&[entry_id])
```

## Technology Decisions

See ADR files for full context, decision, and consequences.

| ADR | Decision | Summary |
|-----|----------|---------|
| ADR-001 | Hybrid gateway injection | SecurityGateway as injected struct, services call methods internally |
| ADR-002 | AuditSource-driven scan bypass | Internal callers skip S1 content scan, retain S3+S5 |
| ADR-003 | Store::insert_in_txn for atomic writes | Expose transaction-accepting insert for service-layer atomic audit |
| ADR-004 | Batched confidence recomputation | Single spawn_blocking per batch, skip-and-log per entry |

## Integration Points

### Existing Components Consumed (Unchanged)

| Component | Used By | How |
|-----------|---------|-----|
| `Store` (unimatrix-store) | StoreService, ConfidenceService, SearchService | Direct `Arc<Store>` reference |
| `VectorIndex` (unimatrix-vector) | StoreService | For vector insertion after store write |
| `AsyncVectorStore` | SearchService | For HNSW search |
| `AsyncEntryStore` | SearchService | For batch entry retrieval |
| `EmbedServiceHandle` | SearchService, StoreService | For embedding generation |
| `AdaptationService` | SearchService, StoreService | For MicroLoRA adaptation |
| `AuditLog` | SecurityGateway | For S5 audit emission |
| `ContentScanner` | SecurityGateway | For S1 content scanning (OnceLock singleton) |
| `CategoryAllowlist` | SecurityGateway (via validate_write) | For category validation |

### New Components Introduced

| Component | Module | Purpose |
|-----------|--------|---------|
| `SecurityGateway` | `services/gateway.rs` | S1/S3/S4/S5 enforcement |
| `SearchService` | `services/search.rs` | Unified search pipeline |
| `StoreService` | `services/store_ops.rs` | Unified write + atomic audit |
| `ConfidenceService` | `services/confidence.rs` | Batched confidence recompute |
| `ServiceLayer` | `services/mod.rs` | Aggregate service access |
| `AuditContext` / `AuditSource` | `services/mod.rs` | Transport-provided audit context |
| `ServiceError` | `services/mod.rs` | Service-specific error type |
| `Store::insert_in_txn` | `unimatrix-store/src/write.rs` | Transaction-accepting insert |

### Modified Components

| Component | Change | Risk |
|-----------|--------|------|
| `UnimatrixServer` | Add `services: ServiceLayer` field | Low — additive |
| `tools.rs` context_search | Replace inline search with `services.search.search()` | Med — behavioral equivalence |
| `tools.rs` context_store | Replace inline write with `services.store_ops.insert()` | Med — behavioral equivalence |
| `tools.rs` context_correct | Replace inline correct with `services.store_ops.correct()` | Med — behavioral equivalence |
| `tools.rs` confidence blocks (5) | Replace with `services.confidence.recompute()` | Low — same logic, batched |
| `uds_listener.rs` handle_context_search | Replace inline search with `services.search.search()` | Med — behavioral equivalence |
| `uds_listener.rs` confidence blocks (3) | Replace with `services.confidence.recompute()` | Low — same logic, batched |
| `Store` (unimatrix-store) | Add `insert_in_txn` method | Low — additive, existing insert unchanged |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `ServiceLayer::new(...)` | `fn(Arc<Store>, Arc<VectorIndex>, ...) -> ServiceLayer` | `services/mod.rs` |
| `SearchService::search(...)` | `async fn(&self, ServiceSearchParams, &AuditContext) -> Result<SearchResults, ServiceError>` | `services/search.rs` |
| `StoreService::insert(...)` | `async fn(&self, NewEntry, Option<Vec<f32>>, &AuditContext) -> Result<InsertResult, ServiceError>` | `services/store_ops.rs` |
| `StoreService::correct(...)` | `async fn(&self, u64, NewEntry, Option<String>, &AuditContext) -> Result<CorrectResult, ServiceError>` | `services/store_ops.rs` |
| `ConfidenceService::recompute(...)` | `fn(&self, &[u64])` | `services/confidence.rs` |
| `SecurityGateway::validate_search_query(...)` | `fn(&self, &str, usize, &AuditContext) -> Result<Option<ScanWarning>, ServiceError>` | `services/gateway.rs` |
| `SecurityGateway::validate_write(...)` | `fn(&self, &str, &str, &str, &[String], &AuditContext) -> Result<(), ServiceError>` | `services/gateway.rs` |
| `SecurityGateway::is_quarantined(...)` | `fn(&Status) -> bool` | `services/gateway.rs` |
| `SecurityGateway::emit_audit(...)` | `fn(&self, AuditEvent)` | `services/gateway.rs` |
| `Store::insert_in_txn(...)` | `fn(&WriteTransaction, NewEntry, u64) -> Result<EntryRecord>` | `unimatrix-store/src/write.rs` |
| `AuditContext` | struct | `services/mod.rs` |
| `AuditSource` | enum (Mcp, Uds, Internal) | `services/mod.rs` |
| `ServiceError` | enum | `services/mod.rs` |
| `ServiceSearchParams` | struct | `services/search.rs` |
| `SearchResults` / `ScoredEntry` | structs | `services/search.rs` |
| `InsertResult` / `CorrectResult` | structs | `services/store_ops.rs` |
| `ScanWarning` | struct | `services/gateway.rs` |

## File Layout

```
crates/unimatrix-server/src/
├── services/
│   ├── mod.rs           (~80 lines)  ServiceLayer, AuditContext, AuditSource, ServiceError
│   ├── gateway.rs       (~200 lines) SecurityGateway (S1, S3, S4, S5)
│   ├── search.rs        (~250 lines) SearchService
│   ├── store_ops.rs     (~200 lines) StoreService
│   └── confidence.rs    (~40 lines)  ConfidenceService
├── tools.rs             (3061→~2300) Reduced by ~760 lines
├── uds_listener.rs      (2271→~1900) Reduced by ~370 lines
├── server.rs            (2105→~2150) Gains ServiceLayer field, slight growth
└── ... (all other files unchanged)
```
