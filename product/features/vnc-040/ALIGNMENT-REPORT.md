# Alignment Report: vnc-040

> Reviewed: 2026-06-19 (re-run against updated source documents)
> Artifacts reviewed:
>   - product/features/vnc-040/architecture/ARCHITECTURE.md
>   - product/features/vnc-040/architecture/ADR-001-per-slug-overlay-at-call-site.md
>   - product/features/vnc-040/architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md
>   - product/features/vnc-040/architecture/ADR-003-post-merge-revalidation.md
>   - product/features/vnc-040/architecture/ADR-004-canonical-per-slug-classification.md
>   - product/features/vnc-040/specification/SPECIFICATION.md
>   - product/features/vnc-040/RISK-TEST-STRATEGY.md (14 risks / 32 scenarios)
> Scope source: product/features/vnc-040/SCOPE.md + SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goals #4946 (personal-cloud), #4678 (domain-agnostic)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances goal #4678 (config-as-config-file: categories, confidence, **server instructions**, domain packs) and goal #4946 (multi-project isolation, one isolation seam). |
| Milestone Fit | PASS | Vinculum / C6 (#5148); C5 prerequisite proven on #789. No future-milestone capability pulled forward; hot-reload, seeding, model-selection all deferred. |
| Scope Gaps | PASS | All 5 SCOPE goals + every constraint map to FR/AC/ADR. No SCOPE item unaddressed. |
| Scope Additions | PASS | ADR-004 canonical classification + drift-guard fulfils SCOPE Goal 5 verbatim; not an addition. No new config knobs. |
| Architecture Consistency | PASS | Call-site seam, `merge_configs` reuse, by-construction invariants, one-way A→B render contract internally consistent and consistent across the 4 ADRs + spec + risk doc. |
| Risk Completeness | PASS | 14 risks / 32 scenarios; SR-01..SR-08 all traced; the two design-gate near-misses (embed_handle, then permissive/instructions) captured as R-07; classification drift captured as R-14. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Coverage | Goal 1 (per-slug overlay at call site) | FR-01/FR-02, ADR-001. |
| Coverage | Goal 2 (thread overlayable values incl. `instructions`) | FR-03/FR-14, ARCH §3, ADR-001. |
| Coverage | Goal 3 (hard model invariant, both models) | FR-04/FR-05, ADR-002 (by construction). |
| Coverage | Goal 4 (byte-for-byte fallthrough) | FR-08, ADR-002 `Arc::ptr_eq`, AC-02. |
| Coverage | Goal 5 (single canonical classification + drift-guard) | FR-16, ADR-004, AC-11, R-14. |
| Simplification | `[embedding]` whole-section lock resolves to "pin global-wins + no new descriptor field" | Rationale (ADR-002 §6b, code inspection): the current `UnimatrixConfig` has no `[embedding]` section; the only descriptor is `inference.embedding_model_sha256`, already global-wins. Forward guard recorded for any future descriptor field. Documented, principled — acceptable. |
| Simplification | Stray global-only section in a per-slug file is silently ignored (no runtime warn) | Rationale (ARCH §7a, R-13): accepted residual; ownership documented via A's canonical classification, which Feature B's seed renders from. Runtime warn deferred as optional future enhancement. Documented — acceptable, see WARN below. |
| Boundary | Feature B (seeding) out of scope; A owns the classification, B renders | One-way A→B contract (ADR-004, SPEC "Known Limitation", R-13). Consistent with goal #4946's "later wave extends, never re-architects" posture. |

## Variances Requiring Approval

None. No VARIANCE or FAIL.

One WARN (awareness only, not blocking) — see Risk Strategy Review.

## Detailed Findings

### Vision Alignment — PASS

The two updated design moves, assessed directly against the question posed:

1. **Canonical classification + one-way Feature B render contract (ADR-004 / FR-16).**
   This serves the architectural principles rather than straining them. Goal #4946's
   "**one isolation seam** across local AND cloud" — `resolve_store` as the single funnel —
   is mirrored here by a single *configuration*-split owner: the registry in `config.rs` is
   the one place the per-slug-vs-global verdict is defined, and three consumers (verdict table,
   `merge_configs` drift-guard, Feature B seed) *render* from it. "A owns the split, B consumes"
   is the same later-wave-extends-earlier-wave discipline goal #4946 demands of the enterprise
   boundary (#4869 one-way seam, cited in the spec's stewardship block). ADR-004 explicitly
   declines to re-architect `merge_configs` into a generic engine — it binds via a *test*, not a
   rewrite — honoring the reuse lock (ADR-001) and the "configured not rebuilt" spirit of
   goal #4678 without over-building. Right altitude: one owner, machine-checked, no new abstraction.

2. **`instructions` made per-slug overlayable (FR-14).**
   Not scope creep — a *named success-criterion field* of goal #4678: "Domain configuration is a
   config file: custom categories, confidence weights, **server instructions**, observation domain
   packs." Three of those four were already per-slug; `instructions` was the one still globally
   fanned (`main.rs:687`→`1095`). Bringing it per-slug completes the goal's own enumerated config
   surface. It weakens **no** invariant: `ServerConfig.instructions: Option<String>` is already a
   field that `merge_configs` already merges project-wins (`config.rs:3862-3864`), and
   `build_project_server` already accepts it (`http_provision.rs:137`). The change is a pure source
   relocation (global var → `resolved.server.instructions`) — no new seam, no signature change, no
   new model path — riding the identical fallthrough discipline as the other overlayable fields
   (AC-10, R-12).

3. **Graceful degradation / no-file fallthrough (principle 5).**
   AC-02 strengthened to `Arc::ptr_eq` on the 3 global handles converts the "absent file = previous
   behavior, not broken behavior" guarantee from review-only to machine-checked — a *strengthening*
   in exactly the direction the graceful-degradation principle points. The silent-majority (local
   UDS / single-project) path is now a pointer-equality-grade regression sentinel.

4. **Model-resource invariants (principle 6, single binary / one model in memory).**
   ADR-002 holds both one-model invariants (NLI + embedding) *by construction* — the 3 handles are
   `Arc::clone`d outside any merge branch and never sourced from the merged config; the embedding
   descriptor is global-wins. The per-slug overlay adds zero model loads at any N (NFR-01). Per-slug
   model selection is not even representable. Fully aligned.

No principle is contradicted. Hash-chain integrity, audit log, capability checks, and in-memory hot
path are untouched (config resolution only) — correctly not invoked.

### Milestone Fit — PASS

C6 (Unimatrix #5148), Vinculum phase, unblocked by C5 (#5190 proven on #789, 2026-06-19). The design
is disciplined about *not* pulling future capability forward: hot-reload, per-slug model selection,
per-slug transport, per-slug embedding, and config seeding (Feature B) are explicit Non-Goals. The
one-way A→B contract is the correct milestone-discipline move — it records the seam B will build on
without building B now.

### Architecture Review — PASS

The four ADRs are mutually consistent and consistent with the spec and risk strategy:
- ADR-001 (call-site overlay, `resolve_slug_config`, reuse `merge_configs`) — confines the change,
  leaves `load_config` layering untouched (NFR-07).
- ADR-002 (invariants + fallthrough by construction, `Arc::ptr_eq`) — the load-bearing safety ADR.
- ADR-003 (post-merge re-validation) — closes the #3905 third-layer cross-field gap (SR-01, the
  highest-rated scope risk).
- ADR-004 (canonical classification) — gives the central policy exactly one owner; the §9 verdict
  table is reframed as a *rendering*, and the drift-guard test pins `merge_configs` to the registry.

The verdict surface was correctly **reframed from a count to the full call-site surface** (~12 inputs:
`embed_handle` + `permissive` + `instructions` + the 9 crt-056 params + the `[embedding]` section),
directly answering the failure mode that materialized twice at design gate. `permissive` is correctly
GLOBAL-locked (process posture, symmetric with transport); `instructions` correctly per-slug — both
now explicit rows rather than silent omissions.

### Specification Review — PASS

16 FRs, 7 NFRs, 11 ACs. Every SCOPE goal and constraint traces to an FR; every FR to at least one AC.
FR-16/AC-11 encode the single-owner classification and machine-checked drift guard. FR-14/AC-10 encode
`instructions` overlay + fallthrough. FR-15/AC-07 lock `permissive`. AC-02 carries the `Arc::ptr_eq`
strengthening. The "Known Limitation" section documents the silent-ignore residual and the one-way
B-renders-from-A contract explicitly. No requirement exceeds SCOPE; no new config knobs.

### Risk Strategy Review — PASS (with one WARN for human awareness)

Updated framing now reflects **14 risks / 32 scenarios**. Changes since the prior report:
- **R-07** reframed: verdict-checklist drop risk now explicitly notes it *materialized twice* at
  design gate (embed_handle, then permissive/instructions); proof obligation is the full ~12-input
  call-site surface, machine-derived from the live call site, not a count.
- **R-12** (reframed): per-slug `instructions` overlay regression — both arms (overlay + absent-file
  fallthrough to global) must be proven; model-free N=2.
- **R-13** (accepted residual, reclassified): silent-ignore of a global-only key stays an accepted
  residual *for the runtime warn only*; the per-slug-vs-global split is now **owned** by A's canonical
  classification, no longer hand-duplicated.
- **R-14** (new, High / proof obligation): classification ↔ `merge_configs` ↔ seed-render drift — the
  crt-031 multi-copy-divergence pattern — closed by a machine-checked drift-guard test (one assertion
  per call-site input). This is what makes the canonical classification a guarantee rather than "a more
  authoritative lie" (the risk doc's own phrasing).
- **R-03** strengthened: byte-for-byte fallthrough now backed by `Arc::ptr_eq` on the 3 handles, not a
  review note — matching crt-056 AC-2.

Coverage: Critical 1 (R-01) / High 6 (R-02,03,04,05,07,14) / Medium 4 (R-06,09,10,12) / Low 3
(R-08,11,13). All eight scope risks SR-01..SR-08 trace to an architecture/spec element and a proving
scenario (SR-06 is a design-recorded decision, no behavioral test).

**WARN (awareness only — not blocking):** the R-13 residual means an operator who hand-places a
per-slug `[server.tls]` / `permissive` / global-only key gets **no runtime diagnostic** — the key is
silently ignored (only `*_sha256` pin divergence warns, AC-05). The mitigation is documentary
(Feature B's annotated seed, rendered from A's classification) and deferred. This is an explicitly
accepted residual, consistent with the "fail-loud only at the trust boundary" posture; but until
Feature B ships, the *only* signal an operator has that a global-only key was ignored is the absence
of effect. Acceptable to proceed; flagged so it is a conscious acceptance, not an oversight.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns -- no `pattern`-category results surfaced
  for the `vision` topic; goals #4946 (personal-cloud) and #4678 (domain-agnostic) provided the
  authoritative success-criteria, notably #4678's explicit "server instructions" config field that
  validates the FR-14 change.
- Stored: nothing novel to store -- the alignment patterns here (config-as-config-file completes a
  named goal field; one-way later-wave-consumes-earlier-wave seam; by-construction invariants over
  test-only) are already captured by goals #4946/#4678 and entries #4869 (one-way seam) and
  #5198/#5206 (vnc-040 ADRs). The variance profile (zero VARIANCE, one documented accepted-residual
  WARN) is feature-specific and does not generalize.
