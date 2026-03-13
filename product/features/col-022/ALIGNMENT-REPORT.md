# Alignment Report: col-022

> Reviewed: 2026-03-13
> Artifacts reviewed:
>   - product/features/col-022/architecture/ARCHITECTURE.md
>   - product/features/col-022/specification/SPECIFICATION.md
>   - product/features/col-022/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly supports hook-driven delivery and observation pipeline integrity |
| Milestone Fit | PASS | Belongs to Activity Intelligence milestone; addresses documented attribution failures |
| Scope Gaps | PASS | All 15 acceptance criteria addressed across source documents |
| Scope Additions | VARIANCE | Architecture introduces force-set semantics (ADR-002) that contradict SCOPE's first-writer-wins requirement |
| Architecture Consistency | WARN | FR-19 `was_set` response field is architecturally undeliverable per the architecture's own Open Question 2 |
| Risk Completeness | PASS | 12 risks, 29 scenarios, full scope-risk traceability matrix |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `set_feature_force` (ADR-002) | Architecture introduces force-set/overwrite semantics not requested in SCOPE. SCOPE AC-03 explicitly requires first-writer-wins preservation. |
| Addition | `SetFeatureResult` enum with `Overridden` variant | Architecture defines a three-variant result type (`Set`, `AlreadyMatches`, `Overridden`) where SCOPE only defines set-or-not behavior. |
| Simplification | Schema v11->v12 (not v10->v11) | Architecture specifies v11->v12, consistent with current schema version. Acceptable. |
| Simplification | `was_set` as acknowledgment-only | Specification FR-19 defines `was_set` boolean but Architecture acknowledges MCP tool cannot know actual attribution state. Risk strategy (R-08) correctly identifies this gap. Acceptable if response is documented as best-effort. |

## Variances Requiring Approval

### 1. Force-Set Semantics Override First-Writer-Wins (VARIANCE)

**What**: The architecture (ADR-002) introduces `SessionRegistry::set_feature_force()` which overwrites existing feature_cycle attribution. This replaces the `set_feature_if_absent` semantic that SCOPE explicitly requires. The specification is internally contradictory: FR-12 states "set_feature_if_absent semantic is preserved" while Constraint 2 says "it wins by being first, not by being privileged," yet the architecture's force-set means it wins by being privileged (explicit over heuristic).

**SCOPE references**:
- AC-03: "If a session already has a non-NULL feature_cycle, calling context_cycle(type: start, topic: Y) does NOT overwrite it (first-writer-wins semantic preserved)"
- Constraint 3: "set_feature_if_absent semantics: Must be preserved. This is the foundational invariant for one-session-one-feature (Unimatrix #1067)."
- Resolved Decision 3: "A subsequent cycle_start in the same session is a no-op (first-writer-wins)."

**Architecture references**:
- ADR-002: "Force-set semantic for explicit cycle_start... Explicit signal must win over heuristic; resolves SR-01 race condition"
- Integration Surface: `SessionRegistry::set_feature_force` with `SetFeatureResult::Overridden { previous: String }`
- Risk R-01 acknowledges: "Force-set overwrites correct heuristic attribution when agent passes wrong topic"

**Why it matters**: This changes a load-bearing invariant (#1067: one-session-one-feature enforced by first-writer-wins). The SCOPE-RISK-ASSESSMENT (SR-01) recommended the architect consider force-set OR protocol ordering. The architect chose force-set, which resolves the race condition but violates three explicit SCOPE statements. The specification failed to reconcile -- FR-12 claims the semantic is preserved while the architecture contradicts this.

**Recommendation**: Human must decide:
- **(A) Accept force-set**: Update SCOPE AC-03, Constraint 3, and Resolved Decision 3 to reflect the new semantic. The specification FR-12 must be corrected to say "set_feature_if_absent is replaced by set_feature_force for explicit cycle_start events." Accept the new risk profile (R-01: wrong-topic overwrite).
- **(B) Reject force-set, keep first-writer-wins**: Revert architecture to use `set_feature_if_absent`. Accept that SM agents must call `context_cycle(start)` before any file-touching tool calls (protocol ordering solution from SR-01). This is simpler but requires the follow-up protocol integration to enforce ordering.

### 2. Specification FR-19 `was_set` Field Inconsistency (WARN)

**What**: The specification (FR-19) defines a `was_set` boolean in the MCP tool response for `type: "start"`. The architecture (Open Question 2) explicitly states: "The MCP server does not have session_id in the tool call context, so the response cannot indicate whether attribution actually succeeded." The risk strategy (R-08, rated High likelihood) identifies this as a disconnect.

**SCOPE reference**: AC-05 says "response indicates whether the feature_cycle was set."

**Architecture reference**: "The tool response should acknowledge parameter acceptance only."

**Why it matters**: If force-set is accepted (Variance 1), the MCP tool still cannot know the hook-side outcome. If first-writer-wins is kept, the tool equally cannot know. Either way, `was_set` is architecturally undeliverable as defined. The specification Workflow 1 shows `was_set: true` and Workflow 2 shows `was_set: false`, but neither can be determined by the MCP tool.

**Recommendation**: Redefine `was_set` in the specification to mean "parameters were valid and the cycle_start event was dispatched" (not "attribution actually succeeded"). Update AC-05 to match. This aligns with the architecture's acknowledgment-only design. The tool description should document that attribution confirmation requires `context_retrospective`.

## Detailed Findings

### Vision Alignment

col-022 directly supports three vision pillars:

1. **Hook-driven delivery** (PRODUCT-VISION.md line 10-12): The feature's core architecture routes attribution through the PreToolUse hook, keeping the MCP server session-unaware. This is consistent with the vision's hook-driven model.

2. **Invisible delivery** (PRODUCT-VISION.md line 5-6): Explicit cycle declaration makes the observation pipeline more reliable, which feeds the self-learning pipeline that enables invisible delivery. Without correct attribution, retrospectives return empty data, breaking the learning loop.

3. **Auditable knowledge lifecycle** (PRODUCT-VISION.md line 17): Feature cycle attribution is the link between observations and feature-level analysis. Fixing attribution gaps strengthens the audit trail.

No vision concerns.

### Milestone Fit

col-022 is part of the Activity Intelligence milestone (PRODUCT-VISION.md lines 58-83). The milestone description states: "sessions have no topic attribution" as a problem to fix. col-022 directly addresses this with explicit attribution.

The feature is listed under Wave 1 conceptually (fixing the data pipeline) even though it was not originally enumerated in the roadmap waves. This is acceptable -- the problem statement (worktree-isolated subagents, single-spawn model, mixed-signal sessions) emerged from operational experience after the milestone was scoped.

No milestone concerns.

### Architecture Review

The architecture is well-structured with five clear components (C1-C5), explicit integration points, and a complete error boundary table. Key strengths:

- **Shared validation (C5)** directly addresses SR-07 from the scope risk assessment.
- **RecordEvent reuse** follows the resolved decision from SCOPE (Option B).
- **Fire-and-forget consistency** matches existing patterns.
- **Component interaction diagrams** clearly show both cycle_start and cycle_stop data flows.

The force-set variance (ADR-002) is the only structural concern, covered in Variances above.

Open Question 1 (`is_valid_feature_id` visibility) is appropriately flagged for implementer decision. Neither option introduces risk.

### Specification Review

The specification is thorough: 23 functional requirements, 5 non-functional requirements, 15 acceptance criteria with verification methods, 4 user workflows, domain model glossary, and dependency mapping.

Strengths:
- Follow-up deliverables (SR-04) are explicitly defined with acceptance criteria.
- NOT in Scope section (items 1-8) maps cleanly to SCOPE non-goals.
- Knowledge Stewardship section documents pattern queries.

Issues:
- **FR-12 / Constraint 2 contradiction**: FR-12 says first-writer-wins is preserved; Constraint 2 and 8 describe ordering-based mitigation. But the architecture uses force-set. The specification did not reconcile these with the architecture's ADR-002 decision.
- **FR-19 `was_set` field**: Architecturally undeliverable as discussed in Variance 2.
- **Spec item 7 in NOT in Scope**: "Override/force-set semantics -- context_cycle(start) does not override existing attribution." This directly contradicts the architecture's ADR-002. The specification explicitly excludes what the architecture introduces.

### Risk Strategy Review

The risk-based test strategy is comprehensive:
- 12 risks identified with severity/likelihood ratings
- 29 test scenarios mapped to risks
- Full scope-risk traceability matrix (all 9 SR items mapped)
- Security risks analyzed (5 categories)
- Edge cases enumerated (9 cases)
- Failure modes documented (6 modes)

Strengths:
- R-01 correctly identifies the new risk introduced by force-set semantics
- R-08 correctly flags the `was_set` disconnect as high likelihood
- R-09 covers the MCP prefix matching risk (practical concern)
- Cross-crate SessionRecord coupling (R-03) is well-analyzed

The risk strategy is internally consistent with the architecture (it tests force-set behavior) even though this conflicts with SCOPE. This is expected -- the risk strategy tests what the architecture defines, and the alignment report flags the scope deviation.

## Knowledge Stewardship

- Queried: /query-patterns for vision alignment patterns -- found #226/#111 (Vision Guardian duties), #134 (scope discipline convention). No prior col-022-specific alignment patterns.
- Stored: nothing novel to store -- the force-set vs first-writer-wins variance is feature-specific (architect resolving a scope risk recommendation in a way that contradicts scope constraints). Does not generalize to a recurring pattern yet. If this pattern recurs (scope risk recommendations leading to scope-violating architecture decisions), it should be stored.
