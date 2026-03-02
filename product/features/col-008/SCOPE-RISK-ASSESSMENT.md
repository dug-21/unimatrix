# Scope Risk Assessment: col-008

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | In-memory session state loss on server restart. SessionState (injection history, compaction payload) lives only in server process memory. If the MCP server restarts mid-session, all injection history is lost and the next PreCompact falls back to category-based lookups — a degraded but functional path. | Medium | Medium | Architect should ensure the briefing fallback produces useful output even with zero session context. The fallback is the primary experience for new sessions and post-restart sessions. |
| SR-02 | col-007 is still in implementation. col-008 extends col-007's server-side session state (CoAccessDedup) and ContextSearch handler. If col-007's implementation changes its session state design or ContextSearch handler shape, col-008's design may need adjustment. | Medium | Medium | Architect should design SessionRegistry to be additive to col-007 — wrapping CoAccessDedup rather than modifying its internals. This minimizes coupling to col-007's implementation details. |
| SR-03 | Token budget allocation is theoretical. The 2000-token budget split (decisions: 400t, context: 200t, injections: 600t, conventions: 400t) is from ASS-014 research with no empirical validation. Suboptimal allocation means agents lose critical context or get flooded with low-value content after compaction. | Medium | High | Define allocation as named constants. Accept that v1 allocation is a best guess. Plan to tune after observing real compaction events. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope adds session_id to ContextSearch wire message — a wire protocol change that affects col-007's hook handler. col-007's build_request() must populate session_id in the ContextSearch request for col-008's injection tracking to work. This creates a cross-feature integration point. | Medium | High | Architect should assess whether the session_id addition to ContextSearch is a col-008 change or a col-007 prerequisite. If col-007 is already implemented without it, col-008 must modify col-007's hook handler — expanding the change surface. |
| SR-05 | SessionRegistry replaces or wraps CoAccessDedup. The shared registry concept is clean architecturally but requires modifying col-007's existing code to thread SessionRegistry through the ContextSearch handler instead of (or in addition to) CoAccessDedup. | Medium | Medium | Architect should design SessionRegistry as backward-compatible — CoAccessDedup continues to work identically, SessionRegistry adds injection tracking alongside it. |
| SR-06 | No disk-based compaction cache (explicit non-goal). If the server is unavailable at PreCompact time, the agent gets zero context preservation. For the UserPromptSubmit case (col-007), this is fine — the agent just misses enrichment on one prompt. For PreCompact, the consequence is more severe — the agent loses all previously-injected context permanently for this compaction event. | Low | Low | Accept this risk for v1. Server unavailability during active sessions is rare (Claude Code starts the MCP server). The briefing fallback covers server-restart scenarios. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | CompactPayload handler needs AsyncEntryStore access for ID-based entry fetching and category lookups. The UDS listener's parameter list (already expanded by col-007 for ContextSearch) grows further. | Low | High | Follow col-007's ADR-001 pattern — pass additional Arc parameters. The signature is already large; one more parameter is mechanical. |
| SR-08 | Entry deletion or status change between injection and compaction. An entry injected in prompt 5 could be deprecated or quarantined by prompt 50. The compaction payload should skip quarantined entries and flag deprecated ones. | Low | Medium | CompactPayload handler should filter by status (exclude quarantined, include deprecated with a note). Same pattern as the search pipeline's quarantine exclusion. |

## Assumptions

1. **Claude Code fires PreCompact before compaction** (SCOPE.md "Background Research"). The hook has the opportunity to inject content into the compacted window via stdout. If Claude Code changes PreCompact semantics (e.g., fires after compaction), the feature is ineffective. Low risk — documented hook behavior.

2. **Session IDs are consistent across hook events** (SCOPE.md "Proposed Approach"). The same session_id appears in SessionStart, UserPromptSubmit, PreCompact, and Stop events. If Claude Code uses different session identifiers across events, injection tracking breaks. Low risk — verified in col-006 research.

3. **ID-based entry fetch is fast enough** (SCOPE.md "Background Research", ~1ms per entry). Fetching 20-30 entries by ID within the 50ms budget requires <30ms. At ~1ms per entry this is feasible but leaves little margin for sort + format. Risk increases with large injection histories. Low risk — typical sessions inject 30-60 unique entries.

4. **The fallback path (category-based lookup) produces useful context** (SCOPE.md "Proposed Approach"). If the knowledge base has no entries with category "decision" or "convention" matching the session's feature, the fallback returns nothing. Medium risk — depends on knowledge base population.

## Design Recommendations

1. **(SR-01, SR-03)** Architect should ensure the fallback path and budget allocation are designed together. The fallback allocates the full 2000-token budget differently (no injection history section) — it needs its own allocation constants.

2. **(SR-02, SR-04, SR-05)** Architect should design the SessionRegistry and ContextSearch session_id changes as a clean layer on top of col-007, not interleaved with it. If col-007 is already merged, the changes are additive modifications.

3. **(SR-08)** Spec writer should include entry status filtering as a constraint on the CompactPayload handler — quarantined entries excluded, deprecated entries included with indicator.
