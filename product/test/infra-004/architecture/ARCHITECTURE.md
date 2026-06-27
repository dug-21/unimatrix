# infra-004 — Architecture: Enforce Cross-Tenant Isolation as a Blocking Release Gate

> DoD (outcome altitude): **a cross-tenant leak cannot ship a release.** Enforcement, not
> detection. Test/CI-only — no `crates/` change. Three files change:
> `multi-tenant-isolation-smoke.sh` (warmup barrier), `release-gate-lib.sh` (exit-2/INFRA
> tri-state handling), `.github/workflows/release.yml` (standing lane + `needs:` flip).

## 1. System Overview

infra-003 (#853/#855) delivered a behavioral, bidirectional 2×2 cross-tenant isolation
**proof** (`multi-tenant-isolation-smoke.sh`) over both served write surfaces (observe + HTTP
MCP-write). It *detects* a leak when run, but nothing makes a RED verdict *block a release*.
infra-004 converts that point-in-time proof into a standing, blocking release gate, realizing
the integrity basis of `goal:personal-cloud` (a write to slug A can only ever land in A's
store). On merge, capability **N3 (#5161)** moves `partial → proven` (as-of the observe +
MCP-write surfaces); **N4** (no false-alarm signals) advances via the warmup barrier + visible
INFRA.

The feature sits entirely in the **release pipeline** subsystem. It joins the existing #788
release-gate spine (`release-gate-lib.sh` sourced by `release.yml`), reusing the nan-019/nan-020
verify-by-name contract (#5180 / #5183 / #5192 / #5258) and the nan-021 non-blocking-lane
precedent. The single new behavioral element is a **bounded warmup/readiness barrier** in the
gate script; everything else is CI wiring + one additive shared-lib function.

Two properties make the blocking flip safe — and are therefore mandatory preconditions:
1. **Warmup determinism (#857):** a healthy run that takes the cold first-boot embedding-model
   download path is deterministically GREEN — no INFRA flap before the load-bearing C3/C4 writes.
2. **Distinct INFRA handling (#5180):** CI discriminates script exit 2 (INFRA) from exit 1
   (RED) so the lane blocks on RED **without** blocking on warmup/dependency noise.

The central risk this architecture defends against is **silently-vacuous enforcement**: "the
gate is blocking" must never be confused with "the gate verified isolation this run." A
chronically-INFRA blocking lane never REDs, never GREENs, and ships the release anyway. The
warmup barrier (deterministic GREEN), the #767-derived bound (real cold-download margin), the
cold-model in-feature proof (AC-11), and visible INFRA (`::warning::` + greppable marker)
together counter it.

## 2. Component Breakdown

| # | Component | File | Responsibility | New / Changed |
|---|-----------|------|----------------|---------------|
| C-WB | Warmup/readiness barrier | `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` | Before the C3/C4 load-bearing writes, confirm embed model loaded + both per-slug stores live, on a #767-derived bounded deadline; timeout → INFRA. | New function (the **only** permitted gate-script change). |
| C-TS | Tri-state release-gate runner | `product/test/infra-001/scripts/release-gate-lib.sh` | New **additive** function discriminating the gate's exit 0/1/2/3 → pass / block / non-block-visible / hard-fail. Single source of truth for the exit-2/INFRA branch (D-1). | New function; `run_smoke_gate` untouched. |
| C-LN | Standing isolation lane | `.github/workflows/release.yml` | New job: provision node + sqlite3, GHCR login, `resolve_image`, invoke the gate via C-TS once with `IMAGE` exported. On tag-push + dispatch. | New job. |
| C-FLIP | Blocking flip | `.github/workflows/release.yml` | Add C-LN to `create-container-manifest.needs:` so RED blocks the manifest. | One-line edit to existing `needs:` list. |

The warmup barrier is the only behavioral change to the gate; its assertions, four-marker
non-substring scheme, read-as-barrier model, terminal run-marker, and tri-state exit contract
are untouched (SCOPE Non-Goals).

## 3. Component Interactions / Data & Exit-Code Flow

```
release.yml: build-container-x64 ──(needs)──▶ C-LN (multi-tenant-isolation lane)
                                                  │  source release-gate-lib.sh
                                                  │  IMAGE = resolve_image(owner,event,ref,amd64)
                                                  ▼
                              C-TS: run_smoke_gate_tristate(IMAGE, bash multi-tenant-isolation-smoke.sh)
                                                  │  set +e; out=$(IMAGE=… cmd 2>&1); rc=$?; set -e
                                                  ▼
                           multi-tenant-isolation-smoke.sh  (script exit code = verdict)
   preflight(C1) ▶ setup_container(C2 boot) ▶ register_both_and_restart(C2)
        ▶ assert_routes_live(C2 precondition: both per-slug dbs exist + 4 routes non-404)
        ▶ ┌─────────────────── C-WB WARMUP BARRIER (NEW) ───────────────────┐
          │  bounded deadline-poll (WARMUP_DEADLINE_SECS, #767-derived ~180s) │
          │  throwaway warmup write→read-as-barrier (reuses write_then_barrier)│
          │   PRESENT → embed warm + store A durable → proceed                 │
          │   timeout → INFRA (exit 2), never RED, never GREEN                 │
          └───────────────────────────────────────────────────────────────────┘
        ▶ run_isolation_matrix (C3/C4 writes on tight READ_DEADLINE_SECS=10
                                 ▶ C5 barrier ▶ C6 negatives ▶ C7 verdict)
```

Script exit code (verbatim, unchanged) → C-TS mapping → blocking outcome:

| Script exit | Meaning | C-TS action | In `needs:` (C-FLIP) effect |
|-------------|---------|-------------|------------------------------|
| 0 + marker `[infra003-smoke] ALL GATES PASSED` | GREEN, isolation verified | return 0 | passes — manifest proceeds |
| 0, no marker | early-exit-0 (false-green class) | `::error::` + return 1 | **blocks** |
| 1 | RED — genuine cross-tenant leak | `::error::` + return 1 | **blocks (realizes DoD)** |
| 2 | INFRA — warmup/durability/dep/pull not established | `::warning::` + distinct greppable marker + **return 0** | **does NOT block, but visible** |
| 3 | SKIP — Docker absent | `::error::` + return 1 (Docker-present lane → hard fail) | **blocks** |
| other | unexpected | `::error::` + return 1 | **blocks** |

RED dominates INFRA dominates GREEN is preserved end-to-end (the script's `verdict()` already
enforces it; C-TS only maps the resulting exit code).

## 4. Blocking-Semantics Mapping (load-bearing)

- **RED (exit 1) → BLOCKS.** Job fails, `needs:` edge unmet, manifest never assembles. DoD.
- **INFRA (exit 2) → MUST NOT block, MUST be VISIBLE.** C-TS emits a `::warning::` annotation
  plus a stable greppable marker (e.g. `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`)
  and returns 0. Preserves N4 (no non-signal to triage) while making "enforcement went dark"
  noticeable. Safe only because C-WB makes healthy runs deterministically GREEN.
- **GREEN (exit 0) → passes** only with the anchored marker (guards early-exit-0).
- **SKIP (exit 3) on a Docker-present lane → hard failure** (mis-provisioned lane; consistent
  with `run_smoke_gate`'s exit-3 policy), never a silent pass.

## 5. SR-04 — Blocking Blast-Radius Containment

Once C-LN is in `create-container-manifest.needs:`, **any** job failure blocks the manifest —
not only the script's tri-state. The script's exit-2 is the **only** path that maps to
non-blocking; every other failure source fails closed. Full classification:

| Failure source | Layer | Blocks manifest? | Rationale |
|----------------|-------|------------------|-----------|
| Runner outage / job infra | harness | yes (job fails) | fail-closed; identical exposure to the four existing blocking lanes |
| `actions/checkout` fail | harness | yes | same as siblings |
| node / **sqlite3** provisioning fail | harness | yes | a real provisioning break must be fixed, not silently passed (AC-10) |
| GHCR `docker/login` expiry/fail | harness | yes | same as siblings |
| Image pull 404 / tag missing | **script (C1/C2 → exit 2)** | **no — visible INFRA** | script classifies pull failure as INFRA (`infra_fail`), so a missing pushed tag is non-blocking-but-visible (diverges from `run_smoke_gate`'s exit-4-blocks; see ADR-002 / ADR-003) |
| sqlite3/busybox absent at runtime | script (C1 → exit 2) | no — visible INFRA | preflight INFRA; provisioning step above is the fail-closed guard against this being chronic |
| Warmup not ready at deadline | script (C-WB → exit 2) | no — visible INFRA | central-risk mitigation; deterministic-GREEN-when-healthy keeps it rare |
| Genuine cross-tenant leak | script (exit 1) | **yes** | DoD |

Containment principles: (1) C-TS maps **only** script-exit-2 to non-blocking — it never makes a
harness failure non-blocking (which would be unsafe) and never makes script-INFRA blocking
(which would block on noise). (2) Harness surface is minimized and mirrors the proven
`smoke-amd64` lane (checkout, setup-node, GHCR login) plus one self-contained sqlite3
provisioning step. (3) The new exposure vs the four existing blocking lanes is small (one extra
provisioning step + a heavier in-container smoke); the AC-11 dispatch run exercises the **entire
harness** on the dispatch path before the flip, so harness breakage is found pre-flip.

## 6. D-2 In-Feature Cold-Model Proof vs SR-05 Post-Merge Tag Strategy

- **AC-11 (in-feature, dispatch):** `workflow_dispatch` on the feature branch builds a
  byte-identical production image to `main` (test-only feature) and runs C-LN against the
  **dispatch** image (`:latest-amd64` via `resolve_image`), taking the real cold first-boot
  HuggingFace download path. This proves **warmup bound + verdict + the full harness** — but on
  the dispatch tag-resolution branch only.
- **SR-05 (never-green-on-tag, #5267):** the **tag-push** path (`:v<version>-amd64`) and the
  blocking `needs:` edge first execute on a real tag only **post-merge**. Per ADR-004 (#5184)
  the two trigger surfaces resolve different tags. AC-11 therefore does **not** prove tag-push
  resolution. Strategy: (a) treat AC-11 as proof of warmup+verdict+harness, not tag resolution;
  (b) the lane is **diagnostic-capture-first** (C-TS echoes the full smoke log on every path,
  the script already logs last-state on timeout) so the first real tag yields a diagnosis, not a
  guess; (c) explicitly **budget one post-merge tag round** as expected cost, not a regression;
  (d) because **INFRA does not block**, a never-green-on-tag failure that surfaces as INFRA
  (e.g. pull 404 from a tag-resolution miss) will *not* block releases — it degrades to
  visible-vacuous, the safe failure mode. The only first-tag path that blocks a healthy release
  is a harness-step failure, which AC-11's dispatch run already exercised. See ADR-004.

## 7. Where the Off-Docker Stub Seam Proves the Wiring Pre-Merge

No Docker, no tag, and no live model are needed to prove the load-bearing logic before merge:

- **Warmup barrier (AC-05):** C-WB reuses `write_then_barrier`, which already routes through
  `SMOKE_WRITE_CMD` / `SMOKE_READ_MARKER_CMD`. The existing gate-logic stub test drives the
  warmup cell to PRESENT (proceed) or to a forced timeout (→ INFRA exit 2) without Docker.
- **Tri-state runner (AC-08/AC-12/AC-13):** C-TS is unit-tested by sourcing the **real**
  `release-gate-lib.sh` and invoking it against a tiny stub smoke that exits with a chosen code
  and prints a chosen marker (the #5192/#5258 pattern). Full truth table proven: (0+marker)→0,
  (0,no-marker)→1, (1)→1, (2)→0 **and** warning+marker emitted, (3)→1, (other)→1. This proves
  RED-blocks / INFRA-passes-visibly **without a tag push**.
- **Blocking graph (AC-12):** a static `needs:` assertion (YAML parse) that C-LN's job id is in
  `create-container-manifest.needs:`, combined with the C-TS forced-RED→return-1 stub, proves
  "RED blocks the manifest" pre-merge.

Stub-seam gotchas to honor (from #5192/#5258): source the lib under `set +e; …; set -uo
pipefail` (never `set -e`); the runner must `return`, not `exit`; capture as
`set +e; out=$(IMAGE=… "$@" 2>&1); rc=$?; set -e` with **no pipe** between the smoke and `$?`;
a sourced function cannot be invoked via `env VAR=x fn` (export-call-unset instead).

## 8. Integration Surface

| Integration point | Type / signature | Source |
|-------------------|------------------|--------|
| `resolve_image OWNER EVENT REF ARCH` → image ref | push → `ghcr.io/<owner>/unimatrix:v<version>-<arch>` (UN-stripped); dispatch → `:latest-<arch>` | `release-gate-lib.sh:26` (reuse verbatim) |
| `run_smoke_gate IMAGE CMD…` | existing 4-lane runner; cases 0/3/4/1/*; **no exit-2 case** | `release-gate-lib.sh:44` (untouched) |
| **NEW** tri-state runner | `run_smoke_gate_tristate IMAGE CMD…` → return 0 on (rc0+marker) or (rc2, after emitting `::warning::` + greppable marker); return 1 on rc1/rc3/(rc0 no-marker)/other | `release-gate-lib.sh` (add; ADR-002) |
| Anchored run-marker grep | `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` (matches `[infra003-smoke] ALL GATES PASSED`) | `release-gate-lib.sh:59` (reuse) |
| Gate script exit contract | GREEN 0 / RED 1 / INFRA 2 / SKIP 3 (RED>INFRA>GREEN) | `multi-tenant-isolation-smoke.sh:23-28` |
| `write_then_barrier surface slug store_dir marker` → sets `WTB ∈ {PRESENT, INFRA}` | bounded read-as-barrier; timeout→INFRA; stub seam via `SMOKE_*_CMD` | `multi-tenant-isolation-smoke.sh:271` (reuse for C-WB) |
| `READ_DEADLINE_SECS` / `READ_POLL_SLEEP` | tight cell deadline (default 10 / 1) | `multi-tenant-isolation-smoke.sh:58-59` |
| **NEW** `WARMUP_DEADLINE_SECS` | warmup-barrier deadline, default ~180 (#767-derived), env-overridable | C-WB (ADR-001) |
| `READY_TIMEOUT_SECS=180` reference | empirically-validated cold-first-boot window (polls past 10/20/40s backoff under real cold HF download) | `docker-embed-readiness-smoke.sh:56,210` (provenance of the bound) |
| Warmup throwaway marker | `infra003-warmup-${RUN}`, `[a-z0-9-]` only, must be pairwise non-substring of the four cell markers | C-WB (ADR-001) |
| Stub seam env | `SMOKE_WRITE_CMD`, `SMOKE_READ_MARKER_CMD`, `READ_DEADLINE_SECS`, `READ_POLL_SLEEP`, `RUN` | `multi-tenant-isolation-smoke.sh:34-45` |
| Lane provisioning | `node` (setup-node@v4) + **`sqlite3`** (self-contained apt step; coordinate #849) | C-LN (ADR-003) |
| Blocking edge | `create-container-manifest.needs: [smoke-amd64, smoke-arm64, embed-amd64, embed-arm64, <isolation-lane-id>]` | `release.yml:615` (C-FLIP) |
| Non-blocking-lane precedent | `nan-021-https-uds-parity` (`needs:[build-container-x64]`, NOT in manifest needs) | `release.yml:573` |

## 9. Residual Risks / Open Questions

- **SR-07 (chronic-INFRA = human-vigilance only):** in-scope mitigation is the visible
  `::warning::` + greppable marker. Automated escalation (count/threshold across releases) is
  **out of scope** (no new mechanism). The marker string is deliberately stable so a future
  scheduled "grep recent release runs for the INFRA marker" alert is cheap. **Open question for
  the human:** accept as documented human-vigilance risk for infra-004, with escalation as a
  follow-up?
- **SR-06 (byte-identical / main drift):** AC-11's dispatch run must build from a feature-branch
  tip == `main` HEAD (rebase immediately before dispatch) or the proof is against a stale image.
  Spec-level constraint — flagged for the specification writer.
- **SR-09 (dispatch-from-branch GHCR write):** D-2(a) relies on the dispatch build pushing
  `:latest-amd64` from a non-default branch. Low risk — `build-container-x64` already runs on
  dispatch and pushes `:latest-amd64`, and `nan-021` exercises this path. Verify GHCR
  `packages: write` from the feature branch **early** before building on D-2(a); two-step
  fallback (land non-blocking → dispatch → follow-up flip PR) stays specified.
- **#849 coordination:** sqlite3 provisioning is self-contained in C-LN (no hard ordering
  dependency on #849); coordinate to avoid duplication.

## 10. ADRs

- ADR-001 — Warmup barrier: placement, mechanism, and #767-derived bound.
- ADR-002 — Exit-2/INFRA tri-state handling: additive function in shared `release-gate-lib.sh`.
- ADR-003 — Blocking-flip blast-radius containment + sqlite3 provisioning (SR-04/SR-03).
- ADR-004 — In-feature cold-model proof + post-merge tag strategy (D-2/SR-05).
