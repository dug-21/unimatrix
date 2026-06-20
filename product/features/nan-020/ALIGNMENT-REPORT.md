# Alignment Report: nan-020

> Reviewed: 2026-06-20
> Artifacts reviewed:
>   - product/features/nan-020/architecture/ARCHITECTURE.md
>   - product/features/nan-020/specification/SPECIFICATION.md
>   - product/features/nan-020/RISK-TEST-STRATEGY.md
> Scope source: product/features/nan-020/SCOPE.md + SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #4946 (personal-cloud); capability #5163 (N5)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves goal:personal-cloud — docs are the operator onboarding path; N5 extended, not duplicated. |
| Milestone Fit | PASS | Targets the right surface; Feature 2 (`.claude/` currency) explicitly fenced out. |
| Scope Gaps | PASS | All eight ACs and C-1..C-6 traced into SPEC FRs and RISK scenarios. |
| Scope Additions | WARN | Three as-shipped refinements (drop `--slug`; node-absent hard-fail; host/container split) extend scope beyond SCOPE's literal text — correctly justified, but they widen the tested surface. |
| Architecture Consistency | PASS | ADR-001..004 consistent with locked D-1/D-2/D-3; A3 correction (no JS in image) handled coherently. |
| Risk Completeness | PASS | All 9 SRs traced; static-layer/over-build risks (R-08, R-13, R-14, R-15) explicitly bounded against gold-plating. |

Status counts: PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE AC-01..AC-08 and C-1..C-6 maps to a SPEC FR and at least one RISK scenario. |
| Addition | `--slug` retired on bundle path | SCOPE AC-02/AC-03 say `--bundle <blob>` (+ `--slug`); ARCH (OQ-A, verified `init.js:353`) and SPEC FR-5 drop `--slug`. Correct: code is authoritative; SCOPE parenthetical was wrong. Recorded as OQ-A for the human, not silently changed. |
| Addition | node-absent → hard-fail (exit 1) | SCOPE C-2 names only Docker-absent as the hard-fail case. ARCH/SPEC FR-15 add node-absence (and bundle-emit/route-absent) as new hard-fail skip paths. Faithful extension of C-2's intent (never silent-green), but it is a new failure surface SCOPE did not enumerate. |
| Addition | Host/container runtime split (ADR-002) | SCOPE assumed (A3) the image ships both runtimes; design CORRECTED this — distroless image has no JS, so `init --bundle` runs on the CI host. This is a material topology decision not present in SCOPE. Well-reasoned (host = operator surrogate) but worth the human's eye. |
| Simplification | Bundle gates fold into existing `fail()`/exit 1 (no new exit codes 5/6/7) | ADR-001. Rationale: bespoke exit codes would force a `run_smoke_gate` edit (blast-radius into the load-bearing wrapper) for zero gate-behavior gain. Satisfies SR-08 intent via distinct *messages*. Sound anti-gold-plating call. |

## Variances Requiring Approval

None rise to VARIANCE or FAIL. One WARN is presented for human awareness (no approval gate required, but confirm the framing):

**WARN-1 — As-shipped refinements widen the scope surface beyond SCOPE's literal text.**
1. **What**: Three design-time corrections push past SCOPE wording — (a) `--slug` dropped on the bundle path, (b) node-absence and other new skip paths added as hard-fails, (c) the host/container runtime split because the image ships no JS.
2. **Why it matters**: Each is a faithful reading of *intent* (code-truth over SCOPE prose; never silent-green; reproduce the real operator topology). None over-builds. But (b) and (c) expand the doc-test's environment surface — exactly the place SR-03/SR-04 warn the existing nan-019 release gate (the project's primary release guard) is fragile. They are correct, not gold-plating; the WARN is to make sure the human ratifies that SCOPE's text is now superseded by the as-shipped surface, and that AC-02/AC-03 wording will be reconciled (OQ-A).
3. **Recommendation**: ACCEPT. The corrections are justified and verified against shipped code. Action: at design close, reconcile SCOPE AC-02/AC-03's "(+ `--slug`)" parenthetical to match the as-shipped `--bundle <blob>` (no `--slug`) so the locked SCOPE and the delivered surface do not themselves drift — the very failure class this feature exists to kill.

## Detailed Findings

### Vision Alignment
The feature advances **goal:personal-cloud (#4946)** on its most literal axis: the goal's intent is "any developer deploys and operates a personal Unimatrix cloud without ops friction… one container, one bearer token, one command," and `docs/client-setup.md` is the operator's onboarding script for exactly that path. The proof case (#768) is a hard-stop on that onboarding — the documented attach path errors and teaches an obsolete pre-bundle model. Healing it directly serves the goal.

The work also extends **N5 (#5163)** — "the shipped artifact is always deployable as released" — from "deployable-as-released" to "usable-as-documented," with the doc-test as the docs-layer regression guard. SCOPE C-6/AC-08 correctly EXTEND N5 rather than mint a new NFR (capability inflation avoided), consistent with the memory note on not overstating defensive structure. N5's status is explicitly unchanged.

**Static-layer principle (PRODUCT-VISION line 13):** the vision states the static layer (workflow/agent/skill definitions, and by extension docs/CLI) "change infrequently." nan-020 honors this *explicitly* and twice over: (1) Non-Goal "Generate-from-contract … is gold-plating for a small, slow-changing attach surface and cuts against the product principle that the static layer changes infrequently" (SCOPE line 35); (2) the mechanism chosen is the *smallest durable thing* — one in-place script extension + manual prose rewrite + a non-machine-checked stamp. This is the correct response to a slow-changing surface: detect the executable claims, human-signal the prose. No standing daemon, no generator, no per-command gate.

### Milestone Fit
Correctly scoped to one milestone-appropriate increment. The adjacent `.claude/` automation-currency mechanism (Feature 2) is explicitly NOT designed here (SCOPE Non-Goals; C-5; ADR-004 "fences Feature 2"); only the single uni-docs remit-text edit rides along as the authorship half of the #768 fix. RISK R-13 and SR-05 both guard against Feature-2 machinery leaking in. This is good milestone discipline — building the future capability now would have been the VARIANCE; the design deliberately does not.

### Architecture Review
ARCH is internally consistent with the locked decisions:
- **D-2 (extend in place)** honored, with the A2/OQ-D caveat preserved verbatim ("split to a sibling ONLY if boot config genuinely diverges") and the reuse-in-place condition affirmatively checked (same container/volume/slug/port/token/cert).
- **A3 CORRECTED honestly**: the design caught that SCOPE's assumption (image ships both runtimes) is false — distroless ships no node. Rather than paper over it, ADR-002 makes the host/container split a load-bearing, documented decision and the runtimes table makes the topology explicit. This is the right behavior for a guardian to see: a verified correction of a scope assumption, surfaced not buried.
- **ADR-001 anti-gold-plating** (no new exit codes 5/6/7) is well-argued and aligns with the vision's preference for not widening blast radius into load-bearing infrastructure for zero behavioral gain.
- The "executable-claim vs narrative-prose" boundary (ADR-003) is rendered as a 3-part operational contract with a worked example — this is the single load-bearing distinction (SR-06) and the design operationalizes it rather than leaving it to prose judgment.

### Specification Review
SPEC FR-1..FR-21 cover every SCOPE AC with stated verification methods. The Ubiquitous Language section pins "executable claim," "narrative prose," "blast radius," and "verified-on stamp" — exactly the terms whose ambiguity SR-06/SR-07 flag as load-bearing. FR-5's refinement (drop `--slug`) and OQ-A explicitly reconcile the SCOPE-vs-shipped tension and route it to the human rather than resolving it silently. NFR-5 ("minimal mechanism / no gold-plating") and NFR-1 ("ZERO new scripts, ZERO new bespoke CI jobs") make the static-layer / anti-over-build posture measurable. No spec requirement over-reaches SCOPE; the only deltas are the as-shipped refinements noted under Scope Additions.

### Risk Strategy Review
The 15-risk register traces all 9 SRs (traceability table is complete: "All nine scope risks covered … None accepted/dropped"). Notably for the over-build caution:
- **R-08** guards *both* failure modes of the executable/prose boundary — under-broad (drift persists, another #768) AND over-broad (a gate per command = gold-plating, C-3).
- **R-13/R-14** treat the N5-extension and uni-docs remit text as human-owned with NO automated coverage, explicitly stating that building coverage for them would be gold-plating (C-3/ADR-004). This is the correct restraint — it resists the over-built-defensive-structure trap the project memory warns about.
- **R-15** guards against silently introducing a second image build / divergent boot config — i.e., scope-creep against D-2's reuse-in-place lock.
The pre-merge-provable vs post-tag-confirmable split (#5189) is applied correctly so the gate logic is proven without waiting on a live container, and post-tag items are labeled PENDING rather than asserted as run — no false-green in the test strategy itself.

## Knowledge Stewardship
- Queried: /uni-query-patterns + context_search for vision-alignment / scope-addition / over-build patterns -- no relevant prior vision-alignment patterns surfaced (hits were config/scoring-weight ADRs, not applicable). The hook-provided pattern on "optional future branch must match scope intent" was checked: nan-020 does NOT exhibit it — Feature 2 is cleanly deferred with zero test scenarios in RISK, so no WARN on that axis.
- Stored: nothing novel to store -- the as-shipped-refinement WARN is feature-specific (SCOPE's `--slug` parenthetical contradicted shipped `init.js`); it does not yet generalize across features. The recurring "self-skipping gate must hard-fail / prove pre-merge" pattern is already captured at #5180/#5183/#5189. If "design corrects a SCOPE assumption verified against shipped code" recurs, store it then.
