# nan-020 — Implementation Brief: Product Documentation Currency (Doc-Test Enforcement for Executable Claims)

> Regenerated 2026-06-20 after a human-directed design revision (revision pass). This brief
> supersedes any prior version and reflects the current sources verbatim. The three load-bearing
> changes this pass — **hermeticity as a hard proof obligation (negative control)**, **node
> explicitly provisioned via a pinned `setup-node@v4`**, and **legacy `--remote` marked legacy +
> documented-but-not-doc-tested** — are surfaced as delivery MANDATES below.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-020/SCOPE.md |
| Scope Risk Assessment | product/features/nan-020/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/nan-020/specification/SPECIFICATION.md |
| Architecture | product/features/nan-020/architecture/ARCHITECTURE.md |
| ADR-001 (extend-in-place exit-code contract) | product/features/nan-020/architecture/ADR-001-extend-in-place-exit-code-contract.md |
| ADR-002 (in-test emission, host/container split; setup-node pin) | product/features/nan-020/architecture/ADR-002-in-test-bundle-emission-host-container-split.md |
| ADR-003 (executable-claim boundary) | product/features/nan-020/architecture/ADR-003-executable-claim-boundary.md |
| ADR-004 (uni-docs remit widen) | product/features/nan-020/architecture/ADR-004-uni-docs-remit-widen.md |
| ADR-005 (hermeticity as proof obligation) | product/features/nan-020/architecture/ADR-005-hermeticity-as-proof-obligation.md |
| Risk Strategy | product/features/nan-020/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-020/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/nan-020/ACCEPTANCE-MAP.md |

## Goal

Establish a minimal, durable mechanism that automatically catches **executable-claim** drift in
product documentation before release, and heal the GH #768 proof case. `docs/client-setup.md` is
rewritten to the current bundle/observe model, both attach modes are documented correctly (legacy
`--remote` MARKED legacy; canonical `--bundle` no `--slug`), the uni-docs authoring remit is widened
from `README.md` to all of `docs/` (blast-radius-scoped), and a doc-test — folded in-place as an
extension of the nan-019 release smoke — exercises the canonical `--bundle` attach producing a
`POST /v1/{slug}/observe` round-trip that lands in the per-slug store, **measured hermetically** so a
prior run's residual credential can never false-green it.

## Delivery Mandates (revision pass — read before anything else)

These three are HARD obligations of this revision. They are not open questions; they are required of delivery.

1. **Hermeticity is a HARD proof obligation, proven by a REQUIRED pre-merge NEGATIVE CONTROL
   (AC-09 / FR-22 / NFR-9 / ADR-005; risk R-07 is now CRITICAL).**
   - Gates 6–7 (host `init --bundle` consume + hook-fire observe) MUST run under an **isolated
     `HOME`/credstore + a throwaway per-run `--project-dir`**, established at the **process/shell
     boundary** — `HOME="$SANDBOX/home" node … init --bundle … --project-dir "$SANDBOX/proj"`
     (`SANDBOX="$(mktemp -d)"`). The SAME isolated env is reused for the Gate 7 hook fire.
   - Isolation MUST be at the process boundary, NOT in-process: Rust-2024 forbids
     `std::env::set_var("HOME", …)` (vnc-041 AC-02 deferred its in-process round-trip for exactly
     this). Do NOT attempt in-process HOME mutation — it is unsound and will silently not isolate.
   - Clean **on ENTRY** (plus trap on exit), so a crashed prior run cannot poison the next.
   - The REQUIRED pre-merge negative control: **pre-seed/poison a stale valid-looking credential at
     the location a non-isolated run would read it AND point Gate 6 at a deliberately BROKEN attach;
     assert Gate 7 STILL FAILS.** A harness that greens that scenario is vacuous and reproduces the
     exact #768 blind spot the feature exists to close. This is PRE-MERGE-PROVABLE against a stub
     broken-attach — classifying it PENDING IS a gap (#5189). Modeled on vnc-041 AC-06 / #5246.

2. **Node is explicitly provisioned + pinned (NFR-10 / amended ADR-002).**
   - Add a pinned `actions/setup-node@v4` step (`node-version: '24'`, matching the `package-npm` job
     at `release.yml:215–218`) to BOTH smoke jobs in `release.yml`, immediately after
     `actions/checkout@v4` and BEFORE the `run_smoke_gate` step.
   - The node-absent hard-fail (ADR-001, `fail()` exit 1, NOT exit 3) is now an INTENTIONAL SAFETY
     NET for a provisioning regression — not the acquisition path. The `command -v node` script
     preflight stays as defense-in-depth. (The #793 "pin your infra" discipline, same as busybox.)

3. **Legacy `--remote` is marked legacy + documented-but-NOT-doc-tested — a CONSCIOUSLY ACCEPTED GAP
   (SCOPE Non-Goals + C-7 + AG-1 + AC-02; risk R-16 accepted residual).**
   - Docs (README + `docs/client-setup.md`) MUST mark the `--remote <url> --token <tok>` form
     **"legacy"** explicitly. The doc-test covers ONLY the canonical `--bundle` chain.
   - N5's "usable-as-documented" claim (Goal 4) is BOUND to the `--bundle` chain only — it MUST NOT
     be read as covering both modes. No `--remote` round-trip coverage is owed.

> Reconciled and kept: **Bundle mode takes NO `--slug`** — `init.js:353` retires it on the bundle
> path (former OQ-A). Docs and the doc-test use `init --bundle <blob>` with no `--slug`.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `docker-http-posture-smoke.sh` (Gates 5–7 extension: emit bundle in-container, host `init --bundle` consume, hook-fire observe round-trip) | pseudocode/docker-http-posture-smoke.md | test-plan/docker-http-posture-smoke.md |
| Hermeticity sandbox + negative control (process-boundary HOME/`--project-dir` isolation; poison+break control) | pseudocode/hermeticity-sandbox.md | test-plan/hermeticity-negative-control.md |
| `release.yml` smoke jobs (pinned `setup-node@v4` provisioning step on both smoke jobs) | pseudocode/release-yml-setup-node.md | test-plan/release-yml-setup-node.md |
| `docs/client-setup.md` rewrite (current bundle/observe model; both modes; `--remote` marked legacy) | pseudocode/docs-client-setup.md | test-plan/docs-client-setup.md |
| `README.md` bundle-example fix (converge all `--remote unimatrix-bundle:`/`--remote <bundle>` forms on `init --bundle <blob>`) | pseudocode/readme-bundle-example.md | test-plan/readme-bundle-example.md |
| `.claude/agents/uni/uni-docs.md` remit widen (README-only → all of `docs/`, blast-radius-scoped) | pseudocode/uni-docs-remit.md | test-plan/uni-docs-remit.md |

> **Stage 3a COMPLETE** (2026-06-20) — all pseudocode + test-plan files produced at the paths above; OVERVIEW files present in both dirs. Paths confirmed against reality.
>
> **Stage 3b routing constraints (MANDATORY — leader-set):**
> - **Same-file coupling:** `docker-http-posture-smoke.md` + `hermeticity-sandbox.md` both edit `product/test/infra-001/scripts/docker-http-posture-smoke.sh` → route BOTH to ONE agent. The sandbox lifecycle is the environment Gates 6–7 run inside; concurrent edits would conflict and break gate ordering.
> - **Stub-drivability seam (load-bearing):** the Gate 5–7 logic MUST be factored so the three new external commands (`client-bundle`, host `node init --bundle`, hook-fire) are env-injectable (reuse the nan-019 `run_smoke_gate`/`SMOKE_CMD` indirection). Without this seam the R-07 negative control and the truth table are not stub-drivable pre-merge. Owned by the same single agent.
> - **Same-PR pairing:** `release.yml` setup-node provisioning + the script's node-absent `fail()` enforcement land together.
> - **Shared doc contract:** `docs-client-setup` + `readme-bundle-example` share one contract — identical `init --bundle <blob>` (no `--slug`) and identical "legacy" marking for `--remote`. README has **four** bundle-via-`--remote` occurrences (lines 123, 130, 585, 587) to converge; line 113 is the legacy `--remote <url> --token` form (mark legacy, do NOT converge).
> - **Independent:** `uni-docs-remit` is fully standalone.
> - The stub-driven gate-logic test (extends nan-019 `release-gate-logic-test.sh`) is a distinct task but must read the FINAL script shape.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Script topology | EXTEND `docker-http-posture-smoke.sh` in place (append-only after Gate 4); no sibling script unless boot config genuinely diverges (D-2 caveat NOT met — same container/volume/slug/port/token/cert reused) | D-2, ARCH | architecture/ADR-001-extend-in-place-exit-code-contract.md |
| Exit-code contract | New bundle-attach failures fold into existing `fail()` (exit 1) with distinct, attributable messages; `run_smoke_gate` and the 0/1/3/4 numerics UNCHANGED; no new codes 5/6/7 | SR-04, SR-08 | architecture/ADR-001-extend-in-place-exit-code-contract.md |
| Bundle source | In-test emission via shipped `unimatrix --project-dir /data client-bundle <slug>` (Rust, in-container); no pre-staged fixture | D-1 | architecture/ADR-002-in-test-bundle-emission-host-container-split.md |
| Runtime topology | Host/container split: emit (Rust) in throwaway container off the shipped distroless image (no JS in image); consume (`init --bundle`, JS) on the CI host = operator surrogate | A3 corrected, SR-03 | architecture/ADR-002-in-test-bundle-emission-host-container-split.md |
| Node provisioning | Pinned `actions/setup-node@v4` (`node-version: '24'`) on BOTH smoke jobs in `release.yml`; node-absent hard-fail becomes a safety net, not the acquisition path | NFR-10, R-04 | architecture/ADR-002-in-test-bundle-emission-host-container-split.md (AMENDED) |
| Executable-claim boundary | 3-part operational test + worked example; tested set is exactly the one canonical attach chain (AC-03), not a gate per command | SR-06 | architecture/ADR-003-executable-claim-boundary.md |
| `--slug` on bundle path | RETIRED — `init --bundle <blob>` takes no `--slug` (`init.js:353`); `--slug` remains valid only for server-side `project register`/`client-bundle` (former OQ-A) | OQ-A, FR-5 | architecture/ADR-002-in-test-bundle-emission-host-container-split.md |
| uni-docs remit | Widen README-only → all of `docs/`, blast-radius-scoped; authorship-text only; NO drift-checker/gate/Phase-4 trigger (Feature 2); narrowly relax "no source reading" to the touched CLI surface | C-5, SR-05, SR-07 | architecture/ADR-004-uni-docs-remit-widen.md |
| Hermeticity | PROOF OBLIGATION: process-boundary HOME + throwaway `--project-dir`, clean-on-entry, proven by a REQUIRED pre-merge negative control (poison stale cred + broken attach → Gate 7 still fails) | AC-09, FR-22, NFR-9, R-07 | architecture/ADR-005-hermeticity-as-proof-obligation.md |
| Legacy `--remote` | Documented + MARKED legacy; NOT doc-tested — consciously accepted gap; N5 "usable-as-documented" bound to `--bundle` only | C-7, AG-1, R-16 | (scope decision; no app-code ADR) |
| N5 framing | EXTEND N5 "deployable-as-released" → "usable-as-documented" (bundle chain only); doc-test named as docs-layer guard; no new NFR; N5 status unchanged | C-6, AC-08 | (scope decision; no app-code ADR) |
| `verified on vX` stamp | Single non-machine-checked footer line per doc-file; pure authoring convention | D-3, C-3 | architecture/ADR-003-executable-claim-boundary.md |

## Files to Create/Modify

| File | Change |
|------|--------|
| `product/test/infra-001/scripts/docker-http-posture-smoke.sh` | EXTEND in place — append Gates 5–7 (in-container bundle emit, host hermetic `init --bundle` consume, hook-fire observe round-trip into per-slug store) after Gate 4; no reordering of Gates 1–4 or the terminal marker |
| `product/test/infra-001/scripts/release-gate-lib.sh` | UNCHANGED — diff-asserted byte-identical (ADR-001); new failures fold into existing exit 1 |
| `release.yml` (smoke-amd64 + smoke-arm64 jobs) | ADD pinned `actions/setup-node@v4` (`node-version: '24'`) after `checkout`, before `run_smoke_gate`, on BOTH smoke jobs |
| `docs/client-setup.md` | REWRITE to current bundle/observe model; remove all `501`/`W2-7`/curl-observe hook scripts; document both modes (`--remote` MARKED legacy; canonical `--bundle`, no `--slug`); add `verified on vX` footer |
| `README.md` | FIX every `init --remote unimatrix-bundle:<blob>` / `init --remote <bundle>` bundle-fed-to-`--remote` form (lines ~123, ~587/130, and any others) to canonical `init --bundle <blob>`; mark `--remote` legacy. NOT in scope: `README:62` ONNX (owned by #767) |
| `.claude/agents/uni/uni-docs.md` | WIDEN remit README-only → all of `docs/`, blast-radius-scoped, full-tree-audit non-goal stated; narrow source-read relaxation; retain prompt-injection defense + "document only what is shipped" (the one `.claude/` edit, C-5) |
| Pre-merge gate-logic tests (stub-driven) | NEW — truth-table per ADR-001 failure mode, exit-code survival, hermeticity negative control; shares bytes with the YAML wrapper (#5189) |

## Data Structures / Contracts

- **Bundle blob:** opaque single line beginning `unimatrix-bundle:` (`v:2`), on `client-bundle`
  stdout only; stderr carries the token-redacted URL/fingerprint echo (MUST NOT be captured/logged).
- **Exit-code truth table (UNCHANGED numerics):** `0` ran+all gates passed (terminal marker
  printed) · `1` ran+a gate failed (`fail()`, incl. ALL new Gates 5–7 failures) · `3` Docker absent
  (preflight → `run_smoke_gate` HARD-fail) · `4` `IMAGE=` tag unpullable/unfound (HARD-fail).
- **Anchored terminal run-marker:** `[783-smoke] ALL GATES PASSED` — single, last line; new gates
  print between Gate 4 and the marker; `run_smoke_gate` asserts `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`.
- **Isolated credstore path (per-run):** `$SANDBOX/home/.unimatrix/<projectHash>/remote.json`
  (HOME-keyed, cannot pre-exist this run; vnc-039 #5125).
- **Per-slug store:** `/data/.unimatrix/<slug>/unimatrix.db` (+ `-wal`/`-shm`); growth measured by
  busybox `du -s` delta (reuse nan-019 `store_size`, WAL-robust).

## Function / Invocation Signatures (consumed as-is — no behavior change)

- Bundle emit: `unimatrix --project-dir /data client-bundle <slug>` → stdout `unimatrix-bundle:<blob>` (sync, pre-tokio; `main.rs:293,437`; ENTRYPOINT in image).
- Bundle consume (hermetic): `HOME="$SANDBOX/home" node packages/unimatrix/bin/unimatrix.js init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj"` (NO `--slug`; `init.js:353,362–518`).
- init validation: pinned `Ping` over fingerprint-pinned HTTPS to `https://localhost:PORT` (throws → exit 1).
- Observe round-trip: hook client → `POST https://localhost:PORT/v1/<slug>/observe` → `204`; per-slug store grows by the NEW write.
- Gate wrapper: `run_smoke_gate IMAGE docker-http-posture-smoke.sh` (UNCHANGED).
- Node provisioning step (NEW): `actions/setup-node@v4` with `node-version: '24'`.

### New-failure-mode → outcome (all `fail()` exit 1; distinct attributable message)

| Failure mode | Message (distinct, names the step) |
|--------------|-----------------------------------|
| `client-bundle` rc≠0 (renamed/absent) | `client-bundle emit failed (rc=N) — subcommand renamed/absent in shipped image?` |
| empty / non-`unimatrix-bundle:` blob | `client-bundle produced no/invalid bundle blob` |
| `node` absent on host | `node not available — the documented init --bundle path cannot be exercised` (hard-fail, NOT exit 3; safety net behind setup-node) |
| `init --bundle` rc≠0 | `init --bundle failed (rc=N) — bundle attach broken` |
| observe non-204 | `documented bundle attach observe returned HTTP C (expected 204)` (distinguishes doc-drift from route change) |
| per-slug store did not grow | `bundle-path observe did not land in per-slug store` |

## Constraints

- **C-1 (load-bearing):** Fold into nan-019 release smoke; EXTEND in place (D-2); no new bespoke CI job; sibling only if boot config genuinely diverges.
- **C-2 (load-bearing):** Docker-absent AND every new skip path MUST hard-fail; never silent-green.
- **C-3:** No generate-from-contract; manual rewrite for prose; doc-test for executable claims; the `verified on vX` stamp is a non-machine-checked footer; minimal mechanism, no gold-plating.
- **C-4:** uni-docs authorship blast-radius-scoped, not full-`docs/` audit per cycle.
- **C-5:** Exactly one `.claude/` edit (uni-docs remit widen); all other `.claude/` currency is Feature 2.
- **C-6:** Extend N5; do not mint a new NFR.
- **C-7 (accepted-gap boundary):** Doc-test covers ONLY the canonical `--bundle` chain; legacy `--remote` documented + MARKED legacy, deliberately NOT doc-tested.
- **Architecture invariants:** append-only after Gate 4; `run_smoke_gate` byte-unchanged; single terminal marker; reuse the single boot; hermeticity at the process boundary only (no in-process HOME mutation).

## Dependencies

- **nan-019** — release smoke infrastructure (`docker-http-posture-smoke.sh`, `release-gate-lib.sh::run_smoke_gate`, 0/1/3/4 truth table, anchored run-marker). The doc-test extends this in place.
- **vnc-038** — per-slug route `POST /v1/{slug}/observe`; the dumb-client attach exercised.
- **vnc-034** — `--bundle` attach mode; `unimatrix client-bundle <slug>` bundle emit.
- **vnc-022** — original `/observe` ship (proof-case context).
- **vnc-039** — HOME-keyed out-of-tree credstore (`~/.unimatrix/<hash>/remote.json`; #5125) — the residue surface hermeticity isolates.
- **vnc-041** — AC-06 negative-control shape (#5246) reused for the hermeticity proof; AC-02 in-process-HOME-mutation hazard (Rust-2024) that forces process-boundary isolation.
- **#793** — "pin your infra" discipline (busybox) — the model for the pinned `setup-node` step.
- **#767** — owns `README:62` ONNX claim (cross-reference only; out of scope).
- **GitHub Actions:** `actions/setup-node@v4` (`node-version: '24'`).

## NOT in Scope

- **Feature 2 — the `.claude/` automation-currency pattern.** Not designed/implemented; only the single uni-docs remit-text edit rides here (C-5).
- **Generate-from-contract** (auto-generating docs). Explicitly killed (C-3).
- **Machine-checking the `verified on vX` stamp** (D-3, C-3).
- **Auditing all of `docs/` every cycle** (C-4).
- **A new bespoke CI job / standalone doc-test script** unless the FR-10 divergence caveat triggers and is documented.
- **Minting a new NFR** (C-6).
- **Fixing `README:62`'s ONNX claim** (owned by #767).
- **Doc-testing the legacy `--remote` mode** — consciously accepted gap (C-7, AG-1, R-16).
- **Any CLI/route behavior change** — nan-020 documents/tests shipped surfaces; does not modify `init`, `client-bundle`, or the observe route.
- **In-process HOME mutation** for isolation — forbidden by Rust-2024; process-boundary only.

## Alignment Status

ALIGNMENT-REPORT.md (2026-06-20): **PASS 5, WARN 1, VARIANCE 0, FAIL 0.**

- **Vision Alignment — PASS.** Directly serves goal:personal-cloud (#4946) on its most literal axis: `docs/client-setup.md` is the operator's onboarding path; #768 is a hard-stop on it. N5 (#5163) extended, not duplicated; honors the static-layer "changes infrequently" principle (minimal mechanism, no generator, no per-command gate).
- **Milestone Fit — PASS.** Feature 2 (`.claude/` currency) cleanly fenced out (zero test scenarios); only the uni-docs remit-text edit rides along.
- **Scope Gaps — PASS.** All ACs and constraints traced into SPEC FRs and RISK scenarios.
- **Architecture Consistency / Risk Completeness — PASS.** ADR-001..005 consistent with locked D-1/D-2/D-3; A3 correction handled coherently; all SRs traced; over-build risks bounded.
- **WARN-1 (presented for awareness; recommendation ACCEPT) — As-shipped refinements widen the surface beyond SCOPE's literal text:** (a) `--slug` dropped on the bundle path; (b) node-absence + new skip paths added as hard-fails; (c) the host/container runtime split (image ships no JS). Each is a faithful reading of intent (code-truth over SCOPE prose; never silent-green; reproduce the real operator topology), verified against shipped code — not gold-plating. **Action taken / required:** reconcile SCOPE AC-02/AC-03's "(+ `--slug`)" parenthetical to the as-shipped `--bundle <blob>` (no `--slug`) so the locked SCOPE and the delivered surface do not themselves drift — the very failure class this feature exists to kill.

> Revision-pass note: hermeticity (AC-09/FR-22/NFR-9/ADR-005) and node-pinning (NFR-10/amended
> ADR-002) were elevated AFTER the alignment review. Both are faithful extensions of the alignment
> report's existing posture (never silent-green; pin the infra you depend on) and introduce no new
> vision variance — they harden the doc-test against the same false-green class the feature targets.

## Open Questions

None blocking. The former OQ-A (`--slug` on bundle path) is resolved — docs/doc-test use `init --bundle <blob>` with no `--slug`; SCOPE's "(+ `--slug`)" parenthetical is superseded by the as-shipped surface (WARN-1 action). OQ-B (enumerate every README `--remote`-bundle phrasing) is a delivery enumeration task, not a design question. OQ-C is RESOLVED by ADR-005 (hermeticity decided in architecture, not deferred to the test plan).
