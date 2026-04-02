# Alignment Report: crt-038

> Reviewed: 2026-04-02
> Artifacts reviewed:
>   - product/features/crt-038/architecture/ARCHITECTURE.md
>   - product/features/crt-038/specification/SPECIFICATION.md
>   - product/features/crt-038/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-038/SCOPE.md
> Scope risk source: product/features/crt-038/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly improves the intelligence pipeline — a stated high priority in the vision |
| Milestone Fit | PASS | Correctly targets Wave 1A (WA-0 correction), not a future wave |
| Scope Gaps | WARN | One scope item underspecified in ARCHITECTURE.md: `write_edges_with_cap` deletion is not listed in the Integration Surface table, though R-05 and AC-11 cover it via the risk strategy |
| Scope Additions | PASS | No out-of-scope work introduced in any source document |
| Architecture Consistency | PASS | Architecture, specification, and risk strategy are internally consistent; ADR references cross-check correctly |
| Risk Completeness | WARN | RISK-TEST-STRATEGY.md R-03 (eval baseline measured on wrong path) is rated Critical/High but no test scenario fully resolves it — it is resolved by a delivery-time human action (PR description evidence), not an automated gate |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | `write_edges_with_cap` deletion not in Integration Surface table | ARCHITECTURE.md's Integration Surface table (page listing retained/deleted symbols) omits `write_edges_with_cap`. However, R-05 in RISK-TEST-STRATEGY.md and AC-11 (clippy gate) indirectly require its deletion. The gap is that the architecture does not explicitly mandate its removal — delivery must infer this from the risk register, not from a direct architectural specification. Low delivery risk because R-05 is explicit; non-zero documentation gap. |
| Simplification | AC-02 new unit test count | SCOPE.md's AC-02 states "Two new unit tests are required"; SPECIFICATION.md repeats this verbatim; RISK-TEST-STRATEGY.md R-01 scenario 3 adds a third test (`test_effective_renormalization_still_fires_when_w_nli_positive`) not named in SCOPE.md or SPECIFICATION.md. This is a scope addition in the risk strategy only. It is a beneficial addition (verifies the guard does not suppress positive-w_nli re-normalization) but was not explicitly requested in SCOPE.md. Acceptable simplification direction. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications were found. Both items below are WARNs for human awareness, not blockers.

---

## Detailed Findings

### Vision Alignment

The product vision states (Intelligence & Confidence section):

> "Intelligence pipeline is additive boosts, not a learned function — High — Roadmapped Wave 1A + W3-1"
> "WA-0: Ranking Signal Fusion — COMPLETE (crt-024, PR #336). Six-term fused linear combination. NLI is dominant at w_nli=0.35 as W3-1's initialization point."

crt-038 directly corrects the shipped WA-0 formula. The vision's W3-1 is the learned function that replaces the manual formula; crt-038 improves the manual formula that W3-1 will eventually supersede. This is fully aligned — the vision states WA-0's formula is the "initialization point" for W3-1, making accuracy of that initialization directly relevant to the pipeline's strategic trajectory.

The vision further states: "It is not a retrieval engine with additive boosts. It is a session-conditioned, self-improving relevance function." crt-038 removes three NLI code paths confirmed to contribute zero MRR lift, improving result quality for every agent query immediately. This supports the vision's "self-improving" principle: the system's own eval harness (W1-3) identified the problem; the fix is grounded in measured data (ASS-035/037/039).

Dead-code removal of `run_post_store_nli` also reduces attack surface (RISK-TEST-STRATEGY.md Security Risks section), consistent with the vision's security integrity emphasis ("tamper-evident from first write to last").

**PASS.**

### Milestone Fit

crt-038 belongs to the Cortical phase (crt-*) and targets a correction to WA-0 (Wave 1A). The vision marks WA-0 as COMPLETE at crt-024 and explicitly notes the eval harness (W1-3) as the gate for intelligence changes. crt-038 uses that harness (AC-12: MRR ≥ 0.2913 on `product/research/ass-039/harness/scenarios.jsonl`).

The feature does not introduce Wave 2 capabilities (containerization, OAuth, HTTP transport) or Wave 3 capabilities (GNN training, learned function). The retained NLI infrastructure (`NliServiceHandle`, `run_graph_inference_tick`) is correctly scoped as Wave 1A forward — Group 2 tick decomposition is explicitly called out as a separate future feature.

No feature carries work that belongs in a future wave. No wave's milestone constraint is violated.

**PASS.**

### Architecture Review

The architecture document is thorough and internally consistent.

**Component 1 (FusionWeights::effective short-circuit):** The AC-02 specification is precise. The architecture correctly identifies that re-normalization with a zero `w_nli` denominator term produces `w_sim'≈0.588, w_conf'≈0.412` instead of the intended `w_sim=0.50, w_conf=0.35`, and mandates the `w_nli == 0.0` short-circuit before the `nli_available` branch. This is supported by Unimatrix entry #4003 (cited in SPECIFICATION.md Knowledge Stewardship).

**Component 2 (config.rs defaults):** All six weight changes are specified with exact values. The sum constraint (0.85 ≤ 1.0) is verified. The interaction with phase signal terms (w_phase_histogram=0.02, w_phase_explicit=0.05) is documented — total 0.92, unchanged.

**Components 3/4/5 (dead-code removal):** Each removal is precisely scoped to function names, line numbers, and cross-module import paths. The distinction between `NliStoreConfig` (deleted) and `InferenceConfig` fields of the same names (retained) is explicitly called out (Integration Points section) — this is the most likely confusion point for delivery and is well-handled.

**Symbol Checklist:** The architecture provides a grep-verifiable checklist for deleted and retained symbols. This directly addresses SR-03 from SCOPE-RISK-ASSESSMENT.md.

**Gap noted:** The Integration Surface table does not explicitly list `write_edges_with_cap` as a symbol to be deleted. R-05 in RISK-TEST-STRATEGY.md requires its deletion (and provides the grep verification command), but a reader relying solely on the architecture's Integration Surface table would not find this requirement. This is a documentation gap, not a functional risk — R-05 is clear and AC-11 enforces it via clippy. Classified as WARN.

**ADR cross-references:** ADR-001 through ADR-004 are referenced consistently across architecture, specification, and risk strategy. No conflicting ADR claims found.

**PASS overall; one WARN documented.**

### Specification Review

The specification translates every SCOPE.md goal and acceptance criterion into verifiable functional requirements.

**FR-01 through FR-08 (formula change):** All seven weight changes and the `effective()` short-circuit requirement are present and match SCOPE.md §Goals exactly.

**FR-09 through FR-17 (dead-code removal):** All three removal groups are specified with file names, line numbers, and symbol names. Deletion lists match the architecture's symbol checklist.

**FR-18 through FR-20 (test cleanup):** The spec provides complete deleted test symbol lists (13 from `nli_detection.rs`, 4 from `background.rs`) and modified test lists (3 from config.rs and search.rs). These are grep-verifiable.

**AC-02 two-test vs. three-test discrepancy:** SCOPE.md AC-02 states "Two new unit tests are required." SPECIFICATION.md repeats two required tests verbatim (AC-02 section). RISK-TEST-STRATEGY.md R-01 adds a third scenario (`test_effective_renormalization_still_fires_when_w_nli_positive`) not named in SCOPE.md or SPECIFICATION.md. The risk strategy's third test is protective and correct (verifies the guard does not suppress positive-w_nli behavior), but it was not formally included in the specification's AC-02. Delivery will likely add this test; it is not a problem unless a strict "no scope additions" policy applies. Classified as WARN (human should be aware that the risk strategy adds one test not in the spec).

**Ordering constraint:** The specification's ordering constraint (FR-01 through FR-08, then eval, then removals) matches SCOPE.md's proposed approach and the architecture's Implementation Ordering Constraints. Consistent across all three source documents.

**Open Question 1 (eval scoring path assumption):** SPECIFICATION.md §Open Questions correctly surfaces the ASS-039 baseline validity risk and requires delivery to verify the code path before accepting AC-12. This is the same concern as SCOPE-RISK-ASSESSMENT.md §Assumptions and RISK-TEST-STRATEGY.md R-03. The three documents agree on the risk and on the resolution mechanism (delivery-time PR evidence). There is no automated test that can resolve R-03 — it requires a human to inspect the ASS-039 harness configuration. This is inherent to the risk and not a documentation failure.

**PASS overall; one WARN documented.**

### Risk Strategy Review

The risk register covers 11 risks across 4 priority levels. All risks map to verifiable test scenarios.

**Coverage of critical risks:**

- **R-01 (effective() short-circuit omitted/misplaced):** Three test scenarios. Covers `effective(false)` with zero `w_nli`, `effective(true)` with zero `w_nli`, and `effective(false)` with positive `w_nli`. Complete.
- **R-02 (eval run before AC-02):** Requires delivery to pass `cargo test --workspace` first and include commit hash in eval output. Procedurally enforced, not automated. Acceptable.
- **R-03 (ASS-039 baseline on wrong scoring path):** Requires delivery to inspect ASS-039 harness configuration and state the scoring path in the PR description. This is a mandatory PR gate — merge is blocked without this determination (per SPECIFICATION.md AC-12). The risk is real (SCOPE-RISK-ASSESSMENT.md §Assumptions agrees) and the resolution is correctly specified. No automated test can close R-03 because the question is about historical eval configuration, not current code behavior. The risk strategy is correct to identify this as requiring human action.

Observation: R-03 is marked Critical/High in severity/likelihood but its only test scenario is "inspect harness config." This is appropriate given the nature of the risk, but the human reviewer should be aware that R-03's resolution is entirely procedural. If delivery misses or glosses over the PR description requirement, there is no automated backstop.

**Coverage of integration risks:**

- **R-04 (shared helpers accidentally deleted):** Three verification steps. The grep command checking `nli_detection.rs` for exactly three `pub(crate)` definitions is specific and correct.
- **R-05 (write_edges_with_cap retained as dead code):** Two verification steps. The `grep -r "write_edges_with_cap" crates/` command is the definitive check. This is the only place in all three source documents where `write_edges_with_cap` deletion is mandated — this confirms the architecture's Integration Surface table gap noted above.
- **R-06 (residual removed-symbol references):** References entry #2758 (gate-3c symbol retention failure) from Unimatrix, demonstrating knowledge stewardship was applied.
- **R-07 (NliStoreConfig partial deletion):** grep for `NliStoreConfig` and `nli_store_cfg` across full workspace. Correctly identifies `mod.rs` as the most likely residual location.

**SR cross-traceability table:** The Scope Risk Traceability table at the end of RISK-TEST-STRATEGY.md maps all 7 SCOPE-RISK-ASSESSMENT.md risks (SR-01 through SR-07) to architecture risks (R-01 through R-11) and resolution strategies. This is complete and consistent.

**Security risks section:** Confirms no new untrusted input surfaces; notes that `run_post_store_nli` removal shrinks the attack surface. Consistent with the vision's security integrity principles.

**PASS overall; one WARN documented (R-03 resolution is procedural only).**

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` (via `mcp__unimatrix__context_search`) for topic `vision`, category `pattern` — found 3 results: #2298 (config key semantic divergence), #3337 (architecture diagram informal headers diverge from spec), #3742 (optional future branch in architecture must match scope intent). None of these patterns manifested in crt-038's documents — the feature's architecture, specification, and risk strategy are tightly aligned with SCOPE.md.
- Stored: nothing novel to store — the misalignment patterns observed in prior features (header divergence, deferred-branch scope additions) did not occur here. The crt-038 documents show notably tight scope discipline: non-goals are enumerated identically across SCOPE.md, SPECIFICATION.md, and ARCHITECTURE.md. If this level of scope discipline continues across the Cortical phase, a positive pattern entry ("tight non-goals enumeration prevents scope drift in surgical removal features") may be warranted after additional data points.
