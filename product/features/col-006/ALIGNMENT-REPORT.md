# Alignment Report: col-006 (Hook Transport Layer — "Cortical Implant")

> Reviewed: 2026-03-02
> Artifacts reviewed:
>   - product/features/col-006/architecture/ARCHITECTURE.md
>   - product/features/col-006/specification/SPECIFICATION.md
>   - product/features/col-006/SCOPE-RISK-ASSESSMENT.md (no RISK-TEST-STRATEGY.md exists)
>   - product/features/col-006/SCOPE.md
>   - product/features/col-006/architecture/ADR-001 through ADR-007
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly establishes the "Hooks" leg of the three-leg boundary. Architecture faithfully serves the vision's automatic delivery goal. |
| Milestone Fit | PASS | Correctly scoped to M5 Collective phase. Foundation for col-007 through col-011. No premature Milestone 6+ work. |
| Scope Gaps | WARN | SCOPE.md references `search.rs`/`query.rs` stubs; architecture recommends deferral. Minor gap on queue replay. |
| Scope Additions | PASS | No scope additions detected. Source documents stay within SCOPE.md boundaries. |
| Architecture Consistency | PASS | Consistent with existing codebase patterns (PidGuard, spawn_blocking, crate graph). Engine extraction follows prior extraction precedent (nxs-004). |
| Risk Completeness | WARN | SCOPE-RISK-ASSESSMENT.md covers risks well. No RISK-TEST-STRATEGY.md exists yet, which is expected at this workflow phase. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `search.rs` / `query.rs` stubs | SCOPE.md Goal 4 says "extract shared business logic... so both MCP tools and hook handlers use the same query pipeline." SCOPE.md Open Question 3 suggests including stubs. Architecture (Open Question 1) recommends deferring to col-007. Specification FR-06.7 includes them as stubs. The architecture document omits them from the crate layout. Rationale is sound: col-006 does not execute search/query operations over UDS. This is an acceptable simplification with documented rationale. |
| Simplification | Queue replay | SCOPE.md Component 6 says "Queue replay on successful connection to server." Specification FR-08.9 explicitly defers replay to col-010. Architecture Open Question 3 discusses replay timing but also treats it as deferred. The degradation queue is built and durable, but replay is not col-006 scope. Documented and reasonable. |
| Simplification | `SocketGuard` RAII | SCOPE.md AC-10 requires socket cleanup. Specification FR-02.4 defines `SocketGuard`. Architecture uses `LifecycleHandles` extension with `socket_path: Option<PathBuf>` instead of a separate guard struct. Either approach satisfies the acceptance criterion. |

## Variances Requiring Approval

None. No VARIANCE or FAIL classifications were identified.

## Detailed Findings

### 1. Three-Leg Boundary (Vision Criterion 1)

**Status: PASS**

The product vision (lines 7-12) defines three legs:
- **Files** define the process
- **Unimatrix** holds the expertise
- **Hooks** connect them

col-006 directly establishes the "Hooks" leg. The architecture document positions the cortical implant as the bridge between Claude Code lifecycle events and the Unimatrix knowledge engine. The system overview diagram (ARCHITECTURE.md lines 11-48) shows this precisely: hook processes spawn per lifecycle event, communicate over UDS to the running server, which holds the knowledge tier.

**Evidence:**
- SCOPE.md line 1: "Hook Transport Layer" -- named after the vision's third leg
- ARCHITECTURE.md lines 9-10: "Every hook process must communicate with the server via IPC. This constraint shapes every component boundary."
- SCOPE.md lines 5-8: "By connecting Unimatrix to these hooks, knowledge delivery becomes automatic -- every prompt enriched, compaction resilient, confidence feedback closed-loop -- without agent cooperation."

The feature correctly establishes transport infrastructure only. It does not implement the knowledge delivery logic (col-007), compaction resilience (col-008), or feedback loops (col-009). This separation is appropriate for a foundation feature.

### 2. Invisible Delivery (Vision Criterion 2)

**Status: PASS**

The vision states knowledge should reach agents as "ambient context" without agent cooperation (PRODUCT-VISION.md lines 5, 12, 18). col-006 does not implement injection itself but provides the transport mechanism that makes invisible delivery possible.

**Evidence:**
- SCOPE.md lines 5-8: "knowledge delivery becomes automatic... without agent cooperation"
- Specification FR-03.8: "The hook subcommand does not initialize tokio runtime, ONNX, redb, or HNSW" -- the hook is designed to be invisible to agents, operating as a Claude Code lifecycle event handler
- Specification FR-08.10: "All degradation paths produce exit code 0. The hook subcommand never blocks the user's workflow." -- invisible even when failing
- The hook subcommand writes to stdout (for synchronous injection) and stderr (for diagnostics), matching Claude Code's hook contract where stdout content is injected into the agent's context

col-006 is correctly positioned as the "nervous system" that future features will use to deliver knowledge invisibly.

### 3. Zero Regression (Vision Criterion 3)

**Status: PASS**

The design takes zero regression seriously at every level.

**Evidence:**
- SCOPE.md Constraints: "All existing MCP tools must continue to work identically. The 1025 unit + 174 integration tests must pass without modification after engine extraction."
- Specification NFR-02: Explicit zero regression requirement with test count cited
- Architecture ADR-001: Re-export pattern preserves backward compatibility -- `unimatrix-server` re-exports engine modules so existing integration test imports compile without modification
- SCOPE-RISK-ASSESSMENT.md SR-01: Engine extraction identified as Critical risk with Medium likelihood. Mitigation strategy (incremental one-module-at-a-time extraction) is sound and follows prior crate extraction precedent (nxs-004)
- Specification FR-01.5: "The UDS listener operates alongside the stdio transport... Both transports share the same underlying Store, VectorIndex, and EmbedServiceHandle via Arc." -- no functional change to MCP path

### 4. Trust + Lifecycle + Integrity (Vision Criterion 4)

**Status: PASS**

The vision emphasizes "Trust + Lifecycle + Integrity + Learning + Invisible Delivery" (PRODUCT-VISION.md line 20) and describes a trust hierarchy (lines 236-243).

**Evidence:**
- Specification FR-07.1 through FR-07.7: Three-layer authentication (filesystem permissions, kernel credentials, process lineage) with zero configuration. Each layer adds defense-in-depth without requiring user action.
- Specification FR-07.6: `cortical-implant` agent pre-enrolled as Internal trust with `[Read, Search]` capabilities. This correctly places the hook transport within the existing trust hierarchy (Internal is between Privileged and Restricted).
- Architecture ADR-003: Explicit decision to use layered auth without shared secrets, with documented platform-specific degradation (macOS loses Layer 3 but retains Layer 1+2)
- SCOPE-RISK-ASSESSMENT.md SR-04: Platform authentication degradation documented and accepted with clear rationale -- same-user processes are within the trust boundary for local development
- Audit trail: All UDS requests go through the same server that maintains the AUDIT_LOG table. The hook transport does not bypass audit logging.

The `cortical-implant` agent's capabilities are limited to `[Read, Search]` -- it cannot write or modify entries. This is appropriate for a transport layer that in col-006 only does Ping/SessionRegister/SessionClose. Future features (col-009 confidence feedback) will need to revisit whether the cortical-implant needs Write capability; this is correctly deferred.

### 5. Self-Contained Embedded Engine / Zero Cloud Dependency (Vision Criterion 5)

**Status: PASS**

The vision states "self-contained embedded engine with zero cloud dependency" (PRODUCT-VISION.md line 20).

**Evidence:**
- UDS is filesystem-local. No TCP, HTTP, or network listeners are opened (Specification NFR-04)
- No new external dependencies beyond the existing workspace (Specification NFR-06)
- The hook subcommand is bundled into the existing binary (SCOPE.md Constraint: "Single binary")
- No cloud services, no network calls, no API keys
- Architecture explicitly rules out remote/HTTPS transport for col-006 (Non-Goals, line 33 of SCOPE.md)

### 6. Auditable Knowledge Lifecycle (Vision Criterion 6)

**Status: PASS**

The vision emphasizes "hash-chained correction histories with attribution" and "auditable knowledge lifecycle" (PRODUCT-VISION.md lines 14-18).

**Evidence:**
- col-006 does not introduce any new data mutation paths. The SessionRegister and SessionClose handlers in col-006 only log to stderr -- they do not write to redb tables (Specification FR-10.4, FR-10.5). Actual session state persistence is deferred to col-010.
- The `cortical-implant` agent identity (FR-07.6) ensures that when future features (col-009) do write through the UDS channel, those writes will be attributed to a known agent identity in the audit log.
- No bypass of existing attribution or audit infrastructure. UDS requests route through the same server that enforces audit logging.
- The wire protocol is internal (same binary) so there is no opportunity for external actors to inject unattributed knowledge.

### 7. Cross-Domain Portability (Vision Criterion 7)

**Status: PASS**

The vision notes "cross-domain portability" (PRODUCT-VISION.md line 22) -- the engine should work for any domain, not just software development.

**Evidence:**
- The hook transport layer is entirely domain-agnostic. The wire protocol types (`HookRequest`, `HookResponse`) carry generic events with `serde_json::Value` payloads (Specification FR-04.7: `ImplantEvent.payload: serde_json::Value`).
- No domain-specific constants, categories, or content interpretation in the transport layer.
- The `cortical-implant` agent enrollment uses generic capabilities (`Read`, `Search`) not domain-coupled permissions.
- The event queue format (JSONL) is domain-neutral.
- The project discovery logic (`compute_project_hash`) works with any project structure -- git or otherwise (detected from the `cwd` field).

No domain coupling is introduced by col-006.

### Milestone Fit

**Status: PASS**

col-006 is correctly placed within Milestone 5 (Orchestration Engine, Collective Phase). The product vision roadmap (PRODUCT-VISION.md lines 103-136) describes col-006 as the "foundation for all delivery" in the dependency graph (line 260), and all col-007 through col-011 features depend on it.

**Evidence of no milestone creep:**
- No Milestone 6 work (thin-shell migration) is introduced
- No Milestone 7 work (dashboard/UI) is attempted
- No new redb tables are created (those are col-010 scope, still within M5)
- No schema v4 migration (deferred to col-010, documented in ADR-007)
- The stub types for future Request/Response variants (`ContextSearch`, `Briefing`, `CompactPayload`, `Entries`, `BriefingContent`) are minimal forward declarations, not implementations. They exist in the enum definition with `#[allow(dead_code)]` -- this is standard Rust practice for extensible enums and does not constitute premature implementation.

### Architecture Review

**Status: PASS**

The architecture is well-structured with clear component boundaries and consistent patterns.

**Strengths:**
- The crate dependency graph maintains the existing directional flow (embed -> store -> vector -> core -> engine -> server). No circular dependencies.
- The engine extraction strategy (ADR-001) is conservative and well-mitigated: one module at a time, re-exports for backward compatibility, full test suite after each move.
- The hook subcommand's minimal initialization path (ADR-002: no tokio, no ONNX, no redb) is architecturally sound -- it avoids loading heavy dependencies that the hook process does not need.
- The socket lifecycle coordination (ADR-004) leverages the existing PidGuard pattern rather than inventing a new mutual exclusion mechanism.
- The wire protocol choice (ADR-005: length-prefixed JSON over full JSON-RPC) is proportionate to the need -- internal protocol between components of the same binary does not need JSON-RPC's full envelope.

**Minor observation (not a variance):**
- The architecture document's `LifecycleHandles` extension uses `socket_path: Option<PathBuf>` rather than a `SocketGuard` struct. The specification defines a `SocketGuard` RAII struct (FR-02.4). The architecture document's approach is simpler but achieves the same cleanup guarantee through the shutdown sequence. The implementation should reconcile these -- either a `SocketGuard` (specification approach) or a `socket_path` field with explicit cleanup (architecture approach). Both are valid. This is a detail for the implementer, not a vision variance.

### Specification Review

**Status: PASS**

The specification is thorough (560 lines), with well-defined functional requirements, non-functional requirements, domain models, user workflows, and interface contracts.

**Strengths:**
- 13 acceptance criteria with clear verification methods
- Domain models are precisely typed with serde annotations documented
- The wire protocol format is fully specified (framing, max size, encoding)
- The hook configuration JSON for `.claude/settings.json` is provided as a copy-paste block
- Startup and shutdown ordering is explicitly sequenced
- Integration points with existing systems (PidGuard, LifecycleHandles, AgentRegistry, ProjectPaths) are enumerated

**One clarification needed (not a variance):**
- Specification FR-04.5 defines `Request` enum variants while Architecture uses `HookRequest` as the type name. Similarly, FR-04.6 uses `Response` while Architecture uses `HookResponse`. This is a naming inconsistency between the two documents. The architecture's naming (`HookRequest`/`HookResponse`) is more descriptive and avoids collision with generic names. Recommend using the architecture's naming in implementation.

### Risk Assessment Review

**Status: WARN (minor)**

SCOPE-RISK-ASSESSMENT.md provides a comprehensive risk register (10 risks, SR-01 through SR-10) with appropriate severity/likelihood assessments, impact analysis, and mitigations. Risk priority matrix correctly identifies which risks need architecture attention vs. specification attention vs. monitoring.

**Why WARN instead of PASS:**
- There is no RISK-TEST-STRATEGY.md document. The workflow convention (from CLAUDE.md feature directory structure) lists this as a Phase 2 source document. The SCOPE-RISK-ASSESSMENT.md (Phase 1b) exists and is thorough, but it focuses on scope risks rather than the complete risk-test mapping that RISK-TEST-STRATEGY.md would provide (linking each risk to specific test scenarios, coverage requirements, and verification strategies). This may be expected if the feature is still in Phase 2 design and the RISK-TEST-STRATEGY.md has not been authored yet.
- SR-06 (concurrent UDS connections during swarm runs) correctly identifies the concern but the mitigation is somewhat deferred ("Future features may add backpressure if needed" per the architecture). Given that swarm runs with 5-10 agents are the standard workflow, a more concrete initial approach (even if simple, like a connection counter log) would strengthen the strategy.

### Specification-Architecture Consistency

Both documents are internally consistent and consistent with each other on all material points:

| Aspect | Specification | Architecture | Consistent? |
|--------|--------------|--------------|-------------|
| Engine extraction modules | confidence, coaccess, project (FR-06.2) | confidence, coaccess, project (Component 1) | Yes |
| Extraction strategy | Incremental, one module at a time (FR-06.4) | Same, plus explicit ordering (ADR-001) | Yes |
| Hook process runtime | No tokio (FR-03.8) | No tokio (ADR-002) | Yes |
| Wire protocol | Length-prefixed JSON, 4-byte BE u32 (FR-05.1) | Same (Component 4, ADR-005) | Yes |
| Socket lifecycle | Unconditional unlink after PidGuard (FR-02.2) | Same (ADR-004) | Yes |
| Authentication layers | 3 layers (FR-07.1-7.4) | 3 layers (Component 6, ADR-003) | Yes |
| Cortical-implant trust | Internal, Read+Search (FR-07.6) | Internal, Read+Search (Integration Surface) | Yes |
| Degradation behavior | Exit 0 always (FR-08.10) | Same (Component 7) | Yes |
| Request/Response naming | `Request`/`Response` | `HookRequest`/`HookResponse` | Minor naming difference |
| `search.rs`/`query.rs` stubs | Included as stubs (FR-06.7) | Recommends deferral (Open Question 1) | Minor disagreement |

The two naming/stub disagreements are minor and do not affect vision alignment.

## Alignment Classification Summary

| Criterion | Classification | Evidence Reference |
|-----------|---------------|-------------------|
| Three-leg boundary | PASS | ARCHITECTURE.md system overview; SCOPE.md goals 1-3 |
| Invisible delivery | PASS | FR-03.8, FR-08.10; SCOPE.md problem statement |
| Zero regression | PASS | NFR-02, FR-06.4, ADR-001; SR-01 mitigation strategy |
| Trust + Lifecycle + Integrity | PASS | FR-07.1-7.7, ADR-003; cortical-implant enrollment |
| Zero cloud dependency | PASS | NFR-04; UDS-only transport, single binary |
| Auditable knowledge lifecycle | PASS | No new mutation paths; agent identity attribution |
| Cross-domain portability | PASS | Generic wire protocol types; no domain coupling |

**Final assessment: 5 PASS, 2 WARN (minor). Zero VARIANCE. Zero FAIL.**

The col-006 design is well-aligned with the product vision. The two WARN items are administrative (missing RISK-TEST-STRATEGY.md document, minor spec-architecture naming inconsistency on stubs) rather than substantive. No variances require human approval.
