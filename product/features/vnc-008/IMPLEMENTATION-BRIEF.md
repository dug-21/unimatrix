# vnc-008 Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-008/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-008/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-008/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-008/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-008/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-008/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| infra-migration | pseudocode/infra-migration.md | test-plan/infra-migration.md |
| mcp-migration | pseudocode/mcp-migration.md | test-plan/mcp-migration.md |
| response-split | pseudocode/response-split.md | test-plan/response-split.md |
| uds-migration | pseudocode/uds-migration.md | test-plan/uds-migration.md |
| tool-context | pseudocode/tool-context.md | test-plan/tool-context.md |
| status-service | pseudocode/status-service.md | test-plan/status-service.md |
| session-write | pseudocode/session-write.md | test-plan/session-write.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Restructure the flat 23-module layout of `crates/unimatrix-server/` into four logical groups (`services/`, `mcp/`, `uds/`, `infra/`), extract ToolContext to reduce MCP handler ceremony, split response.rs into a sub-module hierarchy, extract StatusService, and introduce the SessionWrite capability that formalizes UDS authorization boundaries. Pure restructuring with no behavioral changes except additive UDS capability enforcement (closes F-26).

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| StatusService direct-table access | Inherits existing direct-table scans; Store API expansion deferred | ADR-001 | architecture/ADR-001-statusservice-direct-table-access.md |
| ToolContext construction pattern | Via `self.build_context()` on UnimatrixServer; capability check separate | ADR-002 | architecture/ADR-002-toolcontext-via-server-method.md |
| Mutation formatter unification | Generic `format_status_change()` replaces 3 near-identical functions | ADR-003 | architecture/ADR-003-generic-format-status-change.md |
| Migration ordering | Sequential 5-step migration with temporary re-exports | ADR-004 | architecture/ADR-004-sequential-migration-with-reexports.md |
| ToolContext location | `mcp/context.rs` (MCP-specific, no shared/ module needed) | Architect decision, human-approved | N/A |
| UDS module granularity | Single `uds/listener.rs`, no handler split | Specification NOT in Scope | N/A |

## Files to Create/Modify

### New Files

| Path | Purpose |
|------|---------|
| `src/infra/mod.rs` | Re-exports for 13 infrastructure modules |
| `src/mcp/mod.rs` | Re-exports for MCP transport modules |
| `src/mcp/context.rs` | ToolContext struct definition |
| `src/mcp/response/mod.rs` | Shared helpers, re-exports (from response.rs) |
| `src/mcp/response/entries.rs` | Entry formatting functions (from response.rs) |
| `src/mcp/response/mutations.rs` | Generic format_status_change + enroll (from response.rs) |
| `src/mcp/response/status.rs` | format_status_report (from response.rs) |
| `src/mcp/response/briefing.rs` | format_briefing, format_retrospective (from response.rs) |
| `src/uds/mod.rs` | Re-exports + UDS_CAPABILITIES constant |
| `src/services/status.rs` | StatusService (extracted from tools.rs context_status) |

### Moved Files (rename only, content preserved)

| From | To |
|------|----|
| `src/audit.rs` | `src/infra/audit.rs` |
| `src/registry.rs` | `src/infra/registry.rs` |
| `src/session.rs` | `src/infra/session.rs` |
| `src/scanning.rs` | `src/infra/scanning.rs` |
| `src/validation.rs` | `src/infra/validation.rs` |
| `src/categories.rs` | `src/infra/categories.rs` |
| `src/contradiction.rs` | `src/infra/contradiction.rs` |
| `src/coherence.rs` | `src/infra/coherence.rs` |
| `src/pidfile.rs` | `src/infra/pidfile.rs` |
| `src/shutdown.rs` | `src/infra/shutdown.rs` |
| `src/embed_handle.rs` | `src/infra/embed_handle.rs` |
| `src/usage_dedup.rs` | `src/infra/usage_dedup.rs` |
| `src/outcome_tags.rs` | `src/infra/outcome_tags.rs` |
| `src/tools.rs` | `src/mcp/tools.rs` |
| `src/identity.rs` | `src/mcp/identity.rs` |
| `src/uds_listener.rs` | `src/uds/listener.rs` |
| `src/hook.rs` | `src/uds/hook.rs` |

### Modified Files

| Path | Change |
|------|--------|
| `src/lib.rs` | Replace flat pub mod list with grouped module declarations |
| `src/server.rs` | Add `build_context()` and `require_cap()` methods |
| `src/services/mod.rs` | Add `pub(crate) mod status;` and StatusService to ServiceLayer |
| `src/infra/registry.rs` | Add `SessionWrite` variant to Capability enum |
| `src/uds/listener.rs` | Add UDS capability enforcement at dispatch |

### Deleted Files

| Path | Reason |
|------|--------|
| `src/response.rs` | Split into `src/mcp/response/` sub-module (5 files) |

## Data Structures

### ToolContext (`mcp/context.rs`)

```rust
pub(crate) struct ToolContext {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub format: ResponseFormat,
    pub audit_ctx: AuditContext,
}
```

### Capability Enum Update (`infra/registry.rs`)

```rust
pub enum Capability {
    Read,
    Write,
    Search,
    Admin,
    SessionWrite,  // NEW
}
```

### UDS Capabilities (`uds/mod.rs`)

```rust
pub(crate) const UDS_CAPABILITIES: &[Capability] = &[
    Capability::Read,
    Capability::Search,
    Capability::SessionWrite,
];
```

### StatusService (`services/status.rs`)

```rust
pub(crate) struct StatusService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    gateway: Arc<SecurityGateway>,
}
```

## Function Signatures

```rust
// mcp/context.rs (struct definition)
// server.rs (construction methods)
impl UnimatrixServer {
    pub(crate) fn build_context(
        &self,
        agent_id: &Option<String>,
        format: &Option<String>,
    ) -> Result<ToolContext, rmcp::ErrorData>;

    pub(crate) fn require_cap(
        &self,
        agent_id: &str,
        cap: Capability,
    ) -> Result<(), rmcp::ErrorData>;
}

// services/status.rs
impl StatusService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        embed_service: Arc<EmbedServiceHandle>,
        gateway: Arc<SecurityGateway>,
    ) -> Self;

    pub(crate) async fn compute_report(
        &self,
        topic_filter: Option<String>,
        category_filter: Option<String>,
    ) -> Result<(StatusReport, Vec<EntryRecord>), ServiceError>;

    pub(crate) async fn run_maintenance(
        &self,
        active_entries: &[EntryRecord],
    ) -> Result<MaintenanceResult, ServiceError>;
}

// mcp/response/mutations.rs
pub(crate) fn format_status_change(
    entry: &EntryRecord,
    action: &str,
    status_key: &str,
    status_display: &str,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult;
```

## Constraints

1. Pure restructuring — no behavioral changes (except additive UDS capability enforcement)
2. Post vnc-007 baseline — services/ already contains SearchService, StoreService, ConfidenceService, BriefingService, SecurityGateway
3. Wave independence — ships without requiring vnc-009
4. All changes within `crates/unimatrix-server/` — no new crates
5. No schema changes — no version bump, no new tables
6. Tests move with their code — no test deletions
7. Serde compatibility — SessionWrite addition must not break existing AGENT_REGISTRY deserialization
8. No new direct-storage coupling — StatusService exception documented (ADR-001)
9. rmcp `#[tool]` macro constrains handler signatures — ToolContext constructed inside handler, not injected
10. Sequential migration with re-exports — each step independently compilable (ADR-004)

## Dependencies

| Dependency | Type | Notes |
|---|---|---|
| vnc-006 | Feature prerequisite | Must be merged |
| vnc-007 | Feature prerequisite | Must be merged |
| rmcp 0.16.0 | External crate | Stable, constrains handler signatures |
| redb | External crate | Stable, used by StatusService direct-table access |
| serde + bincode v2 | External crate | Stable, SessionWrite variant must be serde-compatible |

## NOT in Scope

- UsageService extraction (vnc-009)
- Session-aware MCP (vnc-009)
- Rate limiting on search (vnc-009)
- `#[derive(Serialize)]` on StatusReport (vnc-009)
- UDS auth failure audit logging (vnc-009)
- HTTP transport
- Database replacement or storage abstraction
- UDS module split into listener + handlers
- `shared/` directory
- Changes to foundation crates

## Alignment Status

Vision alignment: 5 PASS, 1 WARN, 0 VARIANCE, 0 FAIL.

The WARN is a simplification: UDS module kept as single file instead of split into listener + handlers. This is within scope (documented as NOT in scope in specification).

No variances requiring approval.
