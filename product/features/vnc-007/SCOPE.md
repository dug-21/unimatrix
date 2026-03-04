# vnc-007: Briefing Unification

## Problem Statement

The unimatrix-server has two independent implementations of knowledge assembly for agent orientation:

1. **`context_briefing`** (MCP tool, ~223 lines in tools.rs) — assembles conventions, duties, and semantically relevant entries for a role/task. Agent-initiated via MCP tool call.
2. **`handle_compact_payload`** (UDS handler, ~266 lines in uds_listener.rs) — assembles knowledge from session injection history or category queries as a compaction defense. Automatically triggered by PreCompact hook.

These two implementations solve the same problem (deliver budget-constrained knowledge to agents) through completely independent code paths with no shared backend. They differ in entry source, delivery mechanism, and trigger, but the core assembly logic (fetch entries, prioritize, allocate budget, format) is conceptually identical.

Additionally:
- The `context_briefing` MCP tool includes a **duties section** that is effectively dead. Agent defs are the sole authority for agent responsibilities (per col-011). No active duties entries exist in the knowledge base. The duties lookup, budget allocation, and formatting code are dead weight.
- The `HookRequest::Briefing` variant exists in the wire protocol (unimatrix-engine) but is unimplemented — `dispatch_request` returns `ERR_UNKNOWN_REQUEST` for it. There is no UDS-native briefing delivery path.
- The `context_briefing` MCP tool itself sees minimal production use (disabled in specialist agents, not called by coordinators, replaced by hook injection for primary agents). However, it provides programmatic briefing access that has no alternative endpoint.

vnc-006 (Service Layer + Security Gateway) extracted SearchService, StoreService, and ConfidenceService into a transport-agnostic service layer. vnc-007 continues this pattern by extracting BriefingService — the last major piece of duplicated business logic between the MCP and UDS paths.

## Goals

1. Extract a transport-agnostic `BriefingService` in `services/briefing.rs` that both MCP and UDS paths call for knowledge assembly
2. Remove the duties section from briefing assembly (lookup, budget allocation, formatting, and the `duties` field on the `Briefing` struct in response.rs)
3. Wire the existing `HookRequest::Briefing` variant to `BriefingService` for UDS-native briefing delivery
4. Rewire `handle_compact_payload` to delegate to `BriefingService` instead of its own inline assembly logic
5. Rewire `context_briefing` (MCP tool) to delegate to `BriefingService` instead of its own inline assembly logic
6. Gate the `context_briefing` MCP tool behind a Cargo feature flag (`mcp-briefing`, default-on) so it can be removed from the MCP interface in the future without code surgery
7. (Conditional) Add S2 rate limiting on knowledge writes (60/hour per caller) to SecurityGateway, closing finding F-09 — included if architect determines it fits scope; architect has authority to defer this to vnc-009

## Non-Goals

- **Module reorganization** — deferred to vnc-008 (Wave 3). vnc-007 adds `services/briefing.rs` but does not reorganize existing modules into `mcp/`, `uds/`, `infra/` groups.
- **Merging context_briefing into context_search** — these serve different intents (orientation vs targeted retrieval). They remain separate tools.
- **New hook event types** — no new `SessionRegister`-triggered briefing in this scope. If it falls out naturally from BriefingService work, the architect may include it as a stretch item, but it is not a requirement.
- **Changes to SearchService or ConfidenceService** — vnc-006 owns those. vnc-007 consumes SearchService from BriefingService but does not modify it.
- **Changes to UDS operational write paths** — injection logs, session records, co-access pairs, and signal writes are operational/telemetry writes that flow through dedicated UDS handlers, NOT through StoreService. vnc-007 does not touch these paths. The distinction matters: StoreService hardening (S1 content scan, S2 rate limiting) applies to the MCP knowledge-write path (`context_store`, `context_correct`), not to UDS operational writes.
- **StatusService extraction** — deferred to vnc-008.
- **Unified capability model / SessionWrite** — deferred to vnc-008.
- **HTTP transport** — future work enabled by the service layer.
- **Code changes outside `crates/unimatrix-server/` and `crates/unimatrix-engine/`** — except for the Cargo feature flag addition to `crates/unimatrix-server/Cargo.toml`.
- **Deprecating or quarantining duty entries** — knowledge base cleanup is an operational task, not a code change.

## Background Research

### Existing Research Documents

- **`product/research/optimizations/server-refactoring-architecture.md`** — Full 4-wave refactoring plan. Wave 2 (items 4-6) covers BriefingService extraction, StoreService write hardening, and HookRequest::Briefing wiring.
- **`product/research/optimizations/briefing-evolution.md`** — Analysis of context_briefing's production status, Option C (repurpose as hook backend) recommendation, and detailed before/after architecture for briefing delivery paths.
- **`product/features/vnc-006/`** — Design artifacts for the service layer (Wave 1) that vnc-007 builds on. SearchService, StoreService, ConfidenceService, SecurityGateway, AuditContext, and ServiceLayer are all established.

### Current Codebase State (Post vnc-006)

**Service layer exists** (`crates/unimatrix-server/src/services/`):
- `mod.rs` — ServiceLayer, AuditContext, AuditSource, ServiceError
- `gateway.rs` — SecurityGateway with S1/S3/S4/S5 (no S2 rate limiting yet)
- `search.rs` — SearchService (unified search pipeline)
- `store_ops.rs` — StoreService (unified writes with atomic audit, S1 content scan)
- `store_correct.rs` — StoreService correction operations
- `confidence.rs` — ConfidenceService (batched fire-and-forget)

**MCP briefing** (`tools.rs` lines 1384-1620, ~236 lines):
- Identity resolution + capability check (Read)
- Validation (briefing params, helpful, format, max_tokens)
- Conventions lookup (topic=role, category="convention", status=Active)
- Duties lookup (topic=role, category="duties", status=Active) -- TO BE REMOVED
- Semantic search (embed task, HNSW k=3, feature boost, co-access boost)
- Token budget allocation (conventions > duties > context) -- duties portion TO BE REMOVED
- Build Briefing struct, collect entry IDs, audit, usage recording, format response

**UDS CompactPayload** (`uds_listener.rs` lines 731-1040, ~310 lines including helpers):
- `handle_compact_payload` (lines 735-798): Orchestration, budget, session state, path selection
- `primary_path` (lines 800-847): Fetch from injection history, deduplicate, partition by category
- `fallback_path` (lines 849-925): Query by category when no injection history
- `format_compaction_payload` (lines 928-989): Header, section formatting, byte budget
- `format_category_section` (lines 992+): Per-section entry formatting

**Briefing struct** (`response.rs` lines 458-472):
- Fields: role, task, conventions, duties, relevant_context, search_available
- `format_briefing` function (lines 1039-1120): Summary/Markdown/JSON formatting

**Wire protocol** (`crates/unimatrix-engine/src/wire.rs`):
- `HookRequest::Briefing { role, task, feature, max_tokens }` — exists, marked `#[allow(dead_code)]`
- `HookResponse::BriefingContent { content, token_count }` — exists and working
- `dispatch_request` catch-all returns `ERR_UNKNOWN_REQUEST` for Briefing

**Dispatch routing** (`uds_listener.rs` line 576-580):
- `HookRequest::Briefing` falls through to the `_ =>` catch-all, returning error

### Key Decisions Already Made

| Decision | Resolution | Source |
|----------|------------|--------|
| Remove duties from briefing | Yes, agent defs are sole authority | col-011, briefing-evolution.md |
| Keep context_briefing MCP tool | Yes, behind feature flag for future removal | Human direction |
| Service layer location | In-crate `services/` module | vnc-006 |
| BriefingService design | Configurable entry sources, budget-constrained assembly | server-refactoring-architecture.md |
| MCP tool identity | Keep separate from context_search (Option C1) | briefing-evolution.md, human direction |
| S2 rate limiting inclusion | Conditional, architect decides | Human direction |
| Budget model | Token budget. BriefingService accepts `max_tokens`. MCP passes `max_tokens` directly from its interface. UDS converts its byte budget to tokens (`bytes / 4`). Tokens cascade from interface to service without intermediate char conversion. | Human direction |
| Section priorities | Fixed section priorities (decisions > injections > conventions) preserved. Injection content has high change rate; easier to migrate away from fixed priorities later than to add them. | Human direction |
| Feature boost scope | Feature boost applied to semantic search results only. Not applied to injection history entries or category query results. | Human direction |
| Co-access boost scope | Always apply co-access boost when doing semantic search, regardless of transport. Standardizes behavior across MCP and UDS paths. | Human direction |

### Analysis: MCP vs UDS Briefing Paths

The two paths differ in three dimensions that BriefingService must abstract over:

| Dimension | MCP context_briefing | UDS CompactPayload | Unified in BriefingService |
|-----------|---------------------|-------------------|---------------------------|
| **Entry source** | Metadata query (role/conventions) + semantic search (task) | Session injection history (primary) or category query (fallback) | Configurable: `include_conventions`, `include_semantic`, `injection_history` |
| **Budget model** | Token-based (default 3000 tokens) | Byte-based (default 8000 bytes, sectioned: decisions > injections > conventions) | Token budget (`max_tokens`). MCP passes directly; UDS converts bytes to tokens (`bytes / 4`). |
| **Section priorities** | Linear fill: conventions > relevant context | Fixed sections: decisions > injections > conventions | Fixed section priorities preserved (decisions > injections > conventions) |
| **Feature boost** | On semantic search results | On fallback category queries | On semantic search results only |
| **Co-access boost** | On semantic search results | Not applied | Always applied when semantic search is active |
| **Output** | MCP CallToolResult (summary/markdown/json) | HookResponse::BriefingContent (plain text) | BriefingResult struct; transports format to native response |

BriefingService unifies the entry sourcing and budget allocation into a single pipeline. Each transport converts the BriefingResult into its native response format. The guiding principle: standardize behavior across the two independent systems, allowing two paths to get the same result.

## Proposed Approach

### 1. Extract BriefingService (`services/briefing.rs`)

Create `BriefingService` in the existing `services/` module. The service accepts configurable parameters that support both MCP and UDS entry sources.

```
BriefingService::assemble(params: BriefingParams, audit_ctx: &AuditContext) -> Result<BriefingResult, ServiceError>

BriefingParams {
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    max_tokens: usize,

    // Entry source controls — callers choose which sources to activate
    include_conventions: bool,       // query conventions by role/topic
    include_semantic: bool,          // embed task + vector search (triggers embedding)
    injection_history: Option<Vec<InjectionEntry>>,  // session injection entries
}

BriefingResult {
    conventions: Vec<EntryRecord>,
    relevant_context: Vec<(EntryRecord, f64)>,
    injection_entries: BriefingInjectionEntries,
    entry_ids: Vec<u64>,
    search_available: bool,
}
```

BriefingService uses SearchService (from vnc-006) for semantic search rather than reimplementing embed/HNSW/rerank. It uses AsyncEntryStore for metadata queries (conventions by role). The SecurityGateway applies S3 (input validation on role/task/max_tokens) and S4 (quarantine exclusion on all assembled entries).

**Budget** is token-based (`max_tokens`). The MCP interface already exposes `max_tokens`, so it passes straight through to BriefingService. The UDS path converts its byte budget to tokens (`bytes / 4`) before calling BriefingService. Section priorities are fixed: decisions > injections > conventions.

**Embedding behavior is caller-controlled via `include_semantic`**. When `include_semantic=true` (MCP path, HookRequest::Briefing path), BriefingService invokes SearchService to embed the task query, perform HNSW search, and apply feature boost + co-access boost on the results. When `include_semantic=false` (UDS CompactPayload path), BriefingService performs NO embedding, NO vector search, and NO SearchService involvement — it only fetches entries by ID (injection history) or by category query. The same BriefingService, different params, different behavior. Callers decide.

### 2. Remove Duties

- Remove `duties` field from `Briefing` struct in response.rs
- Remove duties lookup from `context_briefing` handler
- Remove duties budget allocation
- Remove duties formatting sections (markdown, summary, JSON)
- Remove `include_duties` from BriefingParams (it never existed in BriefingService)
- Keep "duties" in category allowlist (entries may still exist; removing the category would break existing stores)

### 3. Rewire MCP context_briefing

The MCP handler becomes a thin transport wrapper:
1. Identity resolution + capability check (transport-specific)
2. Validation (transport-specific MCP params)
3. Construct `BriefingParams { include_conventions: true, include_semantic: true, injection_history: None, max_tokens: params.max_tokens }` — max_tokens passes straight through from MCP interface
4. Call `BriefingService::assemble(params, audit_ctx)` — triggers embedding + vector search because `include_semantic=true`
5. Format response (transport-specific)
6. Usage recording (transport-specific)

### 4. Rewire CompactPayload

`handle_compact_payload` becomes a thin orchestration wrapper:
1. Get session state from SessionRegistry
2. Convert byte budget to tokens: `max_tokens = token_limit.unwrap_or(MAX_COMPACTION_BYTES / 4)`
3. Construct `BriefingParams { include_conventions: false/true (based on path), include_semantic: false, injection_history: Some(session.history) or None, max_tokens }` — `include_semantic=false` means NO embedding, NO vector search
4. Call `BriefingService::assemble(params, audit_ctx)`
5. Format as `HookResponse::BriefingContent`
6. Increment compaction count

The `primary_path`, `fallback_path`, and `format_compaction_payload` helper functions are absorbed into BriefingService or removed.

### 5. Wire HookRequest::Briefing

Replace the `_ =>` catch-all in `dispatch_request` with a proper handler for `HookRequest::Briefing`:
1. Convert `max_tokens` from wire format (default if None)
2. Construct `BriefingParams { include_conventions: true, include_semantic: true, injection_history: None, max_tokens }` — `include_semantic=true` triggers embedding + vector search
3. Call `BriefingService::assemble(params, audit_ctx)`
4. Format as `HookResponse::BriefingContent`

### 6. Feature Flag for context_briefing MCP Tool

Add a Cargo feature `mcp-briefing` (default = on) to `crates/unimatrix-server/Cargo.toml`. Gate the `context_briefing` tool handler and its related response formatting with `#[cfg(feature = "mcp-briefing")]`. When compiled without the feature, the tool is absent from the MCP tool list. BriefingService itself is unconditional — only the MCP endpoint is gated.

### 7. (Conditional) S2 Rate Limiting on Knowledge Writes

If architect determines this fits scope: add a `RateLimiter` to SecurityGateway that tracks write operations per caller. StoreService calls `gateway.check_rate_limit(audit_ctx)` before processing writes. 60 writes/hour default per caller, configurable. Closes F-09. If architect determines it should be deferred, it moves to vnc-009 with no impact on the rest of vnc-007.

## Acceptance Criteria

### BriefingService Extraction

- AC-01: `BriefingService` struct exists in `services/briefing.rs` with an `assemble()` method that accepts `BriefingParams` and `AuditContext`
- AC-02: `BriefingService` supports convention lookup by role/topic when `include_conventions=true`
- AC-03: `BriefingService` performs semantic search (embedding + HNSW + feature boost + co-access boost) when `include_semantic=true`, using SearchService from vnc-006. When `include_semantic=false`, NO embedding or vector search occurs.
- AC-04: `BriefingService` supports injection history as entry source when `injection_history` is provided
- AC-05: `BriefingService` applies token budget allocation across all entry sources, respecting `max_tokens`
- AC-06: `BriefingService` excludes quarantined entries from all assembled results (S4 invariant)
- AC-07: `BriefingService` validates input parameters via SecurityGateway S3 (role length, task length, max_tokens range)
- AC-08: `BriefingService` is registered in `ServiceLayer` and accessible from both MCP and UDS transports

### Duties Removal

- AC-09: The `Briefing` struct in response.rs has no `duties` field
- AC-10: The `context_briefing` MCP handler performs no duties lookup (no query with `category: "duties"`)
- AC-11: The `format_briefing` function in response.rs has no duties section in any format (summary, markdown, JSON)
- AC-12: BriefingService has no duties concept in its params or results

### MCP Rewiring

- AC-13: `context_briefing` delegates to `BriefingService::assemble()` instead of inline assembly logic
- AC-14: `context_briefing` produces equivalent output (minus duties) for the same inputs — conventions and relevant context sections are functionally identical
- AC-15: `context_briefing` retains identity resolution, capability check, validation, usage recording, and response formatting as transport-specific concerns
- AC-16: `context_briefing` is gated behind `#[cfg(feature = "mcp-briefing")]`
- AC-17: The `mcp-briefing` feature is defined in `crates/unimatrix-server/Cargo.toml` with `default = ["mcp-briefing"]`

### UDS CompactPayload Rewiring

- AC-18: `handle_compact_payload` delegates to `BriefingService::assemble()` instead of inline `primary_path`/`fallback_path` logic
- AC-19: CompactPayload produces equivalent output for the same session state and entry data — decision/injection/convention sections have the same entries in the same priority order
- AC-20: Session state (role, feature, compaction count) is still resolved from SessionRegistry before calling BriefingService
- AC-21: Compaction count increment still occurs after assembly

### UDS-Native Briefing (HookRequest::Briefing)

- AC-22: `dispatch_request` handles `HookRequest::Briefing` by delegating to `BriefingService::assemble()`
- AC-23: `HookRequest::Briefing { role, task, feature, max_tokens }` returns `HookResponse::BriefingContent` with assembled briefing (conventions + semantic search for the task)
- AC-24: `HookRequest::Briefing` no longer returns `ERR_UNKNOWN_REQUEST`

### Feature Flag

- AC-25: Compiling with `--no-default-features` (or without `mcp-briefing`) produces a binary where `context_briefing` is not registered as an MCP tool
- AC-26: Compiling with default features (including `mcp-briefing`) produces a binary where `context_briefing` is registered and functional
- AC-27: BriefingService itself is NOT gated — it is always available regardless of feature flag (UDS paths always need it)

### S2 Rate Limiting (Conditional — architect may defer)

- AC-28: (Conditional) SecurityGateway has a `check_write_rate()` method that tracks writes per caller
- AC-29: (Conditional) StoreService calls `check_write_rate()` before processing `insert()` and `correct()` operations
- AC-30: (Conditional) Default rate limit is 60 writes per hour per caller, rejecting with `ServiceError::RateLimited` when exceeded
- AC-31: (Conditional) Rate limiter state is in-memory (no persistence needed — resets on server restart)
- AC-32: (Conditional) `AuditSource::Internal` callers are exempt from rate limiting

### Test and Quality

- AC-33: No net reduction in test count from vnc-006 baseline
- AC-34: BriefingService has unit tests covering: conventions-only assembly, semantic-only assembly, injection-history assembly, mixed sources, budget overflow, empty results, quarantine exclusion
- AC-35: Integration tests verify MCP context_briefing and UDS CompactPayload produce functionally equivalent results when backed by BriefingService
- AC-36: The `dispatch_unknown_returns_error` test is updated (Briefing is no longer unknown)
- AC-37: No changes outside `crates/unimatrix-server/` and `crates/unimatrix-engine/` (wire.rs only if Briefing variant fields change)

## Constraints

1. **Dependency on vnc-006**: vnc-007 builds on the service layer from vnc-006. SearchService, SecurityGateway, AuditContext, and ServiceLayer must be available. vnc-006 must be merged before vnc-007 implementation begins.
2. **Rust workspace structure**: Changes confined to `crates/unimatrix-server/` (service, tools, UDS handler, response, Cargo.toml) and `crates/unimatrix-engine/` (wire.rs only if HookRequest::Briefing fields need modification). No new crates.
3. **Behavioral change**: Unlike vnc-006 (like-for-like), vnc-007 intentionally changes behavior by removing duties. All other output (conventions, relevant context, compaction entries) must be functionally equivalent.
4. **Feature flag mechanism**: Cargo features (`#[cfg(feature = "...")]`) are the standard Rust mechanism. The `mcp-briefing` feature gates only the MCP tool registration and handler — not the underlying service.
5. **Fire-and-forget pattern**: Audit writes, usage recording, and confidence recomputation remain fire-and-forget via `spawn_blocking`. BriefingService must not introduce blocking operations on the hot path.
6. **Schema version**: No schema version bump. BriefingService is pure logic — no new tables or schema changes.
7. **Wave independence**: vnc-007 must ship and merge without requiring vnc-008/009. No forward dependencies.
8. **UDS operational writes are NOT knowledge writes**: Injection logs, session records, co-access pairs, and signal writes flow through dedicated UDS handlers to specific tables (INJECTION_LOG, SESSIONS, CO_ACCESS). These are NOT routed through StoreService and are NOT subject to S1 content scanning or S2 rate limiting. Only MCP knowledge writes (context_store, context_correct) go through StoreService.

## Resolved Questions

1. **BriefingService budget model**: Token budget. BriefingService accepts `max_tokens`. The MCP interface already has `max_tokens` on it, so MCP passes the value straight through to BriefingService with no conversion. The UDS path converts its byte budget to tokens (`bytes / 4`) before calling BriefingService. Tokens cascade from interface to service naturally.

2. **CompactPayload section priorities**: Fixed section priorities preserved (decisions > injections > conventions). Injection content has a high change rate right now, making fixed priorities the safer starting point. It is easier to migrate away from fixed priorities later than to retrofit them.

3. **Feature boost scope**: Feature boost applied to semantic search results only. Not applied to injection history entries or category query results. This standardizes the behavior: feature boost is a relevance signal for search, not a categorization signal.

4. **Co-access boost scope**: Always apply co-access boost when doing semantic search, regardless of which transport triggered the request. This standardizes behavior across MCP and UDS paths. The UDS compaction path previously did not apply co-access boost because it did not do semantic search; when it gains semantic search capabilities via BriefingService, co-access boost comes with it.

## Tracking

https://github.com/dug-21/unimatrix/issues/84
