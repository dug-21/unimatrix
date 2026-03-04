# Scope Risk Assessment: vnc-007

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Cargo feature flag on MCP tool may interact poorly with rmcp's `#[tool]` macro — procedural macros and `#[cfg]` attributes have known ordering sensitivities | Med | Med | Architect should verify `#[cfg(feature = "mcp-briefing")]` composes correctly with rmcp's `#[tool(name = "context_briefing")]` macro on the handler method. May require gating the entire impl block or a wrapper function rather than the method directly. |
| SR-02 | BriefingService depends on SearchService for semantic search, but SearchService was designed for standalone search, not as an internal building block — its `ServiceSearchParams` may not expose the right knobs for briefing (e.g., fixed k=3 for briefing vs configurable k for general search) | Low | Med | Architect should assess whether BriefingService calls SearchService directly or uses the lower-level embed/vector/rerank components. If SearchService is used, verify its interface supports briefing's fixed k=3 and briefing-specific co-access boost semantics. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Behavioral equivalence for CompactPayload is hard to verify — the current UDS compaction uses per-section byte budgets (DECISION_BUDGET_BYTES, INJECTION_BUDGET_BYTES, CONVENTION_BUDGET_BYTES). BriefingService uses a token budget with UDS converting bytes to tokens (`bytes / 4`). Rounding differences in the byte-to-token conversion and per-section proportional allocation could change which entries fit in the budget. | High | Med | Architect should define an explicit budget allocation strategy within BriefingService that preserves the per-section proportions when section priorities are active. Spec should require snapshot tests comparing old vs new CompactPayload output for identical inputs. |
| SR-04 | The conditional S2 rate limiting (AC-28 through AC-32) adds a new ServiceError variant and SecurityGateway method. If included, it touches StoreService (vnc-006 code) in a way that could conflict with other concurrent work. | Low | Low | Architect should make the rate limiting decision early (include or defer) to avoid late-stage scope changes. If included, keep it self-contained in gateway.rs with minimal StoreService changes. |
| SR-05 | "duties" category remains in the allowlist but duties are removed from briefing. Future developers may be confused about why the category exists without briefing support. | Low | Low | Add a code comment on the allowlist entry explaining the deprecation rationale. No code change needed. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | CompactPayload rewiring changes the UDS hot path (PreCompact hook). Embedding is controlled by the caller via `include_semantic` param (CompactPayload sets `false`), so accidental embedding is unlikely. The risk is that BriefingService's `include_semantic=false` path introduces unnecessary overhead vs the current inline code — extra async hops, additional allocations, or unintended SearchService coupling. | Med | Low | Architect should verify that the `include_semantic=false` code path in BriefingService has zero SearchService/embedding involvement. The path should be a direct entry-fetch-and-budget-allocate pipeline with no indirection through search infrastructure. Test with latency assertions if feasible. |
| SR-07 | The `dispatch_unknown_returns_error` test (uds_listener.rs) currently uses `HookRequest::Briefing` as its test case for unknown requests. Wiring Briefing will break this test and the test needs a new "unknown" request type to exercise the catch-all. | Low | High | Trivial fix — pick a different unused variant or create a test-only variant. Flag it so it is not forgotten during implementation. |
| SR-08 | vnc-006 must be merged before vnc-007 starts. If vnc-006 implementation changes service interfaces (SearchService, SecurityGateway, AuditContext), vnc-007's design may need revision. | Med | Med | vnc-007 architecture phase should verify vnc-006 service interfaces against the SCOPE.md assumptions. If vnc-006 is still in-flight, defer vnc-007 architecture until vnc-006 interfaces are stable. |

## Assumptions

1. **vnc-006 service layer is stable** (Proposed Approach, Constraints #1): SCOPE.md assumes SearchService, SecurityGateway, AuditContext, and ServiceLayer exist with the interfaces documented in vnc-006 architecture. If vnc-006 changes these interfaces, BriefingService design must adapt.

2. **`HookRequest::Briefing` wire format is sufficient** (Goals #3): SCOPE.md assumes the existing `{ role, task, feature, max_tokens }` fields are enough for UDS-native briefing. If BriefingService needs additional parameters (e.g., session_id for co-access anchors), the wire protocol in unimatrix-engine must change.

3. **Duties removal has no downstream consumers** (Goals #2): SCOPE.md assumes no code outside tools.rs and response.rs reads the duties field. If external code (tests, integration scripts) depends on duties in briefing responses, removal will break them.

4. **rmcp supports `#[cfg]` on individual tool methods** (Goals #6): SCOPE.md assumes Cargo feature flags can gate individual tool handlers within a `#[tool_router]` impl block. If rmcp generates registration at the impl-block level, the feature flag may need to gate a larger code region.

## Design Recommendations

1. **SR-03, SR-06**: BriefingService behavior is caller-controlled via `include_semantic`. When `false`, the code path must be a direct entry-fetch-and-budget-allocate pipeline with zero SearchService/embedding involvement. When `true`, it invokes SearchService for embedding + HNSW + boosts. Architect should verify these are cleanly separated internal paths with no coupling between them. The token budget (with byte-to-token conversion for UDS) should preserve per-section proportional allocation to maintain CompactPayload equivalence.

2. **SR-01, Assumption 4**: Architect should prototype the feature flag mechanism early (before full design) to verify rmcp macro compatibility. If `#[cfg]` does not compose with `#[tool]`, document the workaround in the architecture.

3. **SR-08**: If vnc-006 is not yet merged at architecture time, architect should pin the design to vnc-006's ARCHITECTURE.md interfaces and note any assumptions that need revalidation post-merge.
