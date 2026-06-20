# Alignment Report: vnc-041

> Reviewed: 2026-06-20
> Artifacts reviewed:
>   - product/features/vnc-041/architecture/ARCHITECTURE.md
>   - product/features/vnc-041/specification/SPECIFICATION.md
>   - product/features/vnc-041/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-041/SCOPE.md
> Risk source: product/features/vnc-041/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Goals consulted: #4946 (personal-cloud), #4678 (domain-agnostic), standing multi-project goal (independent config + db)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances personal-cloud #4946 ("multi-PROJECT... each fully isolated... own DB") and the standing multi-project independent-config goal; provisions the per-slug config seam Feature A made resolvable. |
| Milestone Fit | PASS | C17 is the seeding half of the same vnc-040 serving arc as the shipped C6 resolution; no future-milestone capability pulled forward. |
| Scope Gaps | PASS | All six SCOPE goals/ACs (AC-01..AC-06) carry through to FR-01..FR-15 and the risk register (R-01..R-14). Nothing in SCOPE is dropped. |
| Scope Additions | PASS | No capability beyond SCOPE. The architecture's `http.enabled` gate is a mechanism correction, not a new feature (see Detailed Findings). |
| Architecture Consistency | WARN | Architecture §6 + RISK §"Architect corrections" reverse SCOPE/spec's stated container discriminator (`base_dir = Some(/data)` -> `if config.http.enabled`). The corrected docs are internally consistent, but SCOPE AC-01, SCOPE-RISK SR-04, and SPEC FR-02/AC-01/AC-06 still narrate the superseded `base_dir` mechanism. Human awareness only. |
| Risk Completeness | PASS | Strong coverage of the integrity-relevant risks (no-clobber, regression sentinel, A->B drift). R-13 WARN is correctly scoped WARN-only, consistent with the "don't over-build defensiveness" guidance. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal maps to an FR and a risk scenario. |
| Addition | (none) | No new config knob, no new section, no rejection path — all explicitly held out in Non-Goals and confirmed across all three docs (NFR-02, C-04, ADR-002). |
| Simplification | Annotation render shape (legend block vs inline per-key tags) | Architecture OQ-A recommends a header legend block prepended to the reused `DEFAULT_CONFIG_TOML` rather than weaving inline tags into the template. Rationale: keeps the proven static template intact, avoids a new serializer, and still satisfies AC-03 "proven, not restated" via the flip test. Acceptable — documented with rationale, left as a spec-writer/dev call (OQ-A). |
| Correction (not a simplification) | Container discriminator | SCOPE/spec say container-vs-local is decided by `ensure_data_directory`'s `base_dir = Some(/data)`; architecture §6 demonstrates every live `serve` call passes `base_dir = None` and the real seam is `if config.http.enabled`. The architecture's gate is MORE faithful to SCOPE intent (container-only seed, local writes zero files) — it does not expand scope. See WARN below. |

## Variances Requiring Approval

None rising to VARIANCE or FAIL. One WARN for human awareness (does not block design):

1. **What**: SCOPE.md (AC-01, Background §(a)), SCOPE-RISK-ASSESSMENT.md (SR-04, Assumptions), and parts of SPECIFICATION.md (FR-02, AC-01, AC-06 narration) describe container detection via the `base_dir = Some(/data)` argument. The architecture (§6, ADR-004) and risk-test strategy (Architect corrections, R-01) correctly establish that the live `serve` path always passes `base_dir = None`, so the real structural gate is `if config.http.enabled`. A reader who trusts SCOPE/SCOPE-RISK in isolation would build a gate that never fires.
   - **Why it matters**: This is the highest-severity seam in the feature (R-01/R-02 Critical — regressing the local single-project majority, which is the personal-cloud goal's "common case and the seam's proving ground"). The source docs disagree with each other on the mechanism that protects it. The downstream design/spec are correct; the upstream SCOPE and SCOPE-RISK are stale on this one mechanism.
   - **Recommendation**: Accept the architecture position (it is the correct seam and preserves SCOPE intent). Add a one-line note to SCOPE.md AC-01 and SCOPE-RISK SR-04 that the discriminator was corrected to `http.enabled` during design (architecture §6 / ADR-004), so the seeding mechanism reads consistently across all artifacts. No design or test change needed — RISK-TEST R-01 scenario 4 already proves the gate is `http.enabled`, not `base_dir`. WARN closes when SCOPE/SCOPE-RISK reflect the corrected gate (or explicitly cite ADR-004 as the authority).

## Detailed Findings

### Vision Alignment
PASS. Goal #4946 (personal-cloud) states the destination explicitly: "Multi-PROJECT, multi-CLIENT is the destination — one cloud serves N projects, routed by an operator-declared slug... each fully isolated (own DB...)." The standing multi-project goal (per MEMORY: "each project independently configurable AND independent db") makes per-slug config a first-class outcome. Feature A (#799) delivered resolution (C6); resolution without provisioning is invisible (SCOPE Problem Statement). vnc-041 provisions the exact file the resolver reads (`{base_dir}/{slug}/config.toml`), closing the operator-discoverability gap. This is squarely on the goal, not adjacent to it.

The feature also respects architectural principle #6 ("single binary server, zero required infrastructure / local UDS works without container"): the global seed is deliberately confined to the container/HTTP path so the local STDIO common case is untouched (AC-06). Principle #8 (no secrets in DB) is unaffected — seeds are annotated config templates, not secrets.

### Milestone Fit
PASS. C17 (#5214) is the seeding companion to C6 (#5148, shipped Feature A / #799), both inside the vnc-040 serving arc. No future-milestone capability is pulled forward: hot-reload is held out (Non-Goals, NFR-05), RBAC/multi-tenant/JWT-claim enforcement is untouched (those are the enterprise extension boundary in #4946), and the multi-slug HTTP end-to-end harness is correctly deferred to infra-001 (#800). The feature builds exactly the provisioning the resolved seam needs now.

### Architecture Review
PASS with the WARN above. Strengths:
- The (a)≡(c) vs (b) three-file disambiguation is settled and consistent across SCOPE, ARCHITECTURE §2, and SPEC Domain Models — this was the dominant conflation risk (SR-05) and it is closed by construction (B writes only (b); global seed is serve-time, never inside register).
- The A->B one-way contract is preserved structurally: annotation render (C2) and WARN surface (C5) both bind to `is_per_slug_overlayable` / `PER_SLUG_CONFIG_CLASSIFICATION` at runtime, with `OverlayDisposition` exhaustiveness as a compile-time forcing function (ADR-003/005, NFR-03). B restates nothing — this directly honors the goal's integrity-consistency intent.
- The `http.enabled` correction (§6) is well-reasoned and makes "container only" a compile-time branch fact rather than a runtime flag. This is a strengthening of SCOPE intent, not a deviation from it.

No pattern #3742-class divergence: there is no "optional future branch" that the architecture treats as in-scope while SCOPE defers it. The deferrals (hot-reload, rejection path, infra-001 harness) are consistently out across all three documents with zero test scenarios.

### Specification Review
PASS. FR-01..FR-15 trace cleanly to SCOPE goals; AC-01..AC-06 are preserved verbatim in intent with verification methods added. NFR-01 (local regression, zero files, byte-for-byte) and NFR-02 (no new config surface) are the right guardrails for the personal-cloud common-case-protection intent. The one spec inconsistency is the inherited `base_dir` narration in FR-02/AC-01/AC-06 (the WARN) — the spec text still describes the superseded mechanism even though the architecture it cites corrects it. Note the spec's own OQ-A/OQ-B/OQ-C are open questions routed to the architect, which the architecture answers (legend block; field-less locks via registry key strings; once-per-boot dedup) — confirm these are reconciled before delivery so the spec does not ship with open questions its companion architecture already resolved.

### Risk Strategy Review
PASS, and notably well-calibrated against the "don't over-build defensiveness" guidance (MEMORY: avoid-overstating-defensive-structure). The R-13 seam WARN — the feature's one new defensive signal — is held to WARN-only: one `tracing::warn` per ignored locked key per boot, no rejection, no resolution change (FR-12, C-03, SR-06, R-07). This is the correct posture: it converts a silent-ignore support-ticket generator into a visible signal WITHOUT elevating config-integrity to a hard gate that would over-constrain operators. The strategy explicitly tests that resolution output is identical with and without the WARN (R-07 scenario 1) — proving the defensiveness is signal, not enforcement.

Integrity-relevant risks for the personal-cloud goal are covered: atomic no-clobber (R-03, protects operator-authored config = the goal's "air-gap deployable / hand-placed" model), the regression sentinel verified empirically not structurally (R-01/R-02, citing #4876), and A->B drift prevention with a runtime flip test (R-04/R-06). Security review correctly notes the slug is a validated newtype (path-traversal inherited from vnc-038) and that the WARN logs key+slug only, never the operator's set value (#4749 content-free-logging) — no secret/value leakage, consistent with principle #8.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns -- found #3742 (optional-future-branch divergence WARN pattern), #2298 (config-key semantic divergence), #3337 (architecture/spec header divergence). #3742 was the closest analogue but did NOT match (vnc-041 has no in-scope-vs-deferred branch tension; all deferrals are consistent across docs). The variance here is a different class: an upstream/downstream mechanism correction (SCOPE/SCOPE-RISK stale on the container gate vs architecture's `http.enabled` correction).
- Stored: nothing novel to store -- the one WARN is feature-specific (a single stale mechanism narration in SCOPE/SCOPE-RISK that the architecture already corrected) and does not generalize to a recurring cross-feature pattern. The existing #3742 covers the related "source docs must stay consistent on a load-bearing mechanism" theme; no new entry warranted.
