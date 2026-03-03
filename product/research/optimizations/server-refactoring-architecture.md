# Server Refactoring & Dual-Path Architecture Analysis

**Date**: 2026-03-03
**Scope**: unimatrix-server decomposition, MCP/UDS path unification, briefing evolution
**Status**: Research — no code changes
**Depends on**: [refactoring-analysis.md](refactoring-analysis.md), [architecture-dependencies.md](architecture-dependencies.md)
**Security companion**: [security-surface-analysis.md](security-surface-analysis.md) — full dual-path threat model

---

## Problem Statement

The unimatrix-server crate (19.6K lines, 43% of codebase, 23 modules) has two parallel request paths — MCP (stdio) and UDS (hook injection) — that duplicate core business logic with subtle divergences. As the product evolves toward UDS-native briefing delivery, the current architecture creates four compounding problems:

1. **Duplicated pipelines**: Search/ranking logic exists in both `tools.rs` and `uds_listener.rs` (~400 lines duplicated)
2. **Divergent capabilities**: MCP path has metadata filtering, co-access boost, feature boost; UDS path has injection tracking, confidence/similarity floors, session-scoped co-access dedup. Neither path has all features.
3. **Briefing misalignment**: `context_briefing` (MCP tool, 223 lines) and `handle_compact_payload` (UDS handler, 266 lines) solve the same problem (deliver knowledge to agents) through different mechanisms with no shared code.
4. **Security asymmetry**: MCP path has content scanning, capability checks, input validation, and audit logging. UDS path has **none of these** — zero content scanning on search queries, zero authorization, zero audit trail (see [security-surface-analysis.md](security-surface-analysis.md) findings F-25 through F-28). This is the direct consequence of building a second transport without a shared security layer.

The planned shift to UDS-native briefing delivery (replacing MCP-initiated `context_briefing`) will exacerbate all four problems unless the backend is unified first.

### Scoping Decision: Clean Refactoring vs Briefing Consolidation

**Recommendation: Wave 1 is clean (like-for-like + security). Wave 2 (briefing) is a separate, optional follow-on.**

Rationale:

- **Wave 1 (SearchService + ConfidenceService + StoreService + Security Gateway)** is pure structural refactoring. Both transports continue to produce identical output. The only behavioral additions are security gates (S1-S5) that are defense-in-depth — they don't change happy-path behavior. This can ship as a single PR with high confidence.

- **BriefingService (Wave 2)** is a behavioral change: removing duties, unifying entry sources, potentially changing what CompactPayload returns. It should be validated independently. Merging it into Wave 1 creates a single PR that's both structural AND behavioral — harder to review, harder to bisect if something regresses.

- **The cost of deferral is low.** Wave 1 extracts SearchService, which is the foundation BriefingService needs (both use the same embed→search→rank pipeline). Wave 2 can follow immediately — it's not throwaway work, it builds on Wave 1's services.

- **The cost of inclusion is high.** If briefing changes cause unexpected issues (different output quality, hook formatting changes, budget allocation differences), they're entangled with the structural refactoring. Clean separation means each change can be validated and rolled back independently.

---

## Current Architecture: Two Paths, Duplicated Orchestration

### MCP Path (tools.rs → server.rs → store)

```
stdin (MCP) → rmcp → ToolRouter
  → Identity resolution → Capability check → Validation
  → Business logic (embedding, search, ranking, writes)
  → Format response → Audit → Usage recording
  → stdout (MCP CallToolResult)
```

**Strengths**: Full identity/capability model, metadata-filtered search, audit trail, usage recording, format flexibility (summary/markdown/json).

**Weaknesses**: 13-step ceremony repeated in every handler (79 `.map_err` chains). Only accessible to agents that explicitly call MCP tools.

### UDS Path (hook.rs → uds_listener.rs → store)

```
hook stdin → build_request() → LocalTransport (UDS)
  → Peer auth (UID) → dispatch_request()
  → Handler function (search, compact, session ops)
  → HookResponse → hook stdout → Claude Code context
```

**Strengths**: Automatic injection (no agent action needed), session-scoped state, rework tracking, fire-and-forget for latency, injection history tracking.

**Weaknesses**: No metadata filtering on search, no category/tag pre-filtering, hardcoded similarity/confidence floors, no formal audit trail. **No content scanning** — raw user prompts flow directly to embedding unexamined. No capability checks — any UID-authenticated connection gets full access. No input validation on query strings.

### Where They Diverge

| Capability | MCP (tools.rs) | UDS (uds_listener.rs) |
|-----------|----------------|----------------------|
| Embedding + normalization | ✓ | ✓ (duplicated) |
| MicroLoRA adaptation | ✓ | ✓ (duplicated) |
| HNSW vector search | ✓ (`search_filtered`) | ✓ (`search` unfiltered) |
| Metadata pre-filtering | ✓ (topic/category/tags) | ✗ |
| Re-ranking (0.85*sim + 0.15*conf) | ✓ | ✓ (duplicated) |
| Provenance boost | ✓ | ✓ (duplicated) |
| Co-access boost | ✓ | ✓ (duplicated) |
| Similarity/confidence floors | ✗ | ✓ (SIMILARITY_FLOOR=0.5, CONFIDENCE_FLOOR=0.3) |
| Quarantine exclusion | ✓ | ✓ (duplicated) |
| Injection log (INJECTION_LOG table) | ✗ | ✓ |
| Session co-access dedup | ✗ | ✓ |
| **Content scanning** | **✓ (store/correct)** | **✗ (none — F-25)** |
| **Input validation (query)** | **✓ (lengths, types, chars)** | **✗ (session_id only — F-27)** |
| Identity + capability check | ✓ | ✗ (UID auth only — F-26) |
| Audit logging | ✓ | ✗ (tracing only — F-28) |
| **Rate limiting** | **✗ (F-09)** | **✗ (F-09)** |
| Usage recording + helpfulness | ✓ | ✗ |
| Response formatting | ✓ (3 formats) | ✗ (fixed text) |

### Briefing: Two Implementations of the Same Concept

| Aspect | MCP context_briefing | UDS CompactPayload |
|--------|---------------------|-------------------|
| **Trigger** | Agent explicitly calls MCP tool | Automatic (PreCompact hook) |
| **Entry source** | Metadata query (role→conventions, duties) + semantic search | Session injection history (primary) or category query (fallback) |
| **Semantic search** | ✓ (k=3 hardcoded, embed task) | ✗ |
| **Co-access boost** | ✓ (briefing-specific boost) | ✗ |
| **Feature boost** | ✓ (tag-match reranking) | ✓ (feature-first within category) |
| **Budget allocation** | Linear fill (conventions > duties > context) | Sectioned (decisions > injections > conventions) |
| **Token budget** | Configurable via `max_tokens` param (default 3000) | Configurable via `token_limit` (default MAX_COMPACTION_BYTES=8000) |
| **Output** | MCP CallToolResult (summary/markdown/json) | HookResponse::BriefingContent (plain text) |
| **Session awareness** | ✗ | ✓ (injection history, compaction count) |

**Key insight**: `context_briefing` assembles knowledge from metadata queries + semantic search. `CompactPayload` assembles knowledge from session injection history. A unified briefing backend should support both entry sources and both output targets.

---

## Proposed Architecture: Service Layer Extraction

### Design Principle

Extract business logic into a **transport-agnostic service layer** that both MCP and UDS paths call. The services own orchestration; transport layers own framing, auth, and response formatting. Critically, universal security invariants live in the service layer — not in transports — so they **cannot be bypassed by adding a new transport**. This directly addresses the security asymmetry where the UDS path was built without replicating MCP's security gates.

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│   MCP Transport   │  │   UDS Transport   │  │  HTTP Transport  │
│   (mcp/tools.rs)  │  │ (uds/listener.rs) │  │  (future)        │
│                   │  │                   │  │                  │
│  T: identity/caps │  │  T: UID auth      │  │  T: API key auth │
│  T: rmcp framing  │  │  T: wire framing  │  │  T: HTTP framing │
│  T: format select │  │  T: text format   │  │  T: JSON format  │
│  T: usage record  │  │  T: injection log │  │  T: rate headers │
│                   │  │  T: session track │  │                  │
└────────┬──────────┘  └────────┬──────────┘  └────────┬─────────┘
         │                      │                       │
         └──────────────────────┼───────────────────────┘
                                │
                  ┌─────────────┴─────────────┐
                  │   Security Gateway        │  ← Cannot be bypassed
                  │                           │
                  │  S1: Content scanning     │  Injection detect on writes,
                  │                           │  warn on search queries
                  │  S2: Rate limiting        │  Per-caller, transport
                  │                           │  provides CallerId
                  │  S3: Input validation     │  Length, type, char bounds
                  │  S4: Quarantine exclusion │  Invariant on all results
                  │  S5: Structured audit     │  Transport provides context
                  └─────────────┬─────────────┘
                                │
                  ┌─────────────┴─────────────┐
                  │    Service Layer           │
                  │                            │
                  │  SearchService             │
                  │  BriefingService           │
                  │  StoreService              │
                  │  StatusService             │
                  │  ConfidenceService         │
                  └─────────────┬──────────────┘
                                │
                  ┌─────────────┴─────────────┐
                  │   Foundation Layer         │
                  │                            │
                  │  unimatrix-store           │
                  │  unimatrix-vector          │
                  │  unimatrix-embed           │
                  │  unimatrix-engine          │
                  └────────────────────────────┘
```

### Security Gateway: Why It Must Be Service-Level

The UDS path's lack of content scanning (F-25), authorization (F-26), query validation (F-27), and audit logging (F-28) exists because security was implemented as a transport concern in the MCP layer. When the UDS transport was built, those gates were not replicated. Any future transport (HTTP, gRPC) would face the same omission risk.

The Security Gateway makes five invariants **structurally impossible to skip**:

| Gate | Enforces | Applied To |
|------|----------|-----------|
| **S1: Content scan** | Injection + PII detection | Hard-reject on writes; warn + log on search queries |
| **S2: Rate limit** | Per-caller write/search throttle | All transports, keyed by transport-provided `CallerId` |
| **S3: Input bounds** | Length, type, control-char validation | All service method parameters |
| **S4: Quarantine** | Excluded from all result sets | SearchService, BriefingService (already done, formalized) |
| **S5: Audit** | Structured record of every operation | All services, transport provides `AuditContext` |

Transport-specific security stays in transports:
- **MCP**: Identity resolution, RBAC capability checks, auto-enrollment, usage/helpfulness recording
- **UDS**: UID peer auth, session-id sanitization, fire-and-forget decision, injection/co-access tracking
- **HTTP** (future): API key auth, CORS, TLS, request signing

See [security-surface-analysis.md](security-surface-analysis.md) for the full threat model covering both attack surfaces.

### Service Definitions

All service methods accept `AuditContext` and `CallerId` for the Security Gateway. The gateway validates inputs, scans content, checks rate limits, and records audit events — services focus on business logic.

```rust
/// Transport-provided caller identity for rate limiting and audit.
enum CallerId {
    Agent(String),       // MCP: agent_id
    UdsSession(String),  // UDS: session_id
    ApiKey(String),      // HTTP: API key hash (future)
}

/// Transport-provided context for structured audit records.
struct AuditContext {
    source: AuditSource,
    caller_id: String,
    session_id: Option<String>,
}

enum AuditSource {
    Mcp { agent_id: String, trust_level: TrustLevel },
    Uds { uid: u32, pid: Option<u32>, session_id: String },
    Http { api_key_hash: String, remote_addr: String },  // future
}
```

#### 1. SearchService

Unified search pipeline callable from either transport.

```
SearchService::search(params: SearchParams, audit: AuditContext) -> SearchResults

SearchParams {
    query: String,
    k: usize,
    filters: Option<QueryFilter>,       // metadata pre-filter (MCP uses, UDS can opt-in)
    similarity_floor: Option<f64>,       // UDS uses 0.5, MCP uses none
    confidence_floor: Option<f64>,       // UDS uses 0.3, MCP uses none
    feature_tag: Option<String>,         // feature boost
    co_access_anchors: Option<Vec<u64>>, // override anchors (e.g., from session injection)
}

SearchResults {
    entries: Vec<ScoredEntry>,           // entry + final_score + similarity + confidence
    embedding: Vec<f32>,                 // reusable by caller (avoid double-embed)
}
```

**Security gates applied internally**:
- **S1**: Scan query for injection patterns (warn + audit, don't reject — users may legitimately search for injection-related patterns)
- **S2**: Rate limit check against `audit.caller_id`
- **S3**: Validate query length (≤10,000 chars), reject control chars, validate k range (1-100)
- **S4**: Quarantined entries excluded from results (invariant)
- **S5**: Audit record emitted with search params and result count

**Eliminates**: ~400 lines of duplicated search/rank/boost logic.

**Both transports** call the same pipeline. MCP adds identity/capability/formatting around it. UDS adds injection logging/session tracking around it.

#### 2. BriefingService

Unified briefing assembly callable from either transport.

```
BriefingService::assemble(params: BriefingParams, audit: AuditContext) -> BriefingResult

BriefingParams {
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    max_tokens: usize,

    // Entry sources (not mutually exclusive)
    include_conventions: bool,           // query by role+convention
    include_duties: bool,                // query by role+duties
    include_semantic: bool,              // embed task, search
    injection_history: Option<Vec<InjectionRecord>>,  // from session state
}

BriefingResult {
    sections: Vec<BriefingSection>,      // ordered sections with entries
    total_tokens: u32,
    entry_ids: Vec<u64>,                 // for usage recording / audit
}
```

**Security gates applied internally**:
- **S2**: Rate limit check (briefing is a read operation but may trigger embedding)
- **S3**: Validate role/task lengths, max_tokens range (500-10000)
- **S4**: Quarantined entries excluded from all sections
- **S5**: Audit record with returned entry IDs

**MCP context_briefing** calls with: `include_conventions=true, include_duties=true, include_semantic=true, injection_history=None`

**UDS CompactPayload** calls with: `include_conventions=false, include_duties=false, include_semantic=false, injection_history=Some(session.injection_history)`

**UDS Briefing (future)** calls with: `include_conventions=true, include_duties=true, include_semantic=true, injection_history=Some(session.injection_history)` — combining the best of both.

This is the key evolution: when briefing moves to UDS delivery, the service already supports both metadata-sourced and session-sourced entries in a single call.

#### 3. StoreService

Unified write operations with audit-in-transaction.

```
StoreService::insert(entry: NewEntry, audit: AuditContext) -> EntryRecord
StoreService::correct(original_id: u64, corrected: NewEntry, audit: AuditContext) -> EntryRecord
```

**Security gates applied internally**:
- **S1**: Content scan on title + content (hard-reject on match — same behavior as current MCP path)
- **S2**: Rate limit check (60 writes/hour per caller default)
- **S3**: Full input validation (title length, content length, category, tags, control chars)
- **S5**: Audit record written atomically in same transaction as entry

**Eliminates**: ~200 lines of duplicated index-writing logic between `server.rs` and `store/write.rs`.

**Approach**: Expose `Store::insert_in_txn(&WriteTransaction, NewEntry)` that accepts an external transaction. The service layer opens a single transaction, calls `insert_in_txn`, writes audit in the same transaction, and commits atomically.

**Note**: Currently only the MCP path performs writes. If UDS or HTTP gain write capability in the future, the security gates are already in place. The `SessionWrite` capability (see Capability Model below) separates session-tracking writes from knowledge writes.

#### 4. ConfidenceService

Fire-and-forget confidence recomputation.

```
ConfidenceService::recompute(entry_ids: &[u64])  // batched, fire-and-forget
```

**Eliminates**: 8 duplicated 20-line blocks (~160 lines).

---

## Capability Model Evolution

The current capability model (Read/Write/Search/Admin) is MCP-only. The UDS path has no authorization — any authenticated connection can do anything (F-26). The service layer introduces a unified capability model that all transports map into.

### Capabilities

```rust
enum Capability {
    Read,           // Read entries by ID, lookup by filters
    Search,         // Semantic search (triggers embedding)
    Write,          // Knowledge writes (store, correct, deprecate)
    SessionWrite,   // Session tracking writes (injection log, co-access, rework events)
    Admin,          // Enrollment, quarantine, maintenance
}
```

**`SessionWrite` is new** — it separates session-tracking writes (UDS legitimate need) from knowledge writes. Without this, either UDS gets `Write` (too permissive — a compromised local process could poison the KB through the socket) or UDS gets no write capability (breaks injection logging and co-access recording).

### Transport-Mapped Defaults

| Transport | Capabilities | Rationale |
|-----------|-------------|-----------|
| MCP (known agent) | Per AgentRegistry | Existing RBAC model, unchanged |
| MCP (unknown agent) | `{Read, Search}` | Existing auto-enrollment default |
| MCP ("human") | `{Read, Write, Search, Admin}` | Existing privileged default |
| UDS (authenticated) | `{Read, Search, SessionWrite}` | Hook processes need search + session tracking, not knowledge writes |
| HTTP (future) | Per API key configuration | TBD based on use case |

### Where Capabilities Are Checked

- **Transport layer**: MCP checks capabilities against AgentRegistry (existing). UDS maps UID auth to fixed capability set (new). HTTP maps API key to capabilities (future).
- **Service layer**: Services accept an optional capability set. If provided, the service validates the operation against it. If not provided (internal calls), the check is skipped.

This keeps the existing MCP RBAC working unchanged while closing the UDS authorization gap.

---

## Module Reorganization Plan

### Current (23 flat modules in unimatrix-server)

```
unimatrix-server/src/
├── main.rs           (284)    Application entry
├── lib.rs            (34)     Re-exports
├── tools.rs          (3,061)  MCP tool handlers (monolith)
├── response.rs       (2,550)  Response formatting
├── uds_listener.rs   (2,271)  UDS handler (monolith)
├── server.rs         (2,105)  Shared backend + write ops
├── hook.rs           (1,280)  Hook preprocessing
├── validation.rs     (1,209)  Input validation
├── session.rs        (1,006)  Session registry
├── registry.rs       (933)    Agent registry
├── contradiction.rs  (820)    Contradiction detection
├── coherence.rs      (581)    Coherence computation
├── audit.rs          (599)    Audit logging
├── pidfile.rs        (472)    PID management
├── outcome_tags.rs   (435)    Outcome tag parsing
├── scanning.rs       (423)    Content scanning
├── usage_dedup.rs    (320)    Usage deduplication
├── categories.rs     (242)    Category allowlist
├── embed_handle.rs   (161)    Embedding service handle
├── identity.rs       (140)    Identity resolution
├── error.rs          (567)    Error types
└── shutdown.rs       (179)    Signal handling
```

### Proposed (modular groupings)

```
unimatrix-server/src/
├── main.rs                    Application entry
├── lib.rs                     Public API
├── error.rs                   Error types
│
├── services/                  Transport-agnostic business logic + security gateway
│   ├── mod.rs
│   ├── gateway.rs            Security Gateway (S1-S5: scan, rate limit, validate, audit) (~300 lines)
│   ├── search.rs             SearchService (~200 lines)
│   ├── briefing.rs           BriefingService (~250 lines)
│   ├── store_ops.rs          StoreService (insert/correct with audit) (~200 lines)
│   ├── status.rs             StatusService (split from 628-line context_status) (~300 lines)
│   └── confidence.rs         ConfidenceService (fire-and-forget recompute) (~50 lines)
│
├── mcp/                       MCP transport layer
│   ├── mod.rs
│   ├── tools.rs              Tool handlers (thin: identity+caps+validate → service → format+audit)
│   ├── identity.rs           Identity resolution
│   └── response/             MCP response formatting (split from 2,550-line monolith)
│       ├── mod.rs            Re-exports + shared helpers (format_timestamp, entry_to_json, parse_format)
│       ├── entries.rs        single_entry, search_results, lookup_results, store_success, correct, empty (~300)
│       ├── mutations.rs      Generic status_change + deprecate/quarantine/restore/enroll (~150)
│       ├── status.rs         format_status_report (StatusReport gains #[derive(Serialize)]) (~200)
│       └── briefing.rs       format_briefing, format_retrospective_report (~100)
│
├── uds/                       UDS transport layer
│   ├── mod.rs
│   ├── listener.rs           Accept loop + auth + dispatch
│   ├── handlers.rs           Request handlers (thin: → service → injection tracking)
│   └── hook.rs               Hook preprocessing (sync, no tokio)
│
├── infra/                     Cross-cutting infrastructure
│   ├── mod.rs
│   ├── audit.rs              Audit logging
│   ├── registry.rs           Agent registry
│   ├── session.rs            Session registry
│   ├── scanning.rs           Content scanning
│   ├── validation.rs         Input validation
│   ├── categories.rs         Category allowlist
│   ├── contradiction.rs      Contradiction detection
│   ├── coherence.rs          Coherence computation
│   ├── pidfile.rs            PID management
│   ├── shutdown.rs           Signal handling
│   ├── embed_handle.rs       Embedding service handle
│   ├── usage_dedup.rs        Usage deduplication
│   └── outcome_tags.rs       Outcome tag parsing
│
└── shared/                    Shared types and utilities
    ├── mod.rs
    ├── context.rs            ToolContext (extracted ceremony)
    └── format.rs             Shared formatting utilities
```

### Estimated Impact

| Metric | Current | After Refactor | Delta |
|--------|---------|---------------|-------|
| `tools.rs` | 3,061 lines | ~1,200 lines | -60% |
| `uds_listener.rs` | 2,271 lines | ~800 lines | -65% |
| `server.rs` | 2,105 lines | ~600 lines (renamed to services/store_ops.rs) | -71% |
| `response.rs` | 2,550 lines (1 file) | ~2,080 lines across 4 files in `mcp/response/` | -18% |
| Duplicated search logic | ~400 lines (2 copies) | ~200 lines (1 copy in services/search.rs) | -50% |
| Duplicated write logic | ~200 lines (server.rs + write.rs) | 0 (in-txn methods) | -100% |
| Duplicated confidence recompute | ~160 lines (8 copies) | ~50 lines (1 service) | -69% |
| **Total server LOC** | **19,672** | **~16,880** | **~-14%** |

The line reduction is modest because the refactoring is primarily about organization and deduplication, not removal. The real win is **maintainability**: changes to search ranking or briefing assembly happen in one place, and no single file exceeds ~1,200 lines.

**response.rs decomposition detail**: The original 2,550 lines are ~1,220 lines of formatting code + ~1,330 lines of tests. Three fixes reduce the formatting code by ~250 lines: (1) generic `format_status_change` unifies deprecate/quarantine/restore (Refactor #6, ~70 lines), (2) `#[derive(Serialize)]` on StatusReport eliminates manual JSON arm in `format_status_report` (Refactor #9, ~130-150 lines), (3) merge `format_store_success` / `format_store_success_with_note` (~30 lines). The 4-file split in `mcp/response/` ensures each file is 100-300 lines. Tests move with their functions.

---

## Briefing Evolution: MCP → UDS → Unified

### Phase 1: Current State

```
MCP context_briefing:      Agent calls → query conventions+duties+semantic → format → respond
UDS CompactPayload:        PreCompact hook → session injection history → format → inject
UDS Briefing (dead_code):  Wire type exists, not dispatched
```

Two separate codepaths, no shared logic. `context_briefing` has no session awareness. `CompactPayload` has no semantic search.

### Phase 2: Service Extraction (this proposal)

```
BriefingService:           Unified assembly from multiple entry sources
MCP context_briefing:      identity + caps → BriefingService → format → respond
UDS CompactPayload:        session state → BriefingService → text format → inject
```

Both transports call the same service. Service supports all entry sources.

### Phase 3: UDS-Native Briefing (future)

```
UDS Briefing:              SessionStart or UserPromptSubmit hook → BriefingService
                           (conventions + duties + semantic + injection history)
                           → text format → inject via hook stdout
```

With the service layer in place, enabling UDS-native briefing is a small change:
1. Wire the `HookRequest::Briefing` variant to `BriefingService::assemble()`
2. Hook preprocessing builds Briefing request from SessionRegister or UserPromptSubmit events
3. Response goes through existing injection formatting

**No new business logic needed** — BriefingService already supports all entry sources.

### Phase 4: Retire MCP context_briefing (optional, future)

Once UDS briefing is proven, agents no longer need to explicitly call `context_briefing`. The MCP tool can remain for backward compatibility or be deprecated.

---

## Hook Architecture / Knowledge Architecture Overlap

### Current Overlap

The hook architecture (col-007 through col-010) and the knowledge architecture (Unimatrix knowledge engine) overlap in several areas:

| Concern | Hook Path | Knowledge Path | Redundancy |
|---------|-----------|---------------|------------|
| **Context delivery** | `ContextSearch` (UserPromptSubmit) | `context_search` (MCP tool) | Full pipeline duplication |
| **Briefing delivery** | `CompactPayload` (PreCompact) | `context_briefing` (MCP tool) | Conceptual overlap, no shared code |
| **Session state** | `SessionRegistry` (in-memory) | `SESSIONS` table (persistent) | Dual tracking |
| **Entry scoring** | Co-access dedup in session | Co-access in store (CO_ACCESS table) | Different dedup scopes |
| **Entry tracking** | INJECTION_LOG | Usage recording (access_count, helpful_count) | Different granularity |
| **Rework detection** | `ReworkEvent` tracking | Signal queue → outcome | Different detection phases |
| **Event recording** | `ImplantEvent` → EventQueue/RecordEvent | Usage recording in tools.rs | Different event models |

### Opportunities for Convergence

#### 1. Unified Usage Pipeline

Currently: MCP records usage via `record_usage_for_entries()` (access_count, helpful_count updates). UDS records injections via `INJECTION_LOG` writes. These are two usage signals for the same entries but tracked independently.

**Proposal**: A single `UsageService` that accepts events from both transports:

```
UsageService::record_access(entry_ids, source: AccessSource, context: UsageContext)

enum AccessSource { McpTool, HookInjection, Briefing }
struct UsageContext { session_id: Option<String>, agent_id: Option<String>, helpful: Option<bool> }
```

This feeds both the confidence pipeline (access_count) and the injection pipeline (INJECTION_LOG) from a single service.

#### 2. Session-Aware Search Across Both Paths

Currently: UDS search records co-access pairs and injection history. MCP search records nothing session-scoped.

**Proposal**: If the MCP path gains session context (via rmcp session tracking or a session_id parameter), both paths can share session-aware features:
- Co-access dedup (currently UDS-only)
- Injection history for briefing (currently UDS-only)
- Session-scoped usage analytics

This requires: adding an optional `session_id` to MCP search requests and wiring it through the SearchService.

#### 3. Unified Event Model

Currently: hooks emit `ImplantEvent` (generic JSON payload), while MCP tools emit `AuditEvent` (structured redb records). These capture overlapping information in incompatible formats.

**Observation**: This is lower priority. The two event models serve different purposes (operational audit vs. behavioral observation) and convergence would require schema evolution. Flag for future consideration.

---

## Retrospective Compatibility: Preserving the Path to Active Data Capture

### The Unachieved Goal

The retrospective pipeline (`context_retrospective` → `unimatrix-observe`) currently depends on **passive JSONL session files** written by Claude Code hook scripts to `~/.unimatrix/observation/`. These files contain tool call records (PreToolUse/PostToolUse), subagent lifecycle events, and task operations — parsed by the observation pipeline, attributed to feature cycles, and analyzed by 21 detection rules to produce hotspot findings.

The goal is to **eliminate dependence on passive JSONL capture** and instead derive retrospective insights from data Unimatrix actively captures through its own operations. This refactoring must not prevent that evolution — and ideally should lay groundwork for it.

### What the Retrospective Currently Needs (from JSONL)

| Data | What It Feeds | Example Detection Rules |
|------|--------------|------------------------|
| Tool call events (Pre/PostToolUse) | Friction detection, agent behavior, scope metrics | `permission_retries`, `search_via_bash`, `compile_cycles`, `edit_bloat` |
| Subagent lifecycle (Start/Stop) | Agent lifespan, coordinator respawns | `coordinator_respawns`, `lifespan` |
| Task lifecycle (Create/Update) | Phase extraction, completion tracking | `post_completion_work`, `phase_duration_outlier` |
| File paths from tool inputs | File breadth, mutation spread, reread rate | `file_breadth`, `mutation_spread`, `reread_rate` |
| Timestamps (ms precision) | Session timeout, cold restart, duration | `session_timeout`, `cold_restart` |
| Feature attribution signals | Feature-cycle partitioning | Path-based, task subject, git checkout |

### What Unimatrix Already Captures Actively

| Data | Table/Location | Currently Used in Retrospective? |
|------|---------------|--------------------------------|
| Session lifecycle (start/end/outcome) | SESSIONS | ✓ (feature grouping, status filter) |
| Entry injections (which entries served when) | INJECTION_LOG | ✗ (reserved for col-010) |
| Session outcome signals (helpful/rework) | SIGNAL_QUEUE → entries | ✓ (entry analysis, confidence) |
| Rework events (Edit→Bash(fail)→Edit) | SessionState (in-memory) | ✗ (in-memory only, lost on restart) |
| Agent actions (MCP tool calls) | SessionState.agent_actions (in-memory) | ✗ (in-memory only, lost on restart) |
| Co-access patterns | CO_ACCESS table | ✗ (not in retrospective) |
| Entry metadata (what was created/corrected) | ENTRIES, AUDIT_LOG | ✗ (not in retrospective) |

### The Gap: What Active Capture Can vs Cannot Replace

**Can replace with service-layer data** (no JSONL needed):
- Which knowledge entries were served, to whom, when, in what session → INJECTION_LOG (already captured)
- Session outcomes (success/rework/abandoned) → SESSIONS (already captured)
- Entry-level helpfulness → SIGNAL_QUEUE (already captured)
- Which entries were created/corrected/deprecated → AUDIT_LOG (already captured, just not connected to retrospective)

**Cannot replace — requires hook-captured operational data**:
- Tool call patterns (permission retries, compile cycles, search-via-bash) — Unimatrix doesn't see arbitrary tool calls
- File access patterns (breadth, mutation spread, reread rate) — Unimatrix only sees files referenced in knowledge entries
- Subagent lifecycle (spawn count, lifespan, prompt) — Unimatrix only sees agents via agent_id on MCP calls
- Phase timing and task lifecycle — Unimatrix doesn't track task operations

**Key insight**: Unimatrix sees **knowledge operations** (search, store, briefing). It does NOT see **development operations** (file reads, edits, bash commands, task management). The 21 detection rules are split roughly:

- **8 rules need only knowledge + session data** (can be derived from active capture): session_timeout, cold_restart, coordinator_respawns, post_completion_work, rework_events, phase_duration_outlier, adr_count, knowledge_entries_stored
- **13 rules need tool-level telemetry** (still need external data): permission_retries, sleep_workarounds, search_via_bash, output_parsing_struggle, context_load, lifespan, file_breadth, reread_rate, mutation_spread, compile_cycles, edit_bloat, source_file_count, design_artifact_count

### What the Service Layer Refactoring Should Do

The refactoring should capture **Operational Events** — a structured record of every service-layer operation, richer than audit events, retained for 60 days, queryable by session and feature cycle. This is the bridge between "audit log" (security/compliance, kept indefinitely) and "observation JSONL" (tool telemetry, external).

```rust
/// Operational event emitted by service layer operations.
/// Retained for 60 days. Queryable by session_id, feature_cycle.
struct OperationalEvent {
    event_id: u64,                    // Monotonic
    timestamp: u64,                   // Unix epoch seconds
    session_id: Option<String>,       // From AuditContext
    feature_cycle: Option<String>,    // From session or request
    caller_id: String,                // From CallerId
    operation: OperationType,         // Enum, not string
    detail: OperationDetail,          // Per-operation structured data
}

enum OperationType {
    Search,
    Briefing,
    Store,
    Correct,
    Deprecate,
    Quarantine,
    StatusCheck,
    Maintenance,
}

/// Per-operation structured data (not a generic JSON blob).
enum OperationDetail {
    Search {
        query_len: usize,
        k: usize,
        result_count: usize,
        entry_ids: Vec<u64>,          // What was returned
        had_injection_warning: bool,   // S1 scan triggered
        latency_ms: u32,              // End-to-end service time
    },
    Briefing {
        role: Option<String>,
        entry_count: usize,
        entry_ids: Vec<u64>,
        total_tokens: u32,
    },
    Store {
        entry_id: u64,
        category: String,
        feature_cycle: Option<String>,
    },
    Correct {
        original_id: u64,
        corrected_id: u64,
    },
    // ... other operations
}
```

### How This Enables Active Retrospective (Future)

With `OPERATIONAL_EVENT_LOG` + `INJECTION_LOG` + `SESSIONS` + `SIGNAL_QUEUE`, a future retrospective analysis can answer:

| Question (currently needs JSONL) | Active data source |
|----------------------------------|-------------------|
| How many searches happened per session? | OPERATIONAL_EVENT_LOG (Search events) |
| Which entries were served but led to rework? | INJECTION_LOG + SESSIONS.outcome |
| How many knowledge entries were created per feature? | OPERATIONAL_EVENT_LOG (Store events by feature_cycle) |
| Were there correction chains indicating confusion? | OPERATIONAL_EVENT_LOG (Correct events) + correction chain in entries |
| Did sessions time out or complete successfully? | SESSIONS.status |
| How much context was consumed per session? | OPERATIONAL_EVENT_LOG (Briefing.total_tokens) |
| Was maintenance triggered frequently? | OPERATIONAL_EVENT_LOG (Maintenance events) |

**What it still cannot answer** (still needs hook/external data):
- Tool-level friction (compile cycles, permission retries)
- File-level access patterns
- Subagent lifecycle details

Those 13 rules will continue to need hook-captured data. But the hook data path changes from "write JSONL to disk for later parsing" to "send RecordEvent through UDS for structured capture" — which the service layer is already positioned to receive and store.

### Design Constraints for the Refactoring

1. **Every service method must emit an OperationalEvent** — this is the active capture mechanism. The Security Gateway's S5 (audit) is the right integration point, but OperationalEvents are richer than audit records and stored separately.

2. **OperationalEvents must include session_id and feature_cycle** — these are the join keys for retrospective analysis. The `AuditContext` already carries session_id; feature_cycle comes from the session's registration metadata.

3. **60-day retention with GC** — OperationalEvents are intermediate-lived (longer than signals, shorter than knowledge entries). The existing `gc_sessions()` 30-day cascade for SESSIONS+INJECTION_LOG provides the pattern; OperationalEvents get 60-day TTL.

4. **Structured, not generic** — `OperationDetail` is an enum, not `serde_json::Value`. This ensures the retrospective pipeline can query efficiently without parsing JSON blobs. New operation types add enum variants, not ad-hoc JSON shapes.

5. **Don't block on it** — OperationalEvent writes should be fire-and-forget (same pattern as current usage recording). Service latency is not affected.

6. **The ReworkEvent gap** — Currently, rework tracking (`SessionState.rework_events`) is in-memory only, lost on server restart. The OperationalEvent for Search operations, combined with session outcome, provides an alternative signal: if a session that received entries X, Y, Z ended in rework, those entries are retrospectively flagged. This is weaker than per-file rework tracking but doesn't require in-memory state.

### What This Means for the Refactoring Scope

**In scope (Wave 1)**:
- `AuditContext` on all service methods (already planned)
- `AuditContext` includes `session_id` and `feature_cycle` (minor addition to existing design)
- Service methods return enough structured data for OperationalEvent construction (entry_ids, counts, latency)

**In scope (Wave 1, optional stretch)**:
- Define `OperationalEvent` and `OperationDetail` types
- Add `OPERATIONAL_EVENT_LOG` table (schema extension)
- Fire-and-forget writes from service methods

**Explicitly out of scope**:
- Modifying `unimatrix-observe` detection rules to use OperationalEvents
- Changing the JSONL capture path
- Building the "active retrospective" pipeline
- Expanding RecordEvent to replace JSONL for tool telemetry

The refactoring **enables** the future goal by ensuring every service operation produces a structured, session-aware, feature-tagged event record with 60-day retention. It does not implement the retrospective changes themselves.

---

## Implementation Sequencing

### Wave 1: Foundation + Security Baseline (low risk, high dedup, closes critical gaps)

1. **Extract SearchService with Security Gateway** — Unify search/rank/boost from tools.rs and uds_listener.rs. Gateway applies S1 (scan query, warn on injection), S3 (input bounds on query), S4 (quarantine exclusion), S5 (audit record).
2. **Extract ConfidenceService** — Consolidate 8 fire-and-forget blocks
3. **Expose Store::insert_in_txn** — Enable shared write transactions

These three changes eliminate ~760 lines of duplication AND close the most critical security gap: UDS search queries now pass through content scanning (F-25), input validation (F-27), and generate audit records (F-28).

**Security findings closed**: F-25 (UDS content scanning), F-27 (UDS query validation), F-28 (UDS audit trail — for search ops)

### Wave 2: Briefing Unification + Write Security (medium risk, enables evolution)

4. **Extract BriefingService** — Unify context_briefing and CompactPayload assembly, gateway applies S3 (input bounds), S4 (quarantine), S5 (audit)
5. **Extract StoreService** — Move content scanning (S1 hard-reject) and write validation into service layer, gateway applies S2 (rate limiting on writes)
6. **Wire HookRequest::Briefing** — Enable UDS-native briefing through existing service

This enables the MCP→UDS briefing shift without new business logic. Rate limiting (F-09) on writes is introduced here — deferred from Wave 1 because only MCP currently does writes, and the existing capability check provides some protection.

**Security findings closed**: F-09 (rate limiting on writes), F-04 (maintain=true requires Admin via StatusService capability check)

### Wave 3: Module Reorganization + Authorization (low risk, pure restructuring)

7. **Create services/, mcp/, uds/, infra/ module groups** — Move files into logical groups. Split `response.rs` into `mcp/response/` sub-module with 4 files (entries.rs, mutations.rs, status.rs, briefing.rs). Apply Refactor #6 (generic `format_status_change` unifying deprecate/quarantine/restore) and merge `format_store_success` variants.
8. **Introduce unified capability model** — `SessionWrite` capability, UDS fixed to `{Read, Search, SessionWrite}`
9. **Extract ToolContext** — Reduce per-handler ceremony in MCP tools
10. **Split context_status into StatusService** — Break 628-line function into composable sub-services, enforce Admin for maintenance ops

**Security findings closed**: F-26 (UDS authorization model — UDS gets fixed capabilities), F-04 (maintain=true formally gated)

### Wave 4: Convergence (higher risk, strategic)

11. **Unified UsageService** — Merge MCP usage recording with UDS injection logging
12. **Session-aware MCP** — Add optional session_id to MCP search (prefix session IDs per transport to prevent cross-contamination: `mcp::{id}` vs `uds::{id}`)
13. **Rate limiting on search** — S2 gate for search operations (300/hour default per caller)
14. **Status report refactoring** — `#[derive(Serialize)]` on StatusReport, eliminating ~130-150 lines of manual `json!` macro assembly in `mcp/response/status.rs` (Refactor #9)
15. **UDS audit for auth failures** — Write to AUDIT_LOG, not just tracing::warn

**Security findings closed**: F-23 (UDS auth failures in audit log), rate limiting fully deployed

---

## Risks and Mitigations

### Structural Risks

| Risk | Mitigation |
|------|-----------|
| Service extraction changes behavior | Services must be exact behavioral clones initially; add new features only after extraction is proven |
| Module reorganization breaks imports | Do in a single PR with comprehensive `pub use` re-exports from the old paths; deprecate in next cycle |
| UDS-native briefing quality differs from MCP briefing | Test both paths against same knowledge base, compare output quality |
| Test coverage drops during restructuring | Move tests with their code; no test deletions allowed |
| Hook latency regressed by service indirection | Service layer is zero-cost (same function calls, different module); measure before/after |

### Security Risks

| Risk | Mitigation |
|------|-----------|
| **Service bypass** — transport calls foundation layer directly, skipping security gateway | Code review + module visibility: services/ is the only module that imports foundation crates. Transport modules import services/ only. Enforce via `pub(crate)` visibility on gateway-protected methods. |
| **AuditContext forgery** — transport provides false context to service layer | AuditContext is constructed by transport, but audit events are append-only with monotonic IDs. Inconsistencies (e.g., MCP source from UDS socket) are detectable in forensics. |
| **Rate limiter key collision** — different transports use same CallerId format | `CallerId` is an enum with per-transport variants, preventing collisions structurally. |
| **Search query scanning false positives** — legitimate queries about injection patterns get flagged | Scan queries in warn mode (log + audit, don't reject). Only writes get hard-rejected. |
| **UDS latency regression from security gates** — scanning and validation add overhead to hook path | Content scanning uses pre-compiled regex (`OnceLock` singleton, ~1μs per scan). Input validation is arithmetic checks. Rate limiting is a HashMap lookup. Total added overhead: <10μs per request, negligible vs 10ms embedding. |
| **Session ID cross-contamination** — MCP gains session_id (Wave 4), shares namespace with UDS sessions | Transport-prefixed session IDs: `mcp::{id}` vs `uds::{id}`. Enforced at transport layer before passing to services. |

---

## Relationship to Existing Findings

This analysis builds on and contextualizes the earlier optimization work:

| Earlier Finding | How This Analysis Addresses It |
|----------------|-------------------------------|
| Refactor #1 (index-writing duplication) | Wave 2, item 5: StoreService with `Store::insert_in_txn` |
| Refactor #2 (tool handler ceremony) | Wave 3, item 9: `ToolContext` extraction |
| Refactor #3 (context_status 628 lines) | Wave 3, item 10: StatusService decomposition |
| Refactor #4 (search/ranking duplication) | Wave 1, item 1: SearchService |
| Refactor #5 (confidence recompute 8x) | Wave 1, item 2: ConfidenceService |
| Refactor #6 (identical deprecate/quarantine/restore) | Wave 3, item 7: generic `format_status_change` in `mcp/response/mutations.rs` |
| Refactor #9 (StatusReport 409 lines of json! macros) | Wave 4, item 14: `#[derive(Serialize)]` on StatusReport |
| Summary #31 (split server crate) | **DECIDED**: in-crate `services/` module. Crate split not needed. |
| Summary #34 (evaluate removing direct redb) | StoreService absorbs direct redb access patterns |
| HP-01 (double sort) | Fixed inside SearchService (single sort with cached scores) |
| HP-02 (per-result fetch) | Fixed inside SearchService (batch fetch) |
| **F-04** (maintain=true auth) | Wave 3, item 10: StatusService enforces Admin capability |
| **F-09** (no rate limiting) | Wave 2: S2 rate limiting in Security Gateway |
| **F-21** (best-effort audit logging) | Wave 1: S5 structured audit in SearchService; Wave 2: in StoreService |
| **F-23** (UDS auth failures unaudited) | Wave 4, item 15: UDS auth failure audit |
| **F-25** (UDS no content scanning) | Wave 1: S1 content scanning in SearchService gateway |
| **F-26** (UDS no authorization) | Wave 3, item 8: unified capability model with `SessionWrite` |
| **F-27** (UDS no query validation) | Wave 1: S3 input bounds in SearchService gateway |
| **F-28** (UDS no audit trail) | Wave 1: S5 structured audit for all service operations |

---

## Decision Points for Human Review

1. ~~**Service layer location**: Keep in unimatrix-server as `services/` module, or extract to new `unimatrix-service` crate?~~
   - **DECIDED**: Keep in-crate as `services/` module. response.rs splits into `mcp/response/` sub-files (not a crate concern — it's transport-specific MCP formatting).

2. **Briefing UDS trigger**: Should UDS-native briefing trigger on `SessionRegister` (immediate, before any work) or `UserPromptSubmit` (per-query, like current context injection)?
   - *Recommendation*: `SessionRegister` for role-based briefing (conventions + duties). `UserPromptSubmit` continues to do semantic search injection. These are complementary, not competing.
   - **DEFERRED**: Resolve during Wave 2 scoping (briefing unification). Depends on what hook events exist and how session registration evolves.

3. ~~**Session-aware MCP**: Should MCP tools accept `session_id` to enable session-scoped features?~~
   - **DECIDED**: Yes, as optional parameter. Enables gradual convergence without breaking existing clients.

4. ~~**Module grouping depth**: Flat `services/search.rs` or nested `services/search/mod.rs` with sub-modules?~~
   - **DECIDED**: Flat files for services. Exception: `mcp/response/` uses sub-module (4 files) because the formatting code warrants it. Other modules stay flat unless complexity demands otherwise.

5. ~~**Search query scanning policy**: Hard-reject on injection match (like writes) or warn-and-proceed?~~
   - **DECIDED**: Warn-and-proceed for searches. Hard-reject on writes. Log matches for forensics.

6. ~~**UDS capability strictness**: Should UDS get `{Read, Search, SessionWrite}` (restrictive) or `{Read, Search, SessionWrite, Write}` (permissive)?~~
   - **DECIDED**: Restrictive — `{Read, Search, SessionWrite}`. Least privilege applies: hook callers do not need Write. Auto-outcome writes (currently `write_auto_outcome_entry` in UDS) move to a service-internal caller path. The internal caller concept is defined generically (not outcome-specific) to support future service-initiated writes.

7. ~~**Rate limiting scope**: Per-caller only, or also global server-wide limits?~~
   - **DECIDED**: Per-caller initially. Global limits deferred until HTTP transport is built.

8. **OperationalEvent scope**: Should Wave 1 include `OPERATIONAL_EVENT_LOG` table and `OperationalEvent` types, or defer?
   - **DEFERRED**: AuditContext with session_id/feature_cycle is in-scope for Wave 1. Full OperationalEvent log deferred to separate scoping.

9. ~~**Wave sequencing**: Ship Wave 1 as its own feature cycle before starting Wave 2, or scope together?~~
   - **DECIDED**: Wave 1 is its own feature — scoped, implemented, merged independently. Whether to release the package before starting Wave 2 is a separate decision that doesn't impact the development path.
