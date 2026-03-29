# Alignment Report: crt-031

> Reviewed: 2026-03-29
> Artifacts reviewed:
>   - product/features/crt-031/architecture/ARCHITECTURE.md
>   - product/features/crt-031/specification/SPECIFICATION.md
>   - product/features/crt-031/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-031/SCOPE.md
> Scope risk: product/features/crt-031/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Config-only prerequisite infrastructure; directly advances W0-3 domain-agnosticism direction |
| Milestone Fit | PASS | Correct Wave 1 maintenance-layer work; enables Wave 1A / #409 signal-driven retention without over-building |
| Scope Gaps | PASS | All 5 SCOPE goals and all 15 original acceptance criteria are addressed in source docs |
| Scope Additions | WARN | FR-17 (`merge_configs`) is an unlisted implementation detail not in SCOPE.md goals; minor, low risk |
| Architecture Consistency | PASS | Architecture resolves all SCOPE open questions; ADR-001 locked all three constructor/status/domain-pack questions |
| Risk Completeness | PASS | All 6 scope risks (SR-01 through SR-06) are traced; 10 new risks added with appropriate severity; security, failure modes, and edge cases covered |

**Overall: PASS with one WARN.** No variances require blocking approval. The WARN is surfaced for human awareness.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SCOPE §Open Questions — constructor API (OQ-1) | SCOPE left this as an open question. Architecture locked it as `from_categories_with_policy` + delegating `from_categories`. Resolution is correct and backward-compatible. |
| Simplification | SCOPE §Open Questions — status output format (OQ-2) | SCOPE left asymmetry open. Architecture and SPECIFICATION locked summary=adaptive-only, JSON=all. Rationale: operator scanning text doesn't need noise; JSON is for programmatic audit. |
| Simplification | SCOPE §Open Questions — `add_category` lifecycle (OQ-3) | SCOPE left this open. Architecture locked runtime-added categories to pinned by default. No API change needed. |
| Addition | FR-17: `merge_configs` field inclusion | SCOPE.md does not mention the merge path. SPECIFICATION adds FR-17 requiring `adaptive_categories` in `merge_configs`. This is a correct implementation detail (omitting it would silently drop operator config per FM-04) but is not explicitly scoped. See Variances section. |
| Addition | AC-16, AC-17 (SR-03, SR-05 mitigations) | Two acceptance criteria beyond the original 15 (AC-01 through AC-15). Both are defensive quality gates addressing identified risks; they expand the test scope, not the feature scope. Low concern. |

---

## Variances Requiring Approval

### WARN-01: FR-17 `merge_configs` addition not in SCOPE.md

1. **What**: SPECIFICATION FR-17 requires `merge_configs` in `config.rs` to include `adaptive_categories` in its per-project-wins-else-global merge block. This requirement is absent from SCOPE.md's Goals, Proposed Approach, and Acceptance Criteria.

2. **Why it matters**: This is a correct and necessary implementation detail — without it, the operator's `adaptive_categories` config value is silently dropped for project-level configs (FM-04 in the risk strategy). However, it constitutes scope added by the specifier beyond what SCOPE.md explicitly asked for. Per alignment rules, additions require explicit approval.

3. **Recommendation**: **Accept.** The addition is minimal (one merge field following the established `boosted_categories` pattern), prevents a silent failure mode identified in the risk document, and has zero product-direction risk. Human should confirm acceptance so the delivery agent does not question the requirement.

---

*No FAIL or additional VARIANCE findings.*

---

## Detailed Findings

### Vision Alignment

crt-031 is a config-infrastructure prerequisite for automated knowledge retention. It aligns with the product vision on two axes:

**Domain agnosticism (W0-3 direction).** The product vision's Critical Gaps table lists "lesson-learned category name hardcoded in scoring" as **Fixed** via `boosted_categories` in W0-3. crt-031 continues the same pattern: the lifecycle policy of which categories are adaptive vs pinned is now operator-configurable rather than hardcoded. This directly advances the "configured not rebuilt" principle stated in the vision story: "Any knowledge-intensive domain... runs on the same engine, configured not rebuilt."

**Self-improving knowledge integrity.** The vision describes Unimatrix as a "self-learning knowledge integrity engine." Automated retention (removing stale lesson-learned entries) is a prerequisite for the "ever-improving" part of that claim. crt-031 establishes the categorical guard that makes automated retention safe — entries in `decision` or `convention` are never auto-deprecated, while `lesson-learned` entries may be. This aligns with the vision's emphasis on correctability and provenance.

**No vision anti-patterns detected.** The feature does not hardcode any domain-specific vocabulary, does not add schema changes, and does not expose a runtime mutation path that would bypass the config-as-operator-contract model.

---

### Milestone Fit

crt-031 is filed under the Cortical phase. It is prerequisite infrastructure for #409 (signal-driven auto-deprecation), which is listed in the product vision as a Wave 1A / W1-5 learning signal concern.

The feature explicitly avoids building any retention logic itself (SCOPE Non-Goals, SPECIFICATION §NOT in Scope). The maintenance tick stub is a no-op placeholder with a `TODO(#409)` marker — this is the correct pattern for establishing a tested insertion point without pulling forward future milestone work.

There is no evidence of Wave 2 or Wave 3 capability being pulled into this feature. The architecture explicitly defers `BackgroundTickConfig` composite struct (SR-02 / OQ-05) to a future feature. This is good milestone discipline.

**Milestone fitness: PASS.**

---

### Architecture Review

**Resolved open questions.** All four SCOPE open questions are resolved in the architecture:
- OQ-1 (constructor API): locked as `from_categories_with_policy` canonical + delegating `from_categories`/`new()`. No call-site breakage.
- OQ-2 (status output format): locked as summary=adaptive-only, JSON=all categories. Intentional asymmetry documented via SR-04 with golden-output test requirement.
- OQ-3 (`add_category` lifecycle): locked as pinned-by-default, no API change.
- OQ-4 / SR-01 (file-size split): mandated module split to `infra/categories/mod.rs + lifecycle.rs` before implementation.

**Prior patterns followed.** The architecture correctly cites and follows:
- Entry #86 (ADR-003: `RwLock<HashSet<String>>` pattern) — second independent lock uses the same pattern.
- Entry #2312 (`boosted_categories` default trap) — codified as the mandatory SR-03 test construction pattern in ARCHITECTURE.md §Test Construction Pattern.
- Entry #3770 (`KnowledgeConfig` parallel list pattern) — `adaptive_categories` mirrors `boosted_categories` structure exactly.
- Entry #3721 (5-location lockstep for `INITIAL_CATEGORIES`) — correctly identified as not applicable here since no new category is added.

**SR-02 deference (BackgroundTickConfig composite).** The architecture explicitly defers the `spawn_background_tick` composite struct refactor (OQ-05). Per pattern #3742, when an architecture defers a future branch, the scope intent must match — and it does: the SCOPE.md SR-02 already flagged this as a risk to be evaluated, not a required resolution. The deferral is documented with a `#[allow(clippy::too_many_arguments)]` justification and a forward reference to a follow-up procedure entry. PASS.

**One open question remains in ARCHITECTURE.md (OQ-1 §StatusService wiring).** The architecture lists as open whether `StatusService` holds `Arc<CategoryAllowlist>` as a field or receives it as a `compute_report` parameter. SPECIFICATION FR-11 says "StatusService MUST receive `Arc<CategoryAllowlist>` (it already holds it if not, this wiring is added in the same PR)" and Risk R-02 explicitly calls this out as unconfirmed scope. This is correctly flagged in the risk document and mitigated with a pre-coding verification step. Not a variance — the risk is documented and mitigated.

---

### Specification Review

**Scope goal coverage.** All five SCOPE goals map to SPECIFICATION functional requirements:

| SCOPE Goal | SPECIFICATION FR |
|------------|-----------------|
| 1. `adaptive_categories` field in `[knowledge]` with default `["lesson-learned"]` | FR-01, FR-02, FR-03 |
| 2. `CategoryAllowlist::is_adaptive()` method | FR-06, FR-07, FR-08 |
| 3. Startup validation (`AdaptiveCategoryNotInAllowlist`) | FR-04, FR-05 |
| 4. Expose per-category lifecycle in `context_status` | FR-10, FR-11, FR-12, FR-13 |
| 5. Lifecycle guard stub in maintenance tick | FR-15, FR-16 |

**Acceptance criteria coverage.** All 15 original SCOPE acceptance criteria (AC-01 through AC-15) are reproduced in SPECIFICATION with explicit verification methods added. Two new criteria (AC-16, AC-17) address SR-03 and SR-05 mitigations — these expand test coverage, not feature behavior.

**Non-functional requirements.** NFR-01 through NFR-07 are well-formed. NFR-05 correctly restricts `is_adaptive` to non-hot-path call sites only (maintenance tick and `context_status`), consistent with the vision's zero-regression-on-ranking-signal requirement.

**FR-17 addition noted.** `merge_configs` requirement is the only SPECIFICATION requirement without a direct SCOPE.md antecedent. See WARN-01 above.

**Open questions in SPECIFICATION.** OQ-04 (two `RwLock` fields vs one `RwLock<(HashSet, HashSet)>`) and OQ-05 (composite struct) are correctly marked as architect decisions. OQ-06 (test count gate) is non-binding. These are appropriate deferrals for the design phase.

---

### Risk Strategy Review

**Coverage is comprehensive.** Ten risks are registered, spanning:
- Test fixture correctness (R-01 — Critical, the highest-priority risk)
- Integration wiring (R-02, R-04, R-07, R-09)
- Parameter count and code review friction (R-05)
- Lock hygiene (R-03, R-06)
- Golden-output determinism (R-08)
- Gate delivery completeness (R-10)

**Scope risk traceability is explicit.** All 6 SCOPE risks (SR-01 through SR-06) are mapped to architecture risks and resolution status in the §Scope Risk Traceability table. This is above-average traceability for a design-phase risk document.

**Pattern alignment confirmed.** The risk document queried Unimatrix entries #3579 (gate 3b missing test modules), #2758 (gate 3c false PASS claims), #1560, #1542, #2312, and #3770. All findings were applied correctly:
- #3579 elevated R-10 to High severity — correct.
- #2312 (boosted_categories default trap) directly grounds R-01 as Critical.
- Pattern #3426 (formatter golden-output test required) is independently noted in SCOPE-RISK-ASSESSMENT SR-04, ARCHITECTURE SR-04, and SPECIFICATION FR-12/FR-13 — full traceability chain.

**Security coverage.** S-01 through S-03 are present. S-01 correctly identifies the debug-log blast radius as the only concern for `?adaptive_cats` formatting of operator-supplied strings — minimal risk, correctly characterized.

**One minor gap.** The risk register does not explicitly address the case where the module split (R-04) interacts with the `unimatrix-observe` crate's import path. However, ARCHITECTURE.md explicitly confirms `unimatrix-observe` is not touched by this feature (5-location lockstep rule does not apply), so the gap is benign.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — found entry #3742 (optional future branch deferral WARN pattern from crt-030) and #3426 (formatter golden-output requirement pattern from col-026). Both applied to this review: #3742 confirmed SR-02 deferral is correctly handled; #3426 confirmed SR-04 golden-output test requirement is correctly propagated through all three source documents.
- Stored: nothing novel to store — the WARN-01 scope addition (FR-17 `merge_configs`) is a specific implementation detail, not a recurring cross-feature pattern. The existing entry #3742 covers the general case of architecture scope additions; no new pattern generalizes from this review alone.
