# Alignment Report: crt-019

> Reviewed: 2026-03-14
> Artifacts reviewed:
>   - product/features/crt-019/architecture/ARCHITECTURE.md
>   - product/features/crt-019/specification/SPECIFICATION.md
>   - product/features/crt-019/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances "trustworthy, correctable, and auditable" confidence goal |
| Milestone Fit | PASS | Correctly targets the "Search Quality Enhancements" milestone, Track A, P1 |
| Scope Gaps | PASS | All seven SCOPE.md goals and 12 acceptance criteria are addressed |
| Scope Additions | PASS | No out-of-scope capabilities introduced |
| Architecture Consistency | PASS | All SR-* scope risks resolved; ADRs present and referenced |
| Risk Completeness | VARIANCE | R-05 identifies a threshold contradiction between SPEC (>=5) and ARCHITECTURE (>=10) that is unresolved in the documents — implementation will be forced to choose without authoritative guidance |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Workflow 4 description (SPEC) | SPEC Workflow 4 says "For each entry, if `start.elapsed() > 200ms`, break" then also says "For each entry in budget: compute α₀/β₀ from voted population" — these are sequenced incorrectly; prior computation happens *after* the refresh loop (Step 2b), not inside it. Minor ordering error in the prose; does not affect implementation path which is correctly specified in ARCHITECTURE.md Component 3. |
| Simplification | SPEC §Dependencies — `src/tools/{get,lookup}.rs` | SPEC lists `src/tools/{get,lookup}.rs` as the file path for tool handler changes, while ARCHITECTURE.md correctly identifies `src/mcp/tools.rs` as the single file. Inconsistency is cosmetic; ARCHITECTURE.md is authoritative for file paths. |

---

## Variances Requiring Approval

### VARIANCE 1 — Bayesian Prior Cold-Start Threshold Contradiction (R-05)

**What**: SPEC FR-09 and Constraint C-08 mandate `>= 5` voted entries as the threshold for activating empirical prior estimation. ARCHITECTURE.md ADR-002 and Component 3 mandate `>= 10`. These documents contradict each other. RISK-TEST-STRATEGY.md R-05 identifies this as a High/High unresolved risk and designates `>= 10` as authoritative (citing the architect's population-stability rationale in ADR-002), but it also notes "the SPEC should be updated to match" — implying the SPEC has not been updated.

**Why it matters**: The threshold controls when empirical α₀/β₀ estimation activates versus the cold-start default. At threshold=5, a sparse population produces potentially unstable estimates that propagate to all entry confidence scores on every refresh tick. At threshold=10, five additional voted entries must accumulate before empirical estimation begins. The implementation will implement one value; the un-updated SPEC creates ambiguity about which is authoritative and which tests to write against.

**Recommendation**: Update SPEC FR-09 and Constraint C-08 to read `>= 10` to match ADR-002 before implementation begins. RISK-TEST-STRATEGY.md already designates `>= 10` as the resolution; the SPEC is simply not yet reconciled. This is a documentation fix, not a design decision.

---

## Detailed Findings

### Vision Alignment

PASS.

crt-019 directly serves the product vision's core value proposition: "confidence evolution from real usage signals" is explicitly called out in the vision's description of the Cortical phase (PRODUCT-VISION.md, "Learning & Drift" section). The feature fixes a structural defect — 46.7% dead-weight floor — that prevents confidence from being a meaningful differentiator. The vision states the knowledge system must be "trustworthy, retrievable, and ever-improving"; confidence that cannot differentiate quality undermines all three properties.

The adaptive blend (FR-04) and the deliberate-retrieval signals (FR-06, FR-07) advance the "invisible delivery" pillar: confidence that reflects real usage patterns means injected knowledge is more likely to be the right knowledge.

No shortcuts that contradict the vision were found. The "no schema change" constraint (NFR-05) is consistent with the vision's emphasis on zero cloud dependency and embedded engine reliability.

### Milestone Fit

PASS.

PRODUCT-VISION.md places crt-019 explicitly in the "Search Quality Enhancements — NEXT" milestone, Track A, Priority 1. The feature description in the vision document matches the SCOPE.md goals verbatim. The dependency graph is respected: crt-019 has no upstream dependencies within the milestone, and its outputs (spread >= 0.20, formula calibrated for votes) are the declared preconditions for crt-018b and crt-020.

No future-milestone capabilities are introduced. The feature does not touch petgraph (crt-014, Track B), contradiction clustering (crt-017), or co-access transitivity (crt-016) — all correctly deferred to later milestones.

The `MAX_CONFIDENCE_REFRESH_BATCH` increase (FR-05) is a prerequisite for the larger active entry populations anticipated at the Graph Enablement milestone (entry count > 1000 active), but the change is sized conservatively (500) and gated by a duration guard. Building this headroom now is appropriate and consistent with the milestone's stated goal of sharpening intelligence signals.

### Architecture Review

PASS overall.

The architecture is coherent and well-scoped. Key observations:

**SR-* resolution is complete.** All eight scope risks from SCOPE-RISK-ASSESSMENT.md are addressed with explicit ADRs (ADR-001 through ADR-004) and traced in the RISK-TEST-STRATEGY.md scope risk traceability table. The pattern of documenting ADRs inline in ARCHITECTURE.md and separately referencing them from the risk strategy is consistent with project convention.

**`ConfidenceState` design is sound.** The `Arc<RwLock<ConfidenceState>>` pattern with a short critical section (4-field f64 write) on the background tick side and a clone on the read side is appropriate. The architecture explicitly calls out the `unwrap_or_else(|e| e.into_inner())` poison-recovery pattern from the project's CategoryAllowlist convention (documented in MEMORY.md), which is the correct precedent.

**Component 5 (deliberate retrieval) is correctly constrained.** The architecture confirms the implicit helpful vote for `context_get` is folded into the existing `UsageContext.helpful` field, not a second spawn. The `context_lookup` doubled access via `access_weight: u32` is a clean extension of `UsageContext` with no schema changes.

**One cosmetic inconsistency** in file paths: ARCHITECTURE.md Component 6 refers to `crates/.claude/skills/` but the actual path is `.claude/skills/` (not nested under `crates/`). This does not affect implementation since the skill files are unambiguously identified.

**Unresolved in architecture**: The `compute_confidence` closure at `usage.rs:158` requires migrating from a bare function pointer (`Option<&dyn Fn(&EntryRecord, u64) -> f64>`) to a capturing closure type. ARCHITECTURE.md notes this (Integration Surface section) but defers the exact Store signature change to implementation. RISK-TEST-STRATEGY.md elevates this to Critical/High (R-01). The architecture is correct in identifying it but the Store API signature change is consequential enough that it warrants explicit specification before implementation — the current documentation says "a closure capturing the current state values must be constructed at the call site" without pinning the Store's `record_usage_with_confidence` signature change. This is addressable during implementation but is the highest-risk unspecified detail.

### Specification Review

PASS overall, with one contradiction noted in the VARIANCE section above.

**FR-01 through FR-10 are complete and testable.** Each functional requirement has a corresponding acceptance criterion with exact assertions (AC-01 through AC-12). The Bayesian formula is fully specified with numerical examples and the cold-start behavior is precisely defined.

**AC-02 correction is correctly handled.** SPEC AC-02 explicitly notes the correction from SCOPE.md (which erroneously stated `helpfulness_score(2, 2, ...) > 0.5`). The corrected assertion `== 0.5` is mathematically verified in the spec body (`(2+3)/(4+6) = 0.5`). RISK-TEST-STRATEGY.md R-14 re-confirms this correction. The correction chain across SCOPE → SPEC → RTS is clean.

**C-07 (adaptive blend state management)** explicitly defers the SR-03 resolution to ARCHITECTURE.md. The specification mandates the behavior (`rerank_score must use the observed_spread-derived weight`) while deferring the mechanism. ARCHITECTURE.md resolves it with the `RwLock<ConfidenceState>` / parameter-passing hybrid. The deferral is documented and resolved.

**Workflow 4 prose sequencing error** (noted in Scope Alignment): Steps 4 and 5 in the workflow description conflate the per-entry confidence update with the post-loop prior computation. The specification body in FR-09 and ARCHITECTURE.md Component 3 are correct; only the Workflow 4 narrative prose is imprecise. Does not create implementation risk given the authoritative FR-09 and Component 3 descriptions.

**NFR-01 through NFR-07 are well-grounded** in the implementation concerns. NFR-06 (no blocking pool regression) traces to the vnc-010 fix and entry #771, demonstrating that the non-functional requirements are calibrated against known past failures rather than aspirational targets.

### Risk Strategy Review

PASS overall. The risk strategy is thorough and identifies the one genuine unresolved contradiction (R-05).

**R-01 (Critical/High)** is the highest-risk item and is correctly prioritized. The requirement for an integration test (not just a unit test) to prove empirical prior values flow end-to-end is well-reasoned: a unit test can mock the closure and miss the actual type-incompatibility bug. The three test scenarios for R-01 provide sufficient coverage.

**R-05 is the only unresolved design conflict.** The risk strategy correctly identifies that SPEC and ARCHITECTURE contradict on the threshold value, designates `>= 10` as authoritative, and recommends updating the SPEC. The human must decide which document is authoritative and ensure the SPEC is updated before delivery begins. This is the sole VARIANCE in this review.

**R-11 (store-layer ID dedup)** is correctly elevated to High/High. ADR-004 explicitly flags it as "unresolved." The risk strategy requires a store-layer test with duplicate IDs before the `flat_map` repeat approach is committed. This is the right gate: the feature's doubled-access signal silently vanishes if the store deduplicates internally. The implementation agent must verify store behavior before choosing the access_weight multiplier strategy.

**R-12 (method-of-moments degeneracy)** coverage is adequate. The NaN propagation path through `helpfulness_score` → `compute_confidence` → search re-ranking is correctly identified as the blast radius. The defense-in-depth test (`helpfulness_score(0, 0, NaN, NaN)`) is a strong requirement.

**Security risks** are proportional and correctly scoped. SEC-01 (prior manipulation via vote injection) is mitigated by the `[0.5, 50.0]` clamp and the existing MCP authentication layer. The worst-case analysis (`alpha0=50` → score ≈ 0.99 for unvoted entries) is documented as accepted. SEC-02 confirms `access_weight` is a server-internal field not exposed in MCP schemas — this should be verified during implementation code review.

**FM-03 (RwLock poison recovery)** correctly references the existing `unwrap_or_else(|e| e.into_inner())` pattern from CategoryAllowlist. The recommendation to apply this pattern to `ConfidenceState` lock acquisitions is consistent with project convention and should be treated as a hard requirement.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- no results returned (Unimatrix knowledge base queried; no prior vision alignment patterns stored for the `vision` topic)
- Stored: nothing novel to store -- the single VARIANCE (threshold contradiction between SPEC and ARCHITECTURE) is specific to crt-019's document production sequence and does not generalize as a recurring cross-feature pattern at this time. If future features show the same architect-raises-threshold-post-spec pattern, that would be worth storing as a pattern.
