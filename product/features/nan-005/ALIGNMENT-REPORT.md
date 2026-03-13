# Alignment Report: nan-005

> Reviewed: 2026-03-13
> Artifacts reviewed:
>   - product/features/nan-005/architecture/ARCHITECTURE.md
>   - product/features/nan-005/specification/SPECIFICATION.md
>   - product/features/nan-005/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly delivers on the "Platform Hardening & Release" milestone narrative: comprehensive README and automatic documentation maintenance |
| Milestone Fit | PASS | Correctly targets Platform Hardening. No Graph Enablement or Activity Intelligence capabilities are pulled in. |
| Scope Gaps | WARN | Architecture and Specification state 11 MCP tools; SCOPE.md states 12. The discrepancy is acknowledged (OQ-01) but not resolved in documents. The correct count must be verified before authoring. |
| Scope Additions | WARN | SPECIFICATION.md FR-02a adds extensive technical detail to Core Capabilities (InfoNCE loss, EWC++ regularization, six-factor scoring formula weights, 1.5-sigma outlier detection) that exceeds the "capability-first, not implementation" framing directive in SCOPE.md. |
| Architecture Consistency | PASS | Architecture is minimal and appropriately scoped for a documentation-only feature. Three components (README, uni-docs agent, protocol mod) are well-defined with clear interactions, integration points, and ADR-backed decisions. |
| Risk Completeness | PASS | Risk register is comprehensive, well-prioritized, and fully traced to scope risks and spec requirements. All critical and high risks have concrete test scenarios. |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | Tool count resolution | SCOPE.md says 12 MCP tools; ARCHITECTURE.md Component 1 fact table says 11; SPECIFICATION.md FR-04a says "12 tools" in the heading then notes "wait — that is 11." OQ-01 is raised but left as an open question. The source documents do not definitively resolve the count and the README cannot be authored correctly without this. |
| Gap | Acknowledgments section preservation | SCOPE.md does not mention the acknowledgments section. SPECIFICATION.md FR-01e adds a requirement that the acknowledgments section crediting claude-flow and ruvnet be preserved. This is an addition not in SCOPE.md — a low-risk addition, but it surfaces a gap in SCOPE.md's completeness. |
| Addition | Extensive internal-detail framing in Core Capabilities | SCOPE.md states: "Core capabilities front and center: self-learning knowledge engine, MicroLoRA adaptive embeddings, semantic search with confidence scoring..." and emphasizes "what users experience, not how it works internally." SPECIFICATION.md FR-02a specifies that the Core Capabilities section document the "six-factor additive weighted composite (base, usage, freshness, helpfulness, correction quality, creator trust — weights sum to 0.92, co-access affinity 0.08 at query time)" and the "search re-ranking formula 0.85 * similarity + 0.15 * confidence + co-access boost (max 0.03) + provenance boost (0.02 for lesson-learned)." These formula details are implementation internals that SCOPE.md explicitly excluded under "Architecture deep-dives: The 8-crate workspace structure, scoring formula weights, and detection rule internals are implementation details." |
| Addition | Near-duplicate detection threshold in operational guidance | SCOPE.md lists 6 operational constraints for the operational guidance section. SPECIFICATION.md FR-10a adds a 7th: "Near-duplicate detection threshold: Entries with cosine similarity ≥ 0.92 to existing entries are rejected as duplicates." This is a sensible addition and it is user-actionable guidance, but it was not in SCOPE.md's list. |
| Simplification | `maintain` parameter behavior change note | SPECIFICATION.md FR-04e states `maintain` is silently ignored since col-013 (background tick handles maintenance). SCOPE.md does not anticipate this behavioral change — the SCOPE.md tool table describes `context_status` as "Health metrics and maintenance / maintain=true behavior." The spec correctly identifies and documents the behavioral reality. Rationale: the spec reflects the live codebase state, not the SCOPE.md expectation. This is appropriate correction, not a scope addition. |

---

## Variances Requiring Approval

No FY FAIL classifications found. Two items require human review:

### 1. [WARN] Tool Count Open Question Not Resolved in Source Documents

**What**: SCOPE.md states "12 MCP tools." ARCHITECTURE.md Component 1 fact table lists 11 in the "Verified Value" column and its integration surface table has 11 entries. SPECIFICATION.md FR-04a header says "all 12 tools," then immediately flags the contradiction and says "that is 11" as an inline note, leaving it as OQ-01. The three source documents do not arrive at a single authoritative number.

**Why it matters**: The README tool reference table is the most reader-visible factual claim in the document. If the wrong number ships, AC-02 and NFR-02 fail on the first deliverable of the Platform Hardening milestone — exactly the opposite of the vision's goal of trustworthy, accurate documentation. The RISK-TEST-STRATEGY.md classifies R-04 as High/High/Critical.

**Recommendation**: Resolve OQ-01 before the pseudocode phase. The implementation agent must run `grep -c '#\[tool(' crates/unimatrix-server/src/mcp/tools.rs` and record the authoritative count. The spec should be updated to state the confirmed count, removing the open question. This does not require a full spec revision — a one-line resolution note in the OQ-01 section is sufficient.

---

### 2. [WARN] SPECIFICATION.md FR-02a Contains Implementation-Level Detail in Core Capabilities

**What**: SCOPE.md's framing direction explicitly states "Architecture section is minimal — high-level only, not crate-by-crate breakdown. Users care about SQLite (local, no cloud), hook integration, and MCP transport — not internal module boundaries." The Non-Goals section says "Architecture deep-dives: The 8-crate workspace structure, scoring formula weights, and detection rule internals are implementation details." SPECIFICATION.md FR-02a specifies the Core Capabilities section must cover the six-factor confidence formula with weights and the re-ranking formula with exact coefficients (0.85, 0.15, 0.03, 0.02).

**Why it matters**: If the implementation agent follows FR-02a literally, the resulting README would describe `0.85 * similarity + 0.15 * confidence + co-access boost (max 0.03)` in what is supposed to be a capability-first, user-facing document. This contradicts the framing principle the human explicitly approved ("what users DO, not what was built"). The SCOPE.md Non-Goals exclusion of "scoring formula weights" as implementation details directly conflicts with FR-02a's requirement to state them. Additionally, if the formula weights change in a future feature, the README will be stale again — defeating the purpose of the documentation system.

**Recommendation**: Revise SPECIFICATION.md FR-02a to remove formula-level coefficients from the Core Capabilities content requirement. Replace with user-facing framing: "confidence scoring combines usage signals, correction quality, creator trust, and co-access patterns into a composite score" without the numeric weights. Retain the technical details in SPECIFICATION.md itself as reference for the implementation agent's understanding, but do not mandate they appear in the README.

---

## Detailed Findings

### Vision Alignment

The product vision for the Platform Hardening milestone states: "Unimatrix works — now make it shippable. First multi-repo deployments require backup/restore, initialization, packaging, and documentation. npm/npx distribution for Rust binary." The vision entry for nan-005 is precisely: "Comprehensive README: features, capabilities, MCP tool reference (how/why to use each), benefits, constraints (e.g., new sessions per feature), workflow guidance (phase names with colons), skills reference. Documentation agent added to protocols — automatically updates docs with new features, capabilities, and tips after each shipped feature."

The three source documents faithfully deliver on this vision entry:
- ARCHITECTURE.md defines a three-component system (README rewrite, uni-docs agent, protocol modification) that matches the vision's two stated deliverables exactly.
- SPECIFICATION.md operationalizes the vision's guidance into 12 functional requirements (FR-01 through FR-12), 7 non-functional requirements (NFR-01 through NFR-07), and 12 acceptance criteria.
- RISK-TEST-STRATEGY.md provides 13 risks covering the accuracy, placement, and agent behavior failure modes most likely to undermine the vision's goal of trustworthy documentation.

The vision principle "Trust + Lifecycle + Integrity + Learning + Invisible Delivery" is upheld: the uni-docs agent and mandatory trigger criteria address lifecycle (documentation stays current), the fact verification checklist addresses integrity (no stale claims), and the README being the canonical entry point supports the trust goal for new adopters.

### Milestone Fit

nan-005 is correctly scoped to the Platform Hardening milestone. It does not:
- Introduce Graph Enablement capabilities (petgraph, topology-derived scoring)
- Introduce Activity Intelligence capabilities (query log, multi-session retrospective)
- Introduce Future Horizons capabilities (semantic routing, thin-shell agents, multi-project)

The feature correctly depends on nan-004 (npm packaging) being shipped, referencing `@dug-21/unimatrix` as the primary install path and noting `packages/unimatrix/package.json` as the source of truth for the package name. ARCHITECTURE.md and SPECIFICATION.md consistently use `npm install @dug-21/unimatrix` as the primary Getting Started path, correctly assuming nan-004 is complete.

The single SCOPE.md item "AC-09: includes both the npm install path (referencing nan-004)" is fully reflected in ARCHITECTURE.md Component 1 (README section structure, Getting Started) and SPECIFICATION.md FR-03a.

### Architecture Review

ARCHITECTURE.md is well-structured and appropriate for a documentation-only feature. Key observations:

**Strengths:**
- The three-component architecture (Component 1: README, Component 2: uni-docs agent, Component 3: protocol modification) is minimal, non-overlapping, and complete.
- The fact verification table in Component 1 provides explicit sources for every numeric claim, directly mitigating SR-01/R-01/R-02.
- The trigger criteria table in Component 3 resolves the ambiguity identified in SR-05 with a deterministic mandatory/skip decision table.
- ADR-001 through ADR-004 cover the four consequential design decisions with explicit rationale.
- The component interaction diagram and data flow description are clear.
- Open questions in the architecture (OQ-01 through OQ-04) are honestly documented rather than assumed away.

**Concerns:**
- OQ-01 (tool count) is unresolved. The architecture's fact verification table states "11" as the verified value, but this was populated from design-phase research, not a live verification. The implementation agent must re-verify at authoring time.
- ARCHITECTURE.md Component 1 states "11 MCP tools" in its fact table and integration surface table, but the README section structure says "All 11 tools." This is internally consistent within the architecture but conflicts with SCOPE.md's "12 tools" claim. The architecture appears to have resolved OQ-01 at 11 — but this resolution is not explicitly stated as such and the question remains open.

### Specification Review

SPECIFICATION.md is thorough, well-structured, and covers every requirement from SCOPE.md. Key observations:

**Strengths:**
- The Fact Verification Checklist table (13 rows with verification commands) is an excellent gate mechanism that directly converts SR-01/R-01/R-02 mitigation from recommendation to mandatory procedure.
- FR-04e explicitly documents that `maintain` is silently ignored since col-013 — this is precisely the kind of behavioral nuance that prevents a high-severity documentation error (R-03).
- FR-09g explicitly prohibits documenting unimplemented security features (OAuth, HTTPS, `_meta`) — mitigating R-10.
- FR-01e (preserve acknowledgments section) is a useful addition that protects against accidental removal during a full rewrite.
- The Open Questions section (OQ-01 through OQ-03) is honest and actionable.
- NFR-05 (architecture section target: 20-40 lines) provides a concrete constraint that enforces the SCOPE.md "minimal architecture" framing.

**Concerns:**
- FR-02a tension with SCOPE.md framing (documented as Variance #2 above).
- FR-04a says "document all 12 tools" in the heading, then immediately walks this back with an inline note. The requirement is self-contradictory. The heading should state the intent: document all verified tools; the count will be confirmed at implementation time.
- The Fact Verification Checklist states "Verified Value" for schema version as "11" and storage backend as "SQLite." These match PRODUCT-VISION.md ("schema v9" note is outdated in the vision — the spec's "v11" is more recent). No inconsistency with SCOPE.md, but the vision document reference to "schema v9" may confuse the implementation agent; the spec correctly overrides it.

### Risk Strategy Review

RISK-TEST-STRATEGY.md is comprehensive and well-matched to the source documents. Key observations:

**Strengths:**
- 13 risks are registered with severity, likelihood, and priority — all justified by the feature's failure modes.
- Critical risks (R-01, R-02) have 10 concrete test scenarios that correspond directly to verifiable commands (`grep -r "redb"`, crate count comparison, schema version check).
- The scope risk traceability table at the end maps all 8 SCOPE-RISK-ASSESSMENT.md risks to their RISK-TEST-STRATEGY.md counterparts and resolution status — complete and accurate.
- Integration risks (README as implicit contract for uni-docs, protocol modification additive constraint, agent spawn pattern compatibility) are identified and correctly assessed.
- Security risks for a documentation-only feature are realistically scoped: no new code paths, but the settings.json snippet safety concern and uni-docs agent trust boundary are appropriately flagged.

**Concerns:**
- R-04 (tool count discrepancy) is rated High/High/High but the resolution ("implementation agent must confirm") is deferred to implementation. The source documents should resolve this before implementation begins, not rely on the implementation agent to notice and fix a known discrepancy. This is captured as Variance #1 above.
- The edge case "README Exceeds 800 Lines" correctly references ADR-001's split threshold. The risk is mitigated by SPECIFICATION.md's section structure and NFR-05's architecture depth limit, but the checklist item is worth retaining as a final line count check at review.

---

## Knowledge Stewardship

- Queried: `/query-patterns` for vision alignment patterns — tool not available in this environment (no MCP server connection from worktree); no prior alignment pattern results retrieved.
- Stored: nothing novel to store — the two variance patterns (open tool count not resolved before design is complete; specification adding internal detail that conflicts with scope framing) are potentially generalizable. Recommend recording after delivery: "Specs for documentation-heavy features tend to reintroduce implementation details under the framing of 'comprehensive coverage' — vision guardian should check user-facing framing discipline explicitly." Topic: `vision`, Category: `pattern`.
