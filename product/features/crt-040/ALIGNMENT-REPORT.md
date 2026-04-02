# Alignment Report: crt-040

> Reviewed: 2026-04-02
> Artifacts reviewed:
>   - product/features/crt-040/architecture/ARCHITECTURE.md
>   - product/features/crt-040/specification/SPECIFICATION.md
>   - product/features/crt-040/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-040/SCOPE.md
> Scope risk: product/features/crt-040/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances the typed graph prerequisite for PPR and W3-1 GNN |
| Milestone Fit | PASS | Wave 1 / W1-1 typed graph foundation; crt-040 is a targeted restoration of a signal that was correctly removed then found to have a non-NLI substitute |
| Scope Gaps | WARN | R-01 (category resolution mechanism) flagged critical in risk strategy but architecture leaves the HashMap approach implicit — spec partially addresses but not with a mandatory AC |
| Scope Additions | PASS | No out-of-scope additions detected |
| Architecture Consistency | WARN | One factual discrepancy between ARCHITECTURE.md and SPECIFICATION.md on the `write_nli_edge` delegation strategy; minor but could mislead delivery |
| Risk Completeness | PASS | Risk strategy covers all 9 SCOPE-RISK-ASSESSMENT risks; adds 4 new risks (R-10 through R-13) with appropriate coverage |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap (WARN) | Category resolution HashMap pre-build | SCOPE.md §Constraints says Path C must not add a new HNSW scan and reuses `candidate_pairs: Vec<(u64, u64, f32)>` which has no category data. Architecture says "if let Some(...) = ... with continue on None" but never mandates or specifies the HashMap pre-build. Spec FR-01 says filter by `[source_category, target_category]` but does not specify how categories are looked up. RISK-TEST-STRATEGY R-01 correctly identifies this as critical and specifies the HashMap approach — but no AC formally mandates the HashMap over a per-pair DB lookup. A future delivery agent could implement the slower path and pass all ACs. |
| Gap (WARN) | Observability log when candidate_pairs is empty | RISK-TEST-STRATEGY R-06 specifies that a Path C observability log must fire unconditionally even when zero candidates are present. Architecture and specification do not include a corresponding FR or NFR; R-06 is only in the risk document. The delivery brief is the downstream consumer — if it only reads arch + spec, this requirement may be missed. |
| Simplification | `write_nli_edge` delegation vs. sibling | SCOPE.md §Proposed Approach says either add a `source: &str` parameter to `write_nli_edge` OR add a new `write_graph_edge` function. Architecture ADR-001 picks the sibling. SPECIFICATION.md FR-12 mandates `write_nli_edge` must NOT be modified. This is a valid simplification with full rationale. |
| Simplification | `inferred_edge_count` metric staleness | SCOPE.md §Resolved Decision §5 defers rename. Architecture, spec, and risk strategy all consistently accept this with follow-up issue reference. Fully traced. |

---

## Variances Requiring Approval

### WARN-01: Category Resolution Mechanism Not Formally Specified in AC Set

**What**: SCOPE.md and ARCHITECTURE.md both describe the category filter (AC-03: reject pairs with disallowed `[source_category, target_category]`), and RISK-TEST-STRATEGY R-01 identifies the category resolution mechanism as "MUST RESOLVE BEFORE DELIVERY" with a specific preferred approach (HashMap pre-build from `all_active`). However, no AC in SPECIFICATION.md mandates the HashMap approach or prohibits a per-pair DB lookup. The spec's AC-03 verifies the filter output but not the implementation path. A delivery agent could satisfy AC-03 with an O(n) per-pair DB lookup, which violates the spirit of the no-new-scan constraint and degrades hot-path performance.

**Why it matters**: SCOPE.md §Constraints mandates no new HNSW scan. A per-pair DB lookup does not add an HNSW scan, so it technically satisfies the constraint — but it adds per-tick SQL round-trips that are architecturally equivalent in cost on a hot path. The risk strategy explicitly prefers the HashMap approach. Without a formal AC, the delivery brief may not mandate it.

**Recommendation**: Add an explicit implementation brief instruction (or a new AC in the specification) mandating the HashMap pre-build: "After Phase 2 loads `all_active`, build `HashMap<u64, String>` mapping entry_id to category before entering the Path C loop. Per-pair DB lookups in the Path C loop are prohibited." This is a one-line addition to the implementation brief and does not require rework of the specification.

---

### WARN-02: Path C Observability Log Not in Specification (Only in Risk Strategy)

**What**: RISK-TEST-STRATEGY R-06 specifies that Path C must emit a structured observability log unconditionally — even when `candidate_pairs` is empty — reporting `cosine_supports_candidates` and `cosine_supports_edges_written`. This requirement does not appear in ARCHITECTURE.md or SPECIFICATION.md as an FR or NFR. The test scenarios in R-06 are test coverage requirements, but if the delivery agent only reads arch + spec + the implementation brief, this observability requirement is invisible.

**Why it matters**: ADR-003 (referenced in Architecture) is said to require a Path C observability log. However, if the implementation brief references only the spec for behavioral requirements, the log will be omitted. The risk strategy's test scenarios cannot be satisfied by a log that is never implemented.

**Recommendation**: Add NFR-09 to SPECIFICATION.md (or propagate to implementation brief): "Path C must emit a structured `debug!` or `info!` log after the write loop completes, unconditionally, with fields `cosine_supports_candidates = <count>` and `cosine_supports_edges_written = <count>`. This fires even when both counts are zero." This closes the observability gap before delivery.

---

### WARN-03: Typo in AC-08 Test Assertion Identifier

**What**: SPECIFICATION.md AC-08 contains the text: "Verification: unit test asserts `EDGE_SOURCE_CO_ACCESS == "cosine_supports"`". The constant named is `EDGE_SOURCE_CO_ACCESS`, which is the existing co-access constant, not the new `EDGE_SOURCE_COSINE_SUPPORTS` constant. This is a copy-paste error in the test verification description. The correct assertion should be `EDGE_SOURCE_COSINE_SUPPORTS == "cosine_supports"`. Pattern #3337 from prior features shows that specification test assertion strings are used directly by testers — an incorrect constant name in a test verification statement leads testers to assert against the wrong value.

**Why it matters**: A tester or delivery agent who follows AC-08's verification description literally would write a test asserting `EDGE_SOURCE_CO_ACCESS == "cosine_supports"` — which tests the wrong constant (co_access) and would also fail because `EDGE_SOURCE_CO_ACCESS == "co_access"`. The feature would ship without a test for `EDGE_SOURCE_COSINE_SUPPORTS`.

**Recommendation**: Correct SPECIFICATION.md AC-08 verification text from `EDGE_SOURCE_CO_ACCESS` to `EDGE_SOURCE_COSINE_SUPPORTS`. This is a one-character-group correction with no scope impact.

---

### WARN-04: write_nli_edge Delegation Description Inconsistency (Architecture vs. Spec)

**What**: ARCHITECTURE.md states: "`write_nli_edge` is refactored to delegate to `write_graph_edge` with `source = "nli"`". SPECIFICATION.md FR-06 states: "`write_nli_edge` is NOT modified. It remains as a thin wrapper or independent function using the hardcoded `'nli'` source." These two descriptions are substantively different: "refactored to delegate" implies internal restructuring of `write_nli_edge`; "NOT modified" implies the function body is left as-is. For the purposes of correctness the outcome is the same (callers see identical behavior), but RISK-TEST-STRATEGY R-02 lists this as High severity because incorrect delegation silently retags edges. A delivery agent reading architecture first may attempt to refactor `write_nli_edge` when the spec forbids it.

**Why it matters**: If a delivery agent follows the architecture description and refactors `write_nli_edge` to delegate to `write_graph_edge`, but introduces the wrong `source` argument in the delegation, all Informs edges begin writing with `source='cosine_supports'`. This is the exact failure mode described in R-02 and ADR-001. The spec's "NOT modified" instruction is correct and safer.

**Recommendation**: Align architecture wording with spec. In ARCHITECTURE.md, change "refactored to delegate to `write_graph_edge`" to "NOT modified — `write_nli_edge` retains its hardcoded `source='nli'`; `write_graph_edge` is a sibling function added alongside it." This eliminates the delivery ambiguity.

---

## Detailed Findings

### Vision Alignment

crt-040 directly serves the product vision's typed knowledge graph trajectory. The vision states (Wave 1 / W1-1): "A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency." The `Supports` edge type is the "support" relationship in that list. The vision also explicitly ties the graph's density to the PPR expander and GNN training pipeline (W3-1): "The graph, the confidence system, the observation pipeline, and the GNN are all inputs to this function."

crt-040 restores `Supports` edge production after crt-038 deleted the NLI path that originally produced them. Without `Supports` edges, the graph consists of only `CoAccess` and `Informs` edges — missing one of the three primary typed relation types that W3-1 GNN feature construction depends on. The vision states the intelligence pipeline is "not a retrieval engine with additive boosts" — restoring the entailment signal is prerequisite to that vision.

The choice of cosine similarity as the Supports detection mechanism is consistent with the vision's "graceful degradation" principle (W1-4): the vision explicitly states "absent or hash-invalid model file → server starts on cosine fallback." Using cosine detection as a permanent structural path (not just a fallback) is a deliberate scope choice, well-supported by ASS-035 empirical validation, and consistent with the vision's tolerance for tiered inference approaches.

**PASS.**

---

### Milestone Fit

crt-040 belongs to the Cortical phase (learning and drift). It is positioned as Group 2 completion work — after crt-038 (conf-boost-c formula) and crt-039 (NLI gate decoupling), and before Group 4 (PPR expander). The prerequisite chain is correct: crt-039 is confirmed merged (PR #486), which makes `structural_graph_tick` unconditional — the exact condition Path C requires.

The feature targets Wave 1 graph enrichment infrastructure. It does not implement Group 4 (PPR expander) or W3-1 (GNN), correctly deferring those. The SCOPE.md non-goals are explicit and the source documents honor them: no PPR implementation, no GNN, no S1/S2/S8 edge sources. This is proportional scope for a Cortical phase graph signal restoration feature.

**PASS.**

---

### Architecture Review

The architecture is technically sound, well-structured, and consistent with the existing codebase conventions (EDGE_SOURCE_* pattern, InferenceConfig dual-site default, INSERT OR IGNORE dedup). The UNIQUE constraint verification (SR-04) is explicitly resolved with DDL evidence from four sources — this is exemplary resolution of a high-severity scope risk.

Four concerns worth noting:

1. **write_nli_edge description inconsistency** (WARN-04 above). Architecture says "refactored to delegate"; spec says "NOT modified." The spec is correct.

2. **Category resolution left implicit.** Architecture section "Error Handling Strategy" says "`if let Some(...) = ... with continue on None`" for the category lookup but never says what data structure the lookup targets. The reader must infer that `all_active` is the source and that a HashMap is the intended lookup mechanism. RISK-TEST-STRATEGY R-01 makes this explicit; the architecture does not. See WARN-01.

3. **ADR-003 referenced but not reproduced.** The architecture references ADR-003 for the Path C placement decision and the observability log requirement, but ADR-003 is stored in Unimatrix, not reproduced in the architecture document. The observability log requirement visible in R-06 is therefore missing from the spec. See WARN-02.

4. **Integration point table (Integration Surface section) is well-executed.** All new and consumed interfaces are enumerated with types and sources. Delivery has a clear contract surface.

---

### Specification Review

The specification is comprehensive, with 18 acceptance criteria (15 from SCOPE.md + 3 added). The additions (AC-16 `impl Default` trap, AC-17 `nli_post_store_k` removal grep, AC-18 serde forward-compatibility) are appropriate extensions that cover known delivery risks.

Three issues:

1. **AC-08 typo** (WARN-03): `EDGE_SOURCE_CO_ACCESS` where `EDGE_SOURCE_COSINE_SUPPORTS` is intended in the test verification text.

2. **Observability log absent from spec** (WARN-02): R-06 requires unconditional log emission; no matching FR or NFR in spec.

3. **Category resolution mechanism in FR-01**: FR-01 specifies the filter condition but not the resolution mechanism. Spec says "source_category and target_category for each pair" but does not say how they are obtained. The HashMap pre-build from `all_active` must be explicit in the implementation brief if not in the spec.

The dual-site config enforcement (AC-16) and the merge function update (FR-08) are correctly specified and cross-referenced. The Knowledge Stewardship section in the spec is valuable — it shows which prior patterns directly informed design choices, giving reviewers confidence that known traps were consulted.

---

### Risk Strategy Review

The RISK-TEST-STRATEGY is the strongest of the three source documents in terms of gap coverage. It correctly:

- Elevates R-01 (category resolution) to Critical — the correct severity, given that `candidate_pairs` has no category data and the architecture leaves this implicit.
- Adds R-13 (config merge function) not in SCOPE-RISK-ASSESSMENT — this is a genuine gap addition, not a scope addition. Pattern #4013 is cited correctly.
- Provides actionable test scenarios for all 13 risks, including the edge cases table.
- Correctly resolves all 9 SCOPE-RISK-ASSESSMENT risks in the traceability table.

The security risks section is proportional: the primary attack surface is `supports_cosine_threshold` misconfiguration, which is mitigated by range validation. No new injection surfaces are introduced. The blast radius analysis (graph flooding, recoverable via `context_status(maintain=true)`) is accurate.

The only gap: R-06 (observability log) specifies requirements not reflected in the spec or architecture. The risk strategy cannot be the primary specification vehicle for implementation requirements — it is a testing and review instrument. The log requirement must migrate to the spec before delivery. See WARN-02.

The budget counter semantics edge case in the failure modes table ("Counter must only increment on `true` return from `write_graph_edge`") is a subtle and important correctness requirement not stated in the spec. This should also be captured in the implementation brief.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — found patterns #3742, #3337, #3771, #3231, #3158 via context_search. Most directly applicable: #3742 (architecture and risk diverge from scope deferral — WARN pattern), #3337 (architecture diagram strings diverge from spec — testers assert against wrong strings, which maps directly to WARN-03 in this report).
- Stored: nothing novel at this time. WARN-03 (spec test assertion typo with wrong constant name) matches pattern #3337 exactly — the prior pattern already exists and was applied. WARN-01 (category resolution mechanism not formally in AC set despite being "critical" in risk strategy) is feature-specific in mechanism but the class of gap (risk strategy requirement not propagated to spec) may recur. If this pattern recurs in crt-041 or Group 4, store as a pattern then.
