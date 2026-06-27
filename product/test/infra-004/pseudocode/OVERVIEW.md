# infra-004 — Pseudocode Overview

> DoD: **a cross-tenant leak cannot ship a release.** Test/CI-only. Three files,
> four components. This overview is the contract the four component files share;
> every shared constant, marker literal, and exit-code mapping is pinned here.

## Components & Why

| Component | File touched | What it adds |
|-----------|--------------|--------------|
| **C-WB** | `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` | A bounded warmup/readiness barrier between `assert_routes_live` and `run_isolation_matrix`. Makes a healthy cold-model run deterministically GREEN; a real not-ready state past the deadline → INFRA (exit 2), never RED/GREEN. The **only** permitted gate-script change. |
| **C-TS** | `product/test/infra-001/scripts/release-gate-lib.sh` | New **additive** `run_smoke_gate_tristate`. Discriminates the gate's exit 0/1/2/3/other → pass / block / non-block-visible / hard-fail. `run_smoke_gate` byte-unchanged. Single source of truth for the exit-2/INFRA branch. |
| **C-LN** | `.github/workflows/release.yml` | New job `multi-tenant-isolation-amd64` (node + sqlite3, GHCR login, `resolve_image` amd64, `IMAGE` exported, invoke via `run_smoke_gate_tristate`). `needs: [build-container-x64]`. Runs on `push: tags` + `workflow_dispatch`. NOT yet in the manifest `needs:`. |
| **C-FLIP** | `.github/workflows/release.yml` | Add `multi-tenant-isolation-amd64` to `create-container-manifest.needs:` — the blocking edge. Kept a separate one-line component so the flip is reviewable in isolation. |

Build order (risk-ordered, gates downstream): **C-WB → C-TS → C-LN → (AC-11 cold-model GREEN proof) → C-FLIP.**

## Data Flow — Exit-Code Round Trip

```
release.yml: build-container-x64 ──needs──▶ C-LN (multi-tenant-isolation-amd64)
   source release-gate-lib.sh
   IMAGE = resolve_image(owner, event, ref, amd64)   # push→:v<ver>-amd64 (UN-stripped); dispatch→:latest-amd64
   export IMAGE
        │
        ▼  C-TS: run_smoke_gate_tristate "$IMAGE" bash multi-tenant-isolation-smoke.sh
        │     set +e; out="$(IMAGE="$image" "$@" 2>&1)"; rc=$?; set -e; echo "$out"   # NO pipe between smoke and $?
        ▼
   multi-tenant-isolation-smoke.sh  (script exit code = verdict)
   preflight(C1) ▶ setup_container(C2) ▶ register_both_and_restart(C2)
        ▶ assert_routes_live(C2)
        ▶ C-WB warmup_barrier  ── PRESENT → proceed │ timeout/not-durable → infra_fail (exit 2)
        ▶ run_isolation_matrix (C3/C4 → C5 → C6 → C7 verdict → exit)
```

Script exit code → C-TS action → C-FLIP (in-`needs:`) effect:

| Script exit | Meaning | C-TS action | In manifest `needs:` |
|-------------|---------|-------------|----------------------|
| 0 + runtime marker | GREEN, isolation verified | `return 0` | passes — manifest proceeds |
| 0, no marker | early-exit-0 (false-green class) | `::error::` + `return 1` | **blocks** |
| 1 | RED — genuine cross-tenant leak | `::error::` + `return 1` | **blocks (the DoD)** |
| 2 | INFRA — warmup/durability/dep/pull not established | `::warning::` + canonical INFRA marker + `return 0` | **does NOT block, but visible** |
| 3 | SKIP — Docker absent on a Docker-present lane | `::error::` + `return 1` | **blocks** |
| other | unexpected | `::error::` + `return 1` | **blocks** |

Dominance **RED > INFRA > GREEN** is enforced inside the script's `verdict()` and by C-WB
returning/exiting before the matrix on timeout; C-TS only maps the resulting exit code. A warmup
INFRA (exit 2) precedes the matrix and exits immediately, so it can never mask a downstream RED.

## Shared Constants & Literals (PIN — do not paraphrase in code)

| Name | Value | Owner | Notes |
|------|-------|-------|-------|
| `WARMUP_DEADLINE_SECS` | default `180`, env-overridable | C-WB (new global) | #767-derived (`READY_TIMEOUT_SECS=180`, ~2.5× over the ~70s 10/20/40s embed backoff floor). Barrier's only delta over #767 is model-load (store liveness pre-established by `assert_routes_live`). |
| Warmup throwaway marker | `infra003-warmup-${RUN}` | C-WB | charset `[a-z0-9-]`; asserted **pairwise non-substring** of the four cell markers at runtime (R-02). |
| Canonical INFRA marker | `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN` | C-TS | exact literal; tester asserts it verbatim (WARN #3337). Emitted only on script-exit-2. |
| Verify-by-name GREEN grep | `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` | C-TS (reuse) | matched against the **runtime** `log()`-prefixed line `[infra003-smoke] ALL GATES PASSED`, never the source literal (R-06). Full-line `-x` anchor — a forged marker inside arbitrary output is NOT credited. |
| Exit tri-state | `GREEN=0 RED=1 INFRA=2 SKIP=3` | gate script (unchanged) | RED > INFRA > GREEN; no non-GREEN rounds to 0. |
| Lane job id | `multi-tenant-isolation-amd64` | C-LN / C-FLIP | the id added to `create-container-manifest.needs:` by C-FLIP. |

## Reused Integration Surface (verbatim — never re-author)

- `resolve_image OWNER EVENT REF ARCH` (`release-gate-lib.sh:26`) — push→`:v<ver>-<arch>` UN-stripped; dispatch→`:latest-<arch>`. **Never** `${GITHUB_REF_NAME#v}`.
- `run_smoke_gate IMAGE CMD…` (`release-gate-lib.sh:44`) — **untouched**; C-TS sits beside it.
- `write_then_barrier surface slug store_dir marker` (`multi-tenant-isolation-smoke.sh:271`) — sets `WTB ∈ {PRESENT, INFRA}`; durable read-as-barrier on `READ_DEADLINE_SECS`; routes external probes through `SMOKE_WRITE_CMD`/`SMOKE_READ_MARKER_CMD` (stub seam).
- `assert_routes_live` (`:178`) — per-slug dbs exist + 4 routes non-404; barrier inserted right after.
- `derive_markers` (`:346`) — sets `RUN` (idempotent default) + the four cell markers; reused by C-WB to compute the non-substring check set.

## Stub-Seam Test Surface (off-Docker, no tag, no Docker — proves the load-bearing logic)

- **C-WB:** source the smoke; set `SMOKE_WRITE_CMD`/`SMOKE_READ_MARKER_CMD` + a short `WARMUP_DEADLINE_SECS`; drive `warmup_barrier` to PRESENT (proceed) and to forced-timeout (→ INFRA exit 2). The warmup write must round-trip the SAME `SMOKE_*_CMD` a real write uses (R-01 load-bearing).
- **C-TS:** source the **real** `release-gate-lib.sh` under `set +e; set -uo pipefail`; invoke `run_smoke_gate_tristate` against a tiny stub smoke that exits a chosen code and prints a chosen marker. Full truth table: (0+marker)→0, (0,no-marker)→1, (1)→1, (2)→0 with `::warning::`+INFRA marker, (3)→1, (other)→1. `return` (never `exit`) keeps it sourced-testable.
- **C-FLIP:** static YAML parse — assert `multi-tenant-isolation-amd64 ∈ create-container-manifest.needs:`. Combined with the C-TS forced-RED→return-1 stub, proves "RED blocks the manifest" pre-merge.

Stub-seam gotchas (R-05/R-14, #5192/#5258/#4873/#5345): no pipe between the smoke and `$?`; `return`, never `exit`; re-enable `set +e; set -uo pipefail` after sourcing so an intentionally-RED row does not abort the suite; a sourced function cannot be invoked via `env VAR=x fn` (export-call-unset instead).

## Open Questions / Gaps Flagged

- **OQ-WB-1 (surface choice for embed-warmth).** ADR-001 pins the mechanism (one throwaway `write_then_barrier`) and the bound but not which served surface the warmup write uses. Pseudocode specifies `observe`→`SLUG_A` (lightest reuse); both `observe` and `mcp` route through `resolve_store`→dispatch and a durable own-store write, so either satisfies R-01. If a later finding shows only the MCP/`context_store` path forces the embedding-model load, switch the warmup surface to `mcp`. Non-blocking; recommend confirming against the embed pipeline during AC-11.
- **No other gaps.** All interface names trace to the architecture / existing scripts; the lane id is a new literal introduced here and used identically by C-LN and C-FLIP.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (category `pattern`) — surfaced #5192 (extract the verify-by-name gate spine into a sourceable lib so YAML and the pre-merge test share bytes), #5180 (a self-skipping smoke wired into a CI gate must fail the job on skip, keyed by distinct exit code — never pass green), #5299 (wrap a smoke leg in `run_smoke_gate` without re-authoring the gate runner). Also `mcp__unimatrix__context_search` (category `decision`, topic `infra-004`) — surfaced the feature ADRs #5349 (ADR-001 warmup barrier), #5350 (ADR-002 exit-2/INFRA tri-state), #5351 (ADR-003 blast-radius containment), #5352 (ADR-004 cold-model proof). All folded into the C-WB warmup mechanism, the C-TS no-pipe/return/runtime-marker capture invariants, and the C-LN/C-FLIP fail-closed blast-radius tables.
- Deviations from established patterns: none. C-TS mirrors the proven `run_smoke_gate` capture shape verbatim and adds only the exit-2 branch; C-LN mirrors the `smoke-amd64`/`nan-021` harness; the pull-404→INFRA divergence from `run_smoke_gate`'s exit-4-blocks is an ADR-002/ADR-003-sanctioned decision, not an unflagged deviation.
- Stored: nothing novel to store — read-only design-phase tier; the recurring patterns this feature instantiates (release-gate false-green, sourceable-lib capture invariants, never-green-on-tag, ceremonial-seam) are already captured as #5192/#5180/#5299/#4974, so there is no new cross-component pattern to record.
