# SPECIFICATION — infra-004: Enforce Cross-Tenant Isolation as a Blocking Release Gate

> Derived from `product/test/infra-004/SCOPE.md` (authoritative) and
> `product/test/infra-004/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09). Test/CI-only
> feature. AC-IDs (AC-01..AC-15) and Decisions (D-1/D-2/D-3) trace from SCOPE
> verbatim; this document adds testable phrasing and explicit verification methods.

## Objective

Convert the existing point-in-time cross-tenant isolation proof
(`multi-tenant-isolation-smoke.sh`, delivered by infra-003 / #855) from
**detection** into **enforcement**, so the outcome-level Definition of Done holds:
**a cross-tenant leak cannot ship a release.** This is realized as four in-scope
deliverables landing in one feature — a bounded warmup barrier (#857), a standing
release-gate lane (#856), an in-feature cold-model fresh-build GREEN proof (D-2),
and the blocking flip via `create-container-manifest.needs:` — with no `crates/`
change. On merge, capability **N3 (#5161)** moves `partial → proven` and **N4** is
advanced.

## Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Cross-tenant isolation** | The integrity invariant "a write addressed to slug A can only ever land in A's store." Basis of `goal:personal-cloud`; a mis-route silently corrupts the wrong project's hash chain. |
| **The gate / the smoke** | `multi-tenant-isolation-smoke.sh` (lives under `product/test/infra-001/scripts/`) — the bidirectional 2×2 over-the-wire isolation probe across the observe + MCP-write served surfaces. |
| **Exit-code tri-state** | The gate's verdict contract: `GREEN=0`, `RED=1` (genuine leak / `fail`), `INFRA=2` (`infra_fail` — durability/readiness/dep not established), `SKIP=3` (Docker absent). Dominance: **RED dominates INFRA dominates GREEN**; no non-GREEN ever rounds to 0. |
| **RED** | A genuine cross-tenant leak observed (a foreign marker present in another tenant's store). A definitive negative verdict. Must **block** the release manifest. |
| **INFRA** | A non-verdict: readiness/durability/dependency could not be established (e.g. warmup deadline missed, missing `sqlite3`). The isolation property was **not checked** this run. Must **not block**, but must be **visible**. |
| **GREEN** | Isolation verified healthy this run — credited only when the verify-by-name marker `[infra003-smoke] ALL GATES PASSED` is present. |
| **Cold-model run** | A gate run that exercises the **real first-boot HuggingFace embedding-model download path** (C1 / N5) on a fresh container — not a warm cache, not the stale `:783-smoke` artifact. The path a fresh release container actually takes. |
| **Warmup barrier** | A bounded readiness barrier inserted after `assert_routes_live` and before `run_isolation_matrix` (before the C3/C4 load-bearing writes), confirming the embedding model is loaded and both per-slug stores are live before the isolation probes run. Bound derived from the #767 cold-first-boot window. |
| **Vacuous / silently-vacuous enforcement** | The central hazard: a chronically-INFRA blocking lane that is "blocking" yet never RED and never GREEN — isolation never actually verified, release ships anyway. "The gate is blocking" ≠ "the gate verified isolation this run." |
| **Verify-by-name** | Crediting GREEN only on the presence of the named terminal marker (regex `\[[a-z0-9-]+-smoke\] ALL GATES PASSED`), guarding against an early `exit 0`. |
| **Blocking vs non-blocking lane** | A **blocking** lane is listed in `create-container-manifest.needs:` — its `needs:` edge gates the manifest. A **non-blocking** lane (e.g. `nan-021-https-uds-parity`) runs and is visible on every tag+dispatch but is NOT in `needs:`. infra-004 starts non-blocking, then moves into `needs:` after the AC-11 cold-model GREEN. |
| **Stub seam** | The off-Docker `SMOKE_*_CMD` injection points that let the pre-merge gate-logic test drive the verdict truth-table without Docker. |
| **Single source of truth** | `release-gate-lib.sh` — sourced by both the CI workflow and the pre-merge stub test; the exit-2/INFRA discrimination (D-1) lands here, never inline in YAML. |

## Functional Requirements

Each FR is testable; verification methods are detailed under Acceptance Criteria.

### Warmup barrier (Step 1 / #857)

- **FR-1** The gate SHALL run a bounded warmup/readiness barrier in
  `multi-tenant-isolation-smoke.sh` positioned **after `assert_routes_live` and
  before `run_isolation_matrix`** (before the C3/C4 load-bearing marked writes).
- **FR-2** The barrier SHALL confirm two readiness conditions before proceeding:
  (a) the embedding model is loaded, and (b) **both** per-slug stores are live.
- **FR-3** The barrier's deadline SHALL be **derived from the #767 embed-readiness
  cold-first-boot window** (a documented derivation with headroom over the bare
  #767 number, per SR-01), not a guessed constant.
- **FR-4** The barrier SHALL reuse existing infra-001 readiness idioms
  (`wait_for_http_active`, `store_size`, deadline-poll) — **no new readiness
  mechanism** is introduced.
- **FR-5** When the server is genuinely not-ready past the warmup deadline, the
  gate SHALL classify the outcome **INFRA (exit 2)** — never RED, never GREEN. The
  barrier SHALL NEVER convert a real not-ready condition into a false pass.
- **FR-6** The barrier addition SHALL preserve the gate's existing degradation
  contract: C5 `write_then_barrier` own-store-miss → INFRA; `verdict()` dominance
  (RED > INFRA > GREEN); four-marker / non-substring scheme; read-as-barrier model;
  terminal run-marker — all untouched. The barrier is the ONLY permitted
  gate-script change.
- **FR-7** The barrier SHALL remain compatible with the off-Docker `SMOKE_*_CMD`
  stub seam (the pre-merge gate-logic test still drives the full verdict truth-table
  without Docker).

### Standing-lane wiring (Step 2 / #856)

- **FR-8** A new job in `.github/workflows/release.yml` SHALL run
  `multi-tenant-isolation-smoke.sh` on every release `push: tags: ['v*']` and on
  `workflow_dispatch`, producing an independent job status.
- **FR-9** The job SHALL run against the **pushed per-arch GHCR bytes** via the
  shared `resolve_image` (push → `:v<version>-<arch>` UN-stripped; dispatch →
  `:latest-<arch>`), with `IMAGE` exported — never a local rebuild of the production
  image and never `${GITHUB_REF_NAME#v}`.
- **FR-10** The job SHALL provision every read dependency the gate requires:
  `node` AND **`sqlite3`** (coordinate with #849). Absence of either SHALL be
  classified INFRA, never an empty/silent pass.
- **FR-11** The exit-code tri-state mapping SHALL be implemented in the shared
  `release-gate-lib.sh` (D-1), adding the currently-missing **exit-2 branch**:
  - GREEN (0) → pass, but only with the verify-by-name marker present (FR-12);
  - RED (1) → fail;
  - **INFRA (2) → non-failing return, but VISIBLE** — emit a `::warning::`
    annotation AND a distinct, greppable marker (never a silent return);
  - SKIP (3) → on a Docker-present lane, **hard failure** (mis-provisioned lane),
    consistent with `run_smoke_gate`'s exit-3 policy.
  No non-GREEN verdict SHALL be silently rounded to a pass.
- **FR-12** GREEN SHALL be credited only when the smoke prints
  `[infra003-smoke] ALL GATES PASSED` (matching `\[[a-z0-9-]+-smoke\] ALL GATES
  PASSED`), guarding against an early `exit 0`.
- **FR-13** The exit-2 branch SHALL be **purely additive** to
  `release-gate-lib.sh` — it SHALL NOT alter the behavior of the four existing
  blocking smoke lanes that source the lib (the `*) → return 1` catch-all change
  must not shift sibling-lane behavior; no existing lane emits exit 2 today —
  SR-08).

### Cold-model fresh-build GREEN proof (Step 3 / D-2)

- **FR-14** Before the blocking flip lands, the gate (with the warmup barrier)
  SHALL be demonstrated **GREEN against a fresh, cold-model build of current
  `main`** via a **`workflow_dispatch` run against the feature branch** (D-2
  Option a), exercising the real `resolve_image` dispatch path (`:latest-<arch>`)
  and the cold first-boot HuggingFace download path.
- **FR-15** The dispatch run's GREEN evidence (workflow run URL / log reference)
  SHALL be recorded with the feature.
- **FR-16** The feature branch SHALL be freshly rebased on `main` (or its
  branch-point asserted equal to `main` HEAD) at the time of the AC-11 run, so the
  "byte-identical to `main` production image" claim holds (SR-06).

### Blocking flip (Step 4 — the DoD)

- **FR-17** The isolation lane SHALL be added to
  `create-container-manifest.needs:` so a **RED** verdict fails the job and the
  release manifest does not assemble.
- **FR-18** The blocking wiring SHALL implement the precise mapping: RED blocks;
  INFRA does not block but is visible (FR-11); GREEN passes with marker;
  SKIP-on-Docker-present is a hard failure.
- **FR-19** The wiring SHALL contain the blocking blast radius (SR-04): only the
  **script's exit-2** maps to non-blocking. Non-script harness/setup-step failures
  (runner outage, GHCR login expiry, image-pull 404, checkout fail, the `sqlite3`
  setup step) are NOT covered by the INFRA non-block path and the architecture MUST
  classify/contain them explicitly so the lane does not become a release-wide
  outage vector. (Spec flags this as a constraint the architecture must resolve;
  see Open Questions OQ-1.)
- **FR-20** On merge, delivery SHALL set capability **N3 (#5161) `status: proven`**
  with `proven_by =` the blocking gate + the AC-11 cold-model fresh-build GREEN run.
  The status field value SHALL be the enum literal `proven` (enum: `missing |
  partial | proven | claimed`); "maintained / enforced" is descriptive prose only.
  The note SHALL record that this feature closed the only remaining N3 blocker (the
  C5/#5190 caveat was already resolved by crt-056 / #789). N3 is proven **as-of the
  observe + MCP-write surfaces**; a future NEW served write route reopens the nfr
  proof per the standard lifecycle.

## Non-Functional Requirements

- **NFR-1 (Determinism — no INFRA flap on warmup).** A healthy run that takes the
  cold first-boot embedding-model download path SHALL be deterministically GREEN —
  zero INFRA flap attributable to model warmup before the load-bearing writes.
  Measurable target: the AC-11 cold-model dispatch run completes GREEN; the chosen
  warmup bound carries documented headroom over the #767 cold window (SR-01/SR-02).
- **NFR-2 (INFRA visibility).** Every INFRA (exit 2) outcome SHALL emit a
  GitHub `::warning::` annotation AND a distinct, greppable marker string. Target:
  the marker is verify-by-name greppable in job logs; a silent return is a defect.
- **NFR-3 (Single source of truth).** The exit-2 discrimination SHALL live solely
  in `release-gate-lib.sh` (sourced by CI and the stub test) — never duplicated in
  inline YAML.
- **NFR-4 (Stub-seam compatibility / pre-merge provability).** The warmup-barrier
  and exit-2 additions SHALL be provable pre-merge via the off-Docker `SMOKE_*_CMD`
  stub seam plus a `needs:`-graph assertion — RED-blocks and INFRA-passes-visibly
  wiring logic demonstrable **without a tag push**.
- **NFR-5 (No `crates/` change / change confinement).** Production code SHALL NOT
  change. The full change set SHALL be confined to exactly three files:
  `multi-tenant-isolation-smoke.sh` (warmup barrier only),
  `.github/workflows/release.yml` (lane + `needs:` flip), and `release-gate-lib.sh`
  (exit-2 handling). The production routing seam is exercised as shipped.
- **NFR-6 (Tri-state invariance).** RED > INFRA > GREEN dominance and the four
  distinct, non-collapsible exit codes SHALL be preserved unchanged.
- **NFR-7 (Additive shared-lib safety).** The shared-lib change SHALL be provably
  side-effect-free on the four existing blocking lanes, validated by a full
  tri/quad-state truth-table run through the stub seam (SR-08).

## Acceptance Criteria

Each AC restates the SCOPE AC and pins a concrete verification method.

| AC | Requirement | Verification method |
|----|-------------|---------------------|
| **AC-01** | Bounded warmup barrier runs before the C3/C4 load-bearing writes, confirming model loaded + both per-slug stores live; bound derived from #767, not guessed. | Code review + `git diff` of `multi-tenant-isolation-smoke.sh` showing the barrier between `assert_routes_live` and `run_isolation_matrix`; the bound's source documented as a #767-derivation (cite the #767 window value + headroom). |
| **AC-02** | Barrier reuses existing infra-001 idioms (`store_size` waits, deadline-poll) — no new mechanism. | `git diff` shows only calls to existing `wait_for_http_active` / `store_size` / deadline-poll helpers; no new readiness primitive defined. |
| **AC-03** | Genuine not-ready past deadline → INFRA (exit 2), never RED/GREEN; barrier never converts not-ready to false pass. | Stub-seam truth-table run: force barrier not-ready → assert exit 2 (INFRA), assert not 0 and not 1. |
| **AC-04** | Deterministic GREEN on a COLD-model container; no INFRA flap from model warmup before load-bearing writes. | Proven by AC-11 (the cold-model dispatch run completes GREEN, no warmup INFRA). |
| **AC-05** | Barrier remains compatible with the off-Docker `SMOKE_*_CMD` stub seam. | Run the pre-merge gate-logic test (stub seam) post-barrier; the full verdict truth-table still drives without Docker (exit 0/1/2/3 all reachable). |
| **AC-06** | New `release.yml` job runs the smoke on every `push: tags:['v*']` and `workflow_dispatch`; independent job status. | Inspect `release.yml` job `on:`/trigger inheritance and job presence; confirm independent job id with its own status. |
| **AC-07** | Job runs against pushed per-arch GHCR bytes via `resolve_image` (push→`:v<ver>-<arch>` UN-stripped; dispatch→`:latest-<arch>`), `IMAGE` exported; no local rebuild. | YAML review: job calls `resolve_image`, exports `IMAGE`, has no docker build step; assert no `${GITHUB_REF_NAME#v}` usage. |
| **AC-08** | Wiring discriminates the tri-state in `release-gate-lib.sh` (D-1): GREEN passes, RED fails, INFRA non-failing-but-VISIBLE (`::warning::` + greppable marker), SKIP-on-Docker hard-fails; no non-GREEN rounded to pass. | Stub-seam truth-table run through `release-gate-lib.sh`: assert exit-code→outcome mapping for 0/1/2/3; grep job output for the INFRA marker + `::warning::` on the exit-2 cell. |
| **AC-09** | GREEN credited only on `[infra003-smoke] ALL GATES PASSED`. | Stub test: emit exit 0 WITHOUT the marker → assert NOT credited GREEN; emit with marker → credited. |
| **AC-10** | Lane provisions `node` AND `sqlite3` (#849); absence of either → INFRA, not empty-pass. | YAML review of the setup step (node + sqlite3); stub/preflight test: missing `sqlite3` → exit 2 (INFRA), not 0. |
| **AC-11** | Cold-model fresh-build GREEN demonstrated in-feature via `workflow_dispatch` on the feature branch (D-2) before the flip; real first-boot HF download path; evidence recorded. | The dispatch workflow run URL + log: confirm GREEN verdict, confirm cold-download path taken (HF download log lines, not warm cache / not `:783-smoke`), confirm branch rebased on `main` HEAD at run time (SR-06). Recorded in the feature folder. |
| **AC-12** | Isolation lane IS in `create-container-manifest.needs:`; RED fails the manifest (does not assemble). | `needs:`-graph assertion on `release.yml` (lane id ∈ `create-container-manifest.needs`); stub-seam run forcing a RED cell → assert the lane job exits 1 (would gate the `needs:` edge). |
| **AC-13** | INFRA (exit 2) does NOT fail the manifest, and emits the visible `::warning::` + greppable marker (N4 preserved). | Stub-seam run forcing INFRA → assert lane job returns success (non-blocking) AND log contains `::warning::` + the distinct marker. |
| **AC-14** | Delivery sets N3 (#5161) `status: proven`, `proven_by =` blocking gate + AC-11 run; note records last-blocker-closed + C5/#5190 caveat resolved; proven as-of observe + MCP-write surfaces. | Inspect the N3 capability entry post-merge: `status` field literal `proven`; `proven_by` references the gate + AC-11 run; note + surface-boundary text present. |
| **AC-15** | No `crates/` change; only gate-script edit is the warmup barrier; changes confined to the three named files. | `git diff --stat` of the feature branch: exactly `multi-tenant-isolation-smoke.sh`, `.github/workflows/release.yml`, `release-gate-lib.sh` (plus feature docs); zero `crates/` paths; the smoke diff contains only the barrier. |

## User / Agent Workflows

1. **Release tag push (steady state).** A maintainer pushes `v*`. The release
   workflow builds per-arch bytes, the isolation lane pulls them via `resolve_image`
   (`:v<ver>-<arch>`), provisions node+sqlite3, runs the warmup barrier then the
   2×2 isolation matrix. GREEN → manifest proceeds; RED → manifest blocks; INFRA →
   manifest proceeds with a loud `::warning::` + marker; SKIP-on-Docker → hard fail.
2. **In-feature cold-model proof (D-2).** The delivery agent triggers
   `workflow_dispatch` on the rebased feature branch, observes the gate GREEN over
   the cold-download path (`:latest-<arch>`), records the run URL — gating the flip.
3. **Pre-merge wiring proof (off-Docker).** The stub seam drives the full
   exit-code truth-table through `release-gate-lib.sh` and a `needs:`-graph assertion
   proves RED-blocks / INFRA-passes-visibly without any tag push.
4. **Regression event (the DoD in action).** A future routing change reintroduces a
   cross-tenant mis-route; the lane goes RED on the release tag; the manifest does
   not assemble; the leak cannot ship.

## Constraints

- **C-1 (Test/CI-only).** No `crates/` change; no gate-script logic change beyond
  the warmup barrier. Verified by `git diff`.
- **C-2 (Bounded, no-new-mechanism barrier, #767-derived bound).** Reuse
  `store_size` / `wait_for_http_active` / deadline-poll; bound derived (with margin)
  from the #767 cold-first-boot window; past-deadline outcome is INFRA (SR-01/02).
- **C-3 (Tri-state invariant + now enforced).** RED blocks; INFRA non-blocking but
  visible; GREEN passes with marker; SKIP-on-Docker hard-fails. RED > INFRA > GREEN.
- **C-4 (Pushed-bytes contract, #5180 / nan-019).** `resolve_image`; UN-stripped
  push tag; `:latest-<arch>` on dispatch; never `${GITHUB_REF_NAME#v}`.
- **C-5 (Single source of truth).** Exit-2 discrimination in `release-gate-lib.sh`,
  sourced by CI and the pre-merge test; no inline YAML logic.
- **C-6 (sqlite3 provisioning).** Coordinate with #849; avoid an ordering trap that
  strands the feature (SR-03) — architecture decides self-contained provisioning vs
  hard dependency on #849 landing first.
- **C-7 (Stub-seam compatibility).** Warmup barrier must not break the off-Docker
  `SMOKE_*_CMD` gate-logic test; RED-blocks / INFRA-passes-visibly provable
  pre-merge via that seam + a `needs:`-graph assertion (no tag push required).
- **C-8 (Additive shared-lib change, SR-08).** The `release-gate-lib.sh` change is
  purely additive; no existing blocking lane emits exit 2; truth-table run must
  cover the full tri/quad-state to catch sibling regressions.
- **C-9 (Blocking blast radius, SR-04).** Only the script's exit-2 maps to
  non-blocking; non-script harness/setup-step failures must be explicitly
  classified/contained so the lane is not a release-wide outage vector.
- **C-10 (Never-green-on-a-tag tax, SR-05 / #5267 / ADR-004 #5184).** AC-11's
  dispatch run proves warmup + verdict over `:latest-<arch>`, NOT tag-push
  resolution (`:v<ver>-<arch>`). A post-merge tag round must be budgeted; the lane
  should be diagnostic-capture-first so round 1 yields a diagnosis, not a guess.
- **C-11 (Byte-identical provenance, SR-06).** AC-11 run requires the branch
  rebased on `main` (or branch-point == `main` HEAD) so the build is current
  production bytes, not a stale image.
- **C-12 (D-3 arch coverage).** amd64-only blocking; no arm64 isolation lane this
  round (routing is architecture-independent Rust; human-signed trade-off).

## Dependencies

| Dependency | Relationship |
|-----------|--------------|
| **#788** (closed/merged) | The standing release gate this wires into and flips blocking (`release.yml`). |
| **#855 / #853 / infra-003** | Delivers the gate hardened (warmup), wired, and enforced; established the reusable isolation-gate kernel (Unimatrix #5347) and the human-correction lesson (#5348). |
| **#5180** | Verify-by-name / tri-state exit-code contract; the exit-2 discrimination is load-bearing for blocking. |
| **#767** | Embed-readiness gate (`docker-embed-readiness-smoke.sh`) — source of the empirically-validated cold-first-boot warmup window (AC-01) and the cold-model proof reference. |
| **#789 / crt-056 / C5 (#5190)** | C5 per-slug surface proven (merged 2026-06-19) — removes N3's second `partial` caveat. |
| **#849** | sqlite3 provisioning — coordinate so the new lane's `sqlite3` need aligns (SR-03). |
| **#5161 (N3)** | Capability `partial → proven` on this merge. |
| **#5267 / #5184 (ADR-004)** | Historical never-green-on-a-tag + dispatch-vs-tag resolution evidence (SR-05). |
| **N4** | Capability advanced (no false-alarm signals) via the warmup barrier + visible INFRA. |
| Existing scripts | `product/test/infra-001/scripts/{multi-tenant-isolation-smoke.sh, release-gate-lib.sh, isolation-probe-lib.sh}`. |

## NOT in Scope

- **No `crates/` change.** Production routing seam exercised as shipped, never
  modified.
- **No gate-script change other than the warmup barrier.** Assertions, four-marker /
  non-substring scheme, read-as-barrier model, terminal run-marker, tri-state exit
  contract — untouched. No gate re-architecture.
- **No new readiness mechanism** for the barrier — reuse infra-001 idioms only.
- **No arm64 isolation lane this round** (D-3) — not needed unless a reason emerges.
- **No new smoke script** (script exists, PR #855); no #815 new-smoke-script
  invariant update.
- **No new local validation harness** — the off-Docker `SMOKE_*_CMD` stub seam
  already exists.
- **No UDS behavioral probe / no parity-matrix shape** — the ADR-006 compile-time
  guard (`FORBIDDEN_IN_LOCAL`) is referenced as proof of a single local route, not
  re-run.
- **No automated chronic-INFRA escalation** in this feature (SR-07) — visibility is
  `::warning::` + greppable marker; stronger escalation (tracked count / threshold)
  is out of scope unless the architecture elects to add it. Flagged as accepted
  human-vigilance risk (see OQ-3).

## Open Questions (for architect / human)

- **OQ-1 (SR-04, blast radius — for architect).** How are non-script harness/setup
  failures (GHCR login expiry, image-pull 404, checkout fail, the sqlite3 setup
  step) classified once the lane is in `create-container-manifest.needs:`? Only the
  script's exit-2 is modeled as non-blocking; everything else fails the `needs:`
  edge and blocks all releases. Architecture must specify containment.
- **OQ-2 (SR-09 / D-2 — for architect, verify early).** Can a
  `workflow_dispatch` from the non-default feature branch push `:latest-<arch>` to
  GHCR given the runner+token config? If not, Step 3 falls back to the two-step
  merge (land non-blocking → dispatch to confirm GREEN → follow-up flip). Verify
  before building Step 3 on D-2 (a).
- **OQ-3 (SR-07 — for human).** Is chronic-INFRA defended only by human vigilance
  (the `::warning::` + marker), or does the human want a stronger automated surface
  (tracked INFRA count / escalation threshold)? Spec currently treats stronger
  escalation as out of scope.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #5347 (reusable bidirectional
  N×M isolation-gate kernel: read-as-barrier, tri-state, non-substring markers,
  off-Docker teeth test), #5348 (infra-003 took 3 human-caught corrections — the
  bidirectional/false-GREEN trap), #5161 (N3 capability, currently `partial`, the
  status this feature flips to `proven`). Findings folded into Domain Models,
  tri-state requirements, and the N3 FR/AC. No new knowledge stored (read-only tier).
