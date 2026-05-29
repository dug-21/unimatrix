# Alignment Report: vnc-021

> Reviewed: 2026-05-29
> Artifacts reviewed:
>   - product/features/vnc-021/architecture/ARCHITECTURE.md
>   - product/features/vnc-021/specification/SPECIFICATION.md
>   - product/features/vnc-021/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Roadmap source: product/WAVE2-ROADMAP.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | HTTPS transport directly enables "personal cloud" deployment model per Wave 2 vision |
| Milestone Fit | PASS | W2-2 roadmap updated to reflect deliberate observability de-scope; title now "HTTPS Transport + Static Token Auth" |
| Scope Gaps | PASS | Observability de-scoped (roadmap updated). ASS-060 added to research spike index — findings exist, was a documentation gap |
| Scope Additions | PASS | BearerValidator trait (FR-14) accepted — architecturally necessary for W2-3 enterprise extension surface. CallerId::HttpBearer accepted — compiler-enforced rate-limit differentiation |
| Architecture Consistency | PASS | Architecture cleanly follows existing patterns (Clone server, Arc-shared service layer, CancellationToken shutdown) |
| Risk Completeness | PASS | 18 risks, 51 test scenarios, full traceability from scope risks to architecture risks to test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | Observability (Prometheus metrics, structured logging) | WAVE2-ROADMAP.md W2-2 section explicitly lists "Prometheus metrics endpoint: request count per tool, write queue depth, shed_events_total, pool acquire latency, tick completion time, audit log write latency" and "Structured logging: tracing spans with project_id for log routing" as part of W2-2 scope. SCOPE.md, ARCHITECTURE.md, and SPECIFICATION.md do not address these at all. |
| Gap | ASS-060 reference ungrounded | SCOPE.md line 86 references "ASS-060 specifies path-prefix routing" but ASS-060 does not appear in the WAVE2-ROADMAP.md research spike index (ASS-041 through ASS-064 listed). Source of the ProjectRouter path-prefix design is not traceable. |
| Addition | BearerValidator trait | SPECIFICATION.md FR-14 specifies a `BearerValidator` trait with `async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>`. SCOPE.md mentions `BearerValidator` only as a non-goal enabler ("BearerValidator trait enables this as an additive layer") and in the constraint about using `build_context_with_external_identity`. The trait definition is not an explicit SCOPE.md acceptance criterion. |
| Addition | CallerId::HttpBearer variant | ARCHITECTURE.md adds a new `CallerId::HttpBearer(String)` enum variant to `services/mod.rs`. SCOPE.md does not mention CallerId at all. This is a reasonable architectural necessity (rate limiting differentiation) but was not scoped. |
| Simplification | W2-6 ProjectRouter scope boundary | SCOPE.md Goal 5 asks for "structural seams for W2-6." W2-6 does not exist as a formal delivery item in WAVE2-ROADMAP.md -- it appears only in research spike findings as a future scope holder (ASS-061, ASS-062). The source docs deliver a ProjectRouter struct in single-project default mode, which is a pragmatic placeholder. Rationale: SCOPE.md explicitly constrains it as a seam, not an activation. Acceptable. |
| Simplification | No admin port activation | SCOPE.md Non-Goals, ARCHITECTURE.md, and SPECIFICATION.md all align: admin port 8444 is reserved/registered but not activated. This matches the vision's enterprise-in-private-repo model. Acceptable. |

## Variances — All Resolved

### 1. RESOLVED: Observability De-scoped from W2-2

Observability (Prometheus metrics, structured logging) deliberately de-scoped from vnc-021. WAVE2-ROADMAP.md W2-2 title updated to "HTTPS Transport + Static Token Auth" with a note that observability may ship as a separate feature. The path-dispatching router makes a future `/metrics` endpoint trivial to add.

### 2. RESOLVED: BearerValidator Trait Accepted

BearerValidator trait (FR-14, ~10 lines) accepted as architecturally necessary. Bridges vnc-021 to the enterprise private repo's JWT/OAuth validators per W2-3 security model (ASS-050). CallerId::HttpBearer similarly accepted — compiler-enforced rate-limit differentiation.

### 3. RESOLVED: ASS-060 Added to Research Index

ASS-060 (Multi-Project Data Architecture + ProjectRouter) findings exist and are thorough. The gap was in the roadmap's research spike index, not in the research itself. ASS-060 added to WAVE2-ROADMAP.md research prerequisites table.

## Detailed Findings

### Vision Alignment

The vnc-021 feature directly serves the product vision's Wave 2 goal: "containerized, HTTPS-accessible, multi-LLM compatible." Specific alignment points:

- **"Any LLM client integrates via HTTPS -- same tool API, same behavioral contract"** (PRODUCT-VISION.md, "After Wave 2" section): vnc-021 delivers HTTPS transport for Claude Code, Codex CLI, and Gemini CLI. Client setup documentation (AC-23 through AC-25) covers all three.
- **"Static bearer token auth -- zero enrollment friction"** (PRODUCT-VISION.md, "After Wave 2"): Token auto-generation, 0600 permissions, constant-time validation all align.
- **"Single binary"** (PRODUCT-VISION.md, "What's Preserved Throughout"): ARCHITECTURE.md confirms HTTP transport is compiled into the same binary. SPECIFICATION.md Constraint 7 repeats this.
- **"Capability checks enforced at the service layer, not the transport layer"** (PRODUCT-VISION.md, Security Non-Negotiable #3): ARCHITECTURE.md routes all HTTP requests through the same `ServiceLayer` with the same capability checks. Identity injection via `build_context_with_external_identity` is a transport-to-service bridge, not a transport-layer capability check.
- **"UDS session exemption from rate limiting remains local-only"** (PRODUCT-VISION.md, Security Non-Negotiable #6): ARCHITECTURE.md explicitly states `HttpBearer` is NOT exempt from rate limiting, with compiler-enforced exhaustive match.
- **"No secret material in any database"** (PRODUCT-VISION.md, Security Non-Negotiable #5): Token is stored in a file (0600 permissions), not in knowledge.db or analytics.db. ARCHITECTURE.md and SPECIFICATION.md both confirm.
- **"Audit log is append-only and complete"** (PRODUCT-VISION.md, Security Non-Negotiable #2): HTTP requests produce audit_log entries with `credential_type = "static_token"`. The existing vnc-014 schema (v25) is used without modification.

**Assessment**: PASS. No vision principle is violated.

### Milestone Fit

vnc-021 targets W2-2 in Wave 2 (Personal Cloud Delivery). This is the correct milestone -- HTTPS transport is a Wave 2 critical path item, prerequisite for W2-7 (remote telemetry) and W2-6 (multi-project routing).

The feature does not build Wave 3 capabilities prematurely. The ProjectRouter is a structural seam, not an activated multi-project system. The `/observe` stub returns 501, not a partial implementation.

**Assessment**: PASS. Observability deliberately de-scoped; roadmap updated to reflect the split.

### Architecture Review

ARCHITECTURE.md is well-structured with 8 components (C1-C8), clear interaction diagrams, and explicit integration points.

**Strengths**:
- Clean separation of concerns: each HTTP module has a single responsibility under 500 lines
- Uses existing patterns: `Clone` server pattern (ADR-003), `CancellationToken` shutdown, `Arc-shared` service layer
- Six ADRs recorded in Unimatrix (#4665-#4670) covering all technology decisions
- Open questions are honest about unknowns (rmcp extension propagation, HTTP version)
- Integration surface table explicitly lists all modified and new types/signatures

**No architectural violations detected.** The architecture respects all vision non-negotiables:
- Hash chain integrity: untouched (HTTP is a transport concern, not a storage concern)
- Audit log: extends existing schema with new `credential_type` value, no new migration
- Graceful degradation: absent TLS config falls back to plain HTTP; connection limits prevent resource exhaustion

**Assessment**: PASS.

### Specification Review

SPECIFICATION.md provides 30 functional requirements (FR-01 through FR-30), 10 non-functional requirements (NFR-01 through NFR-10), 25 acceptance criteria (AC-01 through AC-25), domain models, user workflows, constraints, and dependency lists.

**Strengths**:
- Every SCOPE.md acceptance criterion has a corresponding FR and verification method
- User workflows cover all personas (server operator, client developer, container monitoring)
- Explicit "NOT In Scope" section matches SCOPE.md non-goals
- Constraint list includes all SCOPE.md constraints plus SR-mitigations from the scope risk assessment

**One observation**: FR-14 introduces the `BearerValidator` trait as a formal requirement. This is a scope addition (see Variance #2 above) but is architecturally sound and aligned with the W2-3 security model extension surface.

**Assessment**: PASS (with WARN on BearerValidator scope addition noted above).

### Risk Strategy Review

RISK-TEST-STRATEGY.md identifies 18 risks (R-01 through R-18), maps them to 51 test scenarios, and provides full traceability to scope risks (SR-01 through SR-10).

**Strengths**:
- Critical risks (R-01, R-03, R-04, R-07) have the most test scenarios (14 total)
- Security risks analyzed separately with threat model, blast radius, and mitigations
- Edge cases section covers 10 boundary conditions (port 0, concurrent token generation, empty auth header, etc.)
- Failure modes table defines expected behavior and recovery for each failure type
- Scope risk traceability table maps every SR to corresponding architecture risks and resolutions

**Completeness check**:
- All scope risks (SR-01 through SR-10) are addressed in the traceability table
- All architecture ADRs (ADR-001 through ADR-006) are referenced in test scenarios
- Integration risks cover the five critical integration boundaries identified in the architecture

**Assessment**: PASS.

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- found #3158 (deferred scope resolution AC ambiguity), #3337 (architecture diagram divergence from spec), #2298 (config key semantic divergence). These informed the review but no recurring vision-specific misalignment pattern was detected.
- Stored: nothing novel to store -- the observability gap is a one-time roadmap-vs-scope disconnect specific to W2-2's expanded definition in WAVE2-ROADMAP.md, not a generalizable pattern. The BearerValidator scope addition is a natural consequence of the W2-3 security model design (ASS-050) specifying trait signatures that bridge to vnc-021. Neither generalizes to a reusable vision alignment pattern.
