# Alignment Report: vnc-022

> Reviewed: 2026-05-29
> Artifacts reviewed:
>   - product/features/vnc-022/architecture/ARCHITECTURE.md
>   - product/features/vnc-022/specification/SPECIFICATION.md
>   - product/features/vnc-022/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/vnc-022/SCOPE.md
> Scope risk source: product/features/vnc-022/SCOPE-RISK-ASSESSMENT.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances `goal:personal-cloud` and enables `goal:self-learning` + `goal:proactive-delivery` for remote sessions |
| Milestone Fit | PASS | Vinculum-phase feature; builds on vnc-021 infrastructure; no future-milestone scope creep |
| Scope Gaps | PASS | All SCOPE.md requirements addressed in source documents |
| Scope Additions | WARN | Architecture adds session ID per-token scoping (ADR-003) not explicitly requested in SCOPE.md |
| Architecture Consistency | PASS | Reuses existing pipeline; no new deps; addresses all 9 scope risks |
| Risk Completeness | PASS | 14 risks identified; all scope risks traced; security surface covered |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Aligned | dispatch_request pub(crate) in place | SCOPE Proposed Approach 1 -> Architecture C2, Spec FR-08, Constraint C-01 |
| Aligned | /observe handler replaces 501 stub | SCOPE Goal 1 -> Architecture C3, Spec FR-01 |
| Aligned | hook-remote CLI CUT | SCOPE Non-Goals -> Spec NOT in Scope, Architecture has no binary component |
| Aligned | PreCompact briefing-only Day 1 | SCOPE Non-Goals -> Spec FR-05/FR-12, Architecture C5 |
| Aligned | No new Rust dependencies | SCOPE Constraint 4 -> Spec NFR-01/C-04, Architecture line 9 |
| Aligned | CompactPayload transcript_excerpt forward compat | SCOPE AC-09 -> Spec FR-12, Architecture C5 |
| Aligned | Session identity via client-generated session_id | SCOPE Proposed Approach 4 -> Architecture Session Identity section |
| Aligned | All 10 critical+important events handled | SCOPE AC-15 -> Spec FR-13, Architecture Event Tier Coverage table |
| Aligned | Wire contract documented | SCOPE AC-16 -> Spec Wire Contract section (full JSON examples) |
| Aligned | Integration test requirements | SCOPE AC-17 -> Risk-Test-Strategy R-01 through R-14 scenarios |
| Aligned | UDS regression protection | SCOPE AC-18 -> Spec NFR-08, Risk-Test-Strategy R-02 |
| Addition | Session ID per-token scoping via "http:" prefix | Architecture ADR-003 adds session_id prefixing. SCOPE says "No server-assigned session IDs needed." SCOPE-RISK-ASSESSMENT SR-03 recommends architect assess this. The architect responded with a prefix scheme. |
| Addition | PreCompact degradation signaling deferred | SCOPE-RISK-ASSESSMENT SR-02 recommended degradation signaling. Spec explicitly defers it (NOT in Scope). This is a conscious simplification. |
| Simplification | Day 1 prefix is constant "http:" (not per-token hash) | Architecture documents this as sufficient for single-user personal cloud. Per-token isolation deferred to OAuth (W2-3). Rationale provided. |

## Variances Requiring Approval

### 1. Session ID Prefix Scheme (WARN — scope addition, low risk)

**What**: Architecture ADR-003 introduces session_id prefixing (`http:{identity_hash_prefix}:{client_session_id}`) and the Risk-Test-Strategy adds 4 test scenarios (R-03, R-14) for it. SCOPE.md says "No server-assigned session IDs needed" and describes session identity as client-generated with format validation only.

**Why it matters**: This is a scope addition responding to SR-03 (session hijacking risk). The architect added it based on the SCOPE-RISK-ASSESSMENT recommendation. The Day 1 simplification (constant "http:" prefix) is minimal overhead. However, the full per-token hash scheme described in ADR-003 is forward-looking — it won't be exercised until multi-user/OAuth deployments.

**Recommendation**: ACCEPT. The addition is defensive, directly responds to an identified scope risk, and the Day 1 implementation is minimal (constant string prefix). The risk tests are proportional. The forward-looking design in ADR-003 text is documentation, not implementation scope.

---

No FAIL or VARIANCE items identified. One WARN requiring acknowledgment.

## Detailed Findings

### Vision Alignment

**Assessment: PASS**

vnc-022 is the critical bridge feature for the `goal:personal-cloud` strategic goal. The goal's success criteria state (Unimatrix #4676):

> "Remote sessions have same intelligence pipeline fidelity as local UDS sessions"
> "No local binary required for remote clients -- hook events POST to /observe endpoint, injection content returned in response"

The feature directly delivers both criteria. The architecture reuses `dispatch_request` unchanged in logic (Architecture C2), ensuring pipeline fidelity parity between UDS and HTTP paths. The hook-remote CLI cut aligns with the goal's "no local binary required" criterion — clients POST directly.

The feature also enables two other strategic goals for remote sessions:
- **`goal:self-learning`** (#4677): Behavioral signals (observation events) now flow from remote clients, feeding the learning pipeline that was previously blind to remote usage.
- **`goal:proactive-delivery`** (#4673): Proactive injection (UserPromptSubmit, SubagentStart) and PreCompact restoration now work over HTTPS, delivering knowledge before agents ask.

The product vision states: "Any event source -- hooks, webhooks, automated pipelines -- feeds the learning layer without agent cooperation." The `/observe` endpoint is exactly this — an event source that feeds the learning layer from remote hooks.

**Architectural principles checked**:

| Principle | Status | Evidence |
|-----------|--------|---------|
| Hash chain integrity | N/A | No knowledge write path introduced; observations are metadata |
| Audit log append-only | PASS | Spec FR-14: credential_type = "static_token", same audit trail structure as MCP |
| Capability checks at service layer | PASS | Architecture C4: capabilities resolved from ResolvedIdentity, checked inside dispatch_request |
| Typed relationship graph | N/A | No graph operations introduced |
| Graceful degradation | PASS | Architecture documents fire-and-forget semantics, PreCompact Day 1 briefing-only fallback, EmbedService unavailable handling |
| Single binary, zero infrastructure | PASS | No new binary, no new dependencies, same container |
| In-memory hot path | N/A | No analytics path changes |
| No secrets in database | PASS | Bearer token validated in middleware, never stored. session_id is opaque metadata |

### Milestone Fit

**Assessment: PASS**

vnc-022 is a Vinculum-phase (`vnc`) feature, appropriate for the HTTPS transport layer. It builds directly on vnc-021 (HTTPS transport for MCP tools) and the `/observe` 501 stub that vnc-021 shipped. The feature does not pull in future-milestone capabilities:

- No GNN (Wave 3-1) components
- No enterprise OAuth (Wave 2-3)
- No SSE push notifications (blocked on client support)
- No domain pack configuration

The forward-compatibility field (`transcript_excerpt` on `CompactPayload`) is a zero-cost additive change that prevents a wire format break when #670 lands. This is responsible engineering, not scope creep.

### Architecture Review

**Assessment: PASS**

The architecture is clean, minimal, and well-structured:

1. **ObserveContext struct (C1)**: Directly addresses SR-07 (PathRouter cannot reach service handles) and SR-01 (parameter sprawl). The struct bundles Arc-cloned handles, insulating PathRouter and main.rs from dispatch_request parameter evolution. This is the right pattern.

2. **dispatch_request modification (C2)**: Minimal change — visibility to `pub(crate)`, one new parameter. The `uds_has_capability(X)` to `capabilities.contains(&X)` refactor is mechanical. Git history preserved. Consistent with SCOPE resolved decision 1.

3. **Response mapping (C3)**: The HTTP status code mapping (Ack->204, content->200, Error->400) is RESTful and well-documented in the Architecture's HTTP Response Mapping table.

4. **Event tier coverage**: All 10 critical+important events documented. The "Deferred" note clarifying that deferred events still work (dispatch_request handles all variants) is accurate and avoids false scope inflation.

5. **Session identity model**: ADR-003's "http:" prefix provides transport namespace isolation. The Day 1 simplification (constant prefix) is honest about what single-user personal cloud needs vs. what multi-user deployments will need.

6. **Fire-and-forget semantics (SR-06)**: Architecture explicitly documents the data-loss window and justifies it — fire-and-forget events are observational, not correctional. Same semantics as UDS. Acceptable.

7. **Event dependency analysis (SR-05)**: Architecture confirms no critical-path event depends on a deferred nice-to-have event. Analysis is correct — SubagentStop, Ping, and unrecognized events are leaf operations with no downstream consumers.

### Specification Review

**Assessment: PASS**

The specification is thorough and well-structured:

1. **Functional requirements (FR-01 through FR-14)**: Cover all pipeline paths. Each FR maps cleanly to a SCOPE acceptance criterion or proposed approach item.

2. **Non-functional requirements (NFR-01 through NFR-10)**: Correctly capture all SCOPE constraints. No constraint from SCOPE.md is missing from the spec.

3. **Acceptance criteria (AC-01 through AC-19)**: Mirror SCOPE.md ACs exactly. Numbering matches. AC-11, AC-12, AC-13 are absent (consistent with SCOPE — those numbers were likely reserved for hook-remote CLI ACs that were cut).

4. **Wire contract section**: Complete JSON examples for all HookRequest variants, response shapes, session ID rules, and error responses. This satisfies AC-16 and SR-04 (contract drift risk).

5. **NOT in Scope section**: Comprehensive — covers hook-remote CLI, context_observe MCP tool, SSE, client automation, enterprise OAuth, nice-to-have events, offline buffering, full PreCompact, dispatch_request file move, session ID per-token scoping, and PreCompact degradation signaling. Each item includes rationale.

6. **Specification-SCOPE discrepancy on session ID scoping**: Spec NOT in Scope says "No server-side enforcement of per-token session namespacing (SR-03 assumption documented)." However, Architecture ADR-003 introduces session_id prefixing with "http:" prefix. These are reconcilable — the spec defers per-token scoping while the architecture adds transport-level scoping (constant "http:" prefix, not per-token). The spec should ideally reference the architecture's prefix scheme for clarity, but this is a minor editorial note, not a functional gap.

7. **Open Questions for Architect**: All three questions (SR-07 handle access, SR-01 context struct, SR-03 session scoping) are answered in the Architecture document. The specification correctly deferred these to the architect.

### Risk Strategy Review

**Assessment: PASS**

The Risk-Test-Strategy is comprehensive and well-connected to scope risks:

1. **Risk register**: 14 risks, covering structural (R-01, R-02, R-11), security (R-03, R-08, R-09), correctness (R-04, R-05, R-06, R-07, R-12), observability (R-10, R-13), and edge cases (R-14).

2. **Scope risk traceability**: All 9 scope risks (SR-01 through SR-09) are mapped to architecture risks and test scenarios. The traceability table at the end confirms complete coverage.

3. **Test scenario depth**: 38 total test scenarios across the 14 risks. Proportional to severity — High-priority risks (R-01, R-02, R-03, R-06, R-10) have 3 scenarios each. Low-priority risks have 2-3.

4. **Security risks**: The untrusted input assessment table covers all 6 input surfaces (body, session_id, query, payload, transcript_excerpt, bearer token). Specific security scenarios address path traversal in `cwd`, session ID injection (#3902 reference), and payload size within body limits.

5. **Failure modes**: 7 failure modes documented with expected behavior and recovery strategy. The SessionRegistry lock poisoning scenario (manual restart required) is the worst-case failure mode — correctly identified.

6. **Integration risks**: 5 integration risks documented, including the ObserveContext construction ordering risk and the StaticTokenAuth middleware ordering risk. Both are testable via the integration test suite.

7. **Edge cases**: 10 edge cases documented, including empty body, wrong-schema JSON, empty payloads, boundary session IDs, duplicate registration, and Content-Type mismatch. Good coverage of the deserialization boundary.

8. **R-10 (warn+continue failure paths)**: This risk references #4473 and correctly identifies historically omitted test coverage for side-effect failure arms in dispatch_request. This is the highest-likelihood risk (High) and is appropriately prioritized as High priority.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_search for vision alignment patterns -- 5 results returned, none directly applicable to vision guardian review patterns. Closest match was #2298 (config key semantic divergence) and #3337 (architecture diagram header divergence from spec), both specific to individual features rather than generalizable vision alignment patterns.
- Stored: nothing novel to store -- this is the first vision guardian review in Unimatrix. If the session ID prefix WARN pattern (architect adds security hardening not in SCOPE) recurs across multiple features, it should be stored as a pattern: "Architects consistently add defensive security measures beyond SCOPE when risk assessments flag security concerns -- accept if Day 1 implementation is minimal."
