# Scope Risk Assessment: nan-020

Lightweight pre-design risk pass. SR-IDs are traced forward in RISK-TEST-STRATEGY.md.
Historical grounding: lessons #5208 (nan-019 smoke cross-runner acquisition + exit-code contract), #4873 (exit-code false-green trap), #4582 (Dockerfile correctness needs real build).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | D-1 in-test bundle emission runs `unimatrix client-bundle <slug>` — a Rust-binary clap subcommand inside the production image, NOT the JS bin (`packages/unimatrix/bin/unimatrix.js`). The binary may be absent/at a different path in the shipped image, or the subcommand may be renamed/moved (SCOPE flags "confirm name at design"). False-fail or false-skip if invocation assumptions break. | High | Med | Design MUST locate the binary inside the *shipped image* (not the build host) and pin the exact subcommand name+signature against `crates/unimatrix-server/src/main.rs` at design time. Fail with a distinct, diagnosable error if the bundle-emit step itself fails (don't let it masquerade as an attach failure). |
| SR-02 | D-1 couples the doc-test to a Rust CLI surface (`client-bundle`) whose stability nan-020 does not own; a future CLI rename silently breaks the doc-test, not the docs it guards. | Med | Med | Treat the bundle-emit command as a documented executable claim itself (it appears in README "Serving projects"); the doc-test's failure on a rename is correct signal, but the error message must name the command so the break is attributable. |
| SR-03 | Two-runtime attach path (Rust `client-bundle` emit -> JS `init --bundle`) inside one throwaway container increases the environment surface the doc-test depends on (both runtimes present, on PATH, compatible versions). | Med | Med | Architect should confirm the production image actually ships both runtimes the doc-test invokes; if not, the test's environment diverges from what an operator has and the assertion is unsound. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | D-2 EXTENDS `docker-http-posture-smoke.sh` in place. nan-019 (lesson #5208) gave this script a load-bearing, fragile exit-code contract (0/1/3/4) and IMAGE= cross-runner pull-with-fallback logic. Adding a bundle round-trip in place risks regressing the existing release smoke gate — the project's primary release guard. | High | Med | Design MUST preserve the exact exit-code truth table (incl. exit 4 acquisition arm) and the anchored terminal run-marker; add coverage that the *existing* per-slug-observe assertion still passes after extension. Honor the SCOPE caveat: split to a sibling if boot config genuinely diverges. |
| SR-05 | The single `.claude/` edit (widen uni-docs remit, C-5/Goal 3) rides in nan-020 alongside the explicitly-separate Feature 2 `.claude/` automation-currency pattern — scope-creep magnet toward designing Feature 2 here. | Med | Med | Spec writer: constrain the uni-docs edit to authorship-remit text only (README-only -> all of `docs/`, blast-radius-scoped, full-tree-audit non-goal stated). NO drift-checker, NO gate, NO Phase-4 trigger redesign — those are Feature 2. |
| SR-06 | "Executable claim vs. narrative prose" is the load-bearing distinction (what gets doc-tested vs. manually stamped), but is defined only by intent prose. An over-broad reading pulls narrative into the doc-test (gold-plating, violates C-3); an under-broad reading leaves drifting commands untested (defeats the feature). | High | Med | Spec writer: give an operational definition + a worked example from `docs/client-setup.md` (which lines are executable claims). Tie it to AC-03's "canonical `--bundle` attach + `/v1/{slug}/observe` round-trip" as the concrete testable set. |
| SR-07 | uni-docs authorship is blast-radius-scoped, NOT full-`docs/` audit (C-4). Risk the design under-specifies "blast radius," so the agent either audits everything (cost/gold-plating) or misses touched surfaces (drift persists). | Med | Med | Define blast radius concretely: surfaces a change *touches*. State it in both SPEC and the uni-docs definition (AC-07). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | C-2 hard-fail-on-no-Docker is the entire reason this feature exists (lesson #4796/#4970 false-green). If the bundle round-trip is added on a code path that can early-`exit 0`, or the bundle-emit/init step skips silently when a precondition is absent, nan-020 re-creates the exact blind spot it closes. | High | Med | Architect: route the doc-test through `run_smoke_gate` (or an equivalent inheriting exit-3-is-fatal + anchored terminal run-marker, AC-05/AC-06). Every skip path (no Docker, no binary, emit failed) MUST hard-fail with a distinct exit code, never green. |
| SR-09 | Per-slug `/v1/{slug}/observe` (vnc-038) + `--bundle`/`--slug` (vnc-034) are the dependencies the doc-test exercises. If these surfaces shift, the doc-test breaks for reasons unrelated to docs — but that is partly the point. Risk is misattribution. | Low | Low | Ensure assertion failure messages distinguish "documented command failed" (real doc drift) from "underlying route/flag changed" so the signal is actionable. |

## Assumptions

- **A1 (SCOPE §Locked D-1, line 85):** `unimatrix client-bundle <slug>` exists as a sync pre-tokio clap subcommand in the *shipped image*. SCOPE itself flags "confirm name at design if it has moved" — if wrong, D-1's whole in-test-emission approach is invalid (SR-01/SR-02).
- **A2 (SCOPE §Background, line 49 / D-2, line 86):** the `--bundle` round-trip is a near-superset of the existing per-slug-observe gate with the *same boot config*. If boot config diverges, extend-in-place (D-2) is the wrong topology — split to sibling (SCOPE caveat, C-1).
- **A3 (SCOPE §In Scope 3, line 80):** the production image ships both the Rust binary and the JS init client the doc-test invokes. If either is absent in the shipped image, the doc-test environment diverges from the operator's (SR-03).
- **A4 (SCOPE §Goals 2, line 28):** `docs/client-setup.md`'s remaining defects are fully enumerated (6×501/W2-7, curl hooks, broken `--remote unimatrix-bundle:` example) and #767/self-healed items are correctly excluded. If un-enumerated drift exists, AC-01's zero-occurrence assertion may pass while other claims stay stale.

## Design Recommendations

1. **Pin the CLI contract at design (SR-01/A1):** locate `client-bundle` inside the shipped image and confirm exact subcommand name+signature before the spec writer commits AC-03 to it. This is the single highest-leverage de-risking action.
2. **Protect the nan-019 gate (SR-04/SR-08):** treat the existing exit-code truth table (0/1/3/4 per #5208) and run-marker as invariants the extension must not break; add a regression assertion that the original per-slug-observe smoke still passes post-extension.
3. **Make every skip a hard-fail (SR-08):** enumerate all skip paths (no Docker, no binary, emit failed, route absent) and assign each a distinct fatal exit code — the C-2 discipline must cover the *new* failure modes the bundle round-trip introduces, not just the no-Docker case.
4. **Operationalize "executable claim" (SR-06) and "blast radius" (SR-07):** spec writer gives both an operational definition + a worked example to keep the doc-test minimal (C-3) and uni-docs authorship bounded (C-4).
5. **Fence the `.claude/` edit (SR-05):** uni-docs remit-text widen only; explicitly out-of-scope any Feature 2 drift-checker/gate.
