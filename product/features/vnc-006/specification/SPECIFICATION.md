# Specification: vnc-006 — Service Layer + Security Gateway

## Objective

Extract a transport-agnostic service layer within unimatrix-server that unifies ~760 lines of duplicated business logic between MCP and UDS paths, implements a Security Gateway (S1/S3/S4/S5) as service-level invariants closing critical UDS security gaps (F-25/F-27/F-28), and introduces AuditContext for structured audit with session_id and feature_cycle for retrospective compatibility. Zero functional changes to either transport's happy path.

## Functional Requirements

### FR-01: SearchService

- FR-01.1: `SearchService::search()` accepts `ServiceSearchParams` and `AuditContext`, returns `SearchResults`
- FR-01.2: The search pipeline executes: embed query, adapt (MicroLoRA), normalize, HNSW search, batch fetch, quarantine filter, re-rank (0.85*sim + 0.15*conf), provenance boost, co-access boost, feature boost, floor filters, sort, truncate to k
- FR-01.3: When `filters` is `Some(QueryFilter)`, HNSW search uses `search_filtered`; when `None`, uses unfiltered `search`
- FR-01.4: When `similarity_floor` is `Some(f)`, entries with similarity < f are excluded from results
- FR-01.5: When `confidence_floor` is `Some(f)`, entries with confidence < f are excluded from results
- FR-01.6: `SearchResults` includes `query_embedding: Vec<f32>` (pub(crate)) for caller reuse
- FR-01.7: Provenance boost of +0.02 applied when `entry.created_by == caller_agent_id`
- FR-01.8: Co-access boost computed via existing `compute_search_boost()` in a `spawn_blocking` call
- FR-01.9: Quarantined entries (status = Quarantined) never appear in results (S4 invariant)

### FR-02: StoreService

- FR-02.1: `StoreService::insert()` accepts `NewEntry`, optional pre-computed embedding, and `AuditContext`; returns `InsertResult`
- FR-02.2: Insert and audit are written in a single atomic transaction via `Store::insert_in_txn`
- FR-02.3: Near-duplicate detection: if cosine similarity >= 0.92 with an existing entry, return `InsertResult { duplicate_of: Some(id) }` without inserting
- FR-02.4: `StoreService::correct()` accepts original_id, corrected `NewEntry`, optional reason, and `AuditContext`; returns `CorrectResult`
- FR-02.5: Correct atomically deprecates the original and inserts the correction with supersedes/superseded_by links
- FR-02.6: Embedding is computed via `embed_service` + `adapt_service` if not pre-computed

### FR-03: ConfidenceService

- FR-03.1: `ConfidenceService::recompute()` accepts a slice of entry IDs
- FR-03.2: Recomputation happens in a single `spawn_blocking` call (fire-and-forget)
- FR-03.3: Per-entry failures are logged at warn level and skipped (not propagated)
- FR-03.4: Empty entry_ids slice is a no-op (no spawn_blocking call)

### FR-04: SecurityGateway

- FR-04.1: `validate_search_query()` validates query length (max 10,000 chars), rejects control characters (except \n, \t), validates k range (1-100)
- FR-04.2: `validate_search_query()` scans query for injection patterns using `ContentScanner::global()` and returns `ScanWarning` if detected (does not reject)
- FR-04.3: `validate_write()` validates title length, content length, category against allowlist, tags, and control characters
- FR-04.4: `validate_write()` scans title and content for injection and PII patterns; hard-rejects on match (returns `ServiceError::ContentRejected`)
- FR-04.5: `validate_write()` skips S1 content scan when `audit_ctx.source` is `AuditSource::Internal`
- FR-04.6: `is_quarantined()` returns true for entries with status `Quarantined`
- FR-04.7: `emit_audit()` writes audit event via `AuditLog` (fire-and-forget, never blocks caller)
- FR-04.8: `new_permissive()` (cfg(test) only) creates a gateway suitable for unit tests

### FR-05: AuditContext

- FR-05.1: `AuditContext` contains `source: AuditSource`, `caller_id: String`, `session_id: Option<String>`, `feature_cycle: Option<String>`
- FR-05.2: `AuditSource::Mcp` contains `agent_id: String`, `trust_level: TrustLevel`
- FR-05.3: `AuditSource::Uds` contains `uid: u32`, `pid: Option<u32>`, `session_id: String`
- FR-05.4: `AuditSource::Internal` contains `service: String`; visibility is `pub(crate)`
- FR-05.5: MCP transport constructs `AuditContext` from resolved identity
- FR-05.6: UDS transport constructs `AuditContext` from peer credentials and hook session_id

### FR-06: Store::insert_in_txn

- FR-06.1: `Store::insert_in_txn()` accepts external `&WriteTransaction`, `NewEntry`, and timestamp; returns `Result<EntryRecord>`
- FR-06.2: Performs all writes (ENTRIES, indexes, counters) within the provided transaction
- FR-06.3: Does not commit the transaction (caller commits)
- FR-06.4: Existing `Store::insert()` is preserved unchanged

### FR-07: Transport Rewiring

- FR-07.1: `tools.rs::context_search` delegates search to `SearchService::search()`
- FR-07.2: `tools.rs::context_store` delegates insert to `StoreService::insert()`
- FR-07.3: `tools.rs::context_correct` delegates correction to `StoreService::correct()`
- FR-07.4: `uds_listener.rs::handle_context_search` delegates search to `SearchService::search()`
- FR-07.5: All confidence recompute blocks in tools.rs (5) and uds_listener.rs (3) are replaced with `ConfidenceService::recompute()`
- FR-07.6: MCP transport retains identity resolution, capability checks, response formatting, usage recording
- FR-07.7: UDS transport retains injection logging, session tracking, co-access dedup, fire-and-forget decisions

### FR-08: ServiceLayer

- FR-08.1: `ServiceLayer` struct aggregates SearchService, StoreService, ConfidenceService
- FR-08.2: `ServiceLayer::new()` constructs all services with shared dependencies
- FR-08.3: `UnimatrixServer` gains a `services: ServiceLayer` field

### FR-09: ServiceError

- FR-09.1: `ServiceError` enum covers ContentRejected, ValidationFailed, Core, EmbeddingFailed, NotFound
- FR-09.2: MCP transport converts `ServiceError` to rmcp `ErrorData`
- FR-09.3: UDS transport converts `ServiceError` to `HookResponse::Error`

## Non-Functional Requirements

- NFR-01: **Like-for-like behavior** — Both transports produce identical output for the same inputs before and after refactoring. Verified by snapshot/comparison tests.
- NFR-02: **Latency** — Service layer adds < 10 microseconds overhead per request (function call indirection + gateway validation). Embedding and HNSW search dominate at ~10ms. ContentScanner uses OnceLock singleton (~1 microsecond per scan).
- NFR-03: **Test count** — No net reduction in test count. Target: >= 680 server tests after refactoring.
- NFR-04: **Module size** — No new file exceeds 300 lines (estimated: gateway.rs ~200, search.rs ~250, store_ops.rs ~200, confidence.rs ~40, mod.rs ~80).
- NFR-05: **Fire-and-forget** — Audit writes (S5) and confidence recomputation use `spawn_blocking` and never block the calling transport handler.
- NFR-06: **Memory** — No new persistent allocations beyond the `ServiceLayer` struct (which holds `Arc` references to existing components).

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `SearchService::search()` produces identical results to both existing paths | Comparison test: old path vs new path with same inputs |
| AC-02 | MCP tools.rs calls `SearchService::search()` | Code inspection: no inline search/rank/boost in tools.rs context_search |
| AC-03 | UDS uds_listener.rs calls `SearchService::search()` | Code inspection: no inline search/rank/boost in handle_context_search |
| AC-04 | `ConfidenceService::recompute()` replaces all 8 inline blocks | Code inspection: zero `compute_confidence` calls outside ConfidenceService |
| AC-05 | `StoreService::insert/correct()` with atomic audit via `insert_in_txn` | Integration test: verify entry + audit in same transaction |
| AC-06 | S1 scans search queries, logs warning without rejecting | Unit test: injection pattern in query produces ScanWarning, search succeeds |
| AC-07 | S1 hard-rejects writes with injection/PII patterns | Unit test: injection in content returns ContentRejected error |
| AC-08 | S3 validates all service method parameters | Unit test: oversized query, invalid k, control chars all rejected |
| AC-09 | S4 quarantine exclusion in SearchService | Integration test: quarantined entry never in results |
| AC-10 | S5 audit records emitted with AuditContext | Integration test: search/store produce audit events with session_id |
| AC-11 | All service methods accept AuditContext | Code inspection: every pub(crate) service method has AuditContext param |
| AC-12 | AuditSource::Internal exists with pub(crate) visibility | Code inspection: Internal variant, pub(crate) on enum/variant |
| AC-13 | MCP produces identical responses | Before/after comparison test for context_search, context_store, context_correct |
| AC-14 | UDS produces identical responses | Before/after comparison test for handle_context_search |
| AC-15 | No net reduction in test count | CI: >= 680 server tests pass |
| AC-16 | No new crates | Code inspection: Cargo.toml workspace members unchanged |
| AC-17 | No functional changes to happy path | All existing integration tests pass without modification |

## Domain Models

### Core Entities

| Entity | Description | Module |
|--------|-------------|--------|
| `ServiceLayer` | Aggregate providing access to all services | `services/mod.rs` |
| `SecurityGateway` | Enforces S1/S3/S4/S5 invariants | `services/gateway.rs` |
| `SearchService` | Unified search pipeline | `services/search.rs` |
| `StoreService` | Unified write operations | `services/store_ops.rs` |
| `ConfidenceService` | Batched confidence recompute | `services/confidence.rs` |
| `AuditContext` | Transport-provided audit metadata | `services/mod.rs` |
| `AuditSource` | Caller origin (Mcp, Uds, Internal) | `services/mod.rs` |
| `ServiceError` | Service-level error type | `services/mod.rs` |
| `ServiceSearchParams` | Transport-agnostic search input | `services/search.rs` |
| `SearchResults` | Search output with embedding | `services/search.rs` |
| `ScoredEntry` | Entry with composite score breakdown | `services/search.rs` |
| `InsertResult` | Insert output with duplicate detection | `services/store_ops.rs` |
| `CorrectResult` | Correct output with both entries | `services/store_ops.rs` |
| `ScanWarning` | Non-fatal injection pattern detection | `services/gateway.rs` |

### Relationships

```
UnimatrixServer
  └── services: ServiceLayer
        ├── search: SearchService
        │     └── gateway: Arc<SecurityGateway>
        ├── store_ops: StoreService
        │     └── gateway: Arc<SecurityGateway>
        └── confidence: ConfidenceService

SecurityGateway
  └── audit: Arc<AuditLog>

AuditContext
  └── source: AuditSource { Mcp | Uds | Internal }
```

## User Workflows

### Agent Searches Knowledge (MCP)

1. Agent calls `context_search` MCP tool with query, optional filters
2. tools.rs resolves identity, checks Search capability
3. tools.rs constructs AuditContext::Mcp, converts params to ServiceSearchParams
4. SearchService validates query (S3), scans for injection (S1 warn), executes pipeline, filters quarantined (S4), emits audit (S5)
5. tools.rs records usage, formats response, returns CallToolResult

### Hook Injects Context (UDS)

1. Claude Code hook fires UserPromptSubmit, sends ContextSearch via UDS
2. uds_listener.rs authenticates peer (UID), constructs AuditContext::Uds
3. uds_listener.rs converts params to ServiceSearchParams (with similarity_floor=0.5, confidence_floor=0.3)
4. SearchService validates, scans, executes pipeline, emits audit
5. uds_listener.rs records injection log, co-access pairs, formats HookResponse

### Agent Stores Knowledge (MCP)

1. Agent calls `context_store` with entry data
2. tools.rs resolves identity, checks Write capability
3. tools.rs constructs AuditContext::Mcp, builds NewEntry
4. StoreService validates (S3), scans content (S1 reject), inserts atomically with audit (S5)
5. tools.rs calls ConfidenceService::recompute for new entry
6. tools.rs records usage, formats response

### System Writes Auto-Outcome (Internal)

1. UDS session ends with outcome signal
2. uds_listener.rs constructs AuditContext::Internal { service: "auto-outcome" }
3. StoreService validates (S3, no S1 scan for Internal), inserts with audit (S5)
4. ConfidenceService::recompute for new entry

## Constraints

1. All changes within `crates/unimatrix-server/` — no new crates (AC-16)
2. `Store::insert_in_txn` is the only addition to `crates/unimatrix-store/` — `pub(crate)` visibility
3. redb synchronous API — `insert_in_txn` works within existing `WriteTransaction` model
4. rmcp 0.16.0 — MCP tool handler signatures unchanged
5. Existing `TestHarness` and `tempdir` test fixtures — extend, do not replace
6. `ContentScanner` OnceLock singleton reused — no new scanner instances
7. Fire-and-forget pattern preserved for audit writes and confidence recomputation
8. No schema version bump — AuditContext uses existing AUDIT_LOG table
9. Wave independence — no forward dependencies on vnc-007/008/009

## Dependencies

| Dependency | Type | Impact |
|------------|------|--------|
| unimatrix-store | Internal crate | Add `insert_in_txn` method |
| unimatrix-core | Internal crate | Unchanged — `compute_confidence`, `EntryRecord`, `NewEntry`, `QueryFilter` |
| unimatrix-vector | Internal crate | Unchanged — `VectorIndex` used by StoreService |
| unimatrix-embed | Internal crate | Unchanged — `EmbedServiceHandle` used by services |
| unimatrix-adapt | Internal crate | Unchanged — `AdaptationService` used by services |
| redb | External (v3.1.x) | `WriteTransaction` type used in `insert_in_txn` |
| rmcp | External (0.16.0) | MCP tool handler signatures unchanged |
| tokio | External | `spawn_blocking` for fire-and-forget operations |

## NOT in Scope

- BriefingService extraction (vnc-007)
- Module reorganization into mcp/, uds/, infra/ groups (vnc-008)
- Rate limiting / S2 enforcement (vnc-009)
- Unified capability model / SessionWrite (vnc-008)
- StatusService extraction (vnc-008)
- OperationalEvent log / OPERATIONAL_EVENT_LOG table
- response.rs decomposition
- HTTP transport
- Database replacement / storage abstraction beyond insert_in_txn
- Changes to coaccess.rs module (consumed as-is by SearchService)
