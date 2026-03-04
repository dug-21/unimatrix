# Alignment Report: vnc-009

> Reviewed: 2026-03-04
> Artifacts reviewed:
>   - product/features/vnc-009/architecture/ARCHITECTURE.md
>   - product/features/vnc-009/specification/SPECIFICATION.md
>   - product/features/vnc-009/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements Wave 4 convergence items from product vision |
| Milestone Fit | PASS | vnc-009 is the final Milestone 2 feature, completing the Vinculum phase |
| Scope Gaps | PASS | All 43 ACs from SCOPE.md addressed in architecture and specification |
| Scope Additions | WARN | ADR-001 introduces StatusReportJson intermediate struct not mentioned in SCOPE.md |
| Architecture Consistency | PASS | Follows established service layer patterns from vnc-006/007/008 |
| Risk Completeness | PASS | 12 risks, 50 scenarios, all 13 scope risks traced |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | StatusReportJson intermediate struct | Architecture (ADR-001) introduces a new struct not in SCOPE.md. SCOPE.md says "derive(Serialize) on StatusReport, replace json! with serde_json::to_value()". Architecture adds an intermediate StatusReportJson to preserve nested JSON structure. This is an implementation refinement, not a scope expansion — the nested JSON cannot be directly derived from the flat StatusReport struct. |
| Addition | Session ID validation (FR-02.5) | Specification adds length (256) and control char validation on session_id. Not explicitly in SCOPE.md acceptance criteria but consistent with existing S3 validation pattern. |
| Simplification | CallerId without ApiKey | ADR-003 defers `CallerId::ApiKey` variant. SCOPE.md listed it but the architect justifiably removed unused code. |

## Variances Requiring Approval

None. The scope additions are implementation refinements consistent with the approved approach.

## Detailed Findings

### Vision Alignment

The product vision describes vnc-009 as:

> **Wave 4 of server refactoring.** Unified UsageService (merge MCP usage recording with UDS injection logging). Session-aware MCP (optional `session_id` param). Rate limiting on search (300/hour per caller). `#[derive(Serialize)]` on StatusReport (eliminates ~130 lines of `json!` macros). UDS auth failure audit logging (closes F-23). Strategic — positions for future HTTP transport.

All five workstreams are present in the architecture and specification:
1. UsageService with `record_access` / `AccessSource` pattern (FR-01, AC-01 through AC-11)
2. Session-aware MCP with transport-prefixed IDs (FR-02, AC-12 through AC-18)
3. S2 rate limiting with CallerId-based exemptions (FR-03, AC-19 through AC-30)
4. StatusReport serialization modernization (FR-04, AC-31 through AC-35)
5. UDS auth failure audit (FR-05, AC-36 through AC-39)

The architect note ("service layer must not introduce new direct-storage coupling") is respected: UsageService delegates to existing Store methods, not new direct-storage paths.

### Milestone Fit

vnc-009 completes Milestone 2 (Vinculum Phase). The product vision Milestone 2 summary says:

> **Ships**: Agents can search, store, correct, and receive briefings. Knowledge accumulates across features. Server process reliability ensures stable long-running sessions. Config externalization enables multi-domain deployment. Service layer extraction unifies dual-path architecture, closes UDS security gaps, and positions for future transport additions (HTTP/API).

vnc-009 closes the final UDS security gaps (F-09 rate limiting, F-23 auth audit) and positions for future transports (session-aware services, typed CallerIds). This is the correct feature to complete Milestone 2.

The dependency chain is clean: vnc-006 -> vnc-007 -> vnc-008 -> vnc-009, each wave independent and shippable.

### Architecture Review

**Consistency with prior waves**: The architecture follows patterns established in vnc-006:
- New service added to ServiceLayer struct (like SearchService, StoreService)
- SecurityGateway augmented with new gates (like S1/S3/S4/S5)
- Fire-and-forget via spawn_blocking (like ConfidenceService)
- AuditContext threading through service calls

**4 ADRs**: All well-formed with Context/Decision/Consequences sections.
- ADR-001 (StatusReportJson) addresses a real design tension between flat struct and nested JSON
- ADR-002 (Lazy eviction) makes the right tradeoff for expected scale
- ADR-003 (Defer ApiKey) reduces dead code, correct given no HTTP transport
- ADR-004 (Session prefix strategy) addresses SR-06 scope risk cleanly

**Integration surface table**: Comprehensive. 18 integration points documented with types and sources.

### Specification Review

**AC coverage**: All 43 ACs from SCOPE.md appear in the specification's acceptance criteria table with verification methods.

**Functional requirements**: 6 FR groups, 35 individual requirements. Each is testable. FR-02.5 (session_id validation) is a justified addition using the existing S3 pattern.

**Non-functional requirements**: Performance (rate limiter <10us, fire-and-forget latency), memory (bounded at ~480KB), backward compatibility, testability.

**Domain models**: Clear entity definitions with relationship diagram.

**NOT in scope**: Mirrors SCOPE.md non-goals. Adds "OperationalEvent log (deferred to GH issue #89)" which was resolved during scope review.

### Risk Strategy Review

**12 risks identified**: 4 High, 4 Medium, 4 Low priority. This is proportional to the feature's complexity (5 workstreams touching 8 components).

**Scope risk traceability**: All 13 SR-XX risks from SCOPE-RISK-ASSESSMENT.md have entries in the traceability table. No gaps.

**Security risks**: F-09 (rate limiting), session ID injection, and F-23 (UDS auth audit) all assessed with untrusted input analysis and blast radius.

**Edge cases**: 11 edge cases documented including boundary conditions (session_id with `::`, rate limit at exactly 300), empty inputs, and auth failure without credentials.

**Failure modes**: 5 failure modes with expected behavior. Mutex poison recovery follows the vnc-004 pattern (`unwrap_or_else(|e| e.into_inner())`).

**Coverage**: 50 test scenarios across 12 risks. Adequate for the feature's scope.
