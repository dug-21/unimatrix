# Scope Risk Assessment: vnc-008

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Rust module system does not enforce import direction between sibling modules (`mcp/` importing from `uds/`, or transports importing foundation crates directly). Visibility enforcement is convention-only. | Med | Med | Architect should define concrete `pub(crate)` boundaries per module group. Consider a CI lint that greps for disallowed imports. |
| SR-02 | `rmcp` `#[tool]` macro constrains handler signatures to `(&self, Parameters<T>) -> Result<CallToolResult, ErrorData>` — ToolContext cannot be injected as a parameter, only constructed inside the handler. | Med | High | Architect should design ToolContext as a helper method on UnimatrixServer (`self.build_tool_context()`), not an injected dependency. |
| SR-03 | Moving 20+ modules simultaneously changes every `use crate::` import path in the crate — high risk of merge conflicts with any concurrent work on the server crate. | High | Med | Architect should recommend a migration ordering and re-export strategy to minimize import churn. Consider temporary re-exports from old paths. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `SessionWrite` capability is new semantics — defining what it permits vs. what `Write` permits requires precise enumeration. Ambiguity could block UDS operations or over-grant access. | High | Med | Spec writer should enumerate every UDS operation and classify it as Read, Search, SessionWrite, or Write. The boundary must be explicit, not inferred. |
| SR-05 | StatusService extraction pulls 628 lines from tools.rs but context_status uses `redb::ReadableTable` directly and `Store` internal tables (ENTRIES, COUNTERS, etc.) — this creates a new direct-storage coupling path in services/. | Med | High | Architect should decide: does StatusService access Store through existing public methods only, or does it inherit the current direct-table-scan pattern? Either choice has trade-offs (API surface vs. scope). |
| SR-06 | "Pure restructuring" claim conflicts with SessionWrite capability introduction — capability enforcement is a behavioral change for UDS operations that were previously unrestricted. | Med | Low | Scope already acknowledges this exception (AC-25 note). Spec writer should enumerate which UDS operations will be newly restricted. |
| SR-07 | `shared/` module vs ToolContext in `mcp/` — the human defers to architect, but if ToolContext contains types used by both transports (e.g., AuditContext construction helpers), placing it in `mcp/` creates a cross-transport dependency. | Low | Med | Architect should assess whether any ToolContext types are transport-agnostic. If so, `shared/` or placing them in `services/` may be cleaner. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | vnc-008 designs against "post-vnc-007 codebase" but vnc-007 is still being implemented. If vnc-007 changes the services/ module structure, vnc-008 design may be invalid. | Med | Med | Design should reference the vnc-007 SCOPE.md and ARCHITECTURE.md as the expected baseline. Architect should document assumptions about post-vnc-007 state explicitly. |
| SR-09 | Test migration across module boundaries: tests in `tools.rs` may reference private helpers or types that become unreachable after the module move. | Med | High | Architect should audit test dependencies on module-private items. Tests that depend on `pub(crate)` items must move to the same module group. |
| SR-10 | `response.rs` split into 4 files requires deciding which shared types/helpers go in `mod.rs` vs. individual files. Incorrect partitioning leads to circular imports within the sub-module. | Low | Med | Architect should map the dependency graph within response.rs before splitting. Shared types (ResponseFormat, format_timestamp) go in mod.rs. |

## Assumptions

1. **vnc-006 and vnc-007 have landed** and the services/ module contains SearchService, StoreService, ConfidenceService, BriefingService, and SecurityGateway. (Ref: SCOPE.md, "Post vnc-007 baseline")
2. **No concurrent server crate work** during vnc-008 implementation — the mass module moves create high merge conflict risk. (Ref: SR-03)
3. **UDS capability enforcement is additive** — no existing UDS operation that should succeed will start failing, except for operations that should never have been accessible via UDS. (Ref: SCOPE.md, Constraint 1)

## Design Recommendations

1. **(SR-02, SR-03)** Architect should design ToolContext as a method on UnimatrixServer and define a concrete migration ordering (infra/ first, then mcp/, then uds/) with temporary re-exports to avoid a single massive import rewrite.
2. **(SR-04, SR-06)** Spec writer should produce an exhaustive UDS operation capability matrix showing current (unrestricted) vs. proposed (SessionWrite-bounded) behavior for every operation.
3. **(SR-05)** Architect must decide whether StatusService inherits direct-table access or goes through Store public API. Document as an ADR — this affects the database replacement constraint.
4. **(SR-09, SR-10)** Architect should map module-private dependencies before proposing the final file layout.
