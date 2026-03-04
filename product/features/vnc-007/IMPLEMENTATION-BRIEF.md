# Implementation Brief: vnc-007 — Briefing Unification

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-007/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-007/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-007/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-007/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-007/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-007/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| BriefingService | pseudocode/briefing-service.md | test-plan/briefing-service.md |
| MCP Rewiring | pseudocode/mcp-rewiring.md | test-plan/mcp-rewiring.md |
| UDS Rewiring | pseudocode/uds-rewiring.md | test-plan/uds-rewiring.md |
| Duties Removal | pseudocode/duties-removal.md | test-plan/duties-removal.md |
| Feature Flag | pseudocode/feature-flag.md | test-plan/feature-flag.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Extract a transport-agnostic `BriefingService` that unifies MCP `context_briefing` (~236 lines) and UDS `handle_compact_payload` (~310 lines) behind a single caller-parameterized assembly method. Remove the dead duties section from briefing output. Wire `HookRequest::Briefing` for UDS-native briefing delivery. Gate the MCP tool behind a Cargo feature flag.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Feature gate mechanism | `#[cfg(feature = "mcp-briefing")]` on method; fallback to wrapper if rmcp incompatible | SR-01, human direction | architecture/ADR-001-feature-gated-mcp-briefing.md |
| Semantic search approach | Delegate to SearchService with k=3; avoids duplication, gets security gates | SR-02, architecture | architecture/ADR-002-briefing-delegates-to-searchservice.md |
| Budget model | Token budget (`max_tokens`). Proportional section allocation for injection history (40/30/20/5/5%). Linear fill for non-injection path. | SR-03, human direction | architecture/ADR-003-token-budget-proportional-allocation.md |
| S2 rate limiting | Deferred to vnc-009. No overlap with BriefingService (read vs write path). | SR-04, architect decision | architecture/ADR-004-s2-rate-limiting-deferred.md |
| Embedding behavior | Caller-controlled via `include_semantic` param. `false` = zero embedding/vector/SearchService. | Human direction | SCOPE.md Resolved Questions |
| Co-access boost | Always applied when semantic search active, regardless of transport. | Human direction | SCOPE.md Resolved Questions |
| Feature boost | Applied to semantic search results only, not injection history or category queries. | Human direction | SCOPE.md Resolved Questions |

## Files to Create

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/briefing.rs` | BriefingService, BriefingParams, BriefingResult, InjectionSections, InjectionEntry (~280 lines) |

## Files to Modify

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/mod.rs` | Add `pub mod briefing;`, add `briefing: BriefingService` to ServiceLayer, construct in `ServiceLayer::new()` |
| `crates/unimatrix-server/src/tools.rs` | Replace context_briefing inline assembly with BriefingService delegation; add `#[cfg(feature = "mcp-briefing")]` |
| `crates/unimatrix-server/src/uds_listener.rs` | Replace handle_compact_payload inline assembly with BriefingService delegation; add HookRequest::Briefing handler; remove primary_path/fallback_path/format helpers |
| `crates/unimatrix-server/src/response.rs` | Remove `duties` field from Briefing struct; remove duties sections from format_briefing (summary, markdown, JSON) |
| `crates/unimatrix-server/Cargo.toml` | Add `[features]` section with `mcp-briefing` feature (default on) |

## Data Structures

### New Types (services/briefing.rs)

```rust
pub(crate) struct BriefingService {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
}

pub(crate) struct BriefingParams {
    pub role: Option<String>,
    pub task: Option<String>,
    pub feature: Option<String>,
    pub max_tokens: usize,
    pub include_conventions: bool,
    pub include_semantic: bool,
    pub injection_history: Option<Vec<InjectionEntry>>,
}

pub(crate) struct BriefingResult {
    pub conventions: Vec<EntryRecord>,
    pub relevant_context: Vec<(EntryRecord, f64)>,
    pub injection_sections: InjectionSections,
    pub entry_ids: Vec<u64>,
    pub search_available: bool,
}

pub(crate) struct InjectionSections {
    pub decisions: Vec<(EntryRecord, f64)>,
    pub injections: Vec<(EntryRecord, f64)>,
    pub conventions: Vec<(EntryRecord, f64)>,
}

pub(crate) struct InjectionEntry {
    pub entry_id: u64,
    pub confidence: f64,
}
```

### Modified Types (response.rs)

```rust
pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    // duties: REMOVED
    pub relevant_context: Vec<(EntryRecord, f64)>,
    pub search_available: bool,
}
```

## Function Signatures

### BriefingService

```rust
impl BriefingService {
    pub(crate) fn new(
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
    ) -> Self;

    pub(crate) async fn assemble(
        &self,
        params: BriefingParams,
        audit_ctx: &AuditContext,
    ) -> Result<BriefingResult, ServiceError>;
}
```

### ServiceLayer (updated)

```rust
impl ServiceLayer {
    // Updated to construct BriefingService
    pub(crate) fn new(
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        // ... existing params ...
    ) -> Self;
}
```

## Constraints

1. vnc-006 service layer must be merged before implementation begins
2. Changes confined to `crates/unimatrix-server/` and `crates/unimatrix-engine/` (wire.rs only if needed)
3. No new crates, no schema version bump
4. Fire-and-forget patterns preserved for audit and confidence
5. Both compilation configurations (with/without mcp-briefing) must compile and pass tests
6. No changes to SearchService, ConfidenceService, or SecurityGateway interfaces
7. Net line count reduction expected (~280 new - ~480 removed = ~200 reduction)
8. UDS operational writes (injection logs, session records, co-access, signals) are NOT touched

## Dependencies

| Dependency | Type | Status |
|------------|------|--------|
| vnc-006 ServiceLayer | Code dependency | Must be merged |
| SearchService (vnc-006) | Used by BriefingService | Available post-merge |
| SecurityGateway (vnc-006) | Used by BriefingService | Available post-merge |
| AuditContext (vnc-006) | Used by BriefingService | Available post-merge |
| AsyncEntryStore | Used by BriefingService | Available (existing) |
| SessionRegistry (col-008) | Used by UDS transport | Available (existing) |
| HookRequest::Briefing (wire.rs) | Wire protocol variant | Exists, unimplemented |
| rmcp 0.16.0 | MCP framework | Available (existing) |

## NOT in Scope

- S2 rate limiting (deferred to vnc-009 per ADR-004)
- Module reorganization (vnc-008)
- SessionRegister briefing (stretch, not required)
- Unified capability model / SessionWrite (vnc-008)
- StatusService extraction (vnc-008)
- Changes to /query-patterns skill
- Deprecating duties entries in knowledge base
- HTTP transport
- Changes outside unimatrix-server and unimatrix-engine

## Alignment Status

Vision alignment: **PASS** across all checks. No VARIANCE or FAIL findings.

One WARN: The product vision entry mentions S2 rate limiting as part of vnc-007, but SCOPE.md grants the architect authority to defer, and ADR-004 documents the deferral rationale. The vision entry may need a minor update to reflect that S2 moves to vnc-009.

No variances require human approval before proceeding to Session 2.
