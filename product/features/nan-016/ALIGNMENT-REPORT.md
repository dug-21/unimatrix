# Alignment Report: nan-016

> Reviewed: 2026-06-10
> Artifacts reviewed:
>   - product/features/nan-016/architecture/ARCHITECTURE.md
>   - product/features/nan-016/specification/SPECIFICATION.md
>   - product/features/nan-016/RISK-TEST-STRATEGY.md
> Scope source: product/features/nan-016/SCOPE.md (rescoped header authoritative, 2026-06-10)
> Scope risk source: product/features/nan-016/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goal #4922 (personal-cloud)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances goal:personal-cloud — the documented F5 step toward retiring the Rust hook.rs client path |
| Milestone Fit | PASS | Slice A only; cut/deferred surface (#725, #726, F6) cleanly excised; targets exactly the F5 step in goal #4922's delivery path |
| Scope Gaps | PASS | All four SCOPE goals and AC-01..AC-06 are covered by FRs/components/risks |
| Scope Additions | PASS | One in-bounds elaboration (rollback path + negative controls); explicitly scope-allowed, no net-new surface |
| Architecture Consistency | PASS | Honors C-6/C-7/C-8/C-9/C-04; respects "single binary server, JS/TS client is an adapter" and "graceful degradation" principles |
| Risk Completeness | PASS | All nine SR-XX trace to >=1 architecture R-ID; security + fail-open + deferred-flip boundary all covered |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Coverage | Goal 1 (local re-release capability) | SCOPE Goal 1 -> FR-1..FR-4, Component 1 (`dogfood-install.sh`), AC-01. Aligned. |
| Coverage | Goal 2 (switchover mechanism + runbook, delivered not executed) | SCOPE Goal 2 -> FR-5..FR-9, FR-14, Components 2+4, AC-02/AC-04. Aligned. |
| Coverage | Goal 3 (prove copy-install isolation without flipping live) | SCOPE Goal 3 -> FR-10..FR-13, Component 3, AC-03. Aligned. |
| Coverage | Goal 4 (no regressions to client/hook.rs/init path) | SCOPE Goal 4 -> FR-15, NFR-5/6, AC-05/AC-06. Aligned. |
| Addition (in-bounds) | Rollback effect-test + negative controls (R-04, R-06) beyond literal AC text | SCOPE AC-04 names rollback; SCOPE-RISK SR-04 demands "real proof, not tautology." The negative controls strengthen rather than expand scope. Not a variance. |
| Simplification | Component 3 in-repo-edit isolation proof may use a throwaway copy / stash rather than editing the live tracked tree | Rationale: honors SCOPE non-goal "no live-repo perturbation"; left as OQ-D/OQ-1 for pseudocode to pin. Acceptable. |

No scope gaps. No out-of-bounds scope additions. The rescoped SCOPE header's CUT (macOS local, Slice C, AC-07..11) and DEFERRED (#725, #726) items are absent from all three source docs — correctly excised.

## Variances Requiring Approval

None requiring approval. Two items flagged for human awareness (carried forward from the source docs, not new variances):

1. **Follow-up issue for the deferred live flip is not created by this feature.** SCOPE Tracking, ARCHITECTURE OQ-4, and SPECIFICATION "NOT in Scope" all correctly state nan-016 does not create the #682 checklist item / follow-up issue for the eventual flip + F6 soak-clock start. This is a deliberate human action. Surfacing it here so it is not lost: **the human must track the deferred flip on #682 or a follow-up issue.**

2. **Open questions remain for pseudocode/architect, not for vision.** OQ-A (script location), OQ-B (pin `npm pack` vs `npm install --prefix` — ARCHITECTURE picks `npm pack` in ADR-001, SPEC still lists both), OQ-C (re-fire mechanics), OQ-D (AC-03 edit restoration). These are design-level and already routed; none is a vision/scope deviation. Minor consistency note: SPEC OQ-B should be considered resolved by ARCHITECTURE ADR-001 (`npm pack` + extract); confirm the spec is not read as leaving the mechanism open.

## Detailed Findings

### Vision Alignment
The feature advances **goal:personal-cloud** (#4922). Its success criteria include: "Single edge language — JS/TS hook client only ... The Rust hook.rs CLIENT path retires once TS+UDS reaches parity." Goal #4922's delivery path names this feature explicitly: "F5 nan-016 (#681), narrowed to the UDS dogfooding switchover (Slice A) — build + copy-install the in-repo TS client to an external path, repoint this repo's hooks, start the F6 soak clock." The three source docs deliver exactly that minus the flip (correctly deferred). The goal also states the dogfood switchover is "publish-independent (local copy-install — never npm link, never a release) and runs in UDS mode" — matched verbatim by C-6 (copy-install only, never npm link) and OQ-2 (UDS-local).

Architectural-principle conformance:
- Principle 5 (graceful degradation): C-7 fail-open posture is load-bearing across FR-8, FR-11, NFR-2, R-07; the daemon-absent re-fire is a mandated, negative-controlled test. Strong alignment.
- Principle 6 (single binary server; client is an adapter): the feature treats the TS client as an adapter packaged independently of the server binary — ARCHITECTURE notes the platform binary is an optionalDependency excluded from the frozen tree. Consistent.
- Principle 8 (no secrets in DB): N/A — this feature wires hook commands and copies a client tree; no secret handling. Justified N/A.
- Hash-chain / audit-log / capability principles (1,2,3,4,7): N/A for a build/switchover tooling feature that does not touch the knowledge engine's write or query path. Proportional review — infrastructure feature legitimately marks these N/A.

### Milestone Fit
Targets exactly the F5 step. The rescope discipline is exemplary: the SCOPE header enumerates what was CUT (macOS local mode, Goals 3-4 of the draft, Slice C, AC-07..11, darwin build infra) and DEFERRED (#725 remote-init, #726 container HTTPS), and all three source documents omit those surfaces. No future-milestone capability is built ahead of need — F6 soak execution, hook.rs retirement, and the live flip are all explicitly deferred. The feature delivers the F6 reset mechanism without starting the F6 clock (soak-clock boundary held in SCOPE Goal 2 note, FR-14, AC-04, ARCHITECTURE Component 4, RISK R-08). No milestone overreach.

### Architecture Review
Four net-new components, all outside `packages/unimatrix/lib/` (honors C-8). The design correctly identifies the project-root-hash sharing (#4923) as the load-bearing fact behind three decisions and reframes "isolation" (AC-03) as code-freeze, not state-dir separation — matching SCOPE-RISK SR-07 and avoiding a wrong test. ADR-001..005 are coherent and each maps to a scope constraint or SR. Switchover routes through the shipped `mergeSettings` rather than a string swap, which the docs acknowledge introduces a deliberate PreToolUse matcher-narrowing delta — surfaced consistently across ARCHITECTURE, SPEC FR-6, SCOPE-RISK SR-05, RISK R-09, and the runbook (AC-04). Consistency across documents is high; no contradictory claims found.

### Specification Review
FR-1..FR-15 + NFR-1..NFR-8 fully cover SCOPE Goals 1-4 and AC-01..AC-06, each FR tagged to its SR. Acceptance criteria are concrete and effect-based (SR-04 honored): AC-02/AC-03 require a real re-fired hook + negative control, not a string-diff. Domain models pin ubiquitous language (re-release, promotion, soak-reset, deferred flip, copy-install isolation) with the exact meanings the scope intends. "NOT in Scope" mirrors the SCOPE non-goals one-to-one. One minor consistency note (above): OQ-B lists both install mechanisms as open while ARCHITECTURE ADR-001 already pins `npm pack`; not a vision/scope concern.

### Risk Strategy Review
Fifteen architecture risks (R-01..R-15) with a full SR-to-R traceability table — all nine SR-XX map to >=1 R-ID, none accepted out-of-scope without a mitigating R. Critical risks (vacuous verification, non-atomic replace, scratch-hash collision, weak isolation proof) each mandate a non-vacuous proof with a required negative control — directly addressing the central scope-boundary risk (a "delivered but not executed" capability that could be un-validatable, SR-04/A3). The deferred-flip boundary is protected as both a correctness and a security invariant (R-08 tmpdir guard + pre/post live-settings hash). Security risks (path injection into emitted hook command, `rm -rf` target validation, inert postinstall) are appropriate for operator-run tooling. Risk coverage matters for the vision because the whole feature's value is a *provable* re-release mechanism; the strategy guards exactly that.

## Knowledge Stewardship
- Queried: /uni-query-patterns + context_search (category=pattern) for vision alignment / scope-addition / milestone-discipline patterns -- no relevant results (returned unrelated domain patterns: #2298 config divergence, #3426 formatter regression, #2964 signal fusion). No prior vision-alignment pattern exists to apply.
- Stored: nothing novel to store -- nan-016 is a clean, exemplary rescope with no misalignment. The findings are feature-specific (a single tightly-scoped infrastructure slice that fully matches its goal-entry delivery path); there is no recurring 2+-feature variance pattern to generalize.
