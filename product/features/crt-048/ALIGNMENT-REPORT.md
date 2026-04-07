# Alignment Report: crt-048

> Reviewed: 2026-04-06
> Artifacts reviewed:
>   - product/features/crt-048/architecture/ARCHITECTURE.md
>   - product/features/crt-048/specification/SPECIFICATION.md
>   - product/features/crt-048/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Agent: crt-048-vision-guardian

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature resolves a Critical domain-coupling gap explicitly listed in PRODUCT-VISION.md |
| Milestone Fit | PASS | Correctly targeted as a Cortical / Wave 1A cleanup; no future-wave capabilities introduced |
| Scope Gaps | PASS | All nine SCOPE.md goals and all implementation notes covered in source documents |
| Scope Additions | WARN | ARCHITECTURE.md enumerates 8 exact fixture sites vs. SCOPE.md's estimate of 6; not a functional addition but resolves an open count ambiguity |
| Architecture Consistency | PASS | Architecture, specification, and scope are internally consistent; no contradictions found |
| Risk Completeness | PASS | Every scope risk (SR-01 through SR-07) is mapped to an architecture decision and a test scenario; no scope risk is unaddressed |

**Overall: PASS with one WARN.** No variances require human approval. The WARN is informational — the fixture-site count discrepancy is a deliberate scope refinement by the architect, not a scope addition.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `compute_lambda()` named-struct refactor | SCOPE.md SR-02 suggested considering named-field args; ARCHITECTURE.md explicitly declines (ADR Technology Decisions: 4 params with distinct types, low mis-ordering risk). Rationale documented. Acceptable. |
| Clarification | Fixture site count | SCOPE.md estimates "~6 sites / ~12 field removals"; ARCHITECTURE.md enumerates exactly 8 sites / 16 field references. The additional sites (line 1291 and the `make_coherence_status_report()` helper at line 1434 with non-default values 0.8200/15) were discovered by the architect during code review. This is a scope clarification, not a scope addition — the work is strictly contained within the same SCOPE.md goal (Goal 4 / FR-17). |
| Gap | None | All SCOPE.md goals 1–9 are addressed in FR-01 through FR-18 and in the architecture component breakdown. |
| Addition | None | No source document introduces behavior not requested in SCOPE.md. |

---

## Variances Requiring Approval

None.

---

## Detailed Findings

### Vision Alignment

The product vision (PRODUCT-VISION.md, "The Critical Gaps — Domain Coupling" table) lists "Time-based freshness in Lambda — domain-specific assumption" as a **Critical** gap, with status "**Resolved** — freshness dimension dropped from Lambda entirely (#520); Lambda is now a 3-dimension structural health metric (graph, contradiction, embedding)."

crt-048 is the delivery vehicle for that resolution. The feature does exactly what the vision records as the fix: removes the `confidence_freshness` dimension, re-normalizes the three surviving structural dimensions, and leaves Lambda as a domain-neutral metric.

The vision further states that Lambda "embeds a domain-specific cadence assumption (daily cycle) that fails for any non-daily-cadence platform." SPECIFICATION.md Section "NOT in Scope" item 1 and SCOPE.md Non-Goals explicitly prevent introducing any replacement dimension in this feature — the scope correctly defers a future cycle-relative dimension to a separate feature, consistent with the vision's intent to defer Options 2 and 3.

The architecture and specification introduce no new domain-coupling of their own. The three surviving Lambda dimensions (graph quality, contradiction density, embedding consistency) are structural and domain-neutral, aligning with the vision principle "configured not rebuilt."

**PASS.**

### Milestone Fit

crt-048 is positioned in the Cortical phase, addressing a Lambda structural defect that became apparent after crt-036 (Intelligence-Driven Retention, Wave 1A area). The product vision roadmap does not name a specific wave milestone for this fix because it was categorized as a Critical domain-coupling gap requiring resolution before (or alongside) Wave 1A. The feature:

- Does not build any Wave 2 capabilities (no containerization, no HTTP transport, no OAuth).
- Does not build any Wave 1A intelligence pipeline capabilities (no session context, no GNN, no W3-1 work).
- Does not reference W3-1 or the GNN as a dependency or output.
- Restricts itself to cleanup of a defective Lambda computation path.

The ADR supersession (AC-12) required by SCOPE.md and SPECIFICATION.md correctly stores the new weight rationale in Unimatrix, consistent with project convention ("no ADR files — Unimatrix only").

**PASS.**

### Architecture Review

The architecture document organizes the change into four components (A: `infra/coherence.rs`, B: `services/status.rs`, C: `mcp/response/status.rs`, D: `mcp/response/mod.rs`) and provides precise line-level call-site enumeration. Key observations:

1. **`DEFAULT_STALENESS_THRESHOLD_SECS` retention** is correctly handled. SCOPE.md §Implementation Notes explicitly states the constant must survive; ARCHITECTURE.md encodes this as a hard constraint (Component A, "retained") and Technology Decisions ("ADR-002"). SPECIFICATION.md FR-10 and AC-11 elevate it to an explicit acceptance criterion. The path from scope risk SR-03 through the architecture to the spec is complete.

2. **Fixture site enumeration** (Component D): ARCHITECTURE.md provides an exact table of 8 fixture sites with line-number pairs for both removed fields. SCOPE.md estimated "~6 sites / ~12 removals." The architect's enumeration reveals 8 sites / 16 references — the `make_coherence_status_report()` helper (line 1434, non-default values 0.8200/15) and a seventh inline fixture (line 1291) were discovered. This discrepancy is flagged as a WARN below. It is not a scope addition — the same goal applies — but the higher count increases delivery risk.

3. **Named-struct refactor decline**: ARCHITECTURE.md explicitly declines the named-struct refactor suggested in SR-02, arguing four parameters with distinct types (`f64`, `Option<f64>`, `f64`, `&CoherenceWeights`) are sufficiently distinguishable. The decision is documented. Acceptable per Design Principle 2 (Vision over Convenience — this is not a shortcut contradicting the vision, it is an implementation choice within scope).

4. **No schema migration**: The architecture correctly confirms zero schema changes. No migration files, no table changes, no `unimatrix-store` impact. Consistent with SCOPE.md Non-Goal and C-06.

5. **`coherence_by_source` retention**: Both call sites of `compute_lambda()` are explicitly enumerated and both are constrained to the updated 3-dimension signature. The architecture also correctly retains `load_active_entries_with_tags()` (FR-11).

**PASS.**

### Specification Review

SPECIFICATION.md covers all nine SCOPE.md goals as functional requirements (FR-01 through FR-18) and non-functional requirements (NFR-01 through NFR-06). The acceptance criteria (AC-01 through AC-14) provide verification methods for every requirement.

Notable: The spec includes two open questions (OQ-A, OQ-B, OQ-C) that are design-time work items for the architect, not scope ambiguities. OQ-A (exact fixture line numbers) is directly addressed by ARCHITECTURE.md §Component D — the architect resolved it. OQ-B and OQ-C are flagged as "must re-verify at delivery start," appropriately deferred to the implementer.

The `Domain Models` section provides precise post-crt-048 definitions for `CoherenceWeights`, `Lambda`, `StatusReport`, and `DEFAULT_STALENESS_THRESHOLD_SECS`, forming a clean ubiquitous language reference for the implementer.

NFR-06 documents the breaking JSON change as intentional and accepted, with the constraint (C-07) that PR release notes must list both removed keys. This is correctly handled as a documentation obligation, not a code change.

**PASS.**

### Risk Strategy Review

RISK-TEST-STRATEGY.md covers 10 risks (R-01 through R-10) derived from the 7 scope risks (SR-01 through SR-07). All scope risks have a corresponding architecture risk entry in the traceability table at the bottom of the document.

Specific observations:

- **R-01 and R-06** (positional f64 argument mis-ordering, per-source loop inconsistency) are rated Critical and have concrete test scenarios with specific input values (graph=0.8, contradiction=0.3, embedding=Some(0.5)) that distinguish correct from transposed argument order.

- **R-02** (partial fixture removal causing compile failure) is rated Critical/High-likelihood and correctly identifies the `make_coherence_status_report()` helper at line 1434 as the highest-risk site (non-default values, not found by a default-value search-and-replace).

- **R-03** (constant deletion) is rated Critical and references ADR-002, FR-10, and AC-11 as a three-layer defense.

- **R-10** (ADR-003 not superseded) addresses the Unimatrix knowledge divergence risk. It is correctly classified as a required delivery step (not optional knowledge stewardship), and the test scenario requires `context_get` on both the new ADR and entry #179 post-delivery.

- **Security section**: correctly notes crt-048 introduces no new untrusted input surface. The blast radius analysis (incorrect Lambda value → wrong maintenance gate recommendation) is appropriately bounded.

One minor observation: RISK-TEST-STRATEGY.md counts R-07 in both "High" and "Medium" priority rows of the Coverage Summary table (it appears twice). This is a formatting error in the table — R-07 is rated High internally. This does not affect risk coverage.

**PASS.**

---

## WARN Detail

### W-01: Fixture site count discrepancy (SCOPE.md estimate vs. ARCHITECTURE.md exact count)

**What**: SCOPE.md §Implementation Notes states "approximately 12 field removals in `mod.rs` test fixtures." ARCHITECTURE.md §Component D enumerates exactly 8 sites / 16 field references. The discrepancy is 4 additional field removals (2 extra sites: line 1291 and line 1434).

**Why it matters**: The SCOPE.md estimate was written before the architect audited `mcp/response/mod.rs`. The higher exact count is the correct value. The risk is delivery-side: if an implementer uses the SCOPE.md estimate ("~12 removals") as a completion checklist rather than the ARCHITECTURE.md table, they may stop after 12 and miss 4, producing a compile error. RISK-TEST-STRATEGY.md R-02 already accounts for this and designates the `make_coherence_status_report()` helper at line 1434 as a special-case site.

**Recommendation**: No source document change needed — the risk is already mitigated by R-02's explicit call-out. Human reviewers should be aware that the authoritative fixture count is the ARCHITECTURE.md table (8 sites / 16 references), not the SCOPE.md estimate.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — found entries #2298 (config key semantic divergence between TOML and vision), #3337 (architecture diagram informal headers diverging from spec), #3742 (optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral). Entry #3742 is the most directly relevant: it confirms that when a scope explicitly defers a future capability, architecture and risk should not diverge from that deferral. crt-048 correctly defers Options 2 and 3 (cycle-relative dimension) across all three source documents — consistent with the pattern.
- Stored: nothing novel to store. The variances here (fixture-count clarification, named-struct refactor decline) are feature-specific. The pattern of architects enumerating exact struct initialization sites when scope gives estimates is already captured in entry #2398 (API Extension Gap). The complete traceability from scope risk to architecture decision to spec AC to test scenario is exemplary — but is a feature of this specific well-structured feature, not a new generalizable pattern.
