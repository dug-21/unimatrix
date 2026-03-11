# Alignment Report: base-004 Mandatory Knowledge Stewardship

> Reviewed: 2026-03-11
> Artifacts reviewed:
>   - product/workflow/base-004/architecture/ARCHITECTURE.md
>   - product/workflow/base-004/specification/SPECIFICATION.md
>   - product/workflow/base-004/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Closes the knowledge feedback loop -- directly supports the self-learning expertise engine vision |
| Milestone Fit | PASS | Workflow-only feature; no milestone dependency conflicts |
| Scope Gaps | PASS | All 9 acceptance criteria addressed in source documents |
| Scope Additions | WARN | Bugfix protocol linkage (C6/FR-08) extends beyond SCOPE.md's explicit ask; specification agent reclassified from read-only to active-storage |
| Architecture Consistency | VARIANCE | Critical heading mismatch between Architecture and Specification; retro phase insertion point differs between documents |
| Risk Completeness | PASS | Risk strategy identifies both critical inconsistencies and provides actionable test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | FR-08: Bugfix protocol stewardship (FR-08a-d) | SCOPE.md mentions bugfix stewardship in Resolved Question #3 but does not list it as an acceptance criterion. Architecture adds C6 (bugfix protocol linkage) and Specification adds FR-08 with four sub-requirements. This is a reasonable extension of the scope's intent but was not explicitly requested. |
| Addition | FR-04: Table-based report format | SCOPE.md describes a bullet-based report format (`- Stored:`, `- Queried:`, `- Declined:`). Specification FR-04 changes this to a markdown table with `Action` and `Detail` columns. The table format is arguably better for parsing but differs from the scope's description. |
| Addition | uni-specification reclassified to active-storage | SCOPE.md Background Research lists uni-specification under "No stewardship at all." Architecture C1 classifies it as read-only. Specification FR-02 classifies it as active-storage (stores `convention` entries). Three documents give three different answers. |
| Simplification | No advisory rollout period | SCOPE.md does not mention rollout strategy. Specification explicitly states "No advisory/warn-only rollout period" (NOT in Scope #7). SR-07 recommended considering this. Accepted with rationale documented. |

## Variances Requiring Approval

### 1. Heading Mismatch Between Architecture and Specification (VARIANCE)

**What**: Architecture C2 defines the agent report stewardship block heading as `## Knowledge Stewardship`. Specification FR-04 defines it as `## Stewardship`. These are different strings. The validator will parse one specific heading. If documents disagree, implementers will build inconsistent artifacts.

**Why it matters**: This is exactly the failure mode that Scope Risk SR-02 warned about -- brittle parsing of agent report text. The Risk-Test-Strategy correctly identifies this as Critical risk R-01. If not resolved before implementation, every agent report will either match one document or the other, causing systematic false FAILs at the validator gate.

**Recommendation**: Resolve before implementation. Pick one heading string and update both Architecture C2 and Specification FR-04 to use it. The SCOPE.md uses `## Knowledge Stewardship` (in AC-01, AC-02, proposed approach). Recommend standardizing on `## Knowledge Stewardship` for consistency with SCOPE.md and the existing architect agent pattern.

### 2. Retro Quality Pass Phase Insertion Point Differs (VARIANCE)

**What**: Architecture C5 inserts the stewardship quality review between Phase 2 (Pattern & Procedure Extraction) and Phase 3 (ADR Supersession), calling it "Phase 2b." Specification FR-07 inserts it between Phase 1 (data gathering) and Phase 2 (pattern extraction), calling it "Phase 1b."

**Why it matters**: The insertion point determines what data is available. If inserted as Phase 1b (before pattern extraction), the quality pass reviews entries stored during the feature cycle before the retro itself extracts new patterns. If inserted as Phase 2b (after pattern extraction), the quality pass reviews both agent-stored entries and retro-extracted entries. These are different behaviors with different outcomes for entry curation.

**Recommendation**: Resolve before implementation. Phase 1b (Specification's position) is likely correct -- the quality pass should review agent-stored entries before the retro extracts new ones, to avoid reviewing the retro's own output. Update Architecture C5 to match.

### 3. Uni-Specification Agent Tier Classification Inconsistency (VARIANCE)

**What**: Three documents give three different classifications for the `uni-specification` agent:
- SCOPE.md Background Research: "No stewardship at all"
- Architecture C1 tier table: Read-only tier
- Specification FR-02: Active-storage (stores `convention` entries via `/store-pattern`)

**Why it matters**: The validator gate checks enforce different rules per tier. If uni-specification is read-only, it only needs a `Queried` row. If active-storage, it needs `Stored` or `Declined`. Implementers cannot build correct gate checks without a definitive answer. The Risk-Test-Strategy correctly flags this as Critical risk R-10.

**Recommendation**: Resolve before implementation. The SCOPE.md Resolved Questions section does not address uni-specification. The human should decide: does the specification agent produce generalizable knowledge (AC interpretation precedents) or merely compile existing requirements? If active-storage, update Architecture C1. If read-only, update Specification FR-02.

## Detailed Findings

### Vision Alignment

The product vision states: "Unimatrix is a self-learning expertise engine" that "captures the knowledge that emerges from doing work." The vision explicitly notes that "Knowledge that evolves through feature delivery -- coding patterns, interface contracts, testing procedures, architectural decisions -- lives in Unimatrix."

base-004 directly addresses this by closing the feedback loop between agents doing work and knowledge being stored. The SCOPE.md problem statement -- "the feedback loop is broken" with "53 active entries (all ADRs) and empty active categories for duties, patterns, procedures, and lessons" -- identifies a real gap between the vision and current state.

The three-layer approach (agent guidance, gate enforcement, retro curation) aligns with the vision's emphasis on trustworthy, correctable, auditable knowledge. The retro quality pass supports the vision's "ever-improving" principle.

No vision misalignment detected.

### Milestone Fit

base-004 is a workflow feature (file-only changes) that does not belong to a specific milestone. It is infrastructure improvement that benefits all future feature delivery regardless of which milestone is active. The "Activity Intelligence" milestone is the current focus per the vision document, and base-004 does not conflict with or depend on any Activity Intelligence features.

The feature correctly avoids scope creep into Rust code, schema changes, or MCP tool modifications -- all of which would require milestone coordination.

No milestone concern.

### Architecture Review

The architecture is well-structured with 6 clearly defined components (C1-C6), a component interaction diagram, and 5 ADRs. The three-tier agent classification (active-storage, read-only, exempt) is a sound design that scales the stewardship burden proportionally.

**Issues found**:

1. **Heading mismatch** (see Variance #1 above): Architecture C2 uses `## Knowledge Stewardship` for the report block heading. This must match the Specification and all agent definitions exactly.

2. **Retro insertion point** (see Variance #2 above): Architecture C5 says "Phase 2b" while Specification says "Phase 1b."

3. **Open Question #1 (feature_cycle injection)**: Architecture correctly notes that skills are markdown instructions, not executable code, so there is no automatic injection. The skill must instruct the agent to include the tag. This is consistent with the Specification's NOT in Scope #8.

4. **Open Question #3 (CLAUDE.md)**: Architecture recommends relaxing the "no CLAUDE.md changes" constraint for `/store-pattern` discoverability. This is a reasonable recommendation but is outside the current scope. No action required for this review.

### Specification Review

The specification is thorough with 11 functional requirements, 4 non-functional requirements, 5 user workflows, and detailed acceptance criteria verification methods.

**Issues found**:

1. **FR-04 report format differs from SCOPE.md**: SCOPE.md describes a bullet-list format (`- Stored:`, `- Queried:`, `- Declined:`). Specification FR-04 changes this to a markdown table with `Action` and `Detail` columns. The table format is a design improvement (more parseable) but represents a deviation from the scope's description. This is a minor scope addition -- the intent is preserved, the mechanism changed.

2. **FR-02 uni-specification classification** (see Variance #3 above): FR-02 lists uni-specification as active-storage with `convention` category, contradicting Architecture's read-only classification.

3. **FR-08 bugfix protocol scope**: FR-08a through FR-08d add four sub-requirements for bugfix protocol stewardship. SCOPE.md's Resolved Question #3 says "YES, gate checks include stewardship compliance for investigator and rust-dev" in bugfix contexts, and mentions causal feature linkage. This is a reasonable elaboration of the scope's intent, but the level of detail (especially FR-08d adding a bugfix validation gate check not mentioned in SCOPE.md) extends beyond the explicit acceptance criteria. Since SCOPE.md's Resolved Questions section endorsed this direction, this is WARN not VARIANCE.

4. **Specification NOT in Scope #7**: "No advisory/warn-only rollout period" explicitly rejects the approach recommended by SR-07. The rationale is documented: "The quality bar is low (store or explicitly decline); agents that cannot meet it have a genuine gap." This is a conscious design decision, not an oversight.

### Risk Strategy Review

The Risk-Test-Strategy is strong. It identifies 12 risks with appropriate severity/likelihood ratings, maps each to test scenarios, and includes integration risks, edge cases, and security assessment.

**Strengths**:
- Correctly identifies the two critical risks (R-01 heading mismatch, R-10 specification agent tier contradiction) that this alignment review also flags as variances.
- Scope risk traceability table maps all 8 scope risks to architecture-level mitigations.
- Edge cases are practical and cover real failure modes (empty stewardship table, multiple rust-dev agents, vision guardian not spawned).
- Security assessment is appropriately minimal for a markdown-only feature.

**One gap**: The risk strategy identifies the heading mismatch and tier contradiction but does not escalate them as "must resolve before implementation" blockers. It treats them as test scenarios to verify rather than design defects to fix. Given that these are inconsistencies in the design documents themselves, they should be resolved in the source documents before implementation begins, not discovered during testing.
