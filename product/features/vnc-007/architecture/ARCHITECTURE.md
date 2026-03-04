# Architecture: vnc-007 — Briefing Unification

## System Overview

vnc-007 extracts a transport-agnostic `BriefingService` within `crates/unimatrix-server/src/services/` that unifies the two independent briefing assembly implementations: `context_briefing` (MCP tool, ~236 lines in tools.rs) and `handle_compact_payload` (UDS handler, ~310 lines in uds_listener.rs). The service sits in the existing service layer established by vnc-006, alongside SearchService, StoreService, and ConfidenceService.

BriefingService is caller-parameterized: the same `assemble()` method serves all callers, but behavior varies based on `BriefingParams`. Callers control whether semantic search (with embedding) occurs, whether conventions are included, and whether injection history is the entry source. This is the key design — one service, different params, different behavior.

```
┌───────────────────┐   ┌────────────────────┐
│   MCP Transport   │   │   UDS Transport    │
│   (tools.rs)      │   │ (uds_listener.rs)  │
│                   │   │                    │
│ context_briefing  │   │ CompactPayload     │
│  [feature-gated]  │   │ HookRequest::      │
│                   │   │   Briefing         │
└────────┬──────────┘   └────────┬───────────┘
         │                       │
         │  BriefingParams       │  BriefingParams
         │  include_semantic=T   │  include_semantic=F (Compact)
         │  max_tokens=3000      │  include_semantic=T (Briefing)
         │                       │  max_tokens=2000
         └───────────┬───────────┘
                     │
       ┌─────────────┴──────────────┐
       │      services/ module      │
       │                            │
       │  BriefingService (NEW)     │
       │    ├─ conventions lookup   │
       │    ├─ semantic search ─────┼──► SearchService (vnc-006)
       │    ├─ injection history    │
       │    ├─ budget allocation    │
       │    └─ S3/S4 via gateway   │
       │                            │
       │  SecurityGateway (vnc-006) │
       │  SearchService   (vnc-006) │
       │  StoreService    (vnc-006) │
       │  ConfidenceService(vnc-006)│
       └─────────────┬──────────────┘
                     │
       ┌─────────────┴──────────────┐
       │    Foundation Layer        │
       │  Store, VectorIndex,       │
       │  EmbedServiceHandle,       │
       │  AdaptationService         │
       └────────────────────────────┘
```

## Component Breakdown

### 1. BriefingService (`services/briefing.rs`)

The core new component. Assembles knowledge entries into a budget-constrained briefing result.

**Responsibilities:**
- Convention lookup by role/topic via AsyncEntryStore
- Semantic search delegation to SearchService (only when `include_semantic=true`)
- Injection history processing (deduplicate, partition by category, sort by confidence)
- Token budget allocation with fixed section priorities (decisions > injections > conventions)
- Feature boost on semantic search results only
- Co-access boost on semantic search results (always, when semantic search is active)
- Quarantine exclusion on all entry sources (S4 invariant via SecurityGateway)
- Input validation on role/task/max_tokens (S3 via SecurityGateway)

**Not responsible for:**
- Identity resolution (transport concern)
- Capability checks (transport concern)
- Response formatting (transport concern)
- Usage recording (transport concern)
- Compaction count management (UDS transport concern)
- Session state resolution (UDS transport concern)

```rust
pub(crate) struct BriefingService {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
}
```

BriefingService holds a clone of SearchService (which is `Clone` per vnc-006) for the semantic search path. It holds AsyncEntryStore directly for convention/category queries. It holds SecurityGateway for S3/S4 checks.

It does NOT hold EmbedServiceHandle, VectorIndex, or AdaptationService directly — those are accessed through SearchService when semantic search is active.

### 2. BriefingParams (`services/briefing.rs`)

Caller-provided parameters that control BriefingService behavior.

```rust
pub(crate) struct BriefingParams {
    pub role: Option<String>,
    pub task: Option<String>,
    pub feature: Option<String>,
    pub max_tokens: usize,

    // Entry source controls
    pub include_conventions: bool,
    pub include_semantic: bool,
    pub injection_history: Option<Vec<InjectionEntry>>,
}
```

**Critical invariant**: When `include_semantic=false`, BriefingService performs zero embedding, zero vector search, and zero SearchService involvement. The code path is pure entry-fetch-and-allocate.

### 3. BriefingResult (`services/briefing.rs`)

The assembled briefing output, transport-agnostic.

```rust
pub(crate) struct BriefingResult {
    /// Convention entries (from role/topic query).
    pub conventions: Vec<EntryRecord>,
    /// Semantically relevant entries with similarity scores.
    pub relevant_context: Vec<(EntryRecord, f64)>,
    /// Injection history entries partitioned by category.
    pub injection_sections: InjectionSections,
    /// All unique entry IDs included in the briefing.
    pub entry_ids: Vec<u64>,
    /// Whether semantic search was available and attempted.
    pub search_available: bool,
}

/// Injection history entries partitioned by category with fixed section priorities.
pub(crate) struct InjectionSections {
    pub decisions: Vec<(EntryRecord, f64)>,
    pub injections: Vec<(EntryRecord, f64)>,
    pub conventions: Vec<(EntryRecord, f64)>,
}
```

### 4. InjectionEntry (`services/briefing.rs`)

Minimal struct representing an injection history record passed from the UDS transport.

```rust
pub(crate) struct InjectionEntry {
    pub entry_id: u64,
    pub confidence: f64,
}
```

This abstracts over the UDS session's `InjectionRecord` without coupling BriefingService to the session module. The UDS transport converts its session injection history into `Vec<InjectionEntry>` before calling BriefingService.

### 5. Updated ServiceLayer (`services/mod.rs`)

ServiceLayer gains a `briefing` field.

```rust
pub struct ServiceLayer {
    pub(crate) search: SearchService,
    pub(crate) store_ops: StoreService,
    pub(crate) confidence: ConfidenceService,
    pub(crate) briefing: BriefingService,  // NEW
}
```

### 6. Updated Briefing Struct (`response.rs`)

The existing `Briefing` struct loses its `duties` field. The `format_briefing` function is updated to remove duties sections from all three formats (summary, markdown, JSON).

```rust
pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    // duties: removed
    pub relevant_context: Vec<(EntryRecord, f64)>,
    pub search_available: bool,
}
```

### 7. Feature-Gated MCP Tool

The `context_briefing` tool handler and its related `format_briefing` response function are gated behind `#[cfg(feature = "mcp-briefing")]`. BriefingService itself is NOT gated.

**Implementation approach** (ADR-001): Gate the entire `context_briefing` method with `#[cfg(feature = "mcp-briefing")]`. The rmcp `#[tool]` macro generates tool registration at the method level. If the method does not exist (compiled out), the tool is not registered. This must be verified during implementation — if rmcp does not support this, fall back to a wrapper approach where the tool method delegates to an inner function that is feature-gated, returning "tool not available" when the feature is off.

## Component Interactions

### MCP context_briefing Flow (After Refactoring)

```
tools.rs::context_briefing(params)
  1. Resolve identity (transport-specific)
  2. Check capability: Read (transport-specific)
  3. Validate MCP-specific params (format, helpful)
  4. Construct AuditContext::Mcp { agent_id, trust_level }
  5. Construct BriefingParams {
       role: params.role,
       task: params.task,
       feature: params.feature,
       max_tokens: validated_max_tokens(params.max_tokens),
       include_conventions: true,
       include_semantic: true,
       injection_history: None,
     }
  6. self.services.briefing.assemble(briefing_params, audit_ctx).await
  7. Convert BriefingResult -> Briefing struct for format_briefing
  8. Record usage (transport-specific: helpful/unhelpful, access_count)
  9. Format response via format_briefing (summary/markdown/json)
  10. Return CallToolResult
```

### UDS CompactPayload Flow (After Refactoring)

```
uds_listener.rs::handle_compact_payload(session_id, role, feature, token_limit)
  1. Get session state from SessionRegistry
  2. Resolve effective role/feature from session state or request fields
  3. Determine path: injection history exists? primary : fallback
  4. Convert byte budget to tokens: max_tokens = token_limit.unwrap_or(MAX_COMPACTION_BYTES) / 4
  5. Construct AuditContext::Uds { uid, pid, session_id }
  6. Construct BriefingParams {
       role: effective_role,
       feature: effective_feature,
       task: None,
       max_tokens,
       include_conventions: !has_injection_history (fallback includes conventions),
       include_semantic: false,  // NO embedding, NO vector search
       injection_history: if has_injection_history { Some(convert(session.injection_history)) } else { None },
     }
  7. self.services.briefing.assemble(briefing_params, audit_ctx).await
  8. Convert BriefingResult -> formatted text (reuse existing format_compaction_payload logic
     or implement equivalent formatting in the transport layer)
  9. Increment compaction count
  10. Return HookResponse::BriefingContent { content, token_count }
```

### UDS HookRequest::Briefing Flow (New)

```
uds_listener.rs::dispatch_request -> HookRequest::Briefing { role, task, feature, max_tokens }
  1. Construct AuditContext::Uds { uid, pid, session_id }
  2. Resolve max_tokens: max_tokens.unwrap_or(3000) (same default as MCP)
  3. Construct BriefingParams {
       role: Some(role),
       task: Some(task),
       feature,
       max_tokens,
       include_conventions: true,
       include_semantic: true,  // triggers embedding + vector search
       injection_history: None,
     }
  4. self.services.briefing.assemble(briefing_params, audit_ctx).await
  5. Convert BriefingResult -> plain text
  6. Return HookResponse::BriefingContent { content, token_count }
```

### BriefingService Internal Pipeline

```
BriefingService::assemble(params, audit_ctx)
  1. S3: Validate inputs via gateway (role length, task length, max_tokens range)
  2. Initialize token budget tracker (remaining = params.max_tokens)
  3. If injection_history is Some:
     a. Fetch entries by ID from entry_store
     b. S4: Exclude quarantined entries
     c. Deduplicate by entry_id (keep highest confidence)
     d. Partition into InjectionSections (decisions, injections, conventions)
     e. Sort each partition by confidence descending
     f. Allocate budget across sections (decisions first, then injections, then conventions)
  4. If include_conventions and no injection_history conventions:
     a. Query entry_store with topic=role, category="convention", status=Active
     b. S4: Exclude quarantined entries (should already be Active-only, but defense-in-depth)
     c. If feature provided, sort feature-tagged entries first
     d. Allocate remaining budget to conventions
  5. If include_semantic and task is Some:
     a. Delegate to SearchService::search(ServiceSearchParams {
          query: task, k: 3, feature_tag: feature,
          co_access_anchors: derive from already-collected entry_ids,
          similarity_floor: None, confidence_floor: None,
        }, audit_ctx)
     b. If SearchService returns EmbedNotReady, set search_available=false, continue
     c. Apply feature boost (already done by SearchService)
     d. Co-access boost (already done by SearchService)
     e. Allocate remaining budget to relevant_context
  6. Collect all entry_ids, deduplicate
  7. S5: Emit audit event with entry_ids
  8. Return BriefingResult
```

**Key design decision**: Steps 3, 4, and 5 are independent code paths selected by params. They share the budget tracker but do not call each other. The `include_semantic=false` path (steps 3 + optionally 4) never touches SearchService.

## Technology Decisions

| ADR | Decision | Summary |
|-----|----------|---------|
| ADR-001 | Feature-gated MCP tool via `#[cfg]` on method | Gate `context_briefing` method with `#[cfg(feature = "mcp-briefing")]`; fallback to wrapper if rmcp incompatible |
| ADR-002 | BriefingService uses SearchService for semantic search | Delegates to SearchService rather than reimplementing embed/HNSW/rerank; avoids duplication, gets security gates for free |
| ADR-003 | Token budget with proportional section allocation | Token-based budget (`max_tokens`) with fixed section priorities and proportional allocation for injection history sections |
| ADR-004 | S2 rate limiting deferred to vnc-009 | Rate limiting on writes is architecturally independent of BriefingService; deferral reduces vnc-007 scope without risk |

## Integration Points

### Existing Components Consumed (Unchanged)

| Component | Used By | How |
|-----------|---------|-----|
| `SearchService` (vnc-006) | BriefingService | Clone of SearchService for semantic search path |
| `AsyncEntryStore` (vnc-006) | BriefingService | Convention/category queries |
| `SecurityGateway` (vnc-006) | BriefingService | S3 input validation, S4 quarantine check |
| `AuditLog` (via gateway) | BriefingService | S5 audit emission |
| `SessionRegistry` | UDS transport | Session state resolution (before calling BriefingService) |
| `ServiceLayer` (vnc-006) | Both transports | Gains `briefing` field |

### New Components Introduced

| Component | Module | Purpose |
|-----------|--------|---------|
| `BriefingService` | `services/briefing.rs` | Unified briefing assembly |
| `BriefingParams` | `services/briefing.rs` | Caller-provided assembly parameters |
| `BriefingResult` | `services/briefing.rs` | Transport-agnostic assembly output |
| `InjectionSections` | `services/briefing.rs` | Partitioned injection history entries |
| `InjectionEntry` | `services/briefing.rs` | Abstraction over UDS injection records |

### Modified Components

| Component | Change | Risk |
|-----------|--------|------|
| `ServiceLayer` | Add `briefing: BriefingService` field | Low -- additive |
| `Briefing` struct (response.rs) | Remove `duties` field | Low -- duties are dead |
| `format_briefing` (response.rs) | Remove duties sections from all formats | Low -- dead code removal |
| `context_briefing` (tools.rs) | Replace inline assembly with BriefingService delegation; add `#[cfg(feature)]` | Med -- behavioral equivalence for conventions + semantic search |
| `handle_compact_payload` (uds_listener.rs) | Replace inline assembly with BriefingService delegation | Med -- behavioral equivalence for injection/fallback paths |
| `dispatch_request` (uds_listener.rs) | Wire `HookRequest::Briefing` to BriefingService | Low -- new handler |
| `dispatch_unknown_returns_error` test | Update to use a different unimplemented variant | Low -- trivial |
| `Cargo.toml` (unimatrix-server) | Add `[features]` section with `mcp-briefing` | Low -- additive |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `BriefingService::new(...)` | `fn(Arc<AsyncEntryStore<StoreAdapter>>, SearchService, Arc<SecurityGateway>) -> Self` | `services/briefing.rs` |
| `BriefingService::assemble(...)` | `async fn(&self, BriefingParams, &AuditContext) -> Result<BriefingResult, ServiceError>` | `services/briefing.rs` |
| `BriefingParams` | struct (see Component Breakdown) | `services/briefing.rs` |
| `BriefingResult` | struct (see Component Breakdown) | `services/briefing.rs` |
| `InjectionSections` | struct { decisions, injections, conventions } | `services/briefing.rs` |
| `InjectionEntry` | struct { entry_id: u64, confidence: f64 } | `services/briefing.rs` |
| `ServiceLayer.briefing` | `BriefingService` field | `services/mod.rs` |
| `Briefing` (response.rs) | struct without `duties` field | `response.rs` |
| `format_briefing(...)` | `fn(&Briefing, ResponseFormat) -> CallToolResult` (duties sections removed) | `response.rs` |
| `mcp-briefing` feature flag | Cargo feature, default on | `Cargo.toml` |

## File Layout

```
crates/unimatrix-server/src/
├── services/
│   ├── mod.rs           (+10 lines)  Add BriefingService to ServiceLayer
│   ├── briefing.rs      (~280 lines) BriefingService, BriefingParams, BriefingResult, InjectionSections, InjectionEntry
│   ├── gateway.rs       (unchanged)
│   ├── search.rs        (unchanged)
│   ├── store_ops.rs     (unchanged)
│   ├── store_correct.rs (unchanged)
│   └── confidence.rs    (unchanged)
├── tools.rs             (-200 lines) context_briefing becomes thin wrapper + feature gate
├── uds_listener.rs      (-250 lines) CompactPayload delegates to BriefingService; Briefing handler added
├── response.rs          (-30 lines)  Remove duties from Briefing struct and format_briefing
├── Cargo.toml           (+4 lines)   Add [features] section
└── ... (all other files unchanged)
```

Net change: ~280 new lines (briefing.rs) - ~480 removed lines = ~200 line reduction.
