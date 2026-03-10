# Agent Report: col-020b-agent-2-spec

## Status: COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-020b/specification/SPECIFICATION.md`

## Key Decisions

1. **SR-01 resolution (serde alias directionality):** Accepted unidirectional compat (old JSON -> new types works; new JSON -> old types drops renamed fields). Justified because RetrospectiveReport is ephemeral MCP output, never persisted cross-version. Documented explicitly in Backward Compatibility Requirements section.

2. **classify_tool completeness:** Specified that `context_briefing`, `context_status`, `context_enroll`, `context_retrospective` remain in `other` category. Only retrieval, storage, and curation tools get dedicated categories. This keeps the semantic model clean -- administrative tools are not knowledge flow.

3. **delivery_count vs cross_session_count:** Specified delivery_count as the primary metric (all unique entries delivered, any session count) with cross_session_count as a sub-metric. This fixes the #193 symptom where single-session features always got 0 reuse. The invariant `cross_session_count <= delivery_count` is explicit.

4. **category_gaps semantics shifted:** Gaps now measure zero-delivery (not zero-cross-session-reuse). This is more actionable -- a category with active entries that were never delivered to any session is a stronger signal than one that was delivered but only to one session.

5. **#193 root cause time-boxed (SR-03):** Specified as an investigation task with fallback to separate issue. The normalization fixes, field renames, and tests are independently valuable and ship regardless of root cause resolution.

6. **No infra-001 tests in scope (SR-04):** Specified Rust-only tests with realistic MCP-prefixed inputs. Infra-001 integration testing deferred to follow-up. This keeps the feature small.

## Self-Check Results
- [x] SPECIFICATION.md covers all 16 acceptance criteria from SCOPE.md (AC-01 through AC-16)
- [x] Every functional requirement is testable (verification method specified for each AC)
- [x] Non-functional requirements include measurable targets (O(1) normalization, zero new deps)
- [x] Domain Models section defines key terms (8 terms defined)
- [x] NOT in scope section is explicit (8 exclusions)
- [x] Output file in `product/features/col-020b/specification/` only
- [x] No placeholder or TBD sections -- unknowns captured as OQ-01 and OQ-02

## Open Questions
- OQ-01: #193 root cause needs investigation during implementation
- OQ-02: Confirm no persisted RetrospectiveReport instances exist
