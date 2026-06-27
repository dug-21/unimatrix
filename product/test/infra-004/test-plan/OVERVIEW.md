# Test Plan OVERVIEW — infra-004: Enforce Cross-Tenant Isolation as a Blocking Release Gate

> DoD: **a cross-tenant leak cannot ship a release.** Test/CI-only — no `crates/` change.
> The dominant failure class is **silently-vacuous enforcement** (gate blocking yet never
> RED, never GREEN). This plan is centered on what is **provable pre-merge WITHOUT a CI tag
> push** via the existing off-Docker stub seam, plus a clearly carved-out CI-only residue.

## 1. Test Strategy

Three test tiers, mapped to the three touched files:

| Tier | Mechanism | Proves | Provable pre-merge? |
|------|-----------|--------|---------------------|
| **Stub-seam shell logic tests** | Source the REAL shipped bytes (`multi-tenant-isolation-smoke.sh`, `release-gate-lib.sh`); drive functions against `fixtures/stub-read-marker.sh` / `fixtures/stub-smoke.sh` via `SMOKE_*_CMD` | C-WB warmup truth table, C-TS exit-code truth table, capture-shape invariants | **YES** (no Docker / no tag / no model) |
| **Static YAML / `needs:`-graph assertions** | grep / parse `release.yml` | C-LN lane shape (triggers, `resolve_image`, provisioning, no `${REF#v}`), C-FLIP edge (lane id ∈ manifest `needs:`) | **YES** |
| **CI-only operational proof** | `workflow_dispatch` cold-model run | AC-11 cold GREEN → AC-04 deterministic GREEN | **NO — see §5** |

Test convention (matches existing `release-gate-*-logic-test.sh`): `set -uo pipefail` only;
after sourcing the shipped bytes (which run `set -euo pipefail`) explicitly `set +e; set -uo pipefail`
(R-14); `pass`/`oops` counters; emit a final summary line + `[ "$FAIL" -eq 0 ]` as the
completeness witness. **All RC capture proven by EXECUTION, never by reading YAML** (the
#4873 / #5345 / R-05 class).

## 2. Test Files (cumulative — reuse existing fixtures)

| File | Status | Component | Drives |
|------|--------|-----------|--------|
| `release-gate-isolation-logic-test.sh` | **EXTEND** | C-WB | new `warmup_barrier` cases via the existing `SMOKE_*_CMD` seam + `fixtures/stub-read-marker.sh` |
| `release-gate-tristate-logic-test.sh` | **NEW (sibling of `release-gate-logic-test.sh`)** | C-TS | `run_smoke_gate_tristate` truth table via existing `fixtures/stub-smoke.sh` |
| `release-gate-logic-test.sh` | **RE-RUN unchanged** | C-TS (R-07) | sibling-regression: `run_smoke_gate` truth table byte-identical post-change |
| `release-gate-isolation-lane-static-test.sh` | **NEW (sibling of `release-gate-bundle-static-test.sh`)** | C-LN / C-FLIP | YAML grep + `needs:`-graph assertions on `release.yml` |

No new fixtures: `stub-smoke.sh` (honors `STUB_RC`/`STUB_BODY`/`STUB_STREAM`) and
`stub-read-marker.sh` (honors `STUB_PRESENT`/`STUB_INFRA`/`STUB_RETRY`) already cover the seams.

## 3. Risk → Test Mapping (from RISK-TEST-STRATEGY.md)

| Risk | Pri | Test surface | File(s) |
|------|-----|--------------|---------|
| **R-01** warmup ceremonial / false-pass | **Crit** | funnel: WTB consumed-not-discarded; PRESENT requires real `read_marker` round-trip (not liveness `store_size`); + AC-11 cold (CI) | C-WB stub-seam + §5 |
| **R-05** swallowed-exit-code false-green | **Crit** | full exit-code truth table via REAL sourced lib + stub; no-pipe + return-not-exit by execution | C-TS |
| R-03 #767 bound under-covers readiness | High | static: `assert_routes_live` establishes store liveness pre-barrier (delta = model-load only); cold headroom (CI) | C-WB + §5 |
| R-06 anchored run-marker break | High | runtime-line credit / substring-reject in truth table | C-TS |
| R-08 fail-closed inversion | High | truth table: ONLY exit-2→return 0; §5 blast-radius table cell-by-cell; `needs:`-graph | C-TS + C-LN/C-FLIP |
| R-09 pull-404 → visible-INFRA = vacuous | High | exit-2→`::warning::`+canonical marker; assert no `${GITHUB_REF_NAME#v}`; `resolve_image` call-shape | C-TS + C-LN |
| R-10 never-green-on-a-tag | High | AC-11 scoped to dispatch only; budget one post-merge tag round (operational) | §5 |
| R-13 AC-11 ceremonial (warm cache) | High | AC-11 log shows real first-boot HF download, not warm cache / `:783-smoke` (operational) | §5 |
| R-02 warmup-marker collision | Med | runtime non-substring assertion trips loud; warmup row inert to negative greps | C-WB |
| R-07 sibling-lane regression | Med | `git diff` `run_smoke_gate` byte-unchanged; re-run its truth table | C-TS |
| R-14 verification harness false-green | Med | `set +e; set -uo pipefail` post-source; inject RED row first; summary-line witness | C-TS + C-WB (harness self-check) |
| R-04 cold-HF variance | Med | timeout→INFRA-visible (covered by C-WB timeout case); residual accepted | C-WB |
| R-11 stale-image proof | Med | branch-point == `main` HEAD recorded at AC-11 (operational) | §5 |
| R-12 dispatch-from-branch GHCR write | Med | verify `:latest-amd64` push from branch early; two-step fallback (operational) | §5 |
| R-15 chronic-INFRA human-vigilance | Med | marker string stable+greppable; human acceptance of VARIANCE recorded | C-TS + human gate |

## 4. Integration Harness Plan (infra-001)

This is a **release-pipeline / CI-wiring** feature with **no `crates/` change**, so the server
behavior under the MCP interface is unchanged. Suite mapping:

- **`-m smoke` (MANDATORY minimum gate):** run as a **no-regression check** — confirm the
  three-file change did not perturb the server binary or harness. No new MCP-visible behavior
  is introduced, so **no new pytest suite tests are planned or needed** (per "When NOT to plan
  integration tests": pure CI/shell logic with no MCP-visible effect → shell stub-seam suffices).
- **No `tools`/`protocol`/`confidence`/`security` additions** — the feature touches no tool logic.
- **The feature's true integration surface is the shell layer**, exercised two ways:
  1. **Pre-merge (load-bearing):** the off-Docker stub-seam logic tests in §2 — the single
     source of truth (CI and the test source the SAME bytes; drift cannot pass, #5192).
  2. **CI-only (operational):** the full Dockerized `multi-tenant-isolation-smoke.sh` run via
     `workflow_dispatch` (AC-11) — the only place the real cold-model + tag-resolution path runs.

**Stub-seam test surface (the pre-merge contract):**
- C-WB: `SMOKE_WRITE_CMD`/`SMOKE_READ_MARKER_CMD` drive the warmup `write_then_barrier` cell to
  PRESENT (proceed) or forced timeout (→ INFRA exit 2) — no Docker, no model (AC-05).
- C-TS: source the real `release-gate-lib.sh`; invoke `run_smoke_gate_tristate` against
  `fixtures/stub-smoke.sh` exiting with a chosen code + marker — full truth table (AC-08).
- C-FLIP: static `needs:`-graph parse + the C-TS forced-RED→return-1 cell together prove
  "RED blocks the manifest" pre-merge (AC-12).

## 5. CI-Only ACs — NOT Provable Pre-Merge (explicit carve-out)

These require a real `workflow_dispatch` run and are scoped as **documented manual/operational
verification + a budgeted post-merge tag round (C-10)** — NOT unit tests. Gate 3c must record
them as operational evidence, not green/red unit assertions:

| AC | Why CI-only | Stage 3c handling |
|----|-------------|-------------------|
| **AC-11** cold-model dispatch GREEN | Needs a fresh cold-model build + GHCR pull + real HF download on a runner | Record dispatch run URL + log; confirm **real first-boot HF download lines** (not warm cache / not `:783-smoke`, R-13); confirm branch-point == `main` HEAD (R-11); confirm zero warmup-attributable INFRA flap (R-01) |
| **AC-04** deterministic GREEN on cold container | "Proven by AC-11" — same dispatch run | Derived from AC-11 evidence; mark COVERED-BY-AC-11 |
| (R-10) tag-push resolution `:v<ver>-amd64` | First runs on a real tag only post-merge | **Budget one post-merge tag round**; tag-path INFRA degrades to non-blocking (safe), the only blocking first-tag path (harness step) is already exercised by AC-11 |

**Pre-merge proves:** warmup truth table, full exit-code truth table, fail-closed mapping,
`needs:`-graph edge, lane YAML shape, capture-shape invariants, sibling no-regression.
**Pre-merge does NOT prove:** cold-model determinism, `:v<ver>` tag resolution — those are §5.

## 6. Cross-Component Dependencies / Integration Risks

- **C-WB → C-TS exit fidelity:** warmup timeout exits 2 immediately (before the matrix), so a
  warmup INFRA cannot mask a downstream RED; confirm `warmup_barrier` returns/`infra_fail`s
  rather than continuing into the matrix on timeout (R-01/R-08).
- **C-LN → `resolve_image`:** dispatch vs tag-push resolve different tags (#5184); tag-push
  correctness unproven until post-merge (§5).
- **C-FLIP → existing `needs:` siblings:** the new lane joins four existing blocking lanes; its
  harness exposure must mirror (one extra `sqlite3` step), not exceed, theirs (R-07/R-08).
- **Shared `release-gate-lib.sh` ↔ stub test ↔ CI:** the lib is the single source of truth
  sourced by both contexts; no-pipe / return-not-exit / `set -e`-re-enable invariants must hold
  identically (R-05/R-14).

## 7. Open Questions (carry to pseudocode / delivery)

1. **OQ-A (R-02 ordering) — RESOLVED (Gate 3a).** The warmup marker non-substring assertion
   lives **inside `warmup_barrier`**, which calls the idempotent `derive_markers()` first (before
   `write_then_barrier`), so the four cell markers exist when the assertion runs. `derive_markers`
   is idempotent (deterministic markers from the `RUN` global) and is re-invoked harmlessly later
   inside `run_isolation_matrix`. The R-02 test therefore targets `warmup_barrier`'s internal
   `derive_markers()`→non-substring-assertion sequence directly — no longer open.
2. **OQ-B (C-WB testability):** is the warmup barrier a sourceable function (`warmup_barrier`)
   the stub-seam test can call directly (like `run_isolation_matrix`), or only inline in `main`?
   It MUST be source-callable for AC-03/AC-05 to be provable off-Docker.
3. **OQ-C (VARIANCE / R-15):** the chronic-INFRA DoD qualification gates the N3 `proven` claim
   (AC-14). Human ACCEPT-or-escalate decision is a **prerequisite of Gate 3c sign-off**, not a
   test the tester can resolve.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + 2× `mcp__unimatrix__context_search` before
  authoring — surfaced the four infra-004 ADRs (#5349 warmup barrier, #5350 exit-2/INFRA
  tri-state, #5351 blast-radius containment, #5352 cold-model proof + post-merge tag), the
  sourceable-shell-gate testing patterns (#5345 set-e-re-enable / runtime-marker, #5192
  verify-by-name spine + capture invariants), and the ceremonial-seam / N=1-green-≠-proven
  lessons (#5348, #4974). All applied directly into the risk→test mapping and per-component plans.
- Stored: nothing novel — per RISK-TEST-STRATEGY, the recurring patterns (release-gate
  false-green capture, ceremonial seam, never-green-on-tag) are already captured as
  #5192/#5345/#5267/#4974; this feature instantiates them rather than revealing a new
  cross-feature pattern. Will revisit at retro if the cold-model-proof-ceremonial (R-13)
  recurs as a distinct pattern across a second feature.
