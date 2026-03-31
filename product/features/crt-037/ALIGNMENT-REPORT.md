# Alignment Report: crt-037

> Reviewed: 2026-03-31
> Artifacts reviewed:
>   - product/features/crt-037/architecture/ARCHITECTURE.md
>   - product/features/crt-037/specification/SPECIFICATION.md
>   - product/features/crt-037/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-037/SCOPE.md
> Risk assessment source: product/features/crt-037/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements the typed relationship graph intelligence layer (Wave 1A/W1-1 direction) |
| Milestone Fit | PASS | Sits correctly in Wave 1 / Wave 1A foundation; no future-wave capability built prematurely |
| Scope Gaps | WARN | Four open questions in SPEC (OQ-S1 through OQ-S4) are not resolved — they are deferred to delivery. One (OQ-S4 cap split point) has no explicit architectural answer. |
| Scope Additions | PASS | No additions beyond what SCOPE.md requests |
| Architecture Consistency | WARN | ARCHITECTURE.md and SPECIFICATION.md model `NliCandidatePair` differently in one structural detail — spec uses a true tagged union with `nli_scores` embedded per variant; architecture uses a flat struct with `Option` fields. This discrepancy could mislead the implementer. |
| Risk Completeness | PASS | Risk register is thorough; all SCOPE-RISK-ASSESSMENT.md risks are traceable to architecture decisions and test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | OQ-S4 cap split point not architecturally resolved | SPEC §Open Questions surfaces this as unresolved: the precise Phase 5 cap-split computation point (before/after Phase 4b scan, or at merge time) affects whether Informs candidates can be silently zeroed with no log. ARCHITECTURE.md Phase 5 section specifies the algorithm (Supports first, then remaining) but does not explicitly address the case where 100 Supports candidates pre-fill the cap before Phase 4b even runs. SCOPE-RISK-ASSESSMENT.md SR-03 asked for an observable metric — that is addressed — but the timing question from OQ-S4 remains open. |
| Gap | OQ-S2 (NliScores.neutral computation) not resolved in architecture | SCOPE-RISK-ASSESSMENT.md assumption notes `NliScores.neutral` may be a residual `(1 - entailment - contradiction)` carrying higher noise. ARCHITECTURE.md and SPECIFICATION.md both use `neutral > 0.5` as a fixed threshold but neither confirms nor documents the actual model output dimensionality. RISK-TEST-STRATEGY.md correctly flags this as R-07 requiring OQ-S2 resolution before Phase C, but no pre-delivery confirmation mechanism is specified. |
| Simplification | `NliCandidatePair` as struct-with-Options vs. true tagged union | SCOPE.md OQ-2 resolved as "merge into Phase 7 with discriminator tag." Architecture implements it as a flat struct with `Option<String>` fields and a `PairOrigin` enum. Specification implements it as a proper Rust enum with variant-specific fields (`SupportContradicts { ... }` / `Informs { candidate: InformsCandidate, ... }`). Both are acceptable implementations of the discriminator concept; the spec approach is stronger (compile-time exhaustiveness). The simplification in architecture is not harmful but the inconsistency could confuse the implementer about which model to use. Rationale: architecture was written before spec finalized the Rust model. |
| Simplification | `InformsCandidate` record vs. `NliCandidatePair` Option fields | Architecture uses `Option<String>` fields directly on `NliCandidatePair`. Specification separates concerns cleanly into an `InformsCandidate` sub-record carried by the `Informs` variant. Spec model is safer (no null-field vacuous-pass risk per R-05). The spec's approach should take precedence. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications. The two WARN items below require human awareness but do not block delivery.

---

## Detailed Findings

### Vision Alignment

**PASS**

crt-037 directly serves the vision's intelligence pipeline direction. The product vision (Wave 1 / W1-1) established the typed relationship graph with `RelationEdge` and multiple `RelationType` variants. The vision explicitly calls out: "Only supersession edge type — no typed relationships: **Fixed** — W1-1 RelationEdge." It further identifies that "co-access and contradiction never formalized as graph edges: **Fixed** — W1-1" and that `Supports`, `CoAccess`, `Prerequisite`, `Contradicts`, and `Supersedes` are now persisted.

The gap the vision does NOT declare fixed is: "Intelligence pipeline is additive boosts, not a learned function: **Roadmapped** — Wave 1A + W3-1." crt-037 contributes directly to making PPR traversal richer by bridging cross-feature institutional memory — exactly the kind of infrastructure Wave 1A and W3-1 depend on. The `Informs` edge type feeds PPR, which feeds the future session-conditioned relevance function (W3-1 GNN input graph).

The vision's domain agnosticism principle is respected: all category vocabulary is config-driven (`informs_category_pairs`), no domain strings appear in detection logic, and the feature's default pairs are explicitly documented as software-engineering defaults that operators can override.

The vision's integrity and provenance principles are preserved: `Informs` edges use `EDGE_SOURCE_NLI = "nli"`, follow the existing weight-finitude invariant, and go through the same `write_nli_edge` path as all other NLI-inferred edges.

---

### Milestone Fit

**PASS**

crt-037 belongs in Wave 1 / Wave 1A. The typed relationship graph (W1-1) established the foundation; crt-037 adds a sixth edge type within that foundation. This is not a Wave 2 (deployment) concern, not a Wave 3 (GNN/self-improvement) concern, and not a pre-Wave 0 prerequisite.

The feature explicitly does NOT build:
- Config-extensible relation types (deferred per SCOPE.md Non-Goals)
- `Extended(String)` open-ended variants (deferred)
- Changes to graph compaction or VECTOR_MAP (Wave 1 already delivered)
- GNN training signals beyond what existing infrastructure provides (W3-1)

No future-milestone capability is being prematurely built. The `Informs` edge type is a Wave 1A-appropriate graph enrichment that directly supports W3-1 training label quality (more graph structure → better GNN features) without implementing W3-1 itself.

---

### Architecture Review

**WARN: `NliCandidatePair` structural model inconsistency**

ARCHITECTURE.md (§Component D) defines `NliCandidatePair` as a flat struct with a `PairOrigin` enum field and multiple `Option<*>` fields:

```
struct NliCandidatePair {
    source_id: u64,
    target_id: u64,
    similarity: f32,
    origin: PairOrigin,
    source_category: Option<String>,
    source_feature_cycle: Option<String>,
    target_feature_cycle: Option<String>,
    source_created_at: Option<i64>,
    target_created_at: Option<i64>,
}
```

SPECIFICATION.md (§Domain Models) defines `NliCandidatePair` as a proper Rust enum (tagged union):

```
NliCandidatePair::SupportContradicts { source_id, target_id, cosine, nli_scores }
NliCandidatePair::Informs { candidate: InformsCandidate, nli_scores }
```

The spec explicitly states: "The compiler enforces exhaustive matching — a new variant cannot be silently ignored." The spec model is structurally stronger for two reasons:
1. It makes `None`-field vacuous-pass (R-05) impossible at the type level.
2. It satisfies FR-10's requirement that "misrouting is a compile-time error, not a runtime branch."

The architecture's flat-struct model allows an `Informs`-origin pair to have `source_created_at: None`, which Phase 8b guard re-verification must handle defensively. The spec's `InformsCandidate` sub-record makes all fields required for `Informs` pairs.

**Impact**: Implementer may follow the architecture model (flat struct) rather than the spec model (tagged union), producing code that satisfies the discriminator intent but does not achieve compile-time routing safety. The spec is correct; the architecture should be treated as superseded by the spec on this point.

**Recommendation**: The implementer should follow SPECIFICATION.md's tagged union model. No document change is required before delivery — the spec governs — but the discrepancy should be noted at delivery gate-in so the reviewer confirms the tagged-union implementation was used.

---

Additionally, ARCHITECTURE.md is otherwise highly consistent with SPECIFICATION.md:
- All 24 ACs are traceable to architectural components.
- The Phase 4b algorithm, Phase 5 cap priority, Phase 8b composite guard, and Phase 8b write call match the spec's FR and constraint sections exactly.
- The three ADR references (ADR-001 discriminator, ADR-002 combined cap, ADR-003 directional dedup) are cited consistently in both documents.
- The penalty invariant boundary (SR-05) is documented in ARCHITECTURE.md as a named section, fulfilling the scope risk assessment requirement.
- PPR direction semantics are confirmed and documented in both ARCHITECTURE.md and SPECIFICATION.md (both cite entry #3744).

---

### Specification Review

**PASS with two open questions deferred to delivery (WARN)**

SPECIFICATION.md is thorough. All SCOPE.md goals and acceptance criteria are addressed:
- All 24 SCOPE.md ACs appear verbatim in the spec's AC table with verification methods.
- All 8 SCOPE.md constraints (C-01 through C-15) are carried forward as SPECIFICATION constraints.
- All 6 SCOPE.md Non-Goals appear in the spec's "NOT in Scope" section.
- The `informs_category_pairs` default list is frozen at four (C-10 / SR-04), the neutral threshold is fixed at 0.5 (C-09 / OQ-4 resolution), and the dedup is directional (C-08 / OQ-3 resolution).

**OQ-S1 (schema assumption):** The spec correctly requires DDL inspection before Phase C delivery begins. The risk is understood and gated. No concern here.

**OQ-S2 (NliScores.neutral computation):** The spec (§Open Questions OQ-S2) states the architect "should confirm the actual model output dimensionality." This confirmation is not present in ARCHITECTURE.md — the architecture neither confirms nor documents it. This leaves the delivery team without a resolved answer on whether the neutral floor threshold is well-calibrated. The risk is contained by FR-11's entailment exclusion guard (which partially mitigates residual-neutral inflation), but the underlying model property is unconfirmed.

**OQ-S3 (Phase 4b scan scope):** The spec asks whether `select_source_candidates` returns category metadata. ARCHITECTURE.md Phase 4b section resolves this operationally — it builds `entry_meta: HashMap<u64, &EntryRecord>` from `all_active` for O(1) lookup. This is adequate; OQ-S3 is implicitly resolved by the architecture.

**OQ-S4 (Phase 5 cap split point timing):** The spec asks "the precise point in the tick where the cap split is computed... affects whether Phase 4b can produce zero candidates due to full cap without any log signal." ARCHITECTURE.md Phase 5 specifies the algorithm (Supports truncated first, then remaining capacity for Informs) but the log covers candidates-dropped, not candidates-prevented-from-being-generated. In a tick where Supports fills the cap before Phase 4b runs, Phase 4b still runs and generates candidates — they are just all dropped in Phase 5. The logging per FR-14 captures `informs_candidates_dropped`, which is sufficient observability. OQ-S4 is therefore implicitly resolved, but the spec does not explicitly close it.

**FR-11 additional guard:** The spec adds a guard not present in SCOPE.md — "an entry already handled by Supports/Contradicts path must not additionally receive an Informs edge from the same pair." This is a justified defensive addition (Gap-2 mutual exclusion, R-19). It is a scope addition in the specification relative to SCOPE.md, but it is clearly motivated by SCOPE-RISK-ASSESSMENT.md SR-01's composite guard requirement and prevents graph corruption. This addition is sound and does not expand the user-visible scope.

---

### Risk Strategy Review

**PASS**

The RISK-TEST-STRATEGY.md is comprehensive. Key strengths:

1. **All SCOPE-RISK-ASSESSMENT.md risks are traced.** SR-01 through SR-08 each appear in the §Scope Risk Traceability table with specific Risk IDs from the register (R-01 through R-20) and resolution citations.

2. **Critical risks are well-covered.** R-01 (CHECK constraint), R-02 (PPR direction), R-03 (composite guard), and R-20 (missing gate tests) are all classified Critical with multiple scenario requirements. The R-02 treatment explicitly references entry #3754 (the crt-030 direction inversion lesson) and requires the AC-05 test to assert the specific lesson node, not aggregate non-zero.

3. **CI grep gates are specified.** AC-21 (`Handle::current` absence), AC-22 (domain string leakage), and R-02's direction regression check are all specified as hard CI failures, not optional warnings.

4. **Security surface assessment is accurate.** crt-037 introduces no new external input surface. The security section correctly identifies that `informs_category_pairs` strings go through `HashSet` membership check (no SQL interpolation), config fields are validated at startup, and internal entry IDs are not user-supplied.

5. **R-20 severity elevation is appropriate.** Elevating "missing tick integration tests at gate" to Critical (not High) based on entry #3579 (nan-009 lesson) is correct. The gate structure (AC-13 through AC-23 all mandatory, same wave as Phase C code) is explicit.

One minor gap: R-15 (weight NaN/Inf) is classified Low with "Minimum 1 boundary test each," but the test described is only for `similarity = 0.0` or `nli_informs_ppr_weight = 0.0` (which produce 0.0, not NaN). The actual NaN/Inf risk from denormalized f32 input is noted in the risk description but no explicit test scenario covers it. NF-08 in the spec requires finite weight validation — a test confirming the validation fires for a NaN input would close this gap. This is a low-severity observation that does not require resolution before delivery.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision alignment patterns scope additions milestone discipline` — found entries #2298 (config key semantic divergence pattern), #3742 (optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral), #3337 (architecture diagram informal headers diverge from spec — testers assert against wrong strings). Entry #3337 is directly relevant: the `NliCandidatePair` struct-vs-enum discrepancy between ARCHITECTURE.md and SPECIFICATION.md follows the same divergence pattern and carries similar risk of implementer asserting against the wrong model.
- Queried: `/uni-query-patterns` for `scope addition architecture adds items not requested` — found same entries; no novel scope-addition patterns not already captured.
- Stored: nothing novel to store — the `NliCandidatePair` struct-vs-enum discrepancy is feature-specific to crt-037 and does not generalize beyond this feature's design. The pattern of architecture/spec structural model divergence is already entry #3337. The vision alignment patterns found are consistent with prior reviews.
