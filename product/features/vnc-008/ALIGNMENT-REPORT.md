# Alignment Report: vnc-008

> Reviewed: 2026-03-04
> Artifacts reviewed:
>   - product/features/vnc-008/architecture/ARCHITECTURE.md
>   - product/features/vnc-008/specification/SPECIFICATION.md
>   - product/features/vnc-008/RISK-TEST-STRATEGY.md
>   - product/features/vnc-008/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Implements vnc-008 as defined in PRODUCT-VISION.md |
| Milestone Fit | PASS | Milestone 2 (Vinculum Phase), Wave 3 after vnc-006/007 |
| Scope Gaps | PASS | All 30 acceptance criteria addressed |
| Scope Additions | WARN | UDS module not split into listener+handlers (simplification, documented) |
| Architecture Consistency | PASS | Extends vnc-006/007 service layer without replacing |
| Risk Completeness | PASS | 11 risks, 53 scenarios; all scope risks traced |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | UDS not split into listener.rs + handlers.rs | SCOPE.md proposed this split but specification excludes it ("single file sufficient for current size"). Acceptable — research doc proposed the split as optional, and the file is ~2,000 lines post-vnc-007. |
| Simplification | No `shared/` module | SCOPE.md listed this as open question Q3. Architecture resolves: ToolContext is MCP-specific, placed in `mcp/context.rs`. Human approved architect's discretion. |
| — | No gaps found | All SCOPE.md acceptance criteria (AC-01 through AC-30) have corresponding FRs in specification and components in architecture. |

## Variances Requiring Approval

None. All artifacts align with both the product vision and the approved scope. The two simplifications are within the scope's own open questions and non-goals.

## Detailed Findings

### Vision Alignment

PRODUCT-VISION.md defines vnc-008 as: "Wave 3 of server refactoring. Restructure 23 flat modules into services/, mcp/, uds/, infra/ groups. Split response.rs (2,550 lines) into mcp/response/ sub-module (4 files, Refactor #6 + #9). Extract ToolContext reducing 12 MCP handler ceremonies. Split context_status (628 lines) into StatusService. Introduce unified capability model: SessionWrite capability, UDS fixed to {Read, Search, SessionWrite} (closes F-26). Pure restructuring -- no behavioral changes."

The architecture delivers each element:
- **Module groups**: `services/`, `mcp/`, `uds/`, `infra/` with import direction rules (ARCHITECTURE.md, Post-Refactoring Module Layout)
- **response.rs split**: 5 files in `mcp/response/` (mod.rs, entries.rs, mutations.rs, status.rs, briefing.rs) — vision says "4 files" but mod.rs + 4 content files is the standard Rust pattern. The 4 content files match.
- **Refactor #6**: `format_status_change` generic function (ADR-003)
- **Refactor #9**: Deferred to vnc-009, consistent with the vision which lists it under vnc-009
- **ToolContext**: Extracted in `mcp/context.rs` (ADR-002)
- **StatusService**: `services/status.rs` with `compute_report()` and `run_maintenance()` (ADR-001)
- **SessionWrite**: `Capability::SessionWrite` variant, UDS fixed to `{Read, Search, SessionWrite}` (closes F-26)
- **Pure restructuring**: AC-24/25 verify behavioral equivalence; UDS capability enforcement is the only additive change

The vision architect note states: "the refactoring must not introduce new direct-storage coupling." The architecture addresses this with ADR-001: StatusService inherits existing direct-table access (a code move, not new coupling). No new storage paths are introduced. StoreService remains the sole write path.

**Status: PASS**

### Milestone Fit

vnc-008 is Milestone 2 (Vinculum Phase), Wave 3 in the dependency chain:
```
vnc-006: Service Layer + Security Gateway
  vnc-007: Briefing Unification
    vnc-008: Module Reorganization  <-- THIS FEATURE
      vnc-009: Cross-Path Convergence
```

The architecture respects wave independence (Constraint 5 in SCOPE.md, NFR-4.1 in specification). No forward dependencies on vnc-009. ADR-004 (sequential migration) ensures the feature can be delivered incrementally.

The features deferred to vnc-009 are correctly identified: UsageService, session-aware MCP, search rate limiting, `#[derive(Serialize)]` on StatusReport, UDS auth failure audit logging. These match the vision's vnc-009 definition.

**Status: PASS**

### Architecture Review

**Module structure**: Four groups (services, mcp, uds, infra) with clear import direction rules. The architecture diagram shows the allowed dependency graph. Disallowed imports are explicitly listed.

**ToolContext design (ADR-002)**: Resolves SCOPE.md open question Q1. ToolContext is constructed via `self.build_context()` on UnimatrixServer, respecting the rmcp `#[tool]` macro constraint. Capability checking is separated for security auditability. The struct definition lives in `mcp/context.rs`, construction in `server.rs` — this avoids circular imports.

**StatusService design (ADR-001)**: Resolves SCOPE.md open question about direct-table access (linked to SR-05). StatusService inherits direct-table scans. This is documented as a known exception, consistent with the "code move, not redesign" principle.

**Migration strategy (ADR-004)**: Resolves SCOPE.md open question Q4. Five sequential steps with temporary re-exports. Each step compiles independently. Addresses SR-03 (mass import churn).

**SessionWrite capability**: Clean addition to the Capability enum. UDS_CAPABILITIES constant in `uds/mod.rs`. Exhaustive operation-to-capability mapping table. No existing UDS functionality is removed — all current operations map to Read, Search, or SessionWrite.

**Integration surface**: 9 integration points with types and signatures. Sufficient for implementation agents.

**Concern check — no cross-transport imports**: Architecture explicitly disallows `mcp/ <-> uds/` imports. However, the current `tools.rs` imports `run_confidence_consumer` and `run_retrospective_consumer` from `uds_listener.rs`. The architecture notes these must move to `services/` or `infra/` — this is addressed in the risk strategy (R-07 integration risks section).

**Status: PASS**

### Specification Review

**FR coverage**: 6 functional requirement groups (FR-1 through FR-6) with 27 sub-requirements. All 30 SCOPE.md acceptance criteria are mapped to FRs and have verification methods.

**NFR coverage**: 4 groups (behavioral equivalence, test coverage, compilation, scope containment) with 10 sub-requirements. All measurable.

**Domain model**: Module Groups table, Capability Hierarchy, ToolContext Lifecycle. Key terms defined.

**User workflows**: Three workflows (add new MCP tool, add new UDS command, find a module). All trace through the module structure correctly.

**Constraint completeness**: 9 constraints covering rmcp, post-vnc-007 baseline, wave independence, scope containment, serde compatibility, and direct-storage coupling. All derived from SCOPE.md.

**NOT in scope**: 10 items explicitly excluded. Matches SCOPE.md non-goals plus architect's resolution of open questions.

**Status: PASS**

### Risk Strategy Review

**Coverage**: 11 risks identified, 53 test scenarios across 4 priority levels. Critical risk (R-01, import path breakage) has 5 scenarios — one per migration step.

**Security risks**: SessionWrite capability boundary assessed (untrusted input, blast radius, escalation path). Direct-table access in StatusService assessed. Module visibility limitations acknowledged.

**Scope risk traceability**: All 10 scope risks (SR-01 through SR-10) traced to architecture risks or marked as resolved. No orphan scope risks. SR-08 (vnc-007 baseline uncertainty) noted as mitigated by designing against vnc-007 artifacts.

**Integration risks**: Three specific integration concerns documented (import direction violations, service layer bypass, cross-transport coupling). All have concrete mitigation strategies.

**Edge cases**: 5 edge cases including empty database, serde backward compatibility, system agent with ToolContext, UDS Ping no-capability, and re-export type identity.

**Failure modes**: 4 failure modes with expected behavior and recovery. Includes migration failure recovery (git revert per step — enabled by ADR-004).

**Status: PASS**
