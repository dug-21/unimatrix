# Alignment Report: vnc-001

> Reviewed: 2026-02-23
> Artifacts reviewed:
>   - product/features/vnc-001/architecture/ARCHITECTURE.md
>   - product/features/vnc-001/specification/SPECIFICATION.md
>   - product/features/vnc-001/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Server instructions, tool stubs, security infrastructure all match M2 vnc-001 vision |
| Milestone Fit | PASS | Feature targets M2 vnc-001 precisely -- no M3/M4 capabilities pulled forward |
| Scope Gaps | PASS | All 19 acceptance criteria from SCOPE.md addressed in spec and architecture |
| Scope Additions | WARN | `clap` dependency for CLI args not mentioned in SCOPE.md but minimal |
| Architecture Consistency | PASS | Two-layer architecture (lifecycle + request) aligns with nxs-004 trait pattern |
| Risk Completeness | PASS | 16 risks cover all major failure modes; security risks explicitly addressed |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Session ID generation | SCOPE.md mentions `session_id` in AuditEvent but architecture doesn't specify generation strategy. Spec includes it in FR-09a. Acceptable -- session ID can be generated from MCP initialize `requestId` or a UUID. |
| Addition | `clap` for CLI args | Architecture ADR-002 mentions `--project-dir` and `--verbose` CLI args; SCOPE.md mentions `--project-dir` override. clap dependency is a minor addition for standard CLI parsing. |
| Simplification | Tool annotation format | SCOPE.md references tool annotations from ASS-007. Architecture defers the exact rmcp annotation API to implementation. Acceptable -- rmcp's annotation API may differ from the spec notation. |

## Variances Requiring Approval

None. No VARIANCE or FAIL items.

## Detailed Findings

### Vision Alignment

The PRODUCT-VISION.md M2 entry for vnc-001 specifies:

> rmcp 0.16 SDK, stdio transport. Server instructions field for behavioral driving (70-85% agent compliance). Auto-init on first context_store. Project isolation via ~/.unimatrix/{project_hash}/. Persistence note: vnc-001 must coordinate graceful shutdown -- calling both Store::compact() (nxs-001) and VectorIndex::dump() (nxs-002). Security infrastructure: AGENT_REGISTRY table, AUDIT_LOG table, Agent identification via agent_id tool parameter for stdio.

**All vision items addressed:**
- rmcp 0.16 SDK: ADR-001, pinned to `=0.16.0`
- stdio transport: FR-01b, ADR-001
- Server instructions: FR-02a, instructions text matches ASS-006 research
- Auto-init: FR-04c, FR-05a, FR-06c (auto-creates directory, database, vector index)
- Project isolation: FR-04a/b, ADR-004 (SHA-256 hash path)
- Graceful shutdown: FR-12c (compact + dump in correct order), ADR-005
- AGENT_REGISTRY: FR-05c, FR-08a through FR-08h (full registry specification)
- AUDIT_LOG: FR-05d, FR-09a through FR-09e (full audit specification)
- Agent identification: FR-10a through FR-10e, ADR-003 (transport-agnostic pipeline)

**Vision note on auto-init timing:** The vision says "Auto-init on first context_store." The architecture initializes on server startup, not on first tool call. This is an improvement (data directory exists before any tool call, not just context_store), not a deviation. context_store specifically requiring auto-init was an ASS-007 simplification; the architecture correctly generalizes to startup-time init.

**Security cross-cutting alignment:** The vision's Security Cross-Cutting Concerns section specifies M2 vnc-001 responsibilities as "AGENT_REGISTRY table, AUDIT_LOG table (append-only), agent identification flow." All three are fully specified.

### Milestone Fit

The feature is squarely within M2 (Vinculum Phase). No capabilities from M3 (Agent Integration), M4 (Learning), or M5 (Orchestration) are pulled forward. Specific checks:

- No CLAUDE.md integration (M3 alc-001)
- No usage tracking (M4 crt-001)
- No confidence computation (M4 crt-002)
- No cross-project support (M7 dsn-001)
- No CLI commands (M9 nan-001)

The tool stubs (FR-11) correctly defer to vnc-002 rather than implementing tool logic.

### Architecture Review

**Component breakdown** (10 components, C1-C10) is well-defined with clear responsibilities and boundaries. The two-layer architecture (lifecycle management with concrete types vs request handling with trait objects) is a clean solution to the compact()-requires-mut-self tension.

**ADR quality**: 7 ADRs, each following the Context/Decision/Consequences format. ADR-007 (enforcement point architecture) directly addresses the human's directive to make security checks trivially pluggable for vnc-002.

**Integration surface**: Comprehensive table covering all consumed and exposed interfaces. Uses existing unimatrix-core patterns (adapters, async wrappers) without inventing new abstractions.

**Consistency with nxs-004**: The server's use of `AsyncEntryStore<StoreAdapter>` matches exactly the pattern nxs-004 designed for. No deviation from the established trait/adapter/wrapper chain.

### Specification Review

**Functional requirements**: 14 FRs cover all 9 goals from SCOPE.md. Every acceptance criterion (AC-01 through AC-19) has a corresponding FR and a verification method.

**Non-functional requirements**: 5 NFRs with measurable targets (startup < 2s, memory < 50MB, shutdown < 10s). These are reasonable for a local stdio server.

**Domain model**: 6 entities defined with clear ubiquitous language. The distinction between `ResolvedIdentity` (ephemeral) and `AgentRecord` (persisted) is clean.

**Dependencies**: All external crates listed with versions. `rmcp` pinned exactly. No unexpected dependencies.

### Risk Strategy Review

**Coverage**: 16 risks, 60 test scenarios. All Critical and High risks have multiple test scenarios.

**Architecture-specific risks**: R-04 (table creation backward compat), R-08 (Arc lifecycle during shutdown), R-10 (lazy embed state machine) are risks specific to the designed architecture, not generic risks.

**Security risks**: 4 security risks identified (SR-01 through SR-04), covering agent spoofing, path traversal, audit DoS, and deserialization. These align with the MCP Security Analysis findings.

**Failure modes**: 5 failure modes with expected behavior and recovery procedures. FM-05 (Arc::try_unwrap fails) directly validates the ADR-005 graceful degradation strategy.

**Gap check**: No missing risk categories. Integration risks, edge cases, security risks, and failure modes all present.
