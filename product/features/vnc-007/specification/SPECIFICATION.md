# Specification: vnc-007 — Briefing Unification

## Objective

Extract a transport-agnostic BriefingService that unifies the MCP `context_briefing` tool and UDS `handle_compact_payload` handler behind one assembly service. Remove the dead duties section from briefing output. Wire UDS-native briefing delivery via `HookRequest::Briefing`. Gate the MCP tool behind a Cargo feature flag for future removal.

## Functional Requirements

### FR-01: BriefingService Assembly

BriefingService provides a single `assemble()` method that accepts `BriefingParams` and `AuditContext`, returning `BriefingResult`. The method's behavior is entirely determined by the caller's params:

- When `include_conventions=true` and `role` is provided: query conventions by role/topic
- When `include_semantic=true` and `task` is provided: delegate to SearchService for embedding + HNSW search + boosts
- When `injection_history` is provided: fetch entries by ID, deduplicate, partition by category
- When `include_semantic=false`: NO embedding, NO vector search, NO SearchService involvement

### FR-02: Convention Lookup

When `include_conventions=true`, BriefingService queries `AsyncEntryStore` with `topic=role, category="convention", status=Active`. Results are sorted with feature-tagged entries first (if `feature` param provided), then by confidence descending.

### FR-03: Semantic Search

When `include_semantic=true` and `task` is Some, BriefingService calls `SearchService::search()` with `k=3`, the task as query, and the feature tag for feature boost. Co-access boost is always applied (anchors derived from already-collected entry IDs). If SearchService returns EmbedNotReady, `search_available` is set to false and assembly continues with other sources.

### FR-04: Injection History Processing

When `injection_history` is provided, BriefingService:
1. Fetches each entry by ID from AsyncEntryStore
2. Excludes quarantined entries
3. Deduplicates by entry_id (keeps highest confidence)
4. Partitions into three groups: decisions (`category="decision"`), conventions (`category="convention"`), other (injections)
5. Sorts each group by confidence descending

### FR-05: Token Budget Allocation

BriefingService accepts `max_tokens`. Budget allocation strategy depends on entry sources:

**Injection history active**: Fixed proportional allocation:
| Section | Proportion |
|---------|-----------|
| Context header | 5% |
| Decisions | 40% |
| Injections | 30% |
| Conventions | 20% |
| Buffer | 5% |

Unused budget within a section does NOT cascade to later sections.

**No injection history**: Linear fill in priority order: conventions first, then relevant_context. Token estimation per entry: `(title.len() + content.len() + 50) / 4`. Stop adding entries when remaining budget is exhausted.

### FR-06: Duties Removal

All duties-related code is removed:
- `duties` field removed from `Briefing` struct in response.rs
- Duties lookup removed from `context_briefing` handler
- Duties budget allocation removed
- Duties sections removed from `format_briefing` (summary, markdown, JSON)
- BriefingParams has no duties concept
- BriefingResult has no duties concept

### FR-07: MCP context_briefing Rewiring

The `context_briefing` tool handler delegates to `BriefingService::assemble()`. It retains transport-specific concerns: identity resolution, capability check (Read), MCP param validation (format, helpful), response formatting, usage recording. It constructs `BriefingParams` with `include_conventions=true, include_semantic=true, injection_history=None, max_tokens=params.max_tokens`.

### FR-08: UDS CompactPayload Rewiring

`handle_compact_payload` delegates to `BriefingService::assemble()`. It retains transport-specific concerns: session state resolution from SessionRegistry, byte-to-token conversion, compaction count increment, BriefingContent response formatting. It constructs `BriefingParams` with `include_semantic=false` and `injection_history=Some(...)` (primary path) or `injection_history=None` with `include_conventions=true` (fallback path).

### FR-09: UDS HookRequest::Briefing Handler

`dispatch_request` handles `HookRequest::Briefing` by delegating to `BriefingService::assemble()` with `include_conventions=true, include_semantic=true, injection_history=None`. Returns `HookResponse::BriefingContent` with the assembled text.

### FR-10: Feature Flag

A Cargo feature `mcp-briefing` (default on) gates the `context_briefing` tool handler. When compiled without the feature, the tool is absent from the MCP tool list. BriefingService is always available regardless of feature flag.

## Non-Functional Requirements

### NFR-01: CompactPayload Latency

The `include_semantic=false` code path through BriefingService must not introduce measurable latency regression vs the current inline `handle_compact_payload`. Target: no more than 1ms additional overhead per call (current inline path is dominated by entry fetch I/O, not computation).

### NFR-02: Behavioral Equivalence

For identical inputs (same entries in knowledge base, same session state):
- MCP `context_briefing` produces equivalent conventions and relevant_context output (minus duties)
- UDS CompactPayload produces equivalent section content with the same entries in the same priority order

"Equivalent" means the same entries are selected with the same ordering. Exact string formatting may differ if the formatting moves between components.

### NFR-03: Test Coverage

No net reduction in test count from vnc-006 baseline. BriefingService must have unit tests for each entry source (conventions, semantic, injection history), budget allocation, edge cases (empty results, budget overflow, quarantine exclusion), and mixed sources.

### NFR-04: Feature Flag Compilation

Both `cargo build` (default features) and `cargo build --no-default-features` must compile successfully and pass their respective test suites.

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | BriefingService struct exists in `services/briefing.rs` with `assemble()` accepting BriefingParams and AuditContext | Code inspection |
| AC-02 | BriefingService supports convention lookup by role/topic when `include_conventions=true` | Unit test: query with role, verify convention entries returned |
| AC-03 | BriefingService performs semantic search when `include_semantic=true`; NO embedding/vector search when `include_semantic=false` | Unit test: mock SearchService, verify it is called only when include_semantic=true |
| AC-04 | BriefingService supports injection history as entry source | Unit test: provide injection entries, verify partitioned output |
| AC-05 | BriefingService applies token budget allocation respecting `max_tokens` | Unit test: set small budget, verify entries are truncated; test proportional allocation with injection history |
| AC-06 | Quarantined entries excluded from all assembled results | Unit test: include quarantined entry in injection history and conventions, verify excluded |
| AC-07 | Input validation via SecurityGateway S3 (role length, task length, max_tokens range) | Unit test: oversized role/task rejected; max_tokens out of range rejected |
| AC-08 | BriefingService registered in ServiceLayer accessible from both transports | Code inspection: ServiceLayer has `briefing` field |
| AC-09 | Briefing struct in response.rs has no `duties` field | Code inspection + compile check |
| AC-10 | context_briefing handler performs no duties lookup | Code inspection: no query with category "duties" |
| AC-11 | format_briefing has no duties section in any format | Unit test: verify summary/markdown/json output contain no "duties" or "Duties" |
| AC-12 | BriefingService has no duties concept in params or results | Code inspection |
| AC-13 | context_briefing delegates to BriefingService::assemble() | Code inspection + integration test |
| AC-14 | context_briefing produces equivalent output (minus duties) for same inputs | Snapshot test: compare pre/post refactoring output for identical knowledge base |
| AC-15 | context_briefing retains transport-specific concerns (identity, capability, format, usage) | Code inspection |
| AC-16 | context_briefing gated behind `#[cfg(feature = "mcp-briefing")]` | Compile test: build without feature, verify tool not registered |
| AC-17 | mcp-briefing feature defined in Cargo.toml with default on | Code inspection |
| AC-18 | handle_compact_payload delegates to BriefingService::assemble() | Code inspection + integration test |
| AC-19 | CompactPayload produces equivalent output for same session state | Snapshot test: compare pre/post refactoring output for identical session + entries |
| AC-20 | Session state resolved from SessionRegistry before calling BriefingService | Code inspection |
| AC-21 | Compaction count incremented after assembly | Unit test: verify count increases |
| AC-22 | dispatch_request handles HookRequest::Briefing via BriefingService | Unit test: send Briefing request, verify BriefingContent response |
| AC-23 | HookRequest::Briefing returns BriefingContent with conventions + semantic search | Integration test: populate knowledge base, send Briefing request, verify content |
| AC-24 | HookRequest::Briefing no longer returns ERR_UNKNOWN_REQUEST | Unit test: send Briefing request, verify not Error response |
| AC-25 | --no-default-features build has no context_briefing tool | Compile test + runtime verification |
| AC-26 | Default build has functional context_briefing tool | Integration test |
| AC-27 | BriefingService always available regardless of feature flag | Compile test: --no-default-features build can call BriefingService |
| AC-33 | No net reduction in test count | CI: count tests before and after |
| AC-34 | BriefingService unit tests cover all entry sources and edge cases | Test suite inspection |
| AC-35 | Integration tests verify MCP and UDS produce equivalent results | Snapshot comparison tests |
| AC-36 | dispatch_unknown_returns_error test updated | Test passes with new unimplemented variant |
| AC-37 | No changes outside unimatrix-server and unimatrix-engine | Git diff inspection |

Note: AC-28 through AC-32 (S2 rate limiting) are deferred per ADR-004.

## Domain Models

### Key Entities

| Entity | Definition | Location |
|--------|-----------|----------|
| **BriefingService** | Transport-agnostic service that assembles knowledge entries into a budget-constrained briefing | `services/briefing.rs` |
| **BriefingParams** | Caller-provided parameters controlling which entry sources are active and the token budget | `services/briefing.rs` |
| **BriefingResult** | Assembly output: conventions, relevant context, injection sections, entry IDs, search availability | `services/briefing.rs` |
| **InjectionSections** | Injection history entries partitioned by category (decisions, injections, conventions) with fixed priority order | `services/briefing.rs` |
| **InjectionEntry** | Minimal record (entry_id, confidence) passed from UDS session to BriefingService | `services/briefing.rs` |
| **Briefing** | MCP-specific formatting struct (conventions, relevant_context, no duties). Used only by `format_briefing`. Feature-gated with MCP tool. | `response.rs` |

### Relationships

```
BriefingParams ──────► BriefingService::assemble()
                              │
                              ├── include_conventions=true ──► AsyncEntryStore::query()
                              ├── include_semantic=true   ──► SearchService::search()
                              └── injection_history=Some  ──► AsyncEntryStore::get() per entry
                              │
                              ▼
                        BriefingResult
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        MCP transport   CompactPayload    Briefing handler
        (format_briefing) (format text)   (format text)
```

## User Workflows

### Workflow 1: Agent Requests Briefing via MCP

1. Agent calls `context_briefing(role="architect", task="design auth module", feature="vnc-007")`
2. MCP transport resolves identity, checks Read capability
3. Transport constructs BriefingParams with `include_semantic=true`
4. BriefingService assembles conventions + semantic results
5. Transport formats BriefingResult as summary/markdown/json
6. Agent receives formatted briefing

### Workflow 2: Claude Code Triggers Compaction

1. PreCompact hook fires when context window is full
2. UDS transport resolves session state from SessionRegistry
3. Transport constructs BriefingParams with `include_semantic=false`, `injection_history=Some(...)`
4. BriefingService assembles from injection history (no embedding)
5. Transport formats as BriefingContent text
6. Claude Code receives compaction payload

### Workflow 3: Hook Requests Native Briefing

1. Hook sends `HookRequest::Briefing { role, task, feature, max_tokens }`
2. UDS dispatch handler constructs BriefingParams with `include_semantic=true`
3. BriefingService assembles conventions + semantic results
4. Handler formats as BriefingContent text
5. Hook receives briefing

## Constraints

1. vnc-006 service layer must be merged before implementation
2. Changes confined to `crates/unimatrix-server/` and `crates/unimatrix-engine/`
3. No new crates
4. No schema version bump
5. Fire-and-forget patterns preserved for audit and confidence
6. Both compilation configurations (with/without mcp-briefing) must pass tests

## Dependencies

| Dependency | Type | Status |
|------------|------|--------|
| vnc-006 ServiceLayer | Code dependency | Must be merged |
| SearchService (vnc-006) | Used by BriefingService | Available |
| SecurityGateway (vnc-006) | Used by BriefingService | Available |
| AuditContext (vnc-006) | Used by BriefingService | Available |
| AsyncEntryStore (vnc-006) | Used by BriefingService | Available |
| SessionRegistry (col-008) | Used by UDS transport | Available |
| HookRequest::Briefing (wire.rs) | Wire protocol variant | Exists, unimplemented |

## NOT in Scope

- S2 rate limiting (deferred to vnc-009 per ADR-004)
- Module reorganization (vnc-008)
- SessionRegister briefing (stretch only, not required)
- Unified capability model / SessionWrite (vnc-008)
- StatusService extraction (vnc-008)
- Changes to /query-patterns skill
- Deprecating duties entries in knowledge base
- HTTP transport
