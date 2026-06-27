# infra-004: Enforce Cross-Tenant Isolation as a Blocking Release Gate

> **Definition of Done (outcome altitude): a cross-tenant leak cannot ship a
> release.** This is ENFORCEMENT, not detection. The full arc lands in this one
> feature, as four in-scope deliverables: **(1)** add a bounded warmup barrier to
> `multi-tenant-isolation-smoke.sh` (was #857); **(2)** wire it as a standing lane
> in the #788 release-gate job in `.github/workflows/release.yml` (was #856);
> **(3)** run the gate against a **fresh, cold-model build of current `main`** and
> confirm **GREEN** in-feature; **(4)** **flip the lane blocking-on-RED** by adding
> it to `create-container-manifest.needs:` so a genuine cross-tenant leak (RED)
> blocks the release manifest. Test-only; no `crates/` change. On this merge **N3
> (#5161 — writes never mis-routed across projects) becomes `status: proven`**
> (descriptively: the property is now *maintained / enforced* — a leak cannot
> ship); the warmup barrier advances **N4 (no false-alarm signals)**. Authoritative
> scope: GH issues **#856 + #857**, redrawn to the enforcement DoD.

## Problem Statement

Cross-tenant isolation — "a write addressed to slug A can only ever land in A's
store" — is the integrity basis of the personal-cloud model
(`goal:personal-cloud`). A mis-routed write corrupts the wrong project's hash
chain silently and unrollbackably.

infra-003 (#853, PR #855) delivered a genuine behavioral proof:
`multi-tenant-isolation-smoke.sh`, a bidirectional 2×2 gate over both served HTTP
write surfaces (observe + MCP-write). But it is a **point-in-time** proof that
**detects** a leak when manually run — it does not **prevent** a leak from
shipping. Until a RED verdict can block a release, a future isolation regression
(e.g. a routing change in the spirit of vnc-038→041) ships uncaught. The DoD for
this feature is the stronger, outcome-level property: **a cross-tenant leak cannot
ship a release.** Realizing it requires the gate to (a) run on every release, (b)
be deterministically GREEN when healthy (so it can be trusted to block without
false alarms), and (c) actually block the manifest on RED.

Two properties of the gate make the blocking flip safe — and they are why they
are mandatory, not optional polish:

1. **Warmup determinism (#857).** The load-bearing writes (C3/C4) are
   fire-and-forget under `synchronous=NORMAL`; if the embedding-model warmup has
   not completed before them, an own-store marker can miss the read-as-barrier
   deadline → classified **INFRA**. The gate degrades correctly (never false
   RED/GREEN), but an INFRA-flapping gate on a *blocking* lane would either block
   releases on a non-signal (if INFRA blocked) or train operators to ignore the
   lane. A bounded warmup barrier before C3/C4 makes a healthy run deterministically
   GREEN.

2. **Distinct INFRA handling (#5180).** For the lane to block on RED **without**
   blocking on INFRA, the CI wiring must discriminate exit 2 (INFRA) from exit 1
   (RED). The existing `run_smoke_gate` has no exit-2 case (it collapses INFRA into
   a generic failure) — so this discrimination is load-bearing for a blocking lane,
   not cosmetic.

### Central risk — silently-vacuous enforcement (the cold-model hazard)

The embedding model **self-downloads on first use** (C1 / N5). A **cold**
HuggingFace download on a fresh container can blow a tight warmup bound → INFRA. On
a blocking lane, INFRA correctly does **not** block — but that means the isolation
property was **not actually verified that run**. The hazard: a chronically-slow- or
no-warmup environment makes enforcement **silently vacuous** — never RED, never
GREEN, isolation never checked, and the release ships anyway. This is the **central
risk** of this feature: "the gate is blocking" must not be confused with "the gate
verified isolation this run." Three in-scope mitigations counter it:

- **(a) Empirically-derived warmup bound.** Derive the warmup deadline from the
  **#767 embed-readiness gate's empirically-validated first-boot window** (it
  already waits past the embed retry/backoff window for a real
  `context_store → context_search` round trip) — not a guessed number.
- **(b) Cold-model proof.** The AC-11 fresh-build-GREEN run must be a **cold-model
  run** (real first-boot HuggingFace download path, not a warm cache), proving the
  bound holds on the path a fresh release container actually takes.
- **(c) Visible INFRA.** The exit-2/INFRA branch must be **visible** — a
  `::warning::` annotation plus a distinct, greppable marker — never a silent
  return. N4 is "no false-alarm to triage," **not** "no output": a
  repeatedly-INFRA blocking gate must be loud enough that someone notices
  enforcement has gone dark.

## Goals

1. **Warmup barrier (#857):** before the four load-bearing marked writes (C3/C4)
   in `multi-tenant-isolation-smoke.sh`, add a **bounded** warmup/readiness barrier
   confirming the server is fully ready (embedding model loaded, per-slug stores
   live) before the isolation probes run — reusing existing infra-001 readiness
   idioms (boot/liveness `store_size` waits, deadline-poll); **no new mechanism**.
   The bound is derived from the #767 embed-readiness window (not guessed). A
   genuine not-ready state past the warmup deadline remains **INFRA (never
   RED/GREEN)** — the barrier must never convert a real not-ready condition into a
   false pass.
2. **Wire the standing lane (#856):** add `multi-tenant-isolation-smoke.sh` to the
   #788 release-gate job in `.github/workflows/release.yml`, run against the
   **pushed per-arch GHCR bytes** via the shared `resolve_image`, honoring the
   #5180 verify-by-name / tri-state exit contract with **distinct, visible
   exit-2/INFRA handling** in the shared `release-gate-lib.sh` (D-1).
3. **Fresh-build-GREEN (cold-model), in-feature:** run the gate (with the warmup
   barrier) against a **fresh, cold-model build of current `main`** and confirm
   **GREEN** before flipping the lane blocking — front-loading risk discovery into
   this feature, not a backlog item. Mechanism: `workflow_dispatch` against the
   feature branch (D-2).
4. **Flip blocking-on-RED:** add the isolation lane to
   `create-container-manifest.needs:` so a **RED** verdict (genuine cross-tenant
   leak) **blocks** the release manifest — the step that realizes the DoD.

## Blocking Semantics (precise, load-bearing)

The flip to `create-container-manifest.needs:` must implement exactly this mapping:

- **RED (exit 1) → BLOCKS the manifest.** A genuine cross-tenant leak fails the
  job and the manifest never assembles. This realizes the DoD.
- **INFRA (exit 2) → MUST NOT block, but MUST be VISIBLE.** Warmup/durability/dep
  failures surface **non-failing but loud** (a `::warning::` + a distinct greppable
  marker, per mitigation (c)), so the job does not red the manifest yet enforcement
  going dark is noticeable. This preserves **N4** (operators never triage a
  non-signal) while defending against silently-vacuous enforcement. Safe only
  because the warmup barrier (Goal 1) makes healthy runs deterministically GREEN.
- **GREEN (exit 0) → passes** (and only when the verify-by-name marker
  `[infra003-smoke] ALL GATES PASSED` is present — guards early-exit-0).
- **SKIP (exit 3) → does not occur in CI** (release runners have Docker). Defined
  anyway: a SKIP on a Docker-capable lane is a **mis-provisioned lane → hard
  failure** (consistent with `run_smoke_gate`'s exit-3 policy), never a silent pass.

The warmup barrier and the distinct/visible exit-2/INFRA handling are therefore
**mandatory preconditions to flipping the lane blocking** — without them the lane
would either block releases on warmup noise or let enforcement go silently dark.

## Non-Goals

- **No `crates/` change.** All four steps are test/CI-only; the production routing
  seam is exercised as shipped, never modified. Verified by `git diff`.
- **The warmup barrier is the ONLY permitted gate-script change.** Beyond the
  bounded warmup/readiness barrier before C3/C4 (#857), the gate's assertions,
  four-marker / non-substring scheme, read-as-barrier model, terminal run-marker,
  and tri-state exit contract are **untouched**. No re-architecture of the gate.
- **No new mechanism for the barrier** — reuse existing infra-001 readiness idioms,
  not a new readiness primitive.
- **No arm64 isolation lane this round** (D-3) — not needed unless a reason
  emerges; routing is architecture-independent Rust.
- **No new smoke script** — the script already exists (PR #855); no #815
  new-smoke-script invariant update is in scope.
- **No new local validation harness** — the off-Docker `SMOKE_*_CMD` stub seam
  already exists; the warmup-barrier addition must stay compatible with it.
- **No UDS behavioral probe / no parity-matrix shape** — the ADR-006 compile-time
  guard (`FORBIDDEN_IN_LOCAL`) is referenced as proof of a single local route, not
  re-run.

## Background Research

### The gate being hardened, wired, and flipped (`multi-tenant-isolation-smoke.sh` / `isolation-probe-lib.sh`)

- Exit contract (lines 23–28): `GREEN=0 / RED=1 (fail) / INFRA=2 (infra_fail) /
  SKIP=3 (Docker absent)`. RED dominates INFRA dominates GREEN. INFRA=2 is
  deliberately distinct from posture-smoke's exit `4` so both can share one lane.
  Terminal marker `[infra003-smoke] ALL GATES PASSED` is emitted only on GREEN and
  matches `release-gate-lib.sh`'s `\[[a-z0-9-]+-smoke\] ALL GATES PASSED` regex.
- **Where the warmup barrier slots in:** `main` runs `preflight` (C1) →
  `setup_container` (C2 boot) → `register_both_and_restart` (C2) →
  `assert_routes_live` (C2 route-liveness precondition) → `run_isolation_matrix`
  (C3/C4 writes → C5 barrier → C6 negatives → C7 verdict). The warmup barrier
  belongs **after `assert_routes_live` and before `run_isolation_matrix`** (before
  the C3/C4 writes).
- **Idioms to reuse (no new mechanism):** `wait_for_http_active` (deadline-poll on
  `docker logs`), `store_size()` (`vol du -s`, WAL-robust liveness — "C2
  boot/liveness waits ONLY"), and the C5 `write_then_barrier` deadline-poll shape
  (`READ_DEADLINE_SECS`/`READ_POLL_SLEEP`). `docker-embed-readiness-smoke.sh` (the
  **#767** gate) is the reference for "embedding model loaded" AND for the
  empirically-validated cold-first-boot window the warmup bound is derived from.
- **Degradation contract to preserve:** C5 `write_then_barrier` classifies an
  own-store marker that misses its deadline as `WTB="INFRA"` (never RED, never a
  vacuous pass); `verdict()` makes INFRA dominate GREEN but RED dominate INFRA. The
  warmup barrier must keep this.

### The #788 standing gate as it exists today (`.github/workflows/release.yml`)

- Triggers on `push: tags: ['v*']` and `workflow_dispatch` only (**not on PRs**).
  Container branch pushes per-arch bytes to
  `ghcr.io/<owner>/unimatrix:v<version>-<arch>` (push) / `:latest-<arch>`
  (dispatch).
- The #788 gate is four smoke jobs (`smoke-amd64/arm64`, `embed-amd64/arm64`), each
  `needs:` its own-arch build, pulling pushed GHCR bytes via `resolve_image` and
  running through `run_smoke_gate`. These four ARE in
  `create-container-manifest.needs:` — i.e. **blocking** (the model infra-004's
  isolation lane joins after the in-feature cold-model GREEN run).
- `nan-021-https-uds-parity` is the **non-blocking** precedent: `needs:
  [build-container-x64]`, visible on every tag+dispatch, intentionally **NOT** in
  `create-container-manifest.needs:` on a first-green budget — and it uses the same
  `resolve_image` **dispatch** path D-2 relies on. infra-004 starts from this shape,
  then moves INTO `needs:` once the cold-model GREEN is demonstrated.

### The gate spine — `release-gate-lib.sh` (single source of truth)

- `resolve_image OWNER EVENT REF ARCH` → push `:v<version>-<arch>` (UN-stripped),
  dispatch `:latest-<arch>`. The smoke accepts `IMAGE` and pulls/registers against
  it (`setup_container` line 141; `register_both_and_restart` line 170).
- `run_smoke_gate IMAGE CMD...` discriminates exit `0`/`3`/`4`/`1`/`*`. **Critical
  finding:** it has **no case for exit `2` (INFRA)** — the isolation gate's INFRA
  code falls into `*) unexpected → return 1`, collapsing INFRA into a generic
  failure. For a **blocking** lane this is load-bearing: INFRA-as-failure would
  block releases on warmup noise. The exit-2 branch (D-1, in this shared lib) is
  what lets the lane block on RED while surfacing INFRA neutrally-but-visibly.

### Provisioning + capability context

- The gate hard-fails INFRA without `sqlite3` (`preflight` C1); the existing
  `smoke-*` lanes provision only `node`. The new lane must provision `sqlite3` —
  **coordinate with #849**.
- **N3 (#5161)** — `nfr` "writes are integrity-protected — never mis-routed across
  projects," currently `partial`. Its `partial` note cited two blockers: (i) the
  standing gate isn't wired, and (ii) "C5/#787 per-slug surface still open."
  **Caveat (ii) is already gone** — C5 (#5190) is **proven** (crt-056 / #789,
  merged 2026-06-19). This feature closes the **only remaining blocker** (i), so on
  merge N3 moves to **`status: proven`**.
- Reusable shapes in Unimatrix: behavioral cross-tenant isolation smoke (#5347);
  verify-by-name release-gate + pre-merge stub-test (#5192 / ADR-003 #5183).

## Proposed Approach

Deliver the four steps as one feature, in dependency order; steps 1 and 2 are the
preconditions that make step 4 safe:

**Step 1 — Warmup barrier (#857).** Insert a bounded warmup/readiness barrier in
`multi-tenant-isolation-smoke.sh` after `assert_routes_live` and before
`run_isolation_matrix`. On a bounded deadline-poll built from `store_size` /
`wait_for_http_active` idioms (bound derived from the **#767** cold-first-boot
window), confirm the embedding model is loaded and both per-slug stores are live
before any load-bearing write. Past its deadline a genuinely not-ready server
returns **INFRA** — never a false GREEN/RED. Keep compatibility with the off-Docker
`SMOKE_*_CMD` stub seam.

**Step 2 — Wire the standing lane (#856).** Add a new job to `release.yml`
(initially mirroring `nan-021-https-uds-parity`: `needs: [build-container-x64]`,
on `push: tags` + `workflow_dispatch`), provision `node` + `sqlite3` (#849), log in
to GHCR, resolve the image via `resolve_image`, and invoke the smoke once with
`IMAGE` exported, honoring the #5180 contract with the **distinct, visible
exit-2/INFRA branch in the shared `release-gate-lib.sh`** (D-1).

**Step 3 — Fresh-build cold-model GREEN, in-feature (D-2).** Before flipping
blocking, demonstrate the gate is **GREEN against a fresh, cold-model build of
current `main`** via a **`workflow_dispatch` run against the feature branch**.
Because infra-004 is test-only (no `crates/` change), a build of the feature branch
produces a **byte-identical production image to `main`** — so the dispatch run IS a
fresh build of `main`'s production code, with the warmup barrier present in the
harness, exercising the real `resolve_image` dispatch path (`:latest-<arch>`) and
polluting no tag namespace. The run must hit the cold first-boot download path
(mitigation (b)).

**Step 4 — Flip blocking-on-RED.** Add the isolation lane to
`create-container-manifest.needs:` and implement the Blocking Semantics above: RED
blocks, INFRA does not block (but is visible), GREEN passes (with marker),
SKIP-on-Docker-present is a hard failure. This realizes the DoD: **a cross-tenant
leak cannot ship a release.**

Rationale: steps 1 and 2 are *mandatory preconditions* for step 4 — only a
deterministically-GREEN-when-healthy gate with distinct, visible INFRA handling can
block on RED without blocking on warmup noise or going silently dark. Step 3
front-loads the cold-model risk into this feature.

## Acceptance Criteria

### Warmup barrier (#857)

- **AC-01:** A bounded warmup/readiness barrier runs in
  `multi-tenant-isolation-smoke.sh` **before the C3/C4 load-bearing writes**,
  confirming the embedding model is loaded and both per-slug stores are live. The
  bound is **derived from the #767 embed-readiness window**, not a guessed number.
- **AC-02:** The barrier reuses existing infra-001 readiness idioms (`store_size`
  waits, deadline-poll) — **no new readiness mechanism**.
- **AC-03 (degradation contract preserved, load-bearing):** a genuine not-ready
  state past the warmup deadline remains **INFRA (exit 2), never RED and never
  GREEN** — the barrier never converts a real not-ready condition into a false
  pass.
- **AC-04 (deterministic GREEN on a COLD-model container, load-bearing):** a
  healthy run that takes the **cold first-boot embedding-model download path** is
  deterministically GREEN — no INFRA flap attributable to model warmup before the
  load-bearing writes. (This is the AC that defends against silently-vacuous
  enforcement; it is proven by AC-11.)
- **AC-05:** the barrier addition remains compatible with the off-Docker
  `SMOKE_*_CMD` stub seam (the pre-merge gate-logic test still drives the verdict
  truth table without Docker).

### Standing-lane wiring (#856)

- **AC-06:** A new job in `.github/workflows/release.yml` runs
  `multi-tenant-isolation-smoke.sh` on every release `push: tags: ['v*']` and on
  `workflow_dispatch`; its result is an independent job status.
- **AC-07:** The job runs against the **pushed per-arch GHCR bytes** via the shared
  `resolve_image` (push → `:v<version>-<arch>` UN-stripped; dispatch →
  `:latest-<arch>`), `IMAGE` exported — no local rebuild of the production image in
  the job.
- **AC-08 (distinct, visible INFRA handling, load-bearing):** the wiring
  discriminates the #5180 tri-state — GREEN (0) passes, RED (1) fails, **INFRA (2)
  is handled distinctly and non-failing but VISIBLE** (a `::warning::` + a distinct
  greppable marker, never a silent return), SKIP (3) on a Docker-present lane is a
  hard failure. Implemented in the shared `release-gate-lib.sh` (D-1). No non-GREEN
  verdict is silently rounded to a pass.
- **AC-09:** GREEN is credited only when the smoke prints
  `[infra003-smoke] ALL GATES PASSED` (matching `\[[a-z0-9-]+-smoke\] ALL GATES
  PASSED`), guarding against early-exit-0.
- **AC-10:** The lane provisions every read dependency the gate requires: `node`
  and **`sqlite3`** (#849) — absence of either is INFRA, never an empty-pass.

### Fresh-build cold-model GREEN + blocking flip (the DoD)

- **AC-11 (cold-model fresh-build GREEN demonstrated in-feature):** the gate (with
  the warmup barrier) is shown **GREEN against a fresh, cold-model build of current
  `main`** — taking the real first-boot HuggingFace download path, not a warm cache
  or the stale `:783-smoke` artifact — via a `workflow_dispatch` run against the
  feature branch (D-2), before the blocking flip lands. The evidence (run/log
  reference) is recorded with the feature.
- **AC-12 (blocking-on-RED):** the isolation lane IS in
  `create-container-manifest.needs:`, so a **RED** verdict (genuine cross-tenant
  leak) **fails the release manifest** — the manifest does not assemble. (Provable
  pre-merge via the stub seam forcing a RED cell + the `needs:` graph assertion.)
- **AC-13 (INFRA does NOT block, but is visible):** an **INFRA** (exit 2) outcome
  does **not** fail the manifest, and emits the visible `::warning::` + greppable
  marker (AC-08) so a chronically-INFRA (silently-vacuous) gate is noticeable. N4
  preserved.
- **AC-14 (capability):** delivery sets **N3 (#5161) `status: proven`** with
  `proven_by =` the blocking gate + the AC-11 cold-model fresh-build GREEN run.
  Descriptive prose may say *maintained / enforced*; the **status field is
  `proven`** (the enum is `missing | partial | proven | claimed` — no 5th value).
  The note records: this feature closed the only remaining N3 blocker (the C5/#5190
  caveat was already resolved by crt-056 / #789). **Surface boundary:** N3 is
  `proven` **as-of the two served write surfaces (observe + MCP-write)**; a future
  NEW served write route reopens the nfr proof per the standard nfr lifecycle — a
  later third-route mis-route is not waved off with "N3 was proven."

### Shared

- **AC-15:** No `crates/` change; the only gate-script edit is the warmup barrier —
  verified by `git diff` (changes confined to `multi-tenant-isolation-smoke.sh`,
  `.github/workflows/release.yml`, and `release-gate-lib.sh` for exit-2 handling).

## Constraints

- **Test/CI-only:** changes confined to `multi-tenant-isolation-smoke.sh` (barrier),
  `.github/workflows/release.yml` (lane + `needs:` flip), and `release-gate-lib.sh`
  (exit-2 handling). No `crates/` change; no other gate-script logic change.
- **Bounded, no-new-mechanism barrier (#857), #767-derived bound:** reuse
  `store_size` / `wait_for_http_active` / deadline-poll; bound derived from the #767
  cold-first-boot window; past-deadline outcome is INFRA.
- **Tri-state semantics are invariant and now enforced:** RED blocks the manifest;
  INFRA does not block but is visible (warning + greppable marker); GREEN passes
  with marker; SKIP on a Docker-present lane is a hard failure. RED dominates INFRA
  dominates GREEN.
- **Pushed-bytes contract (#5180 / nan-019):** `resolve_image`; UN-stripped push
  tag; `:latest-<arch>` on dispatch; never `${GITHUB_REF_NAME#v}`.
- **Single source of truth:** the exit-2 discrimination lands in the shared
  `release-gate-lib.sh` (sourced by both CI and the pre-merge gate-logic test), not
  inline YAML.
- **`sqlite3` provisioning — coordinate with #849.**
- **Stub-seam compatibility:** the warmup barrier must not break the off-Docker
  `SMOKE_*_CMD` gate-logic test; the RED-blocks / INFRA-passes-visibly manifest
  behavior should be provable pre-merge via that seam + a `needs:` graph assertion
  (no tag push required to prove the wiring logic).

## Delivery Sequence (risk-ordered)

1. Warmup barrier in the gate script, #767-derived bound (Step 1 / AC-01–05).
2. Visible exit-2/INFRA discrimination in `release-gate-lib.sh` + the non-blocking
   lane in `release.yml` (Step 2 / AC-06–10).
3. **Cold-model fresh-build GREEN run** via `workflow_dispatch` on the feature
   branch, demonstrated in-feature (Step 3 / AC-11) — *gates* step 4.
4. Flip the lane into `create-container-manifest.needs:` with RED-blocks /
   INFRA-passes-visibly semantics (Step 4 / AC-12–14).

## Decisions (settled — formerly open questions)

- **D-1 (exit-2/INFRA handling — was OQ-1):** Extend the **shared
  `release-gate-lib.sh`** with the exit-2 branch (single source of truth, sourced
  by both CI and the stub test; covers any future tri-state gate). The branch must
  emit a **distinct, greppable marker + `::warning::`** and return success
  (non-failing), **not** a silent return (mitigation (c)). Mandatory before the
  blocking flip.
- **D-2 (fresh-build cold-model GREEN before the flip — was OQ-2):** **Option (a) —
  `workflow_dispatch` against the feature branch.** Because infra-004 is test-only
  (no `crates/` change), a feature-branch build is a **byte-identical production
  image to `main`**, so the dispatch run IS a fresh build of `main`'s production
  code with the warmup barrier present; it exercises the real `resolve_image`
  dispatch path (the nan-021 precedent) and pollutes no tag namespace. **Fallback:**
  two-step merge (land non-blocking → dispatch to confirm GREEN → follow-up flip)
  **only if** dispatch-from-branch cannot push `:latest-<arch>` for a non-default
  branch in the runner config (it re-splits the feature, so prefer (a)).
  **Rejected:** a pre-release dry-run tag — it risks tripping the real manifest
  path.
- **D-3 (arch coverage — was OQ-3):** **amd64-only blocking; do NOT add an arm64
  lane this round.** `resolve_store` routing is architecture-independent Rust — a
  cross-tenant leak manifests identically on both arches, and the ADR-006
  compile-time guard holds on any arch that compiles. The only arm64-specific
  failure modes are warmup/timing (→ INFRA, not RED), so an arm64 lane adds CI cost
  + INFRA-flap surface for ≈zero correctness signal. Framing: arm64 is **not needed
  unless a reason emerges** (not "deferred / coming later"). **Signed trade-off:**
  amd64-only-blocking means a hypothetical arm64-only routing miscompilation isn't
  caught — near-zero risk given shared routing logic. (Human has signed this.)

## Dependencies

| Dependency | Relationship |
|-----------|--------------|
| **#788** (CLOSED, merged) | The standing release gate this wires into and flips blocking (`release.yml`). |
| **#855 / #853 / infra-003** | Delivers the gate this feature hardens (warmup), wires, and enforces. |
| **#5180** | Verify-by-name / tri-state exit-code contract — the exit-2 discrimination it requires is load-bearing for blocking. |
| **#767** | Embed-readiness gate — source of the empirically-validated cold-first-boot warmup window (AC-01) and the cold-model proof reference. |
| **#789 / crt-056 / C5 (#5190)** | C5 per-slug surface now **proven** (merged 2026-06-19) — removes N3's second `partial` caveat; this feature closes the last one. |
| **#849** | sqlite3 provisioning — coordinate so the new lane's `sqlite3` need aligns. |
| **#5161 (N3)** | Capability moved `partial` → `proven` on this merge. |
| **N4** | Capability advanced (no false-alarm signals) via the warmup barrier + visible INFRA. |

> #857 (warmup barrier), the "promote to blocking" step, and the fresh-build-GREEN
> run are all **in-scope deliverables**, not external dependencies / future items.

## Tracking

- GH Issues: **#856 + #857** (merged into one feature; redrawn to the enforcement
  DoD — "a cross-tenant leak cannot ship a release").
- Capability: **N3 (#5161) `status: proven`** on this merge (`proven_by =` blocking
  gate + AC-11 cold-model fresh-build GREEN; *maintained / enforced* is prose, not a
  status value), proven **as-of the observe + MCP-write surfaces**; **N4** advanced
  (no false-alarm signals) via the warmup barrier + visible INFRA.
- Related: #788 (standing gate), #855 / #853 / infra-003 (gate delivered), #5180
  (tri-state contract), #767 (warmup window), #789 / crt-056 / #5190 (C5 proven),
  #849 (sqlite3 provisioning).
- GH Issue: https://github.com/dug-21/unimatrix/issues/856
- Session 1 deliverables: `product/test/infra-004/IMPLEMENTATION-BRIEF.md`,
  `product/test/infra-004/ACCEPTANCE-MAP.md`.
</content>
