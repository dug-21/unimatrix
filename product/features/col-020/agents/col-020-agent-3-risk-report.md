# Agent Report: col-020-agent-3-risk

## Task
Produce architecture-risk RISK-TEST-STRATEGY.md for col-020 (Multi-Session Retrospective).

## Status: COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-020/RISK-TEST-STRATEGY.md`

## Risk Summary
- **15 risks** identified across 3 priority tiers
- **High priority**: 7 risks, 22 test scenarios
- **Medium priority**: 5 risks, 15 test scenarios
- **Low priority**: 2 risks, 5 test scenarios (+ 1 noted as High in register but Low in summary -- corrected to 7 High)
- **Total**: 42 test scenarios

## Top Risks for Human Attention

1. **R-14 (High): New steps abort existing pipeline** -- The best-effort pattern (compute and attach if successful, log and leave None if not) is critical. If any new computation step propagates an error instead of catching it, the entire retrospective output is lost. This is the highest-impact failure mode.

2. **R-05 (High): Non-idempotent counter updates** -- ADR-002 addresses this with absolute-set, but the implementation must ensure set_topic_delivery_counters truly replaces (not adds). Double-run integration test is mandatory.

3. **R-12 (Med): Double-counting reuse across data sources** -- Entry appearing in both query_log and injection_log for the same cross-session pair must be deduplicated by entry ID. The spec says "distinct entry IDs" but the implementation must enforce this with a HashSet, not by counting rows.

## Open Questions
1. **Grep in file path mapping**: FR-01.4 in the spec lists Read/Edit/Write/Glob but omits Grep. ADR-004 and the architecture table both include Grep. Should Grep be in the mapping? (Likely yes -- spec oversight.)
2. **PreToolUse filtering**: FR-01.2 specifies PreToolUse events for tool_distribution, but the handler loads all observation records (Pre and Post). Who filters -- the caller or compute_session_summaries? Should be documented.

## Unimatrix References Used
- #383: ObservationSource trait independence (ADR-002 col-012)
- #646: Backward-compatible config extension via serde(default)
- #372: Named parameters for multi-column SQL in rusqlite
- #865: ADR-002 col-020 idempotent counter updates
