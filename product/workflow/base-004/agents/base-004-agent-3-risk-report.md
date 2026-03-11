# Agent Report: base-004-agent-3-risk (Risk Strategist)

## Task
Produce architecture-risk assessment for base-004 (Mandatory Knowledge Stewardship) in architecture-risk mode.

## Artifacts Read
- SCOPE.md, SCOPE-RISK-ASSESSMENT.md, ARCHITECTURE.md, SPECIFICATION.md
- ADR-001 through ADR-005
- Agent definition file listing (14 agents in .claude/agents/uni/)

## Artifacts Produced
- `/workspaces/unimatrix/product/workflow/base-004/RISK-TEST-STRATEGY.md`

## Key Findings

### Critical Risks (must resolve before implementation)

**R-01: Stewardship heading mismatch.** The Architecture (C2, ADR-002) defines the report block heading as `## Knowledge Stewardship`. The Specification (FR-04) defines it as `## Stewardship`. These are the exact same parsing contract but with different strings. Whichever heading agents use, the validator will fail to match if it expects the other. This must be resolved to a single canonical heading before implementation begins.

**R-10: Specification agent tier contradiction.** ADR-001 classifies `uni-specification` as read-only tier (no storage expected). The Specification FR-02 table says it stores `convention` entries via `/store-pattern`. These are mutually exclusive expectations. The implementer must be told which is authoritative.

### High Risks

**R-02: Bullet prefix format inconsistency.** Architecture C2 uses `- Stored:` bullet format. Specification FR-04 uses a markdown table with `| Stored | ... |` format. These are structurally different -- one is a bullet list, the other is a table. The validator's parsing logic depends on which format is chosen.

**R-05: Retro quality pass depends on feature_cycle tags that are recommended but not enforced.** The Architecture and Specification both acknowledge this gap (SR-06) but do not close it. The retro query may miss entries.

## Risk Summary
- Critical: 2 risks (R-01, R-10)
- High: 2 risks (R-02, R-05)
- Medium: 5 risks (R-03, R-04, R-06, R-09, R-12)
- Low: 3 risks (R-07, R-08, R-11)
- Total: 12 risks, 26 test scenarios

## Scope Risk Traceability
All 8 scope risks (SR-01 through SR-08) traced. 6 map to architecture risks. 2 resolved at deployment/scope level (SR-03, SR-08).

## Recommendations for Implementers
1. Resolve the heading string inconsistency (R-01) before writing any agent definitions. Pick one: `## Knowledge Stewardship` or `## Stewardship`.
2. Resolve the report format inconsistency (R-02) -- bullet list vs markdown table. Pick one format and use it everywhere.
3. Resolve the uni-specification tier (R-10) -- is it read-only or active-storage?
4. These three resolutions should be documented as errata or clarifications before Stage 3a begins.

## Knowledge Stewardship

- Queried: reviewed SCOPE-RISK-ASSESSMENT.md for SR-XX traceability
- Declined: Nothing novel to store -- risks are feature-specific and captured in RISK-TEST-STRATEGY.md, not generalizable patterns

## Status
COMPLETE
