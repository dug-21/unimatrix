# nan-020 — Specification: Product Documentation Currency (Doc-Test Enforcement for Executable Claims)

Derived from `product/features/nan-020/SCOPE.md` (AC-01..AC-08, D-1/D-2/D-3, C-1..C-6 LOCKED) and `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09). This spec refines the locked scope into testable requirements; it does not contradict it. **Spec-added proof obligations** (post-design human review): AC-09 (hermetic CI negative control), NFR-9 (hermeticity proof obligation), NFR-10 (node pinned via `setup-node` in `release.yml`), AG-1 (legacy `--remote` documented-but-not-doc-tested, consciously accepted) — these refine, not contradict, locked scope.

## Objective

Establish a minimal, durable mechanism that automatically catches **executable-claim** drift in product documentation before release, and fix the GH #768 proof case. `docs/client-setup.md` is rewritten to the current bundle/observe model, both attach modes are documented correctly in README + client-setup, the uni-docs agent's authoring remit is widened from `README.md` to all of `docs/` (blast-radius scoped), and a doc-test — folded in as a sibling of the nan-019 release smoke — exercises the canonical `--bundle` attach producing a `POST /v1/{slug}/observe` round-trip that lands in the per-slug store. No generate-from-contract; the "verified on vX" stamp is a prose convention, not machine-checked.

## Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Executable claim** | Any documentation line that tells an operator to RUN a command and implies a runtime outcome — e.g. a fenced `bash`/`json`-config block an operator copies and executes, or a CLI invocation in prose presented as the way to attach/emit/observe. Operationally for nan-020 the load-bearing set is: `unimatrix client-bundle <slug>` (server-side bundle emit) and `npx @dug-21/unimatrix init --bundle <blob>` (client attach) culminating in a `POST /v1/{slug}/observe` that the server accepts and persists. These are doc-tested. (SR-06) |
| **Narrative prose** | Explanatory text that describes WHAT something is or WHY, without instructing the operator to run a verifiable command — e.g. "what remote mode is", the fingerprint-pinning rationale, the two-modes overview. Narrative prose is manually rewritten and carries the verified-on stamp; it is NOT doc-tested. (SR-06) |
| **Doc-test** | An executable round-trip that exercises the executable claims of the docs against a freshly built/booted production image and fails CI when a documented command stops working. Owns **detection** of executable-claim drift. For nan-020 it is realized as an EXTENSION of `docker-http-posture-smoke.sh` (D-2). |
| **Currency** | The property that documentation matches shipped behaviour. nan-020 makes currency of executable claims machine-enforced (doc-test) and currency of narrative prose human-signalled (verified-on stamp). |
| **Blast radius** | The set of documentation surfaces a given change *touches*. uni-docs authorship is scoped to the blast radius of the delivered change — NOT a full-tree audit of `docs/` every cycle. (C-4, SR-07) |
| **Verified-on stamp** | A single footer line per doc-file (e.g. `_Verified on v0.x.y_`) recording the last release a human confirmed the file's narrative prose against. A pure authoring convention under uni-docs; NOT machine-checked. (D-3, C-3) |
| **Run-marker** | The anchored terminal line printed only after all gates pass (`[783-smoke] ALL GATES PASSED`), asserted by `run_smoke_gate` so an early `exit 0` cannot false-pass. (AC-06) |
| **Skip path** | Any precondition-absent branch in the doc-test (no Docker, bundle-emit binary absent, emit failed, route/flag absent) that must terminate with a distinct fatal exit code, never a silent green. (C-2, SR-08) |

## Functional Requirements

Each requirement is testable; the verification method is stated in Acceptance Criteria.

### Documentation rewrite

- **FR-1** `docs/client-setup.md` MUST be rewritten to the current model: client attach via `init --bundle` (vnc-034) which wires the pure-JS HTTP hook client automatically; telemetry via the per-slug route `POST /v1/{slug}/observe` (vnc-038). (Goal 2, AC-01)
- **FR-2** `docs/client-setup.md` MUST contain zero occurrences of the literal strings `501`, `W2-7`, and zero hand-rolled `curl ... /observe` hook scripts (the three per-client `curl -X POST .../observe` blocks and their "Returns 501 until W2-7" / "Note" callouts MUST be removed). (AC-01)
- **FR-3** `docs/client-setup.md` MUST NOT instruct operators that "no local binary is required and curl-based shell hooks are used" as the telemetry mechanism; it MUST describe the `init`-wired JS HTTP hook client as the telemetry path. (AC-01)
- **FR-4** Both attach modes MUST be documented correctly in README **and** `docs/client-setup.md`:
  - Legacy F3 direct attach: `init --remote <url> --token <tok>` — MUST be explicitly **MARKED as LEGACY** in the docs (observe/telemetry only; cloud MCP unsupported on this path, per shipped `LEGACY_*` message). Legacy means: documented for completeness, effectively unused, will not be invested in, and — per the accepted gap in Constraints — NOT doc-tested.
  - Bundle attach (canonical, vnc-034/vnc-038): `init --bundle <blob>` where the blob is emitted by `unimatrix client-bundle <slug>`. This is the only canonical mode and the only mode the doc-test exercises. (AC-02)
- **FR-5** The previously-broken example `init --remote unimatrix-bundle:<blob>` (README ~line 123) MUST be corrected to the canonical `init --bundle <blob>` form. (AC-02) **Refinement (see Open Question OQ-A):** shipped `init` retires `--slug` on the bundle path — the bundle URLs already encode the slug (`packages/unimatrix/lib/init.js`); docs MUST therefore NOT instruct passing `--slug` alongside `--bundle`. This refines SCOPE's "`--bundle <blob>` (+ `--slug`)" to the as-shipped surface.
- **FR-6** Each rewritten doc-file under uni-docs's widened remit MUST carry a single verified-on footer line stamping the release it was last human-verified against. The stamp is prose only and is NOT asserted by any test. (D-3, AC — narrative-prose half)

### Doc-test (executable-claim detection)

- **FR-7** A doc-test MUST exercise the canonical bundle attach end-to-end against a freshly built/booted production image: emit a bundle in-test, attach with it, and produce a `POST /v1/{slug}/observe` that the server accepts (HTTP 204) and persists. (AC-03)
- **FR-8** The bundle blob MUST be emitted **in-test** by running the shipped CLI bundle-emit subcommand `unimatrix client-bundle <slug>` against the booted container (D-1, in-test emission), then fed to the client attach. A pre-staged fixture bundle is FORBIDDEN — it would re-introduce the docs-vs-reality gap. (D-1)
- **FR-9** The doc-test MUST assert the observe write **landed in the per-slug store** `/data/.unimatrix/{slug}/unimatrix.db` (per-slug store grew; hash store unchanged), reusing the existing nan-019 WAL-robust dir-size grew-assertion. (AC-03)
- **FR-10** The doc-test MUST be realized by **extending `docker-http-posture-smoke.sh` in place** (D-2), reusing its boot/register/cert-pin/busybox-sidecar setup. It MUST NOT be a new standalone script or a new bespoke CI job. Design MUST first confirm the boot config does not genuinely diverge (e.g. a different image build); a sibling script is permitted ONLY if it does. (C-1, D-2, AC-04)
- **FR-11** The extended doc-test MUST be wired into the same release gate path as the existing smoke — through `run_smoke_gate` (or an equivalent that inherits its exit-code discipline and run-marker assertion). (AC-04)
- **FR-12** The existing nan-019 per-slug-observe assertion (gates 1–4, the #783 regression guard) MUST still pass after the extension; the doc-test extension is additive and MUST NOT regress it. (SR-04, design recommendation 2)
- **FR-13** Bundle-emit failure MUST fail with a distinct, diagnosable error that names the `client-bundle` command, NOT masquerade as an attach/observe failure. Assertion-failure messages MUST distinguish "documented command failed" (real doc drift) from "underlying route/flag changed". (SR-01, SR-02, SR-09)

### Skip-path / no-green discipline

- **FR-14** When Docker is unavailable the doc-test MUST cause the gate to HARD-FAIL (the existing `exit 3` → `run_smoke_gate` `::error:: ... HARD failure` path), never silent-skip / false-green. (C-2, AC-05)
- **FR-15** Every NEW skip path the bundle round-trip introduces (bundle-emit binary absent in the shipped image, emit step failed, route/flag absent) MUST terminate with a distinct fatal exit code routed to a hard gate failure — never `exit 0`/green. (C-2, SR-08, design recommendation 3)
- **FR-16** The doc-test MUST print an anchored terminal run-marker emitted only after all gates pass, and the gate MUST assert it so an early `exit 0` cannot pass. (AC-06)

### Hermeticity (the doc-test must measure the fresh attach, not residual state)

- **FR-22** The host attach leg MUST run hermetically: the `init --bundle` step (and its observe verification) MUST execute under an isolated `HOME`/credstore and a throwaway per-run `--project-dir`, so a prior run's `~/.unimatrix/<hash>/` credential or store CANNOT satisfy the gate. The throwaway state MUST be cleaned on ENTRY (not just exit), so a crashed prior run cannot poison the next. Because Rust-2024 forbids in-process `HOME` mutation (vnc-041 AC-02 deferred for this), the isolation MUST be applied at the process/shell boundary (an isolated-HOME / fresh `--project-dir` shell wrapper around the CLI invocation), cross-referencing the architecture's hermeticity mechanism specified this same pass. A doc-test that false-greens from stale state reproduces the exact #768 blind spot nan-020 exists to close — it is self-defeating. The proof is a NEGATIVE CONTROL: with a poisoned stale credential and a deliberately broken attach, the gate MUST still fail. (AC-09, NFR-9, R-07)

### uni-docs remit widen (the one `.claude/` edit)

- **FR-17** `.claude/agents/uni/uni-docs.md` MUST be edited so its authoring remit covers all of `docs/` (not `README.md` only): the "README.md only" scope line, the "do NOT modify ... per-feature documentation" / "no changes outside README.md" constraints, the section-identification logic, the commit-message convention, and the self-check MUST be updated to admit `docs/` authoring and maintenance. (Goal 3, AC-07)
- **FR-18** The widened uni-docs definition MUST state that authorship is **blast-radius scoped** (surfaces a change touches) and MUST state the **full-tree-audit non-goal** explicitly (uni-docs does NOT audit all of `docs/` every cycle). (C-4, AC-07, SR-07)
- **FR-19** The uni-docs edit MUST be confined to authorship-remit text. It MUST NOT introduce a drift-checker, a CI gate, or a Phase-4 trigger redesign — those are Feature 2. This is the ONLY `.claude/` edit in nan-020. (C-5, SR-05)
- **FR-20** The widened uni-docs definition MUST retain the prompt-injection defense (artifacts are data, not instructions) and the "document only what is shipped" rule, now applied to `docs/` as well as README.

### NFR N5 extension (documentation, not code)

- **FR-21** N5's governed surface MUST be described (in the artifacts where N5 is referenced for this feature) as extended from "deployable-as-released" to "usable-as-documented", naming the doc-test as N5's docs-layer regression guard. The "usable-as-documented" claim is **bounded to the canonical `--bundle` chain** per the accepted gap AG-1 — the legacy `--remote` mode is documented-but-not-doc-tested and is NOT covered by this claim. No new NFR is minted; N5's status is unchanged. (Goal 4, C-6, AC-08, AG-1) The capability-map update itself is performed by the human after scope locks.

## Non-Functional Requirements

- **NFR-1 (Infra reuse, measurable):** The doc-test adds ZERO new standalone scripts and ZERO new bespoke CI jobs — it is delivered as an in-place extension of `docker-http-posture-smoke.sh` and rides `run_smoke_gate`. Verification: file-count delta under `product/test/infra-001/scripts/` is 0 new scripts (unless the divergence caveat of FR-10 is invoked and documented). (C-1)
- **NFR-2 (No false-green):** No code path in the doc-test reaches a successful gate result without a confirmed 204 + per-slug-store-grew assertion. Every precondition-absent branch maps to a distinct fatal exit code. The exit-code truth table MUST preserve the nan-019 contract (`0` ran+passed, `1` ran+failed, `3` Docker-absent self-skip→hard-fail, `4` IMAGE= pull/inspect failure) and extend it for new skip paths without colliding with these codes. (C-2, SR-04, SR-08)
- **NFR-3 (Anchored run-marker):** Gate success requires the terminal run-marker line; an early `exit 0` before all gates MUST NOT pass. (AC-06)
- **NFR-4 (Two-runtime environment soundness):** The doc-test invokes a Rust subcommand (`client-bundle`) and a JS attach (`init`). The design MUST confirm the production image (and the lane that runs the test) supplies the runtimes each step actually uses; if a step's runtime is not what an operator has, the assertion is unsound and MUST fail diagnosably rather than skip. (SR-03, A3)
- **NFR-5 (Minimal mechanism / no gold-plating):** No generate-from-contract; no machine-checking of the verified-on stamp; no full-tree docs audit. The mechanism is the smallest durable thing that catches executable-claim drift. (C-3, D-3, C-4)
- **NFR-6 (Blast-radius authoring scope):** uni-docs touches only the doc surfaces a change affects; cost does not scale with `docs/` size. (C-4)
- **NFR-7 (Performance):** The doc-test reuses the single throwaway-container boot of the existing smoke (no second image build / second container) where boot config does not diverge — no material increase in release-gate wall-clock beyond the bundle-emit + attach + one extra observe round-trip. (D-2)
- **NFR-8 (Terminology consistency):** Rewritten docs MUST use shipped terminology: "Unimatrix", `context_*`, `/v1/{slug}/observe`, `client-bundle`, `init --bundle`. (carried from uni-docs NFR-07)
- **NFR-9 (Hermetic CI — proof obligation):** The doc-test's host attach leg MUST be hermetic: isolated `HOME`/credstore + throwaway per-run `--project-dir`, cleaned on entry, applied at the process/shell boundary (Rust-2024 forbids in-process `HOME` mutation — vnc-041 AC-02 deferred for this). Verification is NOT "isolation code exists" but a NEGATIVE CONTROL: a poisoned stale credential + a broken attach MUST still fail the gate. Residual state must never substitute for a working fresh attach. This is a hard proof obligation (AC-09), not a test-plan footnote — a stale-state false-green reproduces the #768 blind spot the feature exists to close. (FR-22, AC-09, R-07)
- **NFR-10 (node explicitly provisioned / pinned):** The host JS leg (`init --bundle`) requires `node`. `node` MUST be **explicitly provisioned on the release runner via a pinned `setup-node` step in `release.yml`** — NOT relied upon as incidental presence in the runner image. Rationale: node-absence is already a hard-fail (ADR-001/002, FR-15); if node is merely incidental, an unrelated runner-image change silently arms a release-blocker (the hard-fail becomes latent, not intentional). Same "pin your infra" discipline as #793 (busybox). Verification: inspect `release.yml` for a `setup-node` step with a pinned version on the lane that runs the doc-test. (FR-15, R-04)

## Acceptance Criteria (with verification methods)

| AC | Criterion | Verification method |
|----|-----------|---------------------|
| **AC-01** | `docs/client-setup.md` has zero `501`, zero `W2-7`, zero hand-rolled `curl ... /observe` hook scripts; documents the `init`-wired hook client and `/v1/{slug}/observe`. | Grep assertion: `grep -c -E '501|W2-7' docs/client-setup.md` returns 0; no fenced block matches `curl .* (POST .*)?/observe`; positive grep finds `init --bundle` and `/v1/{slug}/observe`. (FR-1..FR-3) |
| **AC-02** | Both attach modes documented correctly in README + client-setup; the legacy `--remote <url> --token <tok>` mode is explicitly MARKED as LEGACY; broken `init --remote unimatrix-bundle:<blob>` corrected to canonical `init --bundle <blob>`. | Grep/inspection: both `--remote <url> --token <tok>` and `init --bundle <blob>` present in both files; the `--remote` mode is annotated "legacy" (or equivalent) in both files; zero occurrences of `init --remote unimatrix-bundle:`; no `--slug` paired with `--bundle` (OQ-A). (FR-4, FR-5) |
| **AC-03** | Doc-test exercises canonical `--bundle` attach → successful `POST /v1/{slug}/observe` (write accepted 204, lands in per-slug store) against a freshly built/booted image. | Run the extended `docker-http-posture-smoke.sh` in a Docker-capable lane: it emits a bundle in-test via `client-bundle <slug>`, attaches via `init --bundle`, POSTs to `/v1/{slug}/observe`, asserts HTTP 204 AND per-slug store grew while hash store unchanged. Gate returns 0 with run-marker. (FR-7..FR-9) |
| **AC-04** | Doc-test is a SIBLING of nan-019 smoke (lives under `product/test/infra-001/scripts/`, reuses throwaway-container pattern, same release gate path); NOT a new bespoke CI job. | Inspection: no new script file added (D-2 extend-in-place) and no new CI job; the round-trip runs inside `docker-http-posture-smoke.sh` and is invoked via `run_smoke_gate`. (FR-10, FR-11, NFR-1) |
| **AC-05** | Docker unavailable ⇒ gate HARD-FAILs (no silent skip / false-green). | No-Docker run: execute the doc-test on a host without Docker; assert it `exit 3`s with a SKIP reason and that `run_smoke_gate` converts it to `::error::smoke SKIPPED (exit 3) ... HARD failure` returning non-zero (gate fails). Repeat for each new skip path (FR-15): each yields a distinct fatal exit and a failing gate. (FR-14, FR-15, NFR-2) |
| **AC-06** | Doc-test asserts an anchored terminal run-marker so an early `exit 0` cannot pass. | Inject/simulate an early `exit 0` before all gates; assert `run_smoke_gate`'s `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` fails the gate. (FR-16, NFR-3) |
| **AC-07** | `.claude/agents/uni/uni-docs.md` remit widened from "README.md only" to authoring/maintaining all of `docs/`, blast-radius-scoped, with the full-tree-audit non-goal stated. | Inspection: scope/constraint/self-check sections admit `docs/`; "blast radius" defined as surfaces a change touches; explicit "does NOT audit all of docs/ every cycle" statement present; no drift-checker/gate/trigger text added. (FR-17..FR-20) |
| **AC-08** | N5 described as extended to "usable-as-documented" with the doc-test named as its docs-layer regression guard; no new NFR minted. | Inspection of the artifact(s) referencing N5 for this feature: N5 framing reads "deployable-as-released → usable-as-documented", doc-test named as guard, N5 status unchanged, no new NFR/capability id introduced. (FR-21) |
| **AC-09** | The doc-test is HERMETIC: it runs the host `init --bundle` attach (Gate 6) under an isolated credstore/`HOME` and a throwaway per-run `--project-dir`, so a prior run's `~/.unimatrix/<hash>/` credential or store cannot satisfy the gate. The gate MUST measure the fresh attach, not residual state. This is a PROOF OBLIGATION, not a test-plan footnote. | **NEGATIVE CONTROL (discrimination, #4977):** pre-seed/poison the credstore with a stale credential AND point the attach at a deliberately broken target; assert Gate 7 STILL FAILS. A test that greens with a poisoned cred + broken attach is vacuous and reproduces the #768 blind spot nan-020 exists to close. Plus: assert HOME/credstore isolation and the throwaway `--project-dir` are cleaned on ENTRY (not just exit, so a crashed prior run cannot poison the next), and assert non-skip evidence (store grew by the *new* write). The isolation MUST operate at the process/shell boundary — Rust-2024 forbids in-process `HOME` mutation (vnc-041 AC-02 deferred for exactly this), so the architecture's hermeticity mechanism is a shell-level isolated-HOME/`--project-dir` wrapper, not in-process env mutation. (FR-22, NFR-9; R-07) |

## Constraints (mirrored from SCOPE C-1..C-6)

- **C-1 (load-bearing):** Fold into the existing nan-019 release smoke — LOCKED form D-2: extend `docker-http-posture-smoke.sh` in place; split to a sibling only if boot config genuinely diverges. No new bespoke CI job.
- **C-2 (load-bearing):** Docker-absent (and every new skip path) MUST hard-fail, never silent-green.
- **C-3:** No generate-from-contract. Manual rewrite for prose; doc-test for executable claims. The verified-on stamp is a single non-machine-checked footer line (D-3). Minimal durable mechanism, no gold-plating.
- **C-4:** uni-docs authorship is blast-radius scoped, not full-`docs/` audit per cycle.
- **C-5:** Exactly one `.claude/` edit (uni-docs remit widen). All other `.claude/` currency work is Feature 2.
- **C-6:** Extend NFR N5; do not mint a new NFR.

### Accepted Gap (named so it is not later mistaken for full coverage)

- **AG-1 (legacy `--remote` is documented-but-not-doc-tested — DELIBERATE):** The doc-test exercises ONLY the canonical `--bundle` chain (AC-03). "Usable-as-documented" coverage (N5 extension, Goal 4, AC-08) therefore applies to the canonical `--bundle` chain ONLY. The legacy `init --remote <url> --token <tok>` mode is **documented (marked LEGACY per AC-02/FR-4) but NOT doc-tested**, by deliberate scope decision: bundle is the only important/canonical mode; `--remote` is legacy, effectively unused, and will not be invested in. This is named explicitly so nobody later reads N5's "usable-as-documented" as covering both modes. It is the seed-of-the-next-#768 risk, **consciously accepted** — if `--remote` ever returns to active use, doc-testing it must be re-opened. (FR-4, FR-21, AC-02, AC-08)

## Dependencies

- **nan-019** — release smoke infrastructure (`docker-http-posture-smoke.sh`, `release-gate-lib.sh::run_smoke_gate`, exit-code 0/1/3/4 truth table, anchored run-marker). The doc-test extends this in place.
- **vnc-038** — per-slug route `POST /v1/{slug}/observe`; the dumb-client attach the doc-test exercises.
- **vnc-034** — bundle attach mode (`init --bundle`); `unimatrix client-bundle <slug>` bundle emit.
- **vnc-022** — original `/observe` ship (proof-case context).
- **#767** — owns the `README:62` ONNX claim (cross-reference only; out of scope).
- **Shipped CLI surfaces confirmed at spec time:** `packages/unimatrix/bin/unimatrix.js` plumbs `--remote/--token` and `--bundle/--slug`; `init` consumes `--bundle <blob>` as the canonical bundle path and emits `LEGACY_*` guidance pointing operators to `init --bundle <bundle>`; `crates/unimatrix-server/src/main.rs` has `ClientBundle { slug }` (`client-bundle <slug>`) as a pre-tokio sync clap subcommand.

## NOT in Scope

- **Feature 2 — the `.claude/` automation-currency pattern.** Not designed or implemented here; only the single uni-docs remit-text edit rides in nan-020 (C-5).
- **Generate-from-contract** (auto-generating docs from a contract). Explicitly killed (C-3).
- **Machine-checking the verified-on stamp's freshness** (D-3, C-3).
- **Auditing all of `docs/` every cycle** (C-4).
- **A new bespoke CI job or standalone doc-test script** (C-1, D-2) unless the FR-10 divergence caveat is triggered and documented.
- **Minting a new NFR** (C-6).
- **Fixing `README:62`'s ONNX claim** (owned by #767).
- **Any CLI/route behaviour change** — nan-020 documents and tests shipped surfaces; it does not modify `init`, `client-bundle`, or the observe route.
- **README's other two self-healed defects** — no action.

## Open Questions (for architect / human)

- **OQ-A (refinement, surfaced by shipped code):** SCOPE AC-02/AC-03 say "`--bundle <blob>` (+ `--slug`)", but shipped `init` (`packages/unimatrix/lib/init.js`) RETIRES `--slug` on the bundle path — the bundle URLs already encode the slug, and `--slug` is the legacy/pre-bundle flag. This spec specifies docs as `init --bundle <blob>` WITHOUT `--slug` (FR-5). Confirm the architect/docs author follow the as-shipped surface, not the parenthetical in SCOPE. (Non-blocking: the as-shipped form is the testable truth.)
- **OQ-B (SR-01/A1, highest-leverage de-risk for the architect):** Confirm `client-bundle <slug>` is invocable inside the *shipped* image (correct binary path, exact subcommand name+signature) before AC-03 is committed to it. If the binary is absent or renamed in the shipped image, D-1's in-test-emission is invalid and FR-8 must fail diagnosably rather than skip.
- **OQ-C (SR-03/A3, for the architect):** Confirm the production image AND the gate lane supply both runtimes the doc-test invokes (Rust `client-bundle`, JS `init`). If the JS attach runs on the build host rather than mirroring an operator's environment, document that boundary so the assertion stays sound (NFR-4).
- **OQ-D (D-2 caveat, for the architect):** Confirm the bundle round-trip can reuse the existing smoke's boot config (same image build); only if it genuinely diverges may the doc-test split to a sibling script (FR-10, C-1).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced nan-019 patterns (#5192 anchored run-marker + exit-code case unit-test; #5185 AC-05 grew-assertion via busybox sidecar), nan-005 uni-docs/delivery-protocol precedent (#1255), and the personal-cloud "Bundle attach, dumb client — server owns routes" capability (#5151). Applied: exit-code truth table, grew-assertion reuse, blast-radius framing. No spec-specific storage (read-only tier).
