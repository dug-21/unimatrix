# Alignment Report: crt-024

> Reviewed: 2026-03-21
> Artifacts reviewed:
>   - product/features/crt-024/architecture/ARCHITECTURE.md
>   - product/features/crt-024/specification/SPECIFICATION.md
>   - product/features/crt-024/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-024/SCOPE.md
> Scope risk source: product/features/crt-024/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly implements the WA-0 prerequisite stated in the product vision; resolves an explicitly named Critical Gap |
| Milestone Fit | PASS | Wave 1A prerequisite; upstream dependencies (W1-4, W0-1) are complete; downstream (WA-2, W3-1) not pre-empted |
| Scope Gaps | PASS | All 14 SCOPE.md acceptance criteria are addressed in specification; no scope item left unresolved |
| Scope Additions | WARN | Six-term formula (w_util, w_prov) extends the vision's four-term illustrative formula; canonicalized in ADR-001 with explicit rationale, but SR-02 constitutes a documented extension from the vision text |
| Architecture Consistency | PASS | Architecture is internally consistent; all open spec questions are explicitly flagged for spec writer |
| Risk Completeness | WARN | R-01 names a divergence between FR-05 (spec) and ARCHITECTURE.md on utility normalization; this unresolved text conflict requires human confirmation before implementation |

**Overall: PASS with two WARNs. No FAILs. One item requires human confirmation (R-01 spec/architecture text divergence).**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Scope Addition | `w_util` and `w_prov` weight terms | SCOPE.md Goals §1-4 describe a unified formula for "all ranking signals"; the Background Research §Additional Signals section explicitly proposes adding w_util/w_prov. However the product vision WA-0 section shows a four-term formula (`w_sim`, `w_nli`, `w_conf`, `w_coac`). The six-term formula is an intentional extension, canonicalized by ARCHITECTURE.md ADR-001 with explicit rationale: every signal that influences ranking must be a learnable W3-1 dimension. This is documented scope evolution, not undiscovered scope creep. |
| Simplification | Eval harness gate waived | SCOPE.md Non-Goals and SPECIFICATION.md Constraint 7 both agree: no eval gate for crt-024 (formula-deterministic, no model). Product vision W1-3 specifies an eval gate for W1-4 (model-involved). WA-0 is formula math, not ML inference. Waiver is principled and documented. |
| Simplification | Config migration tooling absent | SCOPE.md Constraint §"Config migration tooling" and SPECIFICATION.md Constraint 8 explicitly scope this out; operators migrate manually. Acceptable for a formula-only change. |

---

## Variances Requiring Approval

### WARN-1: Six-Term Formula Extends Vision's Four-Term Illustrative Formula (SR-02)

**What**: The product vision WA-0 section shows:
```
score = w_sim * similarity_score
      + w_nli * nli_entailment_score
      + w_conf * confidence_score
      + w_coac * co_access_affinity
```
The architecture and specification implement a six-term formula that additionally includes `w_util * util_norm` and `w_prov * prov_norm`.

**Why it matters**: The product vision is the authoritative source of product direction (PRODUCT-VISION.md). Any deviation — even a principled one — requires explicit acknowledgement. The WA-0 section does not describe `utility_delta` or `PROVENANCE_BOOST` as named formula terms.

**Evidence of intentionality**: ARCHITECTURE.md ADR-001 is titled "Six-Term Formula Canonicalization" and states: "Six-term formula is the implementation target; vision's four-term formula was illustrative." SCOPE.md Background Research §"Additional Signals in Current Pipeline" explicitly raises the question and proposes the six-term answer. SPECIFICATION.md FR-01 repeats the explicit canonicalization: "The product vision's four-term illustrative formula is descriptive, not exhaustive."

**Recommendation**: Accept. The rationale is sound — `utility_delta` and `PROVENANCE_BOOST` already influence ranking today; leaving them outside the fused formula would recreate the structural defect WA-0 is designed to fix. The six-term formula is more aligned with the vision's stated goal ("it is not a retrieval engine with additive boosts") than a four-term formula that leaves two additive terms outside the formula. The scope agents correctly identified and resolved this in SCOPE.md and ADR-001. Human confirmation of acceptance is recommended for the record.

---

### WARN-2: R-01 Spec/Architecture Text Divergence on utility_delta Normalization

**What**: The RISK-TEST-STRATEGY.md identifies R-01 as a Critical risk: "Spec FR-05 specifies `÷ UTILITY_BOOST` with a clamp; architecture specifies `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`. These produce different values."

Reviewing the actual text:
- SPECIFICATION.md FR-05 states the correct shift-and-scale formula: `util_norm = (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)` — it does not say "divide by UTILITY_BOOST with a clamp."
- ARCHITECTURE.md §Signal Normalization Details also states the correct formula.
- The risk document's description of the divergence does not match what is actually written in the current spec and architecture texts.

**However**, the risk document was written during or after spec review. If R-01 described an earlier version of FR-05 that was corrected before final spec, the text divergence the risk agent saw may no longer exist. If FR-05 was already correct when the risk agent reviewed it, R-01 may be a false alarm. Either way, the risk register rates this Critical/High and the implementer will see it.

**Why it matters**: R-01 tells the implementer there is a divergence between FR-05 and the architecture. The implementer may doubt which formula is authoritative. If they read R-01 and act on its description of the divergence (rather than reading the spec directly), they may introduce a wrong implementation path.

**Recommendation**: The human should confirm: (a) FR-05's current text (shift-and-scale) is the final authoritative formula, and (b) R-01's description of the divergence was written against an earlier draft and no longer reflects the documents as filed. If confirmed, a one-line note should be added to R-01 in the risk document to prevent implementer confusion: "Note: FR-05 was updated to use shift-and-scale before finalisation; the divergence described here is resolved."

---

## Detailed Findings

### Vision Alignment

The product vision names the structural defect crt-024 fixes in explicit terms:

- **Critical Gap**: "Intelligence pipeline is additive boosts, not a learned function" — Status: **Roadmapped — Wave 1A + W3-1**
- **WA-0 mandate** (product vision, Wave 1A section): "WA-0 comes first. Before adding session-conditioned signals to the ranking pipeline, the pipeline's existing signals must be fused correctly. Adding more additive terms to a structurally broken formula makes the problem worse."
- **Vision's strategic framing**: "It is not a retrieval engine with additive boosts. It is a session-conditioned, self-improving relevance function."

All three source documents are faithful to these vision statements. The architecture explicitly positions crt-024 as "a prerequisite for the entire Wave 1A roadmap." The formula's config-driven weights are correctly characterized as W3-1's cold-start initialization point.

**No vision alignment issues found.**

---

### Milestone Fit

crt-024 targets Wave 1A (Adaptive Intelligence Pipeline). Prerequisites:
- W1-4 (NLI + Cross-Encoder) — **COMPLETE** (crt-023, PR #328). The NLI path crt-024 integrates is confirmed shipped.
- W0-1 (sqlx Migration) — **COMPLETE** (nxs-011, PR #299). The `spawn_blocking` removal context is in place.
- W1-5 (Observation Pipeline) — **IN PROGRESS** (col-023). Not a prerequisite for crt-024; the two are independent (confirmed in SCOPE.md Background Research §col-023 Interaction).

crt-024 explicitly does **not** pre-empt:
- WA-1 (Phase Signal): SPECIFICATION.md §NOT in Scope lists it first.
- WA-2 (Session Context): extension point documented in ARCHITECTURE.md §WA-2 Extension Point, not implemented.
- W3-1 (GNN training): formula provides initialization weights; training is explicitly out of scope.

**Milestone discipline: PASS. No future-milestone capabilities built in. Extension point for WA-2 is correctly documented but not implemented.**

---

### Architecture Review

**Strengths:**
- Formula correctness is numerically verified in ADR-003 for all three critical constraints (AC-11, Constraint 9, Constraint 10).
- All four scope open questions are resolved with ADRs (ADR-001 through ADR-004).
- SR-02, SR-03, SR-04, SR-07 from the scope risk assessment are all explicitly addressed with resolution evidence.
- WA-2 extension contract is defined precisely: `w_phase` as a new field, sum validation updated, default 0.0 means no-op until configured.
- Lock ordering is unchanged from crt-023 — no new deadlock surface.
- `apply_nli_sort` removal decision (ADR-002) is documented with the test migration requirement noted.

**Open questions passed to spec writer:** The architecture explicitly surfaces five open questions for the spec writer (OQ-01 through OQ-05). These are not omissions — they are correctly scoped handoffs. All five are addressed in the specification (FR-05 for OQ-03, FR-07/FR-08 for OQ-05, FR-09 for OQ-04, AC-01 for OQ-01, FR-11 for OQ-02).

**No architecture consistency issues found.**

---

### Specification Review

The specification covers all 14 SCOPE.md acceptance criteria (AC-01 through AC-14) with one-to-one mapping. Each AC in the specification adds explicit verification steps (unit tests, code review checks, integration tests) that were absent in the scope's AC definitions — a net improvement in testability.

**Domain models** (FusedSignals, ScoreWeights, BoostMap, StatusPenalty) are correctly aligned with the architecture's data flow and the formula's terminology.

**NFR coverage:**
- NFR-01 (no latency regression) — addressed; single pass replaces two passes.
- NFR-02 (score range guarantee) — addressed via shift-and-scale normalization for utility.
- NFR-03 (determinism) — explicitly stated.
- NFR-04 (no engine crate changes) — confirmed in constraints.
- NFR-05 (config backward compatibility) — covered by `#[serde(default)]` requirement and numerical verification.

**Boundary condition from SCOPE.md Constraint 10 (sim dominant over conf at defaults):** verified numerically in ARCHITECTURE.md and referenced in SPECIFICATION.md NFR-05. The linkage is clear.

**Minor observation**: SPECIFICATION.md AC-05 verification math contains an arithmetic note worth checking at implementation: the expected value is computed as `0.24+0.21+0.09+0.05+0.025+0.05 = 0.665`. The note `coac_raw=0.015` with `÷ MAX_CO_ACCESS_BOOST = 0.03` gives `coac_norm = 0.5`, then `0.10 * 0.5 = 0.05`. This is consistent. The computation is correct.

**No specification issues found beyond the R-01 text question documented in WARN-2.**

---

### Risk Strategy Review

The risk register covers 16 implementation risks across Critical, High, Medium, and Low priority tiers. Coverage is thorough for a formula-deterministic feature of this scope.

**Traceability**: All 9 scope risks (SR-01 through SR-09) are mapped to architecture risks or explicitly closed in the Scope Risk Traceability table in the risk document. No scope risk is dropped without a resolution record.

**Critical risks (R-01, R-04, R-05):**
- R-01: see WARN-2 above. The description may refer to a superseded draft; human confirmation recommended.
- R-04 (regression test churn): well-covered with specific audit procedures and reference to entry #751.
- R-05 (`apply_nli_sort` removal): correctly identified as Critical; test migration requirement explicitly stated.

**Security risks:**
- SeR-01 (untrusted weight config) — startup validation gate is the correct mitigation.
- SeR-02 (NaN propagation from division) — identified with specific division points named; all three are addressed in the spec and architecture with guards.
- SeR-03 (future hot-reload bypass) — correctly labeled as forward-looking; no mitigation required for crt-024.

**Edge cases (EC-01 through EC-07):** Appropriate coverage for the feature scope. EC-04 (all-zero NLI entailment scores) and EC-07 (candidate count mismatch) are implementation-facing and correctly surfaced for the implementer.

**No risk strategy gaps found.**

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` (via `mcp__unimatrix__context_search`) for "vision alignment patterns scope addition variance illustrative formula ranking pipeline" — Key findings: entry #2298 (config key semantic divergence pattern) is directly relevant to WARN-1; entry #2964 (signal fusion pattern) confirms the scope team correctly captured the structural defect class; no prior vision alignment reports found for comparison (this is the first crt-series feature with a vision guardian review in the knowledge base).
- Stored: nothing novel to store — WARN-1's "vision four-term formula is illustrative, not exhaustive" pattern could generalize, but it is feature-specific until a second feature exhibits the same pattern. The six-term expansion is justified by the vision's own prose ("not a retrieval engine with additive boosts"). If a future feature cites a vision formula as exhaustive when it is illustrative, revisit storing this as a pattern at that point.
