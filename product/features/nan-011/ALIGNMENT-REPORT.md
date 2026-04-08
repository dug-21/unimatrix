# Alignment Report: nan-011

> Reviewed: 2026-04-08
> Artifacts reviewed:
>   - product/features/nan-011/architecture/ARCHITECTURE.md
>   - product/features/nan-011/specification/SPECIFICATION.md
>   - product/features/nan-011/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/nan-011/SCOPE.md
> Scope risk source: product/features/nan-011/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature corrects documentation to match shipped vision; no strategic contradiction |
| Milestone Fit | PASS | Nanoprobes phase; no Wave/milestone capability is built — only documentation corrected |
| Scope Gaps | WARN | SR-06 qualifier sentence: SCOPE.md marks it out-of-scope; spec FR-1.2 puts it in-scope |
| Scope Additions | WARN | FR-1.2 qualifier requirement and R-10 scenario 3 add a deliverable not in SCOPE.md non-goals |
| Architecture Consistency | PASS | Architecture maps to all five SCOPE.md deliverables; open questions flagged |
| Risk Completeness | PASS | All 15 risks trace to scope risks; coverage table is consistent |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | FR-1.2 qualifier sentence | SCOPE.md §Non-Goals explicitly defers the "Invisible Delivery" README copy correction: "This README copy correction is out of scope for nan-011 and should be addressed in a follow-on session." SPECIFICATION.md FR-1.2 introduces a mandatory qualifier sentence ("This workflow-phase-conditioned delivery…") and R-10 scenario 3 requires it as a pass condition. This is a new deliverable absent from SCOPE.md. |
| Simplification | SR-07 minimum version note deferred | RISK-TEST-STRATEGY.md §Scope Risk Traceability notes that the minimum compatible Unimatrix version note for the distributed `uni-retro` skill is "out of scope for nan-011 — filed as follow-on concern." This is an acceptable documented deferral; it generalizes a scope risk recommendation rather than contradicting a SCOPE.md requirement. |
| Gap | None detected | All eight SCOPE.md goals map to explicit FRs in the specification and components in the architecture. |

---

## Variances Requiring Approval

### 1. FR-1.2 Qualifier Sentence — Scope Addition (WARN)

**What**: SCOPE.md §Non-Goals states: "The 'Invisible Delivery' README bullet ('Agents do not need to ask for context') oversells the hook-driven injection capability — it is additive to explicit agent queries, not a replacement. This README copy correction is out of scope for nan-011 and should be addressed in a follow-on session."

SPECIFICATION.md FR-1.2 adds a mandatory one-sentence qualifier immediately after the vision statement: "This workflow-phase-conditioned delivery means knowledge is surfaced at phase transitions based on what the engine has learned about each phase — it is not unconditional injection into every prompt."

RISK-TEST-STRATEGY.md R-10 scenario 3 adds this qualifier as a gate condition: "Confirm FR-1.2's qualifier sentence appears immediately after the vision block in README.md."

**Why it matters**: SCOPE.md's approved vision statement contains the phrase "before agents need to ask for it." SR-06 in the SCOPE-RISK-ASSESSMENT.md identifies this as a high-likelihood integration risk: the statement oversells current capability. The spec authors resolved SR-06 by adding the qualifier in FR-1.2. However, the SCOPE.md non-goals section explicitly places the oversell correction out of scope. This is an internal contradiction between two SCOPE.md sections: §Non-Goals defers the correction, but SCOPE-RISK-ASSESSMENT SR-06 recommends it, and the spec authors acted on that recommendation.

The addition is well-motivated and low-risk — it is a single explanatory sentence, not a content change. However, it is a scope addition that requires human sign-off.

**Recommendation**: Accept. The qualifier directly addresses a high-likelihood risk (SR-06) identified in the scope risk assessment, and the SCOPE.md non-goals language was written before that risk assessment was complete. The addition prevents the released README from making a false capability claim. The project owner should confirm acceptance and update SCOPE.md non-goals to remove the deferral or explicitly approve FR-1.2.

---

## Detailed Findings

### Vision Alignment

nan-011 is a documentation and distribution synchronization feature in the Nanoprobes phase. Its purpose — correcting inaccurate public-facing documents to match the current implementation — is directly supportive of the product vision.

The product vision (PRODUCT-VISION.md §Vision) describes Unimatrix as a platform delivering "the right knowledge at the right time" across any domain. The feature's approved vision statement, which replaces the stale README opening, accurately represents that framing: "workflow-aware, self-learning knowledge engine… configurable for any workflow-centric domain."

The feature does not introduce capabilities, does not modify Rust code, and does not shift strategic direction. Every deliverable (README repair, config.toml expansion, skills audit, protocols packaging, uni-seed update) is a surface-level alignment between shipped code and public documentation. No strategic contradiction detected.

The product vision includes Wave 1A capabilities (PPR expansion, phase-conditioned affinity, behavioral signal delivery, domain-agnostic observation pipeline) as COMPLETE or IN-PROGRESS items. The feature correctly adds these to the README and removes NLI content that was removed in crt-038. This is accurate synchronization, not scope inflation.

One vision-document correction is in scope: PRODUCT-VISION.md W1-5 and HookType gap row. Both are simple status updates (IN PROGRESS → COMPLETE / Fixed). Verified that PRODUCT-VISION.md at the time of review still shows these as needing correction, consistent with the SCOPE.md description of the problem.

**PASS** — vision alignment is solid. No strategic drift detected.

---

### Milestone Fit

nan-011 is in the Nanoprobes phase (prefix: `nan`), which the project layout defines as "Build, deploy, CI." Release preparation, documentation, and distribution packaging fit cleanly in this phase.

The feature does not build Wave 2 or Wave 3 capabilities. It does not introduce deployment infrastructure (Wave 2) or intelligence pipeline work (Wave 1A / Wave 3). It packages existing artifacts (protocols, uni-retro skill) for distribution — a build/deploy concern.

The only milestone-adjacent question: does packaging `protocols/` and `uni-retro` in the npm distribution create a forward commitment to maintaining those artifacts across future milestone releases? The RISK-TEST-STRATEGY.md §Scope Risk Traceability acknowledges SR-07 (versioning contract) as out of scope for nan-011 but files it as a follow-on concern. This is appropriate milestone discipline — the packaging decision is scoped correctly to the current release cycle.

**PASS** — feature targets the correct phase and does not build future-milestone capabilities.

---

### Architecture Review

The architecture document covers five components that map directly to SCOPE.md's five deliverables:

| SCOPE.md Deliverable | Architecture Component |
|---|---|
| Deliverable 1 — Vision Statement and README | Component 1 — README + PRODUCT-VISION.md Repair |
| Deliverable 2 — Default config.toml | Component 2 — config.toml Full Rewrite |
| Deliverable 3 — Skills MCP Format Audit | Component 3 — Skills MCP Format Audit |
| Deliverable 4 — Protocol and uni-retro Packaging | Component 4 — protocols/ Directory; Component 5 — npm Package Update |
| Deliverable 5 — uni-seed Update | Component 3 (accuracy audit) |

Coverage is complete. No component appears without a scope anchor.

**Integration surface table is precise**: The architecture documents the `boosted_categories` serde-vs-Default discrepancy (showing `["lesson-learned"]` as the serde default), the dynamic `rayon_pool_size` formula, `INITIAL_CATEGORIES` as the authority for uni-seed category verification, and the `package.json files` array location. These match corresponding risks (SR-01/R-01/R-02/R-03) in the risk strategy.

**Open question 4 (SR-06 qualifier)**: Architecture open question 4 correctly identifies the SR-06 qualifier as a spec-level decision, not architectural. The architecture does not resolve it — that is appropriate role separation. The specification resolved it via FR-1.2, which is the source of the WARN above.

**Open question 1 (skills/ directory at root)**: Architecture correctly flags that the existence of `skills/` at repo root is uncertain and must be verified at delivery time. This is accurately surfaced as an open question rather than a committed design decision.

No architecture additions beyond SCOPE.md scope detected. No choreography changes introduced. The architecture explicitly states "no runtime component interactions" — appropriate for a static-artifact feature.

**PASS** — architecture is consistent with scope and vision.

---

### Specification Review

The specification translates all SCOPE.md acceptance criteria into functional requirements with consistent coverage. The AC-to-FR mapping is tight:

| SCOPE.md AC | SPECIFICATION FR |
|---|---|
| AC-01 (vision verbatim) | FR-1.1, FR-1.3 |
| AC-02 (NLI removed) | FR-2.1–2.3 |
| AC-03 (new sections) | FR-3.1–3.2 |
| AC-04 (binary name) | FR-4.1–4.3 |
| AC-05 (PRODUCT-VISION.md fixes) | FR-5.1–5.2 |
| AC-06 (config.toml 8 sections) | FR-6.1–6.13 |
| AC-07 (domain_packs example) | FR-6.7 |
| AC-08 (config.toml valid TOML, defaults match) | FR-7.1–7.3, NFR-1, NFR-2 |
| AC-09 (NLI block commented) | FR-6.9 |
| AC-10 (skills format audit) | FR-8.1–8.3 |
| AC-11 (uni-init lists 14 skills) | FR-9.2 |
| AC-12 (uni-retro no HookType) | FR-9.3 |
| AC-13 (uni-release + package.json + npm pack) | FR-9.1, FR-13.1–13.5 |
| AC-14 (protocols/ directory + README) | FR-11.1–11.4 |
| AC-15 (protocols stale refs removed) | FR-12.1–12.3 |
| AC-16 (uni-seed format + use case description) | FR-10.1–10.5 |
| AC-17 (uni-seed categories match INITIAL_CATEGORIES) | FR-10.3 |

**Scope addition (FR-1.2)**: As documented in the Variances section above, FR-1.2 adds a qualifier sentence not present in SCOPE.md's accepted deliverables. This is the only addition found.

**FR-9.2 binary fix**: The specification adds a requirement (FR-9.2) to fix the `unimatrix-server` binary reference in `uni-init`'s Prerequisites section. SCOPE.md §Deliverable 3 says uni-init requires accuracy review beyond format only for the CLAUDE.md block and tool call examples. The binary name fix in the Prerequisites section is a minor accuracy correction consistent with AC-04's spirit, though the scope does not call it out explicitly. This is a sensible addition within the spirit of the scope — classified as a simplification (minor addition, same spirit), not a variance.

**NFR-5 (no choreography changes)** and **NFR-6 (no Rust code changes)** are explicitly stated in the specification, matching SCOPE.md's non-goals precisely.

**PASS** — specification covers all scope items with one WARN-level addition (FR-1.2) already documented above.

---

### Risk Strategy Review

The risk strategy covers 15 risks across Critical, High, Medium, and Low priorities. All risks trace to scope risks (SR-01 through SR-07) or to implementation-specific failure modes.

**Traceability table is complete**: RISK-TEST-STRATEGY.md §Scope Risk Traceability maps all seven SCOPE-RISK-ASSESSMENT.md risks to architecture risks and risk strategy scenarios. No scope risk is dropped.

**Coverage elevation**: R-01 (config defaults) and R-02 (serde-vs-Default) are rated Critical. This is a justified elevation from SR-01's High/High — the risk strategy cites Unimatrix entries #3817 and #4044 as confirming this is a persistent failure mode across features. Consistent with the scope risk assessment recommendation.

**R-10 scenario 3 adds a gate condition for FR-1.2**: As noted in the Variances section, R-10 test scenario 3 gates on the FR-1.2 qualifier sentence. This extends the test strategy to cover the scope addition. If FR-1.2 is not accepted, R-10 scenario 3 must be removed to avoid a false gate failure.

**SR-07 deferral is documented**: The minimum version note for distributed uni-retro is explicitly called out as a follow-on concern. This is appropriate knowledge stewardship — the risk is acknowledged, not ignored.

**Security section is proportional**: The security risks section correctly scopes the attack surface to static file content consumed by agents. No runtime user input is accepted. Path traversal in `nli_model_path` and `rule_file` is noted as an informational concern (config comments should note this) without requiring a code fix.

**PASS** — risk strategy is complete, proportional, and consistently traceable to scope.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found entries #3337 (architecture diagram informal headers diverging from spec) and #3742 (optional future branch architecture must match scope intent). Neither pattern applies directly to nan-011: nan-011 has no runtime components or deferral branches. No sample output blocks in the architecture that could diverge from spec.
- Stored: nothing novel to store — the FR-1.2 scope addition is a feature-specific one-off (a non-goals section written before the risk assessment was complete, causing an internal scope contradiction). The pattern of "scope risk assessment resolving a non-goals clause" is plausible as a cross-feature pattern but has only one data point here; not sufficient to generalize.
