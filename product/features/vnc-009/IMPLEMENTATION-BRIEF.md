# vnc-009: Cross-Path Convergence — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-009/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-009/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-009/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-009/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-009/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-009/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| usage-service | pseudocode/usage-service.md | test-plan/usage-service.md |
| rate-limiter | pseudocode/rate-limiter.md | test-plan/rate-limiter.md |
| session-aware-mcp | pseudocode/session-aware-mcp.md | test-plan/session-aware-mcp.md |
| status-serialize | pseudocode/status-serialize.md | test-plan/status-serialize.md |
| uds-auth-audit | pseudocode/uds-auth-audit.md | test-plan/uds-auth-audit.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Extract UsageService to unify MCP and UDS usage recording, add session-aware MCP with transport-prefixed session IDs, implement S2 rate limiting in SecurityGateway, modernize StatusReport JSON serialization via derive(Serialize), and audit UDS auth failures. This completes the 4-wave server refactoring series and positions the server for future HTTP transport.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| StatusReport JSON serialization | Intermediate StatusReportJson struct preserving nested JSON | ADR-001 | architecture/ADR-001-status-report-serialization.md |
| Rate limiter eviction strategy | Lazy eviction on each check, no background timer | ADR-002 | architecture/ADR-002-rate-limiter-lazy-eviction.md |
| CallerId::ApiKey variant | Deferred until HTTP transport ships | ADR-003 | architecture/ADR-003-defer-callerid-apikey.md |
| Session ID storage strategy | Prefix at service boundary, strip before storage writes | ADR-004 | architecture/ADR-004-session-id-prefix-strategy.md |
| UsageService API | Unified `record_access` with `AccessSource` enum | Human direction | SCOPE.md (Resolved Questions) |
| CallerId type | Typed enum (Agent, UdsSession) | Human direction | SCOPE.md (Resolved Questions) |
| MCP session_id source | From hooks augmenting MCP requests | Human direction | SCOPE.md (Resolved Questions) |
| BriefingService rate limiting | check_search_rate when include_semantic=true | Human direction | SCOPE.md (Resolved Questions) |

## Files to Create/Modify

### New Files

| File | Description |
|------|-------------|
| `crates/unimatrix-server/src/services/usage.rs` | UsageService with record_access, AccessSource, UsageContext |

### Modified Files

| File | Changes |
|------|---------|
| `crates/unimatrix-server/src/services/mod.rs` | Add `CallerId` enum, `ServiceError::RateLimited` variant, `pub(crate) mod usage`, UsageService to ServiceLayer, prefix/strip helpers |
| `crates/unimatrix-server/src/services/gateway.rs` | Add `RateLimiter` struct and field, `check_search_rate()`, `check_write_rate()` methods |
| `crates/unimatrix-server/src/services/search.rs` | Add `caller_id: &CallerId` parameter to `search()`, call `check_search_rate()` |
| `crates/unimatrix-server/src/services/store_ops.rs` | Add `caller_id: &CallerId` parameter to `insert()` and `correct()`, call `check_write_rate()` |
| `crates/unimatrix-server/src/services/briefing.rs` | Add `caller_id: Option<&CallerId>` parameter to `assemble()`, call `check_search_rate()` when `include_semantic=true` |
| `crates/unimatrix-server/src/mcp/context.rs` | Add `caller_id: CallerId` field to ToolContext |
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `session_id: Option<String>` to SearchParams, LookupParams, GetParams, BriefingParams. Update build_context to set session_id. Replace record_usage_for_entries calls with UsageService. Pass CallerId to service methods. |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Add `#[derive(Serialize)]` to StatusReport, CoAccessClusterEntry. Add StatusReportJson struct. Replace JSON branch. |
| `crates/unimatrix-server/src/infra/contradiction.rs` | Add `#[derive(serde::Serialize)]` to ContradictionPair, EmbeddingInconsistency |
| `crates/unimatrix-server/src/uds/listener.rs` | Add `Arc<AuditLog>` parameter to handle_connection. Write AuditEvent on auth failure. Replace inline injection/co-access recording with UsageService. Construct CallerId::UdsSession. Add uds:: prefix to session IDs. |
| `crates/unimatrix-server/src/server.rs` | Remove `record_usage_for_entries()`. Add UsageService construction. Pass Arc<AuditLog> to UDS listener. |

## Data Structures

### New Types

```rust
// services/usage.rs
pub(crate) struct UsageService {
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
}

pub(crate) enum AccessSource {
    McpTool,
    HookInjection,
    Briefing,
}

pub(crate) struct UsageContext {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub helpful: Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level: Option<TrustLevel>,
}

// services/mod.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CallerId {
    Agent(String),
    UdsSession(String),
}

// services/gateway.rs (internal)
struct RateLimiter {
    windows: Mutex<HashMap<CallerId, SlidingWindow>>,
    search_limit: u32,
    write_limit: u32,
    window_secs: u64,
}

struct SlidingWindow {
    timestamps: VecDeque<Instant>,
}

// mcp/response/status.rs (internal)
#[derive(Serialize)]
struct StatusReportJson { /* nested JSON structure */ }
```

### Modified Types

```rust
// services/mod.rs - new ServiceError variant
pub(crate) enum ServiceError {
    // ... existing variants ...
    RateLimited { limit: u32, window_secs: u64, retry_after_secs: u64 },
}

// services/mod.rs - new ServiceLayer field
pub struct ServiceLayer {
    // ... existing fields ...
    pub(crate) usage: UsageService,
}

// services/gateway.rs - new field
pub(crate) struct SecurityGateway {
    pub(crate) audit: Arc<AuditLog>,
    rate_limiter: RateLimiter,
}

// mcp/context.rs - new field
pub(crate) struct ToolContext {
    // ... existing fields ...
    pub caller_id: CallerId,
}

// mcp/tools.rs - new field on 4 param structs
pub struct SearchParams {
    // ... existing fields ...
    #[serde(default)]
    pub session_id: Option<String>,
}

// mcp/response/status.rs - derive added
#[derive(serde::Serialize)]
pub struct StatusReport { /* existing fields */ }

#[derive(serde::Serialize)]
pub struct CoAccessClusterEntry { /* existing fields */ }

// infra/contradiction.rs - derive added
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContradictionPair { /* existing fields */ }

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingInconsistency { /* existing fields */ }
```

## Function Signatures

### New Functions

```rust
// UsageService
impl UsageService {
    pub(crate) fn new(store: Arc<Store>, usage_dedup: Arc<UsageDedup>) -> Self;
    pub(crate) fn record_access(&self, entry_ids: &[u64], source: AccessSource, ctx: UsageContext);
}

// SecurityGateway rate limiting
impl SecurityGateway {
    pub(crate) fn check_search_rate(&self, caller: &CallerId) -> Result<(), ServiceError>;
    pub(crate) fn check_write_rate(&self, caller: &CallerId) -> Result<(), ServiceError>;
}

// Session ID helpers (services/mod.rs or infra/)
pub(crate) fn prefix_session_id(transport: &str, raw: &str) -> String;
pub(crate) fn strip_session_prefix(prefixed: &str) -> &str;

// StatusReportJson
impl From<&StatusReport> for StatusReportJson;
```

### Modified Signatures

```rust
// SearchService - add caller_id parameter
pub(crate) async fn search(&self, params: ServiceSearchParams, audit_ctx: &AuditContext, caller_id: &CallerId) -> Result<SearchResults, ServiceError>;

// StoreService - add caller_id parameter
pub(crate) async fn insert(&self, ..., caller_id: &CallerId) -> Result<EntryRecord, ServiceError>;
pub(crate) async fn correct(&self, ..., caller_id: &CallerId) -> Result<(EntryRecord, EntryRecord), ServiceError>;

// BriefingService - add optional caller_id
pub(crate) async fn assemble(&self, params: BriefingParams, audit_ctx: &AuditContext, caller_id: Option<&CallerId>) -> Result<BriefingResult, ServiceError>;

// UDS listener - add audit_log parameter
async fn handle_connection(..., audit_log: Arc<AuditLog>) -> Result<(), Box<dyn Error + Send + Sync>>;
```

## Constraints

1. All changes in `crates/unimatrix-server/` only
2. No new tables, no schema version bump
3. Fire-and-forget for all usage recording (spawn_blocking)
4. rmcp 0.16.0 tool handler signature constraints
5. Backward compatible: no session_id = identical behavior
6. StatusReport JSON output field names must match existing
7. Test count >= 739 (post-vnc-008 baseline)
8. serde already a workspace dependency
9. Extends existing TestHarness and tempdir fixtures

## Dependencies

| Dependency | Version | Purpose |
|-----------|---------|---------|
| serde | workspace | Serialize derives on StatusReport, ContradictionPair, etc. |
| serde_json | workspace | StatusReportJson serialization |
| std::time::Instant | stdlib | RateLimiter timestamp tracking |
| std::collections::VecDeque | stdlib | SlidingWindow storage |
| std::sync::Mutex | stdlib | RateLimiter thread safety |
| tokio::task::spawn_blocking | existing | UsageService fire-and-forget |

## NOT in Scope

- New tables or schema changes
- HTTP transport or CallerId::ApiKey (ADR-003)
- Persistent rate limiter state
- MCP injection logging
- UDS usage dedup mechanism changes
- Session lifecycle management
- Rate limiting on UDS path
- OperationalEvent log (deferred to GH #89)
- Changes outside `crates/unimatrix-server/`

## Alignment Status

**Vision Alignment**: PASS — directly implements Wave 4 convergence items.
**Milestone Fit**: PASS — completes Milestone 2 (Vinculum phase).
**Scope Gaps**: PASS — all 43 ACs addressed.
**Scope Additions**: WARN — StatusReportJson intermediate struct (implementation refinement, not scope expansion). Session ID validation added (consistent with existing S3 pattern).
**Architecture Consistency**: PASS — follows established service layer patterns.
**Risk Completeness**: PASS — 12 risks, 50 scenarios, all scope risks traced.

**Variances requiring approval**: None.
