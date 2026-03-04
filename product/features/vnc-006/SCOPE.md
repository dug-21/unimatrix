# vnc-006: Service Layer + Security Gateway

## Problem Statement

The unimatrix-server crate (19,672 lines across 22 modules) has two parallel request paths — MCP (stdio, 3,061 lines in tools.rs) and UDS (Unix domain socket, 2,271 lines in uds_listener.rs) — that duplicate core business logic with subtle divergences. This creates four compounding problems:

1. **Duplicated pipelines**: Search/ranking logic (embed, HNSW search, re-rank with 0.85*similarity + 0.15*confidence, provenance boost, co-access boost, quarantine exclusion) exists in both `tools.rs` and `uds_listener.rs` (~400 lines duplicated). Changes must be made in two places or divergence grows.

2. **Divergent capabilities**: MCP path has metadata filtering, co-access boost, feature boost. UDS path has injection tracking, confidence/similarity floors, session-scoped co-access dedup. Neither path has all features — a superset pipeline does not exist.

3. **Critical security asymmetry**: MCP path has content scanning, capability checks, input validation, and audit logging. UDS path has **none of these**:
   - **F-25**: Zero content scanning — raw user prompts flow directly to embedding unexamined
   - **F-27**: Zero query validation — only session_id is sanitized
   - **F-28**: Zero audit trail — UDS operations are invisible to AUDIT_LOG
   - **F-26**: Zero authorization — any UID-authenticated connection gets full access

4. **Confidence recomputation duplication**: 8 nearly identical fire-and-forget `spawn_blocking` blocks (~160 lines total) scattered across tools.rs and server.rs for confidence recalculation after mutations.

The planned shift to UDS-native briefing delivery and future HTTP transport will exacerbate all four problems unless the backend is unified first. Any new transport added today would inherit the same security gaps as UDS.

## Goals

1. Extract a transport-agnostic service layer (`services/` module) with `SearchService` and `ConfidenceService` that both MCP and UDS call
2. Implement a Security Gateway (S1, S3, S4, S5) at the service level that cannot be bypassed by any transport
3. Introduce `AuditContext` with `session_id` and `feature_cycle` on all service methods for structured audit and retrospective compatibility
4. Define `StoreService` with `Store::insert_in_txn` for atomic write+audit transactions
5. Eliminate ~760 lines of duplicated logic (400 search + 160 confidence + 200 write)
6. Close critical UDS security findings F-25, F-27, F-28
7. Achieve like-for-like behavior — zero functional changes to either transport's happy path
8. Maintain or increase existing test coverage (~680 server tests)

## Non-Goals

- **BriefingService extraction** — deferred to vnc-007 (Wave 2). The briefing has behavioral differences between MCP and UDS that require separate validation.
- **Module reorganization** — deferred to vnc-008 (Wave 3). vnc-006 creates `services/` but does not reorganize existing modules into `mcp/`, `uds/`, `infra/` groups.
- **Unified capability model / SessionWrite** — deferred to vnc-008 (Wave 3). UDS gets fixed capabilities only after the service layer proves stable.
- **Rate limiting (S2)** — deferred. Only MCP currently does writes (protected by capability checks). Rate limiting on search is Wave 4 (vnc-009). The S2 gate interface is defined but not enforced.
- **response.rs decomposition** — deferred to vnc-008.
- **OperationalEvent log / new tables** — deferred. AuditContext is in scope; the OPERATIONAL_EVENT_LOG table is not.
- **Database replacement** — the service layer must not prevent future storage changes, but no abstraction beyond `Store::insert_in_txn` is added.
- **HTTP transport** — future work enabled by the service layer.
- **StatusService extraction** — deferred to vnc-008 (context_status is 628 lines but only has one caller path).
- **Code changes outside `crates/unimatrix-server/`** — the service layer lives inside the server crate. No new crates.

## Background Research

### Existing Research (Completed)

Extensive research has been completed in `product/research/optimizations/`:

- **server-refactoring-architecture.md** — Full 4-wave refactoring plan with service definitions, module reorganization, capability model evolution, and retrospective compatibility analysis. Includes all decision points resolved.
- **security-surface-analysis.md** — Dual-path threat model with 8 MCP risks (M-01 through M-08), 9 UDS risks (U-01 through U-09), 5 cross-path risks (X-01 through X-05), and security gate implementation plan.
- **refactoring-analysis.md** — Line-by-line analysis of duplication patterns across tools.rs, uds_listener.rs, server.rs.
- **architecture-dependencies.md** — Dependency graph between modules, import analysis, circular dependency risks.

### Key Decisions Already Made

| Decision | Resolution | Source |
|----------|------------|--------|
| Service layer location | In-crate `services/` module, not a new crate | server-refactoring-architecture.md |
| Search query scanning | Warn-and-proceed (log + audit), not hard-reject | security-surface-analysis.md |
| UDS capability set | `{Read, Search, SessionWrite}` (restrictive) | server-refactoring-architecture.md |
| Wave sequencing | Each wave ships independently | server-refactoring-architecture.md |
| Module grouping style | Flat files for services | server-refactoring-architecture.md |
| Rate limiting scope | Per-caller initially, global deferred | server-refactoring-architecture.md |
| OperationalEvent scope | Deferred — AuditContext in scope | server-refactoring-architecture.md |

### Current Codebase State

- **tools.rs** (3,061 lines): MCP tool handlers — identity resolution, capability checks, validation, business logic, response formatting, audit, usage recording all interleaved per handler
- **uds_listener.rs** (2,271 lines): UDS handlers — peer auth, dispatch, search/rank (duplicated), session tracking, injection logging, fire-and-forget
- **server.rs** (2,105 lines): Shared backend — `UnimatrixBackend` struct with store/vector/embed handles, write operations, confidence computation, embedding helpers
- **response.rs** (2,550 lines): MCP response formatting (all three format modes)
- **validation.rs** (1,209 lines): Input validation (MCP-only currently)
- **scanning.rs** (423 lines): Content scanning (MCP-only currently)
- **audit.rs** (599 lines): Audit logging (MCP-only currently)
- **~680 tests** across the server crate

### Search Pipeline Duplication Analysis

The search pipeline in both paths follows the same flow with divergences:

```
Embed query → HNSW search → Fetch entries → Re-rank → Boost → Filter → Return
```

| Step | MCP (tools.rs) | UDS (uds_listener.rs) | Unified |
|------|---------------|----------------------|---------|
| Embedding | embed_handle.embed() | embed_handle.embed() | SearchService |
| HNSW search | search_filtered (metadata) | search (unfiltered) | SearchService (optional filters) |
| Entry fetch | Per-result from store | Per-result from store | SearchService (batch fetch) |
| Re-ranking | 0.85*sim + 0.15*conf | 0.85*sim + 0.15*conf | SearchService |
| Provenance boost | +0.02 for self-authored | +0.02 for self-authored | SearchService |
| Co-access boost | From co-access table | From co-access table | SearchService |
| Feature boost | From tag match | From tag match | SearchService |
| Similarity floor | None | 0.5 | SearchService (optional param) |
| Confidence floor | None | 0.3 | SearchService (optional param) |
| Quarantine check | Per-entry | Per-entry | SearchService (invariant) |

## Proposed Approach

### 1. Create `services/` Module

Add `crates/unimatrix-server/src/services/` with:
- `mod.rs` — Re-exports, `ServiceLayer` struct holding all services
- `gateway.rs` — Security Gateway (S1, S3, S4, S5 gates)
- `search.rs` — `SearchService` with unified search pipeline
- `store_ops.rs` — `StoreService` with `insert_in_txn` for atomic writes
- `confidence.rs` — `ConfidenceService` for fire-and-forget recomputation

### 2. SearchService

Single unified search pipeline callable from both transports.

```
SearchService::search(params: SearchParams, audit: AuditContext) -> Result<SearchResults>
```

Where `SearchParams` includes optional fields for capabilities that currently differ between paths (filters, similarity_floor, confidence_floor, feature_tag, co_access_anchors). Both transports call the same pipeline; transport-specific parameters become optional `SearchParams` fields.

Security gates applied internally:
- S1: Scan query for injection patterns (warn + audit, don't reject)
- S3: Validate query length (max 10,000 chars), reject control chars, validate k range (1-100)
- S4: Quarantined entries excluded from results (invariant)
- S5: Audit record emitted with search params and result count

### 3. ConfidenceService

Single fire-and-forget confidence recomputation replacing 8 duplicated blocks.

```
ConfidenceService::recompute(entry_ids: &[u64])
```

Batched, fire-and-forget via `spawn_blocking`. Called after any mutation (store, correct, deprecate, helpful/unhelpful vote).

### 4. StoreService

Unified write operations with audit-in-transaction.

```
StoreService::insert(entry: NewEntry, audit: AuditContext) -> Result<EntryRecord>
StoreService::correct(original_id: u64, corrected: NewEntry, audit: AuditContext) -> Result<EntryRecord>
```

Requires exposing `Store::insert_in_txn(&WriteTransaction, NewEntry)` for atomic write+audit commits.

Security gates applied internally:
- S1: Content scan on title + content (hard-reject on match)
- S3: Full input validation (title length, content length, category, tags, control chars)
- S5: Audit record written atomically in same transaction as entry

### 5. Security Gateway

Service-level security enforcement that cannot be bypassed by any transport.

```
SecurityGateway {
    scanner: ContentScanner,     // S1
    // rate_limiter: RateLimiter, // S2 — interface defined, not enforced in vnc-006
    validator: InputValidator,    // S3
    // quarantine: invariant      // S4 — built into SearchService
    auditor: AuditService,       // S5
}
```

### 6. AuditContext

Transport-provided context for structured audit records.

```rust
struct AuditContext {
    source: AuditSource,
    caller_id: String,
    session_id: Option<String>,
    feature_cycle: Option<String>,
}

enum AuditSource {
    Mcp { agent_id: String, trust_level: TrustLevel },
    Uds { uid: u32, pid: Option<u32>, session_id: String },
    Internal { service: String },  // For service-initiated writes (auto-outcome)
}
```

### 7. Transport Rewiring

- **MCP tools.rs**: Replace inline search/rank/boost with `SearchService::search()`. Replace inline confidence blocks with `ConfidenceService::recompute()`. Replace inline write logic with `StoreService::insert/correct()`. Keep identity resolution, capability checks, response formatting, usage recording.
- **UDS uds_listener.rs**: Replace inline search/rank/boost with `SearchService::search()`. Keep injection logging, session tracking, co-access dedup, fire-and-forget decisions.
- **server.rs**: Remove duplicated search/confidence logic. Retain `UnimatrixBackend` as the provider of store/vector/embed handles that services consume. Expose `Store::insert_in_txn`.

### 8. Internal Caller Concept

For service-initiated writes (e.g., auto-outcome entries currently written by UDS), define an `AuditSource::Internal` variant. This allows the service layer to perform writes on behalf of the system without requiring external caller credentials. The concept is generic — not specific to outcomes.

## Acceptance Criteria

- AC-01: `SearchService::search()` exists and produces identical results to both the MCP and UDS search paths for the same query and parameters
- AC-02: MCP tools.rs calls `SearchService::search()` instead of inline search/rank/boost logic
- AC-03: UDS uds_listener.rs calls `SearchService::search()` instead of inline search/rank/boost logic
- AC-04: `ConfidenceService::recompute()` exists and replaces all 8 inline fire-and-forget confidence blocks
- AC-05: `StoreService::insert()` and `StoreService::correct()` exist with atomic write+audit transactions via `Store::insert_in_txn`
- AC-06: Security Gateway S1 (content scan) scans search queries for injection patterns and logs warnings without rejecting
- AC-07: Security Gateway S1 (content scan) hard-rejects store/correct operations that match injection/PII patterns
- AC-08: Security Gateway S3 (input validation) validates all service method parameters (query length, k range, control chars, title/content length)
- AC-09: Security Gateway S4 (quarantine exclusion) is enforced as an invariant in `SearchService` — quarantined entries never appear in results
- AC-10: Security Gateway S5 (structured audit) emits audit records for all service operations with `AuditContext` including `session_id` and `feature_cycle`
- AC-11: All service methods accept `AuditContext` as a parameter
- AC-12: `AuditSource::Internal` variant exists for service-initiated writes
- AC-13: Like-for-like behavior: MCP path produces identical responses before and after refactoring for the same inputs
- AC-14: Like-for-like behavior: UDS path produces identical responses before and after refactoring for the same inputs
- AC-15: No net reduction in test count (~680 server tests maintained or increased)
- AC-16: No new crates added — all changes within `crates/unimatrix-server/`
- AC-17: No functional changes to either transport's happy path (security gates are additive defense-in-depth on UDS, existing behavior on MCP)

## Constraints

1. **Rust workspace structure**: Changes confined to `crates/unimatrix-server/`. No new crates.
2. **redb synchronous API**: `Store` uses redb's synchronous transaction API. `insert_in_txn` must work within the existing `WriteTransaction` model.
3. **rmcp 0.16.0**: MCP tool handler signatures are constrained by the rmcp framework.
4. **Existing test infrastructure**: Tests use `TestHarness` and `tempdir`-based fixtures. Extend, do not replace.
5. **Fire-and-forget pattern**: Confidence recomputation and some audit writes use `spawn_blocking` for non-blocking operation. This pattern must be preserved.
6. **ContentScanner singleton**: `ContentScanner` uses `OnceLock` for compiled regex (~1 microsecond per scan). Service layer reuses the existing singleton.
7. **No behavioral changes**: Both transports must produce byte-identical output for the same inputs after refactoring. Security gates on UDS are purely additive.
8. **Schema version**: No schema version bump unless `Store::insert_in_txn` requires table changes. AuditContext uses existing AUDIT_LOG table structure.
9. **Wave independence**: vnc-006 must ship and merge without requiring vnc-007/008/009. No forward dependencies.
10. **Module visibility**: Services can access `Store`, `VectorIndex`, `EmbedHandle` directly. In vnc-006, `pub(crate)` visibility enforcement for gateway bypass prevention is a documentation requirement, not a compiler-enforced constraint (compiler enforcement comes in vnc-008 with module reorganization).

## Resolved Questions

1. **SearchResults embedding vector**: Include `query_embedding: Vec<f32>` in `SearchResults` with `pub(crate)` scope. Query vectors stay f32 at the service tier regardless of future storage/index quantization concerns.

2. **Gateway pattern**: Hybrid approach. `SecurityGateway` as a struct injected into services. Services call gateway methods internally (e.g., `self.gateway.validate_search()`). Gets testability of a struct without boilerplate of full decoration. vnc-008 can enforce gateway references at module boundary later.

3. **Confidence batching**: Batched. `ConfidenceService::recompute(entry_ids: &[u64])` — single `spawn_blocking`, single read txn + write txn. Skip-and-log per entry on failure within the batch. Aligns with crt-005's batch refresh pattern.

4. **Internal caller writes**: `AuditSource`-driven behavior. `AuditSource::Internal` skips S1 content scan (system-generated content), still applies S3 validation + S5 audit. Single `StoreService::insert()` method, behavior varies by caller identity. `pub(crate)` visibility prevents external callers from claiming Internal.

## Tracking

https://github.com/dug-21/unimatrix/issues/82
