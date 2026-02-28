# Alignment Report: vnc-004

> Reviewed: 2026-02-28
> Artifacts reviewed:
>   - product/features/vnc-004/architecture/ARCHITECTURE.md
>   - product/features/vnc-004/specification/SPECIFICATION.md
>   - product/features/vnc-004/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Bug fix for existing M2 infrastructure; directly supports reliability of the knowledge engine |
| Milestone Fit | WARN | Feature ID vnc-004 conflicts with "Config Externalization" in product vision roadmap |
| Scope Gaps | PASS | All 6 fixes from SCOPE.md are addressed in architecture and specification |
| Scope Additions | PASS | No scope additions detected |
| Architecture Consistency | PASS | Changes confined to existing modules, no new patterns that conflict with established architecture |
| Risk Completeness | PASS | 10 risks identified covering all component interactions; scope risk traceability complete |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| N/A | — | All scope items covered, no gaps or additions detected |

## Variances Requiring Approval

1. **What**: Feature ID vnc-004 is listed in the product vision as "Config Externalization" but is being used here for "Server Process Reliability" (bug fix for #52).
   **Why it matters**: Feature ID reuse could cause confusion in tracking and roadmap references. The dependency graph in PRODUCT-VISION.md references vnc-004 as Config Externalization.
   **Recommendation**: Accept for now — this is a bug fix that is more urgent than config externalization. Update PRODUCT-VISION.md to assign a different ID to Config Externalization (e.g., vnc-005) in a subsequent session.

## Detailed Findings

### Vision Alignment

The product vision states Unimatrix is a "self-learning context engine" served via MCP. vnc-004 directly supports this by ensuring the MCP server can reliably start, run, and recover from failures. Without these fixes, the knowledge engine is effectively unavailable during extended sessions — contradicting the core value proposition of "trustworthy, correctable, and auditable" knowledge delivery.

The `#![forbid(unsafe_code)]` constraint is preserved. The `fs2` dependency is minimal and focused. No new patterns introduced that conflict with the established architecture.

### Milestone Fit

vnc-004 is an M2 (MCP Server) bug fix. The M2 milestone is marked as complete in the vision, so this is post-release maintenance. The feature appropriately targets the Vinculum phase infrastructure.

The WARN is for the ID conflict only — the work itself fits M2 perfectly.

### Architecture Review

The architecture is well-scoped:
- Single crate affected (`unimatrix-server`).
- No new modules — all changes in existing files plus a new struct in `pidfile.rs`.
- Component interactions are clearly documented with a flow diagram.
- Integration surface table provides concrete function signatures.
- Two ADRs with clear context/decision/consequences.

The `PidGuard` design (flock + PID write + drop cleanup) follows established Rust RAII patterns. The session timeout approach (ADR-002) is appropriately conservative.

### Specification Review

All 6 acceptance criteria from SCOPE.md are present as AC-01 through AC-06. Functional requirements FR-01 through FR-06 map directly to the 6 fixes. Non-functional requirements cover the `forbid(unsafe_code)` constraint, backward compatibility, and startup recovery time.

The "NOT in Scope" section appropriately excludes agent permissions (#46), Config Externalization, and Windows support.

### Risk Strategy Review

10 risks identified with 20 test scenarios. All scope risks (SR-01 through SR-07) are traced to architecture risks. The security assessment is proportionate — PID files and flock are low-risk attack surfaces in a local-only server.

The integration risks section correctly identifies the PidGuard ownership transfer and error variant exhaustiveness as key concerns. Edge cases are well-enumerated, particularly the `/proc` hidepid=2 and PID=0 scenarios.
