# Gate 3a Report: vnc-022

> Gate: 3a (Design Review)
> Date: 2026-05-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 5 components map 1:1 to architecture. Interfaces, types, file locations match. |
| Specification coverage | PASS | All 14 FRs, 10 NFRs, and 19 ACs addressed in pseudocode. No scope additions. |
| Risk coverage | PASS | All 14 risks (R-01 through R-14) have test scenarios. 38 scenarios total across 5 test plans. |
| Interface consistency | PASS | ObserveContext fields, dispatch_request signature, capability sets, wire types consistent across all pseudocode files. |
| Knowledge stewardship | PASS | All 4 design-phase agents have stewardship blocks with evidence of queries and storage/decline. |

## Detailed Findings

### Architecture Alignment
**Status**: PASS
**Evidence**:

Component-to-architecture mapping verified:

1. **C1 (ObserveContext)**: Pseudocode `observe-context.md` defines `ObserveContext` struct in `router.rs` with exactly the 9 fields listed in ARCHITECTURE.md Integration Surface row 2. Fields: `store, embed_service, vector_store, entry_store, adapt_service, server_version, session_registry, pending_entries_analysis, services`. All `Arc`-wrapped. `#[derive(Clone)]` per ADR-001. `pub` visibility with re-export from `http/mod.rs` for `main.rs` construction -- correctly addresses the binary-crate visibility constraint.

2. **C2 (dispatch_request refactor)**: Pseudocode `dispatch-request-refactor.md` changes visibility to `pub(crate)`, adds `capabilities: &[Capability]` as final parameter, replaces 9 `uds_has_capability` call sites. Verified against actual source: lines 540, 625, 662, 736, 868, 1006, 1171, 1201 all contain `uds_has_capability` as documented. UDS call site at line 478 passes `UDS_CAPABILITIES`. Matches ARCHITECTURE.md Integration Surface row 1.

3. **C3 (/observe handler)**: Pseudocode `observe-handler.md` replaces `observe_stub_response()` at router.rs line 115-119 (verified: actual code shows the 501 stub at those lines). Handler follows the 7-step flow from architecture: identity extraction, CL fast-path, Limited body collection, deserialization, session_id prefix, dispatch_request call, response mapping. References existing `payload_too_large_response()` and `internal_error_response()` (verified: both exist at router.rs lines 346 and 363).

4. **C4 (Capability extension)**: Pseudocode `capability-extension.md` adds `SessionWrite` to the `[Read, Write, Search]` vec at auth.rs line 122 (verified: actual code shows `vec![Capability::Read, Capability::Write, Capability::Search]` at line 122). Matches ARCHITECTURE.md Integration Surface row 4.

5. **C5 (CompactPayload wire)**: Pseudocode `compact-payload-wire.md` adds `transcript_excerpt: Option<String>` with `serde(default, skip_serializing_if)` to CompactPayload at wire.rs line 151 (verified: actual CompactPayload variant spans lines 151-161 with no `transcript_excerpt` field currently). Matches ARCHITECTURE.md Integration Surface row 5.

Technology choices consistent with ADRs:
- ADR-001: ObserveContext struct bundles 9 handles (not individual fields on PathRouter)
- ADR-002: Capability parameter replaces uds_has_capability (not new function per transport)
- ADR-003: "http-" prefix for session_id scoping (not per-token hash -- Day 1 simplification documented)
- ADR-004: Ack->204, content->200, Error->400 mapping
- ADR-005: Optional field with serde(default) for backward compatibility

### Specification Coverage
**Status**: PASS
**Evidence**:

Functional requirements coverage:

| FR | Pseudocode Component | Covered |
|----|---------------------|---------|
| FR-01 (stub replacement) | observe-handler.md: replaces stub at line 115-119 | Yes |
| FR-02 (body deserialization) | observe-handler.md: Step 4, serde_json::from_slice | Yes |
| FR-03 (Ack->204) | observe-handler.md: observe_response_to_http Ack arm | Yes |
| FR-04 (Entries->200) | observe-handler.md: Entries arm | Yes |
| FR-05 (BriefingContent->200) | observe-handler.md: BriefingContent arm | Yes |
| FR-06 (Pong->200) | observe-handler.md: Pong arm | Yes |
| FR-07 (Error->400) | observe-handler.md: Error arm | Yes |
| FR-08 (shared dispatch_request) | dispatch-request-refactor.md: pub(crate) + capabilities | Yes |
| FR-09 (SessionWrite capability) | capability-extension.md: adds SessionWrite to HTTP caps | Yes |
| FR-10 (body size limit) | observe-handler.md: Steps 2-3, CL fast-path + Limited | Yes |
| FR-11 (service handle access) | observe-context.md: ObserveContext struct on PathRouter | Yes |
| FR-12 (transcript_excerpt) | compact-payload-wire.md: Optional field with serde annotations | Yes |
| FR-13 (all tier-1/2 events) | Handled by dispatch_request (unchanged logic) | Yes |
| FR-14 (audit log) | observe-handler.md: identity extracted, passed to dispatch_request | Yes |

NFRs: No new dependencies (NFR-01), 1MB limit (NFR-02), no axum (NFR-07), no unsafe (NFR-05), no rmcp change (NFR-06), wire stability (NFR-09) all addressed.

No scope additions: pseudocode implements only what specification requires. No extra endpoints, no extra wire types, no extra middleware.

### Risk Coverage
**Status**: PASS
**Evidence**:

All 14 risks from RISK-TEST-STRATEGY.md have corresponding test scenarios in the test plans:

| Risk | Priority | Test Plan | Scenarios |
|------|----------|-----------|-----------|
| R-01 (ObserveContext divergence) | High | observe-context.md + observe-handler.md | 5 (compilation gate + 3 E2E per handle + 1 RecordEvent) |
| R-02 (UDS regression) | High | dispatch-request-refactor.md | 5 (existing tests + grep audit + capability denial/grant) |
| R-03 (Session ID prefix) | High | observe-handler.md | 4 (prefix applied, prefix on ContextSearch, sanitize compat, audit) |
| R-04 (Wrong status code) | Med | observe-handler.md | 5 (all 5 HookResponse variants) |
| R-05 (Body size bypass) | Med | observe-handler.md | 4 (CL fast-path, chunked, boundary, accepted) |
| R-06 (Missing SessionWrite) | High | capability-extension.md + observe-handler.md | 3 (unit caps check, no Admin, integration 204) |
| R-07 (CompactPayload compat) | Low | compact-payload-wire.md | 5 (round-trip, default None, omit None, null, empty string) |
| R-08 (Session cross-bleed) | Med | observe-handler.md | 1 (concurrent sessions isolated) |
| R-09 (Serde error leak) | Low | observe-handler.md | 1 (no internal type paths in 400 body) |
| R-10 (Warn+continue paths) | High | observe-handler.md | 2 (unregistered session RecordEvent, unregistered SessionClose) |
| R-11 (PathRouter Clone) | Med | observe-context.md | 2 (compilation gate, concurrent requests) |
| R-12 (Serialization edge cases) | Low | observe-handler.md | 2 (empty Entries, empty BriefingContent) |
| R-13 (Audit inconsistency) | Med | observe-handler.md | 1 (credential_type + agent_id assertions) |
| R-14 (sanitize_session_id prefix) | Low | observe-handler.md | 3 (UUID, max 128, over 128) |

Risk priorities are reflected in test plan emphasis: High-priority risks (R-01, R-02, R-03, R-06, R-10) have the most scenarios and are highlighted in the test overview.

Integration and edge case scenarios from the Risk Strategy are also covered: the test plan overview explicitly lists integration harness requirements (infra-001 smoke suite) and the observe-handler test plan includes edge cases (empty body, wrong schema, boundary body size).

### Interface Consistency
**Status**: PASS
**Evidence**:

1. **ObserveContext fields**: OVERVIEW.md defines 9 fields. `observe-context.md` defines the same 9 fields with matching types. `observe-handler.md` calls `dispatch_request` using `observe_ctx.{field}` for all 9 fields plus `identity.capabilities`. The dispatch_request signature in `dispatch-request-refactor.md` has 11 parameters (10 original + capabilities), and the handler call passes all 11 in correct order.

2. **Capability sets**: OVERVIEW.md defines UDS as `[Read, Search, SessionWrite]` and HTTP as `[Read, Write, Search, SessionWrite]`. `dispatch-request-refactor.md` passes `UDS_CAPABILITIES` at UDS call site. `capability-extension.md` sets HTTP caps to `[Read, Write, Search, SessionWrite]`. `observe-handler.md` passes `&identity.capabilities`. No contradictions.

3. **Session ID prefix**: OVERVIEW.md documents `format!("http-{}", client_session_id)`. `observe-handler.md` implements `prefix_session_id` matching this format for all HookRequest variants. `ContextSearch` correctly handles `Option<String>` session_id.

4. **Wire types**: `compact-payload-wire.md` adds `transcript_excerpt` to CompactPayload. `dispatch-request-refactor.md` adds `transcript_excerpt: _` to the destructuring pattern. No contradictions.

5. **Response mapping**: OVERVIEW.md data flow shows `observe_response_to_http(hook_response) -> HTTP Response`. `observe-handler.md` implements this function with all 5 variants matching ARCHITECTURE.md HTTP Response Mapping table.

6. **PathRouter::new signature**: `observe-context.md` changes it to accept `(ProjectRouter, ObserveContext)`. `observe-handler.md` uses `self.observe_ctx` inside `call()`. Consistent.

### Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**:

| Agent | Role | Block Present | Queries | Storage |
|-------|------|--------------|---------|---------|
| architect (agent-1) | Active storage | Yes (line 46) | 2 queries (briefing, search) | 5 ADRs stored (#4692-#4696) |
| risk-strategist (agent-3) | Active storage | Yes (line 31) | 4 queries (lessons, session, dispatch, risk patterns) | "nothing novel to store -- all risks feature-specific, not cross-feature patterns. The warn+continue gap (#4473) is already stored." |
| pseudocode (agent-1-pseudocode) | Read-only | Yes (line 33) | 3 queries (briefing, pattern search, decision search) | N/A (read-only agent) |
| test-plan (agent-2-testplan) | Read-only | Yes (line 41) | 3 queries (briefing, ADR search, HTTP testing patterns) | "nothing novel to store -- test plan follows established patterns" |

All agents have stewardship blocks. Active-storage agents have Stored or Declined entries with reasons. Read-only agents have Queried entries. Risk strategist's decline reason ("all risks feature-specific, not cross-feature patterns") is well-reasoned.

## Rework Required

None.

## Scope Concerns

None.
