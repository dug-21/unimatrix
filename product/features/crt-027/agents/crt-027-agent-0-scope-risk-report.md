# Agent Report: crt-027-agent-0-scope-risk

## Output
- Produced: `product/features/crt-027/SCOPE-RISK-ASSESSMENT.md`
- Line count: 38 (under 100-line constraint)

## Risk Summary
| Severity | Count |
|----------|-------|
| High     | 3 (SR-01, SR-03, SR-04) |
| Medium   | 5 (SR-02, SR-05, SR-06, SR-08, SR-09) |
| Low      | 1 (SR-07) |

## Top 3 Risks for Architect/Spec Attention

1. **SR-01 (High/Med)** — SubagentStart stdout injection behavior in Claude Code is unconfirmed at scope time. SCOPE.md asserts Claude Code reads SubagentStart hook stdout and injects it into the subagent context before its first token. No ASS spike or documentation citation supports this. If the behavior is not present, WA-4a delivers zero user value. Must be verified before architecture is finalized.

2. **SR-04 (High/High)** — `format_compaction_payload` has 10+ unit tests asserting on `CompactionCategories` (decisions/injections/conventions). These cover budget enforcement, UTF-8, histogram rendering, and token limits. The format change removes the category structure but not the underlying invariants. Deleting the tests without replacing them is an AC-15 violation. Spec must enumerate which invariants survive by name.

3. **SR-03 (High/Med)** — `BriefingService` holds `EffectivenessStateHandle` (Arc<Mutex<EffectivenessSnapshot>>) wired in at `ServiceLayer` construction. Removing `BriefingService` without migrating this dependency into `IndexBriefingService` silently breaks effectiveness-based ranking in the new briefing path. The architect must specify the new service's construction signature.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection", "outcome rework hook wire protocol", "risk pattern", "SubagentStart hook stdout injection", "BriefingService migration caller removal", "mcp-briefing feature flag" — found ADR-001 (feature-gated MCP briefing flag, entries #269/#283), SubagentStart routing pattern (#3230), EffectivenessStateHandle sharing pattern (#3026, #2938), no directly relevant prior gate failures for this domain.
- Stored: nothing novel to store — risks are feature-specific to crt-027's unique combination of hook behavior assumptions and service deletion scope. No cross-feature pattern visible yet.
