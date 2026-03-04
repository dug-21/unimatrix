# Architecture: vnc-008 — Module Reorganization

## System Overview

vnc-008 restructures the flat module layout of `crates/unimatrix-server/` into four logical groups — `services/`, `mcp/`, `uds/`, `infra/` — establishing clear import direction boundaries between transport, service, and infrastructure layers. It extracts `ToolContext` to reduce MCP handler ceremony, splits `response.rs` into a sub-module hierarchy, extracts `StatusService` from the 628-line `context_status` handler, and introduces a `SessionWrite` capability that formalizes UDS authorization boundaries.

This is pure restructuring with one additive behavioral change: UDS connections gain formal capability restrictions (`{Read, Search, SessionWrite}`) that prevent unauthorized access to Admin and Write operations.

### Post-Refactoring Module Layout

```
crates/unimatrix-server/src/
├── main.rs                    Application entry, server bootstrap
├── lib.rs                     Public re-exports (grouped modules)
├── error.rs                   Crate-wide error types
├── server.rs                  UnimatrixServer struct, backend wiring
│
├── services/                  Transport-agnostic business logic
│   ├── mod.rs                 ServiceLayer, AuditContext, AuditSource, ServiceError
│   ├── gateway.rs             SecurityGateway (S1-S5)
│   ├── search.rs              SearchService
│   ├── briefing.rs            BriefingService
│   ├── store_ops.rs           StoreService insert
│   ├── store_correct.rs       StoreService correct
│   ├── confidence.rs          ConfidenceService
│   └── status.rs              StatusService (NEW — extracted from context_status)
│
├── mcp/                       MCP transport layer (NEW group)
│   ├── mod.rs                 Re-exports
│   ├── tools.rs               Tool handlers (reduced ceremony via ToolContext)
│   ├── context.rs             ToolContext struct + construction (NEW)
│   ├── identity.rs            Identity resolution (moved from root)
│   └── response/              Response formatting (split from root response.rs)
│       ├── mod.rs             Re-exports, shared helpers (format_timestamp, parse_format, ResponseFormat)
│       ├── entries.rs         single_entry, search_results, lookup_results, store_success, correct, duplicate
│       ├── mutations.rs       Generic format_status_change + format_enroll_success
│       ├── status.rs          format_status_report
│       └── briefing.rs        format_briefing, format_retrospective_report
│
├── uds/                       UDS transport layer (NEW group)
│   ├── mod.rs                 Re-exports, UDS capability constants
│   ├── listener.rs            Accept loop, UID auth, dispatch (moved from uds_listener.rs)
│   └── hook.rs                Hook preprocessing (moved from root)
│
└── infra/                     Cross-cutting infrastructure (NEW group)
    ├── mod.rs                 Re-exports
    ├── audit.rs               Audit logging
    ├── registry.rs            Agent registry + Capability enum (SessionWrite added)
    ├── session.rs             Session registry
    ├── scanning.rs            Content scanning
    ├── validation.rs          Input validation
    ├── categories.rs          Category allowlist
    ├── contradiction.rs       Contradiction detection
    ├── coherence.rs           Coherence computation
    ├── pidfile.rs             PID management
    ├── shutdown.rs            Signal handling
    ├── embed_handle.rs        Embedding service handle
    ├── usage_dedup.rs         Usage deduplication
    └── outcome_tags.rs        Outcome tag parsing
```

### Import Direction Rules

```
          ┌─────────────┐
          │ main.rs     │  (wires everything together)
          │ server.rs   │
          └──────┬──────┘
                 │ imports all groups
    ┌────────────┼────────────┐
    ▼            ▼            ▼
┌────────┐  ┌────────┐  ┌────────┐
│  mcp/  │  │  uds/  │  │services│
└───┬────┘  └───┬────┘  └───┬────┘
    │           │            │
    │  imports  │  imports   │ imports
    ▼           ▼            ▼
┌────────┐              ┌────────────┐
│services│              │   infra/   │
└───┬────┘              └────────────┘
    │ imports                ▲
    ▼                        │ imports
┌────────┐              ┌────────────┐
│ infra/ │              │ foundation │
└───┬────┘              │  crates    │
    │ imports           └────────────┘
    ▼
┌────────────┐
│ foundation │
│  crates    │
└────────────┘
```

**Allowed imports:**
- `mcp/` -> `services/`, `infra/`
- `uds/` -> `services/`, `infra/`
- `services/` -> `infra/`, foundation crates (`unimatrix-store`, `unimatrix-core`, `unimatrix-vector`, `unimatrix-embed`, `unimatrix-engine`, `unimatrix-adapt`)
- `infra/` -> foundation crates only (no upward imports)
- `server.rs`, `main.rs` -> all groups (wiring layer)

**Disallowed imports:**
- `mcp/` -> foundation crates directly (must go through `services/`)
- `uds/` -> foundation crates directly (must go through `services/`)
- `infra/` -> `services/`, `mcp/`, `uds/` (no upward imports)
- `mcp/` <-> `uds/` (no cross-transport imports)

**Exception (ADR-001):** `mcp/tools.rs` retains limited direct imports of `unimatrix_store` types (`ENTRIES`, `COUNTERS`, `CATEGORY_INDEX`, `TOPIC_INDEX`, `deserialize_entry`) for the `context_status` handler until StatusService fully absorbs direct-table access. These imports are tracked and scheduled for removal.

**Exception:** `mcp/response/` and `uds/listener.rs` import `unimatrix_store::EntryRecord` and `unimatrix_core::Status` for type usage in formatting/serialization. These are data types, not storage access.

## Component Breakdown

### 1. ToolContext (`mcp/context.rs`)

Encapsulates the pre-validation ceremony that every MCP tool handler repeats. Constructed via a method on `UnimatrixServer` (ADR-002).

```rust
/// Pre-validated context available to every MCP tool handler.
pub(crate) struct ToolContext {
    /// Resolved agent identity.
    pub agent_id: String,
    /// Agent trust level.
    pub trust_level: TrustLevel,
    /// Parsed response format.
    pub format: ResponseFormat,
    /// Pre-built audit context for service calls.
    pub audit_ctx: AuditContext,
}
```

**Construction:**

```rust
impl UnimatrixServer {
    /// Resolve identity, parse format, build audit context.
    /// Capability check is NOT included — callers specify per-tool.
    pub(crate) fn build_context(
        &self,
        agent_id: &Option<String>,
        format: &Option<String>,
    ) -> Result<ToolContext, rmcp::ErrorData> {
        let identity = self.resolve_agent(agent_id)
            .map_err(rmcp::ErrorData::from)?;
        let format = parse_format(format)
            .map_err(rmcp::ErrorData::from)?;
        let audit_ctx = AuditContext {
            source: AuditSource::Mcp {
                agent_id: identity.agent_id.clone(),
                trust_level: identity.trust_level,
            },
            caller_id: identity.agent_id.clone(),
            session_id: None,
            feature_cycle: None,
        };
        Ok(ToolContext {
            agent_id: identity.agent_id,
            trust_level: identity.trust_level,
            format,
            audit_ctx,
        })
    }

    /// Check a capability for the given agent.
    pub(crate) fn require_cap(
        &self,
        agent_id: &str,
        cap: Capability,
    ) -> Result<(), rmcp::ErrorData> {
        self.registry.require_capability(agent_id, cap)
            .map_err(rmcp::ErrorData::from)
    }
}
```

**Design decision (ADR-002):** ToolContext is constructed via `self.build_context()` on UnimatrixServer, not injected as a parameter (rmcp `#[tool]` macro constrains handler signatures). Capability checking is a separate `self.require_cap()` call because different tools require different capabilities. This reduces the 15-25 line ceremony to 2-3 lines while keeping capability checks explicit.

### 2. StatusService (`services/status.rs`)

Extracts the ~628-line `context_status` computation from `tools.rs` into a service.

```rust
pub(crate) struct StatusService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    gateway: Arc<SecurityGateway>,
}

/// Computed status report data.
pub(crate) struct StatusReport {
    pub total_active: u64,
    pub total_deprecated: u64,
    pub total_proposed: u64,
    pub total_quarantined: u64,
    pub category_distribution: BTreeMap<String, u64>,
    pub topic_distribution: BTreeMap<String, u64>,
    pub entries_with_supersedes: u64,
    pub entries_with_superseded_by: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: BTreeMap<String, u64>,
    pub entries_without_attribution: u64,
    pub co_access_clusters: Vec<CoAccessClusterEntry>,
    pub coherence: Option<CoherenceReport>,
    pub session_stats: Option<SessionStats>,
    pub outcome_stats: Option<OutcomeStats>,
}

impl StatusService {
    /// Compute the full status report. Read-only, single transaction.
    pub(crate) async fn compute_report(
        &self,
        topic_filter: Option<String>,
        category_filter: Option<String>,
    ) -> Result<(StatusReport, Vec<EntryRecord>), ServiceError> { ... }

    /// Run maintenance operations (confidence refresh, graph compaction, co-access cleanup).
    /// Requires Admin capability (enforced by caller via ToolContext).
    pub(crate) async fn run_maintenance(
        &self,
        active_entries: &[EntryRecord],
    ) -> Result<MaintenanceResult, ServiceError> { ... }
}
```

**Design decision (ADR-001):** StatusService inherits direct-table access from the existing `context_status` code. The `compute_report` method opens a read transaction and scans `ENTRIES`, `COUNTERS`, `CATEGORY_INDEX`, `TOPIC_INDEX` tables directly, exactly as the current code does. This is documented as a known exception to the "services go through Store public API" principle — the Store lacks a `compute_report` method and adding one would be scope creep. The exception is tracked for future resolution.

### 3. SessionWrite Capability (`infra/registry.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Read context entries.
    Read,
    /// Write (store, correct) context entries.
    Write,
    /// Search context entries.
    Search,
    /// Administrative operations (status maintain, enrollment).
    Admin,
    /// Session-scoped writes (injection logs, session records, signals, co-access pairs).
    SessionWrite,
}
```

**UDS fixed capabilities:**

```rust
// uds/mod.rs
/// Fixed capabilities for UDS connections. Not configurable at runtime.
pub(crate) const UDS_CAPABILITIES: &[Capability] = &[
    Capability::Read,
    Capability::Search,
    Capability::SessionWrite,
];
```

**Capability matrix for UDS operations:**

| UDS Operation | Required Capability | Status |
|---|---|---|
| Ping | None | Unchanged |
| SessionRegister | SessionWrite | NEW enforcement |
| SessionClose | SessionWrite | NEW enforcement |
| RecordEvent / RecordEvents | SessionWrite | NEW enforcement |
| ContextSearch | Search | Unchanged (was implicit) |
| CompactPayload | Search + Read | Unchanged (was implicit) |
| Briefing | Search + Read | Unchanged (was implicit) |
| Injection log write | SessionWrite | NEW enforcement |
| Signal queue write | SessionWrite | NEW enforcement |
| Co-access pair write | SessionWrite | NEW enforcement |

No UDS operation currently requires `Write` or `Admin`, so the capability restriction is purely a formal boundary — no existing UDS functionality is removed.

### 4. response.rs Split (`mcp/response/`)

The ~2,550-line `response.rs` monolith splits into 5 files:

#### `mcp/response/mod.rs` (~80 lines)
- `ResponseFormat` enum, `parse_format()`, `format_timestamp()`
- `entry_to_json()` helper (used by entries.rs, mutations.rs)
- Re-exports from sub-modules

#### `mcp/response/entries.rs` (~700 lines)
- `format_single_entry()`
- `format_search_results()`
- `format_lookup_results()`
- `format_store_success()`, `format_store_success_with_note()`
- `format_correct_success()`
- `format_duplicate_found()`

#### `mcp/response/mutations.rs` (~250 lines)
- `format_status_change()` — generic formatter replacing deprecate/quarantine/restore (ADR-003, Refactor #6)
- `format_enroll_success()`

```rust
/// Generic status change formatter. Replaces format_deprecate_success,
/// format_quarantine_success, and format_restore_success.
pub(crate) fn format_status_change(
    entry: &EntryRecord,
    action: &str,        // "Deprecated", "Quarantined", "Restored"
    status_key: &str,    // "deprecated", "quarantined", "restored"
    status_display: &str, // "deprecated", "quarantined", "active"
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult { ... }
```

Existing public function names (`format_deprecate_success`, etc.) become thin wrappers calling `format_status_change` with the appropriate parameters, preserving backward compatibility during migration.

#### `mcp/response/status.rs` (~350 lines)
- `format_status_report()` — StatusReport formatting across all three output modes
- `StatusReport` struct (moved from response.rs)
- `CoAccessClusterEntry` struct

#### `mcp/response/briefing.rs` (~150 lines)
- `format_briefing()`
- `format_retrospective_report()`
- `Briefing` struct

### 5. Module Migration Strategy (ADR-004)

The migration is executed in a specific order to minimize breakage at each step:

**Step 1: Create `infra/` and move infrastructure modules**
- Create `infra/mod.rs` with re-exports
- Move audit, registry, session, scanning, validation, categories, contradiction, coherence, pidfile, shutdown, embed_handle, usage_dedup, outcome_tags
- Update `lib.rs` to expose `infra` module
- Add temporary re-exports in `lib.rs` from old paths: `pub use infra::audit;` etc.

**Step 2: Create `mcp/` and move MCP modules**
- Create `mcp/mod.rs` with re-exports
- Move tools.rs -> `mcp/tools.rs`
- Move identity.rs -> `mcp/identity.rs`
- Split response.rs -> `mcp/response/` (5 files)
- Create `mcp/context.rs` (ToolContext — NEW)
- Update imports in tools.rs

**Step 3: Create `uds/` and move UDS modules**
- Create `uds/mod.rs` with re-exports + UDS_CAPABILITIES constant
- Move uds_listener.rs -> `uds/listener.rs`
- Move hook.rs -> `uds/hook.rs`
- Update imports in listener.rs

**Step 4: Add SessionWrite capability and StatusService**
- Add `SessionWrite` to `Capability` enum in `infra/registry.rs`
- Add `UDS_CAPABILITIES` enforcement in `uds/listener.rs` dispatch
- Extract StatusService to `services/status.rs`
- Wire ToolContext into `mcp/tools.rs`

**Step 5: Clean up re-exports**
- Remove temporary re-exports from `lib.rs`
- Final import verification

**Design decision (ADR-004):** Sequential migration with temporary re-exports. Each step is independently compilable and testable. Re-exports from old paths (`pub use infra::audit as audit;`) prevent breaking all `use crate::audit::*` imports at once. Step 5 removes re-exports once all consumers are updated. This reduces the "big bang" risk identified in SR-03.

## Component Interactions

### MCP Tool Handler (After Refactoring)

```
mcp/tools.rs::context_search(params)
  1. let ctx = self.build_context(&params.agent_id, &params.format)?;   // ToolContext
  2. self.require_cap(&ctx.agent_id, Capability::Search)?;               // Capability
  3. validate_search_params(&params).map_err(rmcp::ErrorData::from)?;    // Validation
  4. let service_params = ServiceSearchParams { ... };                    // Build params
  5. let results = self.services.search.search(service_params, &ctx.audit_ctx).await?;
  6. let result = format_search_results(&results_with_scores, ctx.format);
  7. self.record_usage_for_entries(...).await;                            // Usage
  8. Ok(result)
```

Lines 1-2 replace the previous 15-25 line ceremony.

### context_status Handler (After Refactoring)

```
mcp/tools.rs::context_status(params)
  1. let ctx = self.build_context(&params.agent_id, &params.format)?;
  2. self.require_cap(&ctx.agent_id, Capability::Admin)?;
  3. validate_status_params(&params).map_err(rmcp::ErrorData::from)?;
  4. let (report, active_entries) = self.services.status
       .compute_report(params.topic, params.category).await?;
  5. if params.maintain.unwrap_or(false) {
       self.services.status.run_maintenance(&active_entries).await?;
     }
  6. Ok(format_status_report(&report, ctx.format))
```

The handler drops from ~628 lines to ~20 lines.

### UDS Dispatch (After Refactoring)

```
uds/listener.rs::dispatch(request, ...)
  // UDS connections have fixed capabilities: {Read, Search, SessionWrite}
  match request {
    HookRequest::SessionRegister { .. } => {
      // SessionWrite capability check (new)
      ...
    }
    HookRequest::ContextSearch { .. } => {
      // Search capability (was implicit, now explicit)
      handle_context_search(services, ...).await
    }
    ...
  }
```

## Technology Decisions

| ADR | Decision | Summary |
|-----|----------|---------|
| ADR-001 | StatusService direct-table access | StatusService inherits existing direct-table scans; Store API expansion deferred |
| ADR-002 | ToolContext via UnimatrixServer method | Constructed inside handler via `self.build_context()`, not injected |
| ADR-003 | Generic format_status_change | Single function replaces 3 near-identical formatters (deprecate/quarantine/restore) |
| ADR-004 | Sequential migration with re-exports | 5-step migration with temporary re-exports from old paths |

## Integration Points

### Existing Components (Relocated, Not Changed)

All 14 infrastructure modules move from root to `infra/` with no API changes. All `pub` and `pub(crate)` interfaces remain identical. Tests move with their modules.

### New Components

| Component | Module | Purpose |
|-----------|--------|---------|
| `ToolContext` | `mcp/context.rs` | Pre-validated MCP handler context |
| `build_context()` | `server.rs` (method on UnimatrixServer) | ToolContext factory |
| `require_cap()` | `server.rs` (method on UnimatrixServer) | Capability check helper |
| `StatusService` | `services/status.rs` | Status computation + maintenance |
| `format_status_change()` | `mcp/response/mutations.rs` | Generic mutation formatter |
| `SessionWrite` | `infra/registry.rs` (Capability variant) | Session-scoped write permission |
| `UDS_CAPABILITIES` | `uds/mod.rs` | Fixed UDS capability set |

### Modified Components

| Component | Change | Risk |
|-----------|--------|------|
| `lib.rs` | Replace flat `pub mod` list with grouped modules | Low — additive |
| `Capability` enum | Add `SessionWrite` variant | Low — additive, serde compatible |
| All `use crate::*` imports | Update paths to `crate::infra::*`, `crate::mcp::*`, `crate::uds::*` | Med — high churn, mitigated by re-exports |
| `mcp/tools.rs` | Replace ceremony with `build_context()` + `require_cap()` | Med — behavioral equivalence |
| `context_status` handler | Delegate to StatusService | Med — behavioral equivalence |

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `ToolContext` | struct { agent_id, trust_level, format, audit_ctx } | `mcp/context.rs` |
| `UnimatrixServer::build_context()` | `fn(&self, &Option<String>, &Option<String>) -> Result<ToolContext, ErrorData>` | `server.rs` |
| `UnimatrixServer::require_cap()` | `fn(&self, &str, Capability) -> Result<(), ErrorData>` | `server.rs` |
| `StatusService::new()` | `fn(Arc<Store>, Arc<VectorIndex>, Arc<EmbedServiceHandle>, Arc<SecurityGateway>) -> Self` | `services/status.rs` |
| `StatusService::compute_report()` | `async fn(&self, Option<String>, Option<String>) -> Result<(StatusReport, Vec<EntryRecord>), ServiceError>` | `services/status.rs` |
| `StatusService::run_maintenance()` | `async fn(&self, &[EntryRecord]) -> Result<MaintenanceResult, ServiceError>` | `services/status.rs` |
| `format_status_change()` | `fn(&EntryRecord, &str, &str, &str, Option<&str>, ResponseFormat) -> CallToolResult` | `mcp/response/mutations.rs` |
| `Capability::SessionWrite` | enum variant | `infra/registry.rs` |
| `UDS_CAPABILITIES` | `&[Capability]` constant | `uds/mod.rs` |

## File Layout (Estimated Line Counts)

```
services/                        (~3,550 lines, was ~2,910 from vnc-006/007)
  mod.rs                         (~265)  unchanged
  gateway.rs                     (~474)  unchanged
  search.rs                      (~272)  unchanged
  briefing.rs                    (~1,159) unchanged
  store_ops.rs                   (~346)  unchanged
  store_correct.rs               (~333)  unchanged
  confidence.rs                  (~58)   unchanged
  status.rs                      (~600)  NEW — extracted from tools.rs

mcp/                             (~3,350 lines)
  mod.rs                         (~20)   re-exports
  tools.rs                       (~1,800) reduced from ~2,600 via ToolContext + StatusService extraction
  context.rs                     (~80)   NEW
  identity.rs                    (~140)  moved from root
  response/                      (~1,300) split from 2,550-line response.rs
    mod.rs                       (~80)
    entries.rs                   (~700)
    mutations.rs                 (~250)
    status.rs                    (~350, moved to response/, was inline)
    briefing.rs                  (~150)

uds/                             (~3,280 lines)
  mod.rs                         (~20)   re-exports + UDS_CAPABILITIES
  listener.rs                    (~2,000) moved from uds_listener.rs
  hook.rs                        (~1,280) moved from root

infra/                           (~6,100 lines)
  mod.rs                         (~30)   re-exports
  audit.rs                       (~599)  moved
  registry.rs                    (~940)  moved + SessionWrite added
  session.rs                     (~1,006) moved
  scanning.rs                    (~423)  moved
  validation.rs                  (~1,209) moved
  categories.rs                  (~242)  moved
  contradiction.rs               (~820)  moved
  coherence.rs                   (~581)  moved
  pidfile.rs                     (~472)  moved
  shutdown.rs                    (~179)  moved
  embed_handle.rs                (~161)  moved
  usage_dedup.rs                 (~320)  moved
  outcome_tags.rs                (~435)  moved

root                             (~900 lines)
  main.rs                        (~296)
  lib.rs                         (~35 -> ~25)
  error.rs                       (~567)
  server.rs                      (~2,150 -> ~2,200, adds build_context/require_cap)
```

**Total:** ~19,380 lines (from ~19,186 — slight growth from new ToolContext, StatusService, module mod.rs files).
