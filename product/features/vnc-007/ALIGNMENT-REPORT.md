# Alignment Report: vnc-007

> Reviewed: 2026-03-03
> Artifacts reviewed:
>   - product/features/vnc-007/architecture/ARCHITECTURE.md
>   - product/features/vnc-007/specification/SPECIFICATION.md
>   - product/features/vnc-007/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Architecture directly implements Wave 2 vision entry; supports three-leg boundary (files/Unimatrix/hooks) |
| Milestone Fit | PASS | Stays within Milestone 2 (Vinculum) scope; no forward dependencies on vnc-008/009 |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria are addressed in architecture and specification |
| Scope Additions | WARN | ADR-004 formalizes S2 rate limiting deferral; this is a legitimate simplification but the vision entry explicitly mentions S2 rate limiting for vnc-007 |
| Architecture Consistency | PASS | BriefingService design follows vnc-006 service layer patterns; clean caller-parameterized design |
| Risk Completeness | PASS | 10 risks covering all 8 scope risks with traceability; 22 test scenarios provide thorough coverage |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | S2 rate limiting deferred to vnc-009 | ADR-004 documents rationale: no overlap with BriefingService (read vs write path), vnc-009 can implement unified RateLimiter for both write and search rate limiting. SCOPE.md marked AC-28 through AC-32 as conditional with architect authority to defer. |
| Simplification | SessionRegister briefing not included | SCOPE.md listed this as a non-goal ("not a requirement, stretch only"). Architecture and specification correctly omit it. |

## Variances Requiring Approval

None. The S2 rate limiting deferral is a legitimate simplification exercising architect authority explicitly granted in SCOPE.md (Goals #7: "included if architect determines it fits scope; architect has authority to defer this to vnc-009"). No variances or failures require human approval.

## Detailed Findings

### Vision Alignment

The product vision entry for vnc-007 (PRODUCT-VISION.md line 65) states:

> **Wave 2 of server refactoring.** Extract BriefingService unifying `context_briefing` (MCP, 223 lines) and `handle_compact_payload` (UDS, 266 lines) behind one assembly service. Extract StoreService with S1 content scan hard-reject and S2 rate limiting on writes (60/hour per caller, closes F-09). Wire `HookRequest::Briefing` for UDS-native briefing delivery. Remove duties section (per col-011). Behavioral change — validated independently from vnc-006.

The architecture and specification deliver on each point:

1. **BriefingService extraction**: ARCHITECTURE.md defines `BriefingService` in `services/briefing.rs` with a caller-parameterized `assemble()` method. Both MCP and UDS paths delegate to it. PASS.

2. **StoreService S1 content scan**: Already delivered in vnc-006. The vision entry groups S1 and S2 under vnc-007 but S1 hard-reject was actually completed in vnc-006's StoreService. Not a gap — it is already done. PASS.

3. **S2 rate limiting**: Deferred to vnc-009 via ADR-004. The vision entry mentions it, but SCOPE.md gives the architect authority to defer. The rationale (no overlap with BriefingService, vnc-009 unified RateLimiter) is sound. WARN — the vision entry will need a minor update to reflect the deferral.

4. **HookRequest::Briefing wiring**: ARCHITECTURE.md section "UDS HookRequest::Briefing Flow" and SPECIFICATION.md FR-09 both address this. The dispatch handler delegates to BriefingService with `include_semantic=true`. PASS.

5. **Duties removal**: ARCHITECTURE.md section 6, SPECIFICATION.md FR-06, and AC-09 through AC-12 all address duties removal comprehensively. PASS.

6. **Behavioral change validated independently**: SPECIFICATION.md NFR-02 requires behavioral equivalence (same entries, same ordering) with snapshot tests. RISK-TEST-STRATEGY.md R-01 and R-09 provide specific test scenarios. PASS.

The architecture also supports the product vision's three-leg boundary:
- **Files** define process (agent defs remain sole authority for duties, per col-011 decision)
- **Unimatrix** holds expertise (BriefingService assembles it)
- **Hooks** connect them (CompactPayload and HookRequest::Briefing are hook-delivered briefings)

### Milestone Fit

vnc-007 is within Milestone 2 (Vinculum Phase). The architecture:
- Does NOT depend on vnc-008 module reorganization or vnc-009 cross-path convergence
- Does NOT introduce capabilities belonging to other milestones (no learning, no agent management, no UI)
- Does depend on vnc-006 (same milestone) which is explicitly listed as a prerequisite

The feature flag (`mcp-briefing`) prepares for future MCP tool removal but does not implement future milestone work. It is a clean architectural boundary. PASS.

### Architecture Review

**Service layer consistency**: BriefingService follows the same patterns as SearchService and StoreService from vnc-006:
- Holds `Arc<AsyncEntryStore>`, `SearchService` (Clone), `Arc<SecurityGateway>`
- Accepts `AuditContext` for audit trail
- Returns `Result<BriefingResult, ServiceError>`
- Registered in `ServiceLayer`

This consistency is aligned with the vision's architect note: "StoreService and `Store::insert_in_txn` should be the only paths to the database from the service layer." BriefingService accesses the store through `AsyncEntryStore` (read-only queries and fetches), not through direct database operations. PASS.

**Caller-parameterized design**: The `include_semantic` flag creates a clean separation between the embedding path and the fetch-only path. This is well-documented in ARCHITECTURE.md ("Critical invariant: When `include_semantic=false`, BriefingService performs zero embedding, zero vector search, and zero SearchService involvement"). PASS.

**Integration surface**: ARCHITECTURE.md provides a complete integration surface table (6 new types, 2 modified types, 1 new Cargo feature). The net change estimate (~280 new, ~480 removed = ~200 line reduction) is consistent with a refactoring that consolidates duplication. PASS.

**ADR quality**: Four ADRs with clear Context/Decision/Consequences structure:
- ADR-001: Feature gate mechanism (sound, with fallback plan)
- ADR-002: Delegate to SearchService (avoids duplication, gets security gates)
- ADR-003: Token budget with proportional allocation (preserves existing behavior)
- ADR-004: Rate limiting deferral (well-justified)

All ADRs are traceable to scope risks. PASS.

### Specification Review

**Functional requirements coverage**: 10 FRs (FR-01 through FR-10) map cleanly to SCOPE.md goals:
- FR-01 through FR-05: BriefingService assembly (Goals 1, SCOPE AC-01 through AC-08)
- FR-06: Duties removal (Goal 2, AC-09 through AC-12)
- FR-07: MCP rewiring (Goal 5, AC-13 through AC-17)
- FR-08: UDS CompactPayload rewiring (Goal 4, AC-18 through AC-21)
- FR-09: UDS HookRequest::Briefing (Goal 3, AC-22 through AC-24)
- FR-10: Feature flag (Goal 6, AC-25 through AC-27)

No SCOPE.md goal is unaddressed. PASS.

**Non-functional requirements**: NFR-01 (latency), NFR-02 (behavioral equivalence), NFR-03 (test coverage), NFR-04 (feature flag compilation) directly support the quality acceptance criteria (AC-33 through AC-37). PASS.

**Acceptance criteria alignment**: The specification's AC table matches SCOPE.md's AC table with verification methods added. AC-28 through AC-32 (conditional S2) are explicitly marked as deferred per ADR-004. No AC is missing. PASS.

**Domain model completeness**: 6 entities defined (BriefingService, BriefingParams, BriefingResult, InjectionSections, InjectionEntry, Briefing). Relationship diagram shows all data flows. Three user workflows cover the three entry points (MCP, CompactPayload, HookRequest::Briefing). PASS.

**Constraints**: 6 constraints in the specification match the 8 constraints in SCOPE.md (some are consolidated). The key constraint — no changes outside unimatrix-server and unimatrix-engine — is preserved. PASS.

### Risk Strategy Review

**Scope risk traceability**: All 8 scope risks (SR-01 through SR-08) are traced to architecture risks:
- SR-01 -> R-03 (feature flag + rmcp)
- SR-02 -> R-02 (SearchService interface)
- SR-03 -> R-01 (budget behavioral equivalence)
- SR-04 -> deferred (ADR-004)
- SR-05 -> R-07 (duties category confusion)
- SR-06 -> R-04 (CompactPayload latency)
- SR-07 -> R-10 (dispatch test breakage)
- SR-08 -> not applicable (vnc-006 interface stability)

No scope risk is unaddressed. PASS.

**Risk severity calibration**: The two High-severity risks (R-01: CompactPayload behavioral regression, R-04: injection history path latency) correctly target the highest-impact areas. R-01 gets 3 test scenarios including snapshot comparison. R-04 gets a code review check and a SearchService-panics-on-call isolation test. Proportionate. PASS.

**Edge case coverage**: 8 edge cases documented with expected behaviors, covering empty knowledge base, all-quarantined entries, zero budget, missing role/task, duplicate injection entries, large injection history, and feature tag with no matches. PASS.

**Security review**: The risk strategy correctly identifies BriefingService as read-only with bounded blast radius. Input validation via S3, entry IDs as u64 (no injection), compile-time feature flag (no runtime bypass). PASS.

**Coverage summary**: 10 risks, 22 test scenarios across High/Med/Low priorities. This is thorough for a refactoring feature that introduces behavioral changes. PASS.
