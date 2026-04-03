# Alignment Report: crt-045

> Reviewed: 2026-04-03
> Artifacts reviewed:
>   - product/features/crt-045/architecture/ARCHITECTURE.md
>   - product/features/crt-045/specification/SPECIFICATION.md
>   - product/features/crt-045/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-045/SCOPE.md
> Scope risk source: product/features/crt-045/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Fix directly enables the W1-3 eval harness gate condition; no vision drift |
| Milestone Fit | PASS | Correctly scoped to Wave 1 infrastructure; no future-milestone capabilities introduced |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria addressed in all three documents |
| Scope Additions | WARN | SPECIFICATION.md adds C-10 (`#[cfg(test)]` guard option on accessor) not present in SCOPE.md; acceptable but undocumented in scope |
| Architecture Consistency | PASS | Architecture, specification, and risk documents are internally consistent and mutually reinforcing |
| Risk Completeness | PASS | All six SCOPE-RISK-ASSESSMENT.md risks (SR-01 through SR-06) resolved in RISK-TEST-STRATEGY.md with dedicated test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SR-02 (rebuild timeout) | SCOPE.md recommended adding `tokio::time::timeout` if sqlx timeout is not configured. SPECIFICATION.md and RISK-TEST-STRATEGY.md accept this as residual risk with an explicit follow-up recommendation. Rationale documented in SPECIFICATION.md "NOT in Scope" section and R-07 scenario. Acceptable simplification. |
| Addition | C-10 — `#[cfg(test)]` guard on `typed_graph_handle()` | SPECIFICATION.md Constraint C-10 introduces an option to gate the accessor body with `#[cfg(test)]` if only used in test code. This conditional compilation detail is not in SCOPE.md. It is a minor implementation detail rather than a scope change, but it is not explicitly approved in SCOPE.md. |
| Addition | EC-05 edge case — `distribution_change=false` with explicit targets block | RISK-TEST-STRATEGY.md documents EC-05 (both flag false and targets block present should not cause parse error). Not present in SCOPE.md but is a natural edge case that falls within the TOML fix scope. Informational. |

---

## Variances Requiring Approval

None. No VARIANCE or FAIL classifications were assigned. The WARN on Scope Additions is informational — see Detailed Findings below.

---

## Detailed Findings

### Vision Alignment

**PASS.**

The product vision (W1-3 Evaluation Harness) states:

> "Gate condition for W1-4: Eval results show measurable improvement on a representative query set before model ships."
> "Gate condition for W3-1: Hook simulation client validates GNN training label quality on synthetic behavioral patterns before production deployment."

crt-045 repairs the fundamental defect that made the eval harness unable to distinguish between the baseline and PPR+graph_expand profiles. Without this fix, the W1-3 gate condition for W1-4 and W3-1 cannot be validated through the offline harness — both profiles produce bit-identical results regardless of configuration, because `use_fallback = true` prevents all graph-dependent search phases from executing.

The fix is minimal and targeted: a single `TypedGraphState::rebuild()` call inside `EvalServiceLayer::from_profile()`. No new capabilities are introduced. No architectural patterns are altered. The fix makes the eval harness do what the vision already requires it to do.

The SCOPE.md background research section explicitly traces how the config wiring from TOML through `UnimatrixConfig` → `InferenceConfig` → `SearchService` is correct and complete — the missing piece is the rebuild call, not any config or search logic.

All three source documents are consistent with this vision framing.

---

### Milestone Fit

**PASS.**

crt-045 is a Wave 1 infrastructure correction. Specifically, it restores the W1-3 eval harness to its intended function, which is a prerequisite for validating Wave 1A intelligence pipeline features (WA-0 through WA-4, W3-1 GNN training).

The architecture and specification make no attempt to introduce Wave 1A or Wave 2 capabilities:

- No new `InferenceConfig` fields are added (SCOPE.md Non-Goal, confirmed in SPECIFICATION.md "NOT in Scope").
- No changes to `ServiceLayer::with_rate_config()` signature.
- No changes to `SearchService`, `graph_expand`, or PPR algorithms.
- No new runner or report capabilities (`eval scenarios`, `eval report`, `run_eval.py` are explicitly out of scope in all documents).
- No HTTP transport, containerization, or OAuth-related changes.

The TOML fix (`ppr-expander-enabled.toml`) is also milestone-appropriate: it resolves a parse failure that predates crt-045 and blocks the harness from reaching the graph fix at all.

The threshold values (`mrr_floor = 0.2651`, `p_at_5_min = 0.1083`) are grounded in measured baseline metrics from crt-042, not invented floors. SCOPE.md documents the OQ-01 resolution explicitly, and SPECIFICATION.md C-06 mandates delivery agents use these exact values.

---

### Architecture Review

**PASS.**

The architecture document accurately describes the system as it stands and the minimal surgery required:

- The component table in "Component Breakdown" correctly identifies the five affected components (layer.rs, typed_graph.rs, services/mod.rs, search.rs, layer_tests.rs) and one TOML file.
- The interaction diagram (ASCII flow in "Component Interactions") precisely documents the two new steps: Step 5b (rebuild call) and Step 13b (post-construction write-back).
- The post-construction write rationale is sound and backed by code-level evidence (`services/mod.rs:419` confirmed as `Arc::clone()`). SR-01 from SCOPE-RISK-ASSESSMENT.md is addressed as a resolved item with a note that the live search call in AC-06 provides runtime confirmation.
- The NLI handle init precedent (crt-023) is correctly cited as the existing pattern that the graph rebuild follows — with an accurate explanation of why it deviates at Step 3 (pre-populated handle parameter not supported by `with_rate_config()` without signature change).
- Five ADRs are referenced for the five non-trivial decisions. All five correspond to decisions visible in SCOPE.md open questions (OQ-01 through OQ-04) and constraints.

One minor observation (informational, not a variance): the Architecture document notes in "Open Questions" that all OQs are resolved, but then states "The architect must verify SR-01 (that `SearchService` holds `Arc::clone()` of the handle, not a value copy) before committing to the write-after-construction approach." This is written in the future tense, as a delivery pre-condition, even though SR-01 is listed as resolved in the "Constraints Checklist." The document is internally consistent — this is a pre-implementation read task, not an unresolved architectural question — but future readers may find the mixed tense confusing. This is not a variance.

---

### Specification Review

**PASS.**

The specification covers all SCOPE.md goals with fidelity:

- FR-01 through FR-08 map directly to SCOPE.md Goals 1 through 4 and Acceptance Criteria AC-01 through AC-08.
- All eight SCOPE.md acceptance criteria are reproduced verbatim or with equivalent wording in SPECIFICATION.md.
- All eight SCOPE.md non-goals are reproduced in the SPECIFICATION.md "NOT in Scope" section.
- All eight SCOPE.md constraints (C-01 through C-08) are reproduced and a ninth (C-09) and tenth (C-10) are added.

**C-09 addition** (Active entries + edges in test fixture): directly derives from SR-06 in SCOPE-RISK-ASSESSMENT.md. The scope risk assessment explicitly recommended seeding the fixture with Active entries and S1/S2/S8 edges; C-09 formalizes that as a constraint. This is a scope-consistent elaboration, not an addition.

**C-10 addition** (`#[cfg(test)]` guard option on accessor): This is the one item not rooted in SCOPE.md or SCOPE-RISK-ASSESSMENT.md. C-10 states: "The `typed_graph_handle()` accessor body MAY use `#[cfg(test)]` if it is only invoked from test code." This is a Rust-specific implementation consideration that the architect determined was worth documenting. It does not expand scope — it restricts the accessor further than required — but it is not explicitly approved in SCOPE.md.

Classification: **WARN**. This is a minor, scope-conservative addition. It requires no human approval to proceed, but should be noted.

The TOML schema section in SPECIFICATION.md provides precise, human-approved values for all three gate fields, including the note about why `distribution_change = false` is intentional (OQ-01). This is well-specified.

---

### Risk Strategy Review

**PASS.**

The RISK-TEST-STRATEGY.md comprehensively addresses all risks:

- All six scope risks from SCOPE-RISK-ASSESSMENT.md (SR-01 through SR-06) appear in the "Scope Risk Traceability" table with explicit resolution status and references to architecture risks (R-01 through R-10) and test scenarios.
- SR-01 (post-construction write propagation) is resolved via code-level confirmation at `services/mod.rs:419` and reinforced by the three-layer AC-06 test assertion requirement.
- SR-05 (wired-but-unused anti-pattern) is elevated to High priority and given dedicated three-layer test coverage (handle state + graph connectivity + live search call). The citation of entry #3935 (prior gate failure on structural-only coverage) is appropriate and directly applicable.
- SR-06 (Quarantined entries in test fixture producing vacuous pass) is addressed by C-09 in SPECIFICATION.md and scenario R-03, which requires at least two Active entries with a confirmed S1/S2/S8 edge via raw SQL.
- SR-02 (rebuild timeout) is explicitly accepted as residual risk with a follow-up recommendation. This is the one simplification relative to SCOPE.md's recommendation; it is documented and justified.
- SR-03 (accessor visibility) is addressed by ADR-004 and classified as a PR review gate (compile-time enforcement by Rust).
- SR-04 (future `distribution_change=true` regression) is addressed by requiring a TOML comment explaining the intentional `false` value, plus a parse-time unit test.

The four "non-negotiable test scenarios" in the Coverage Summary are traceable to specific acceptance criteria (AC-05, AC-06, AC-07, AC-08) and risk items (R-02, R-03, R-04, R-06). This gives the delivery agent a clear minimum test bar.

Integration risks IR-01 through IR-04 are all new additions relative to SCOPE-RISK-ASSESSMENT.md. They are appropriate elaborations, not scope additions:
- IR-01 (future `with_rate_config()` refactor) is a documentation requirement, not a code change.
- IR-02 (pre-crt-021 snapshots) is a pre-existing constraint on snapshot age, not a new scope item.
- IR-03 (VectorIndex artifact must accompany snapshot) cites lesson-learned entry #2661 from a prior feature — this is knowledge reuse, not scope addition.
- IR-04 (`find_terminal_active` visibility) is a delivery pre-condition noted for the implementation agent.

Security risks SR-SEC-01 through SR-SEC-03 are proportionate to the feature. crt-045 is a bug fix on an internal eval tool; the blast radius of any security concern is bounded to degraded eval run behavior. The analysis is accurate.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found entries #2298 (config key semantic divergence pattern, not applicable to crt-045), #3742 (optional future branch in architecture must match scope intent, applicable: architecture correctly defers all future-branch work), #3337 (architecture diagram informal headers diverge from spec, not present here — all headers match), #3426 (formatter regression risk, not applicable).
- Stored: nothing novel to store — variances found are feature-specific (C-10 `#[cfg(test)]` guard option). The pattern of scope-risk risks being elevated to specification constraints (SR-06 → C-09) is already captured in general form. No cross-feature generalization warranted.
