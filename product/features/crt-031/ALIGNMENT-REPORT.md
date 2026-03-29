# Alignment Report: crt-031

> Reviewed: 2026-03-29
> Artifacts reviewed:
>   - product/features/crt-031/architecture/ARCHITECTURE.md
>   - product/features/crt-031/specification/SPECIFICATION.md
>   - product/features/crt-031/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-031/SCOPE.md
> Scope risk source: product/features/crt-031/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances W0-3 domain-agnosticism; provides the categorical guard prerequisite for ASS-032 Retention work |
| Milestone Fit | PASS | Scoped as policy-layer infrastructure; defers all #409 mechanics; no future-milestone capabilities built |
| Scope Gaps | PASS | All 10 SCOPE.md goals and all 23 original ACs are present in source documents |
| Scope Additions | WARN | `list_adaptive()` public method and `lifecycle.rs` stub file added beyond SCOPE.md; both minor and internally motivated |
| Architecture Consistency | PASS | All SCOPE open questions resolved before implementation; construction-site enumeration for StatusService wiring is incomplete (caught by risk strategy) |
| Risk Completeness | PASS | All 9 scope risks (SR-01 through SR-09) traced; 11 runtime risks with scenario coverage; three Critical risks have adequate test plans |

**Overall: PASS with one WARN.** No variances require blocking approval before proceeding.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `CategoryAllowlist::list_adaptive()` public method | ARCHITECTURE.md §Component 1 "New public methods" adds `list_adaptive() -> Vec<String>`. Not listed in SCOPE.md goals or ACs. Used internally by the maintenance tick stub (one lock acquisition instead of per-category `is_adaptive()` calls per R-06) and `context_status` population. Not an MCP-facing surface. Well-reasoned. |
| Addition | `lifecycle.rs` reserved stub module | ARCHITECTURE.md creates `infra/categories/lifecycle.rs` as an initially-empty reserved file for future lifecycle extensions. SCOPE.md describes the module split but does not reference a `lifecycle.rs` file. Impact: one near-empty file committed. |
| Simplification | `BackgroundTickConfig` composite struct deferred | SCOPE-RISK-ASSESSMENT SR-02 recommended evaluating a composite struct for `spawn_background_tick`. Architecture explicitly defers this as OQ-05 out of scope for crt-031. Rationale documented: existing `#[allow(clippy::too_many_arguments)]` is in place; 22→23 parameters is acceptable. |
| Simplification | All four SCOPE open questions resolved in architecture | OQ-1 (constructor API), OQ-2 (status format), OQ-3 (`add_category` behavior), OQ-5 (eval harness path) all resolved before implementation. OQ-4 (test count) correctly treated as non-binding. |

---

## Variances Requiring Approval

None. The two scope additions (WARN) are internal implementation details with no product-direction implications. They are surfaced for human awareness, not approval.

---

## Detailed Findings

### Vision Alignment

**Finding: PASS**

The product vision's "Critical Gaps — Domain Coupling" table lists two entries directly relevant to this feature:

- `"lesson-learned" category name hardcoded in scoring` — Status shown as **Fixed** via W0-3 `boosted_categories` config. crt-031 completes that fix: seven hardcoded `HashSet::from(["lesson-learned"...])` literals scattered outside the config load path are eliminated. The vision declared this Critical and Fixed; crt-031 closes the remaining gap.

- The vision's "configured not rebuilt" principle (W0-3 story) is directly served by the new `adaptive_categories` field. An operator deploying Unimatrix in a non-software domain can now express that their equivalent of `lesson-learned` should be eligible for automated lifecycle management — without code changes.

The ASS-032 ROADMAP (the active planning document) places issue #445 — which is exactly what crt-031 implements — in the "Retention" section with the note: "Prerequisite for entry auto-deprecation. Resolves unbounded quarantine/lesson accumulation at the policy layer." The dependency graph shows `#445` as having no blocking deps, and correctly positioned as the policy layer that #409 will consume. All source documents honor this positioning — the feature delivers the policy layer and nothing more.

No vision anti-patterns detected. The feature adds no domain-specific vocabulary to production code, introduces no schema changes, and does not expose a runtime mutation surface for lifecycle policy (operators use `config.toml` and restart).

---

### Milestone Fit

**Finding: PASS**

crt-031 falls in the current work horizon between the completion of PPR (#398, gate passed 2026-03-29) and the upcoming Retention cluster (#445/#409). The ROADMAP.md dependency graph shows `#445 — no blocking deps` and `#409 — #445 schema should land first`. This feature is #445.

The feature does not reach into any future-milestone capability:
- No #409 auto-deprecation mechanics are implemented. The maintenance tick guard stub is explicitly a no-op with a `TODO(#409)` annotation.
- No Wave 2 infrastructure (container, HTTP, OAuth) is touched.
- No Wave 3 GNN or learning pipeline is referenced.

The architecture explicitly defers the `BackgroundTickConfig` composite struct refactor (OQ-05/SR-02) — a Wave 1 maintenance-quality improvement that was identified as a candidate but correctly judged out of scope for this feature. This is correct milestone discipline per pattern #3742: deferred branches must match scope intent, and the deferral is documented.

---

### Architecture Review

**Finding: PASS**

**Strengths:**

All seven SCOPE open questions and scope risks are resolved before implementation begins. This is the correct approach — the SCOPE-RISK-ASSESSMENT.md SR-07 specifically required the architect to trace the eval harness config path before the spec was written, and ARCHITECTURE.md §OQ-5 Resolution does exactly that with a concrete one-line fix.

The construction hierarchy (`new` → `from_categories` → `from_categories_with_policy`) preserves backward compatibility. All existing call sites continue to compile without modification; only the two `main.rs` call sites are proactively updated to wire operator config.

Two independent `RwLock` fields (`categories` and `adaptive`) correctly avoid adding contention to the hot `validate` path. This follows ADR-003 (entry #86) precisely.

SR-03 (parallel-list default collision) is elevated to an architectural documentation concern in ARCHITECTURE.md §SR-03: the mandatory test construction pattern (zero both `boosted_categories` and `adaptive_categories` in any test using a custom `categories` list) is documented directly in the architecture document, not just the risk strategy. This is uncommon and appropriate given the Critical severity.

The `default_boosted_categories_set()` public helper in `infra/config.rs` is correctly designed as the single consolidation point for seven test infrastructure sites. ARCHITECTURE.md §Component 2 confirms import safety ("infra/config.rs has no upward dependency on any of the seven test infrastructure files").

**One architectural gap (mitigated by risk strategy):**

ARCHITECTURE.md §Component 4 specifies wiring `StatusService` via `ServiceLayer::new()` from `main.rs`. However, RISK-TEST-STRATEGY.md R-02 (Critical) identifies three additional `StatusService::new()` construction sites that bypass `ServiceLayer`:
- `run_single_tick` in `background.rs` (~line 446)
- Two test helpers in `services/status.rs` (~lines 1886 and 2038)

The architecture document does not enumerate these three sites as required update targets. An implementer following ARCHITECTURE.md alone could miss the `run_single_tick` path, causing `context_status` calls from the maintenance tick to return empty `category_lifecycle` data without a compile error (the compile-time catch only covers the two test helpers, not `run_single_tick` if it constructs `CategoryAllowlist::new()` inline).

This gap is caught by the risk strategy (R-02 scenarios 1-4 are explicit), but the architecture is the implementer's primary design reference. The risk strategy's pre-implementation grep requirement (R-02 scenario 1) and the wiring assertion requirement (scenario 4) are the mitigating controls.

This is not a FAIL because the risk is documented and mitigated, but it would have been stronger if the architecture had enumerated all four `StatusService::new()` sites directly.

---

### Specification Review

**Finding: PASS**

**Scope goal coverage.** All ten SCOPE.md goals map to SPECIFICATION functional requirements:

| SCOPE Goals | SPECIFICATION FRs |
|-------------|-------------------|
| Goals 1-5 (adaptive_categories) | FR-01 through FR-13 |
| Goals 6-10 (boosted_categories de-hardcoding) | FR-14 through FR-20 |

Every AC from SCOPE.md (AC-01 through AC-23) is present in SPECIFICATION with explicit verification methods added — a meaningful upgrade. AC-24 through AC-27 add four defensive quality gates (SR-03 and SR-09 mitigations). These expand the test scope, not the feature behavior; they are appropriate additions.

The `#409 Dependency Contract` section in SPECIFICATION is exemplary: five explicit numbered commitments define what crt-031 provides to #409 and what it does not. This pre-declares the interface contract before #409 is scoped, preventing future scope ambiguity.

FR-10 (`merge_configs` handling) is present in SPECIFICATION but has no direct antecedent in SCOPE.md Goals or the Proposed Approach. It is a correct and necessary implementation detail — without it, operator `adaptive_categories` config would be silently dropped for project-level configs (FM-04 in the risk strategy). The specifier identified this gap and correctly added the requirement. No approval is needed because it is a direct logical consequence of the scope (adding a new `KnowledgeConfig` field without a merge rule would be a defect), but it is documented here as a scope addition for completeness.

FR-12 (maintenance tick guard) specifies use of `list_adaptive()` for the tick log rather than per-category `is_adaptive()` calls — this directly mitigates R-06 (double lock acquisition). The specification correctly locks the implementation pattern at the FR level.

---

### Risk Strategy Review

**Finding: PASS**

The risk register covers 11 risks (R-01 through R-11), 4 integration risks (I-01 through I-04), 7 edge cases (E-01 through E-07), 3 security risks (S-01 through S-03), and 6 failure modes (FM-01 through FM-06). Coverage depth is above average for a design-phase risk document.

The Scope Risk Traceability table maps all nine SR items (SR-01 through SR-09) from SCOPE-RISK-ASSESSMENT.md to architecture risk IDs and resolution status. This explicit traceability chain is rare and valuable.

Three risks are correctly classified Critical:
- R-01 (validate_config fixture collision) — 4 test scenarios, including a mandatory pre-implementation grep.
- R-02 (StatusService bypass sites) — 4 test scenarios, pre-implementation grep required.
- R-11 (KnowledgeConfig Default impl change — silent test failures) — 5 test scenarios including FR-19 mandatory pre-implementation step.

R-02 was elevated from Medium (in SCOPE-RISK-ASSESSMENT SR-05) to Critical after architecture phase discovery of the three non-ServiceLayer `StatusService::new()` construction sites. This represents appropriate risk lifecycle management and the elevation is well-justified with reference to historical pattern #3216.

Security risks S-01 through S-03 are lightweight but proportionate. The `{category:?}` debug-format recommendation in S-02 is correct and consistent with the existing `BoostedCategoryNotInAllowlist` pattern.

Knowledge stewardship in the risk document is thorough: four separate Unimatrix queries were made, each finding applicable patterns that were directly applied to elevate or shape specific risks. No novel patterns were stored (all applicable patterns were already captured from prior features) — a correct determination.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision`, category `pattern` — found 3 entries: #2298 (config key semantic divergence), #3337 (architecture diagram header divergence), #3742 (optional future branch in architecture must match scope intent). Applied: #3742 confirmed the SR-02 deferral is handled correctly (architecture documents the deferral, scope intent matches). #2298 and #3337 not applicable to crt-031.
- Stored: nothing novel to store — the architecture's incomplete StatusService construction-site enumeration is an instance of the already-documented #3216 pattern (arc-threading gap + hidden run_single_tick bypass). The risk strategy correctly identifies and traces it. No new cross-feature generalization arises from this review.
