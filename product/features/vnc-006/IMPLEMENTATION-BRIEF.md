# Implementation Brief: vnc-006 — Service Layer + Security Gateway

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-006/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-006/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-006/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-006/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-006/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-006/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| SecurityGateway | pseudocode/gateway.md | test-plan/gateway.md |
| SearchService | pseudocode/search.md | test-plan/search.md |
| StoreService | pseudocode/store-ops.md | test-plan/store-ops.md |
| ConfidenceService | pseudocode/confidence.md | test-plan/confidence.md |
| ServiceLayer + Types | pseudocode/service-layer.md | test-plan/service-layer.md |
| Store::insert_in_txn | pseudocode/insert-in-txn.md | test-plan/insert-in-txn.md |
| Transport Rewiring | pseudocode/transport-rewiring.md | test-plan/transport-rewiring.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Extract a transport-agnostic service layer within unimatrix-server that unifies ~760 lines of duplicated business logic between MCP and UDS paths (SearchService: ~400 lines, ConfidenceService: ~160 lines, StoreService: ~200 lines), implements a Security Gateway with content scanning, input validation, quarantine exclusion, and structured audit as service-level invariants, and introduces AuditContext with session_id + feature_cycle for retrospective compatibility. Zero functional changes to either transport's happy path.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Gateway pattern | Hybrid: SecurityGateway as injected struct, services call methods internally | Human decision + SCOPE.md | architecture/ADR-001-hybrid-gateway-injection.md |
| Internal caller scan bypass | AuditSource::Internal skips S1 scan, retains S3+S5 | Human decision + SCOPE.md | architecture/ADR-002-auditsource-driven-scan-bypass.md |
| Atomic write+audit | Store::insert_in_txn accepts external WriteTransaction | Architecture | architecture/ADR-003-store-insert-in-txn.md |
| Confidence batching | Single spawn_blocking per batch, skip-and-log per entry | Human decision + SCOPE.md | architecture/ADR-004-batched-confidence-recompute.md |
| SearchResults embedding | Include query_embedding: Vec<f32>, pub(crate) scope | Human decision | SCOPE.md Resolved Questions |
| Rate limiting (S2) | Deferred to vnc-009 — interface placeholder only | SCOPE.md Non-Goals | — |
| Service location | In-crate services/ module, not a new crate | Research | server-refactoring-architecture.md |

## Files to Create

| Path | Description |
|------|-------------|
| `crates/unimatrix-server/src/services/mod.rs` | ServiceLayer, AuditContext, AuditSource, ServiceError (~80 lines) |
| `crates/unimatrix-server/src/services/gateway.rs` | SecurityGateway: S1 scan, S3 validate, S4 quarantine, S5 audit (~200 lines) |
| `crates/unimatrix-server/src/services/search.rs` | SearchService: unified search pipeline (~250 lines) |
| `crates/unimatrix-server/src/services/store_ops.rs` | StoreService: insert/correct with atomic audit (~200 lines) |
| `crates/unimatrix-server/src/services/confidence.rs` | ConfidenceService: batched fire-and-forget recompute (~40 lines) |

## Files to Modify

| Path | Change |
|------|--------|
| `crates/unimatrix-server/src/lib.rs` | Add `mod services;` declaration |
| `crates/unimatrix-server/src/server.rs` | Add `services: ServiceLayer` field to `UnimatrixServer`, construct in `new()` |
| `crates/unimatrix-server/src/tools.rs` | Replace inline search/rank/boost in `context_search` with `services.search.search()`. Replace inline write logic in `context_store`/`context_correct` with `services.store_ops.insert()`/`correct()`. Replace 5 confidence blocks with `services.confidence.recompute()`. |
| `crates/unimatrix-server/src/uds_listener.rs` | Replace inline search/rank/boost in `handle_context_search` with `services.search.search()`. Replace 3 confidence blocks with `services.confidence.recompute()`. |
| `crates/unimatrix-store/src/write.rs` | Add `insert_in_txn()` method (pub(crate)) |

## Data Structures

```rust
// services/mod.rs
pub(crate) struct AuditContext {
    pub source: AuditSource,
    pub caller_id: String,
    pub session_id: Option<String>,
    pub feature_cycle: Option<String>,
}

pub(crate) enum AuditSource {
    Mcp { agent_id: String, trust_level: TrustLevel },
    Uds { uid: u32, pid: Option<u32>, session_id: String },
    Internal { service: String },
}

pub(crate) enum ServiceError {
    ContentRejected { category: String, description: String },
    ValidationFailed(String),
    Core(CoreError),
    EmbeddingFailed(String),
    NotFound(u64),
}

pub(crate) struct ServiceLayer {
    pub search: SearchService,
    pub store_ops: StoreService,
    pub confidence: ConfidenceService,
}

// services/search.rs
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

// services/store_ops.rs
pub(crate) struct InsertResult {
    pub entry: EntryRecord,
    pub duplicate_of: Option<u64>,
}

pub(crate) struct CorrectResult {
    pub corrected_entry: EntryRecord,
    pub deprecated_original: EntryRecord,
}

// services/gateway.rs
pub(crate) struct ScanWarning {
    pub category: String,
    pub description: String,
    pub matched_text: String,
}
```

## Function Signatures

```rust
// ServiceLayer
impl ServiceLayer {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        audit: Arc<AuditLog>,
    ) -> Self;
}

// SearchService
impl SearchService {
    pub(crate) async fn search(
        &self,
        params: ServiceSearchParams,
        audit_ctx: &AuditContext,
    ) -> Result<SearchResults, ServiceError>;
}

// StoreService
impl StoreService {
    pub(crate) async fn insert(
        &self,
        entry: NewEntry,
        embedding: Option<Vec<f32>>,
        audit_ctx: &AuditContext,
    ) -> Result<InsertResult, ServiceError>;

    pub(crate) async fn correct(
        &self,
        original_id: u64,
        corrected: NewEntry,
        reason: Option<String>,
        audit_ctx: &AuditContext,
    ) -> Result<CorrectResult, ServiceError>;
}

// ConfidenceService
impl ConfidenceService {
    pub(crate) fn recompute(&self, entry_ids: &[u64]);
}

// SecurityGateway
impl SecurityGateway {
    pub(crate) fn new(audit: Arc<AuditLog>) -> Self;
    pub(crate) fn validate_search_query(
        &self, query: &str, k: usize, audit_ctx: &AuditContext,
    ) -> Result<Option<ScanWarning>, ServiceError>;
    pub(crate) fn validate_write(
        &self, title: &str, content: &str, category: &str, tags: &[String], audit_ctx: &AuditContext,
    ) -> Result<(), ServiceError>;
    pub(crate) fn is_quarantined(status: &Status) -> bool;
    pub(crate) fn emit_audit(&self, event: AuditEvent);
    #[cfg(test)]
    pub(crate) fn new_permissive() -> Self;
}

// Store (unimatrix-store addition)
impl Store {
    pub(crate) fn insert_in_txn(
        &self, txn: &WriteTransaction, entry: NewEntry, now: u64,
    ) -> Result<EntryRecord>;
}
```

## Constraints

1. All changes within `crates/unimatrix-server/` except `Store::insert_in_txn` in `crates/unimatrix-store/`
2. No new crates
3. redb synchronous API for `insert_in_txn`
4. rmcp 0.16.0 tool handler signatures unchanged
5. Extend existing TestHarness/tempdir fixtures
6. ContentScanner OnceLock singleton reused
7. Fire-and-forget for audit and confidence
8. No schema version bump
9. Wave independence (no vnc-007/008/009 dependencies)
10. pub(crate) visibility on all service types

## Dependencies

| Dependency | Version | Used By |
|------------|---------|---------|
| unimatrix-store | workspace | Store::insert_in_txn |
| unimatrix-core | workspace | compute_confidence, EntryRecord, NewEntry, QueryFilter |
| unimatrix-vector | workspace | VectorIndex (StoreService vector insertion) |
| unimatrix-embed | workspace | EmbedServiceHandle (embedding generation) |
| unimatrix-adapt | workspace | AdaptationService (MicroLoRA) |
| redb | 3.1.x | WriteTransaction in insert_in_txn |
| rmcp | 0.16.0 | MCP tool handler signatures |
| tokio | workspace | spawn_blocking for fire-and-forget |

## NOT in Scope

- BriefingService extraction (vnc-007)
- Module reorganization into mcp/uds/infra groups (vnc-008)
- Rate limiting / S2 enforcement (vnc-009)
- Unified capability model / SessionWrite (vnc-008)
- StatusService extraction (vnc-008)
- OperationalEvent log / OPERATIONAL_EVENT_LOG table
- response.rs decomposition
- HTTP transport
- Database replacement / storage abstraction beyond insert_in_txn
- Changes to coaccess.rs module

## Alignment Status

All checks PASS. Zero variances. Zero scope gaps. Zero scope additions. Full alignment with PRODUCT-VISION.md definition of vnc-006. See ALIGNMENT-REPORT.md for detailed findings.
