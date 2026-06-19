# nan-019 — Test Plan Overview

> Standing release gate wiring `docker-http-posture-smoke.sh` into `release.yml` as a
> verify-by-name / skip-is-failure gate that blocks the multi-arch manifest. **This is a
> CI/release-workflow + shell feature — no Rust crate change.** The gate's first real
> execution is **post-merge on a `v*` tag push** (#4796). The test strategy therefore
> draws a hard line between what is **PROVABLE PRE-MERGE** (the gate's own correctness:
> exit-code/marker logic, tag-string parity, `needs:`-graph, AC-05 grew-signal monotonicity)
> and what is **CONFIRMABLE ONLY POST-TAG / POST-DISPATCH** (hosted-runner execution, arm64
> cold-boot margin). Coverage for the latter is phrased **"configured + verified locally;
> GH execution confirmed post-tag,"** never asserted as executed fact before it runs.

---

## Test Strategy

This feature has no `cargo test` surface and no new MCP-visible behavior, so the
infra-001 **Python suites do not apply** (see Integration Harness Plan). Testing is three
tiers of **shell/static** validation, all cumulative with infra-001:

| Tier | What | Provability | Gate role |
|------|------|-------------|-----------|
| **T1 — Gate-logic stub-smoke unit test** | Drive the exact `set +e; …; RC=$?` capture + `case` + anchored `grep -qx` against a stub smoke over the truth table {0,1,3,early-0,unexpected} × {marker present/absent} | **PRE-MERGE, fully provable** | **HARD gate (MUST exist before merge — R-01/R-02/R-03)** |
| **T2 — Tag-parity static assertion** | Assert the smoke's resolved per-arch tag string is byte-identical to the metadata-action push pattern, both surfaces | **PRE-MERGE, fully provable** | **HARD gate (MUST exist before merge — R-09, the OCCURRED defect)** |
| **T3 — AC-05 grew-signal validation** | Run the full smoke ≥5× locally; confirm the WAL-robust grew-signal is monotone AND discriminating (fails on a #783-style mis-route) | **PRE-MERGE, provable locally** | **HARD gate (R-04; cannot be retried away — OQ-6)** |
| **T4 — `needs:`-graph + trigger-surface static assertion** | Parse `release.yml` and assert the edge topology, dispatch gate, and `on:` triggers | **PRE-MERGE, fully provable** | **HARD gate (R-06/R-08/R-11)** |
| **T5 — `workflow_dispatch` dry-run** | Trigger the workflow manually; exercise real hosted `ubuntu-22.04`/`ubuntu-22.04-arm` against `:latest-<arch>` | **PRE-TAG, hosted** | Confirmation (first cross-platform proof; primary R-05 pre-tag signal) |
| **T6 — First `v*` tag run** | Watch both smoke jobs green + manifest publishes on the real release | **POST-TAG only (AC-07)** | Confirmation (in-scope rework on surprise; never `\|\| retry`) |

**Test home (cumulative, no parallel scaffolding):** the shell glue under test is extracted
verbatim from `release.yml` into a sourceable helper so the same bytes are exercised by the
test and shipped by the workflow (avoids the "test asserts X, ship emits Y" divergence,
lesson #3548). Place the new tests beside the existing smoke in
`product/test/infra-001/scripts/`:

| New file | Purpose | Component file |
|----------|---------|----------------|
| `product/test/infra-001/scripts/release-gate-lib.sh` | Sourceable functions: `run_smoke_gate()` (the capture/`case`/grep) and `resolve_image_tag()` (per-trigger tag resolution). The workflow step sources/inlines the SAME logic. | smoke-amd64 / smoke-arm64 |
| `product/test/infra-001/scripts/release-gate-logic-test.sh` | T1 truth-table + T2 tag-parity + T4 graph assertions; self-contained, exits non-zero on any failure. Runnable locally and as a pre-merge gate step. | smoke-amd64 / smoke-arm64 / create-container-manifest |
| `product/test/infra-001/scripts/fixtures/stub-smoke.sh` | A stub that prints a chosen fixture body and `exit`s a chosen code (driven by `STUB_RC` / `STUB_BODY` env) — stands in for the real smoke so RC propagation is verified by **execution**, not reading (R-02 / #4873). | smoke-amd64 / smoke-arm64 |

> Implementation note for Stage 3b/3c: the impl agent may instead choose `bats` if a bats
> harness already exists; none does today, so a plain `set -e` bash test that `exit 1`s on
> first failed assertion is the cumulative-minimal choice. Whichever is used, **the workflow
> step and the test MUST exercise the same `run_smoke_gate` bytes** — do not re-type the
> `case` in YAML and again in the test (that is the divergence the feature exists to kill).

---

## Risk → Test Mapping (from RISK-TEST-STRATEGY.md)

| Risk | Priority | Test(s) | Tier | Pre-merge provable? |
|------|----------|---------|------|---------------------|
| **R-01** gate logic wrong & untested | Critical | `test_gate_*` truth table (5 RC × 2 marker rows) | T1 | **YES — MUST exist before merge** |
| **R-02** RC swallowed before read (#4873) | Critical | `test_gate_rc_survives_capture` (exit 1→RC=1, exit 3→RC=3 by **execution**) + adversarial pipe/pipefail/`continue-on-error` rejection in review | T1 | **YES** |
| **R-03** early-exit-0, marker absent passes green | Critical | `test_gate_early_exit0_red`, `test_gate_marker_anchored` (substring/echo/trailing not matched by `grep -qx`) | T1 | **YES** |
| **R-04** AC-05 grew-signal flaky/un-discriminating | Critical | `test_ac05_signal_monotone_5x`, `test_ac05_positive_control`, `test_ac05_negative_control` (hash-mis-route FAILS) | T3 | **YES (local)** |
| **R-05** arm64 cold-boot exceeds deadline | Critical | dispatch dry-run timing; first-tag wall-time vs 90s margin | T5/T6 | **NO — post-dispatch/post-tag** |
| **R-06** ADR-004 independence regression | Critical | `test_needs_no_cross_branch_edge`, `test_manifest_single_block_point` | T4 | **YES** |
| **R-07** pushed-bytes degrades to rebuild | High | `test_smoke_job_sets_IMAGE`, `test_no_production_build_in_smoke` | T4 | **YES (config); log post-tag** |
| **R-08** manifest not actually gated | High | `test_manifest_needs_both_smokes`, `test_no_continue_on_error`, dispatch green-skip | T4 | **YES (config); skip behavior post-tag** |
| **R-09** tag-resolution mismatch (**OCCURRED**) | High | `test_tag_parity_push_amd64/arm64`, `test_tag_parity_dispatch`, `test_no_v_strip` | T2 | **YES — MUST exist before merge** |
| **R-10** GHCR push not yet pullable | Med | `test_smoke_needs_own_arch_build` (ordering) | T4 | **YES (ordering); race post-tag** |
| **R-11** trigger surface over/under-reach | Med | `test_on_includes_dispatch_excludes_pr` | T4 | **YES** |
| **R-12** AC-05 hardening regresses smoke/marker | Med | `test_smoke_marker_still_last`, `test_ac05_uses_vol_not_exec` | T3 | **YES (local)** |
| **R-13** inherited latent smoke bug | Med | amd64 re-run 3/3 (T3); arm64 first-run as discovery | T3/T5/T6 | amd64 YES; arm64 post-dispatch |
| **R-14** briefly-public intermediates | Low | none — accepted by design (NFR-09), documented | — | n/a |

---

## Pre-Merge-Provable vs Post-Tag-Only Split (load-bearing)

**Fully provable PRE-MERGE (these are HARD gates — the heart of the feature's verifiability):**
- **R-01/R-02/R-03** — the gate-logic truth table + RC-survives-capture, run by **execution** of the stub smoke. Only `(0, marker present)` is green; `exit 1`/`exit 3` read as 1/3 (not 0).
- **R-09** — the tag-parity byte-identity assertion. RED at merge on any `${...#v}` strip, missing/extra `v`, or swapped per-arch suffix. **No tag push required.**
- **R-04** — AC-05 grew-signal monotonicity (≥5 local full-smoke runs) + discrimination (negative control fails on a mis-route). Cannot be retried away (OQ-6), so it must be proven non-flaky before merge.
- **R-06/R-08/R-11** — `needs:`-graph topology, single manifest block point, dispatch gate, and `on:` trigger surface, by static parse of `release.yml`.
- **R-07/R-10** — `IMAGE=` set, no production build, strict push→smoke ordering (config-level).

**Confirmable ONLY POST-DISPATCH / POST-TAG (configured + verified locally; GH execution confirmed post-tag):**
- **AC-07** — both smoke jobs actually run green on the hosted runners; manifest publishes.
- **R-05** — arm64 cold first-boot wall-time vs the 90s deadline margin (first true signal on the dispatch dry-run; confirmed on first tag).
- **R-08 behavior** — a red smoke leaving the manifest **skipped** (not run); dispatch run shows manifest **green-skipped**.
- **R-07 log** — "using prebuilt image: ghcr.io/...:v<version>-<arch>" in the real smoke log.
- **R-10 race** — first-try pull success after `--push` propagation; any race is in-scope structural rework, never `|| retry`.
- **R-13 arm64** — arm64 is a never-before-run path for this smoke; first run watched as discovery.

> The validator (Gate 3c) must accept that AC-07 / R-05 cannot be green pre-merge and must
> NOT treat their "PENDING — post-tag" status as a gap. Conversely, R-01/R-02/R-03/R-09/R-04
> MUST be green pre-merge — they are the provable-now core and a PENDING there IS a gap.

---

## Integration Harness Plan (infra-001)

**Suite selection:** Per the suite-selection table, this feature touches **no server tool
logic, no MCP-visible behavior, no schema/storage** — it is workflow YAML + a bounded shell
edit. Therefore **none of the Python suites (`tools`/`protocol`/`lifecycle`/`security`/
`confidence`/`contradiction`/`edge_cases`/`volume`/`adaptation`) are in scope**, and **no
new Python suite test is added.** Adding a Python harness test here would be parallel
scaffolding for behavior that has no MCP surface — explicitly out of scope per the agent
definition ("Pure internal logic with no MCP-visible effect → unit tests suffice").

**Smoke minimum gate (`pytest -m smoke`):** Run once in Stage 3c as the standing
"any-change" regression baseline to prove the AC-05 edit to `docker-http-posture-smoke.sh`
did not perturb the broader harness — but note this smoke is the **MCP `pytest -m smoke`
subset**, distinct from the Docker HTTP-posture smoke this feature wires. The Docker smoke is
exercised directly via T3 (≥5 full runs with `IMAGE=` set).

**Cumulative extension, not new infra:** the only script change is the AC-05 grew-assertion
in the existing `docker-http-posture-smoke.sh` (one bounded pair, before the terminal marker,
via the existing `vol()` sidecar). The new gate-logic / tag-parity / graph tests live beside
it in `scripts/` and reuse the existing repo-root resolution and bash conventions. No new
suite, no new framework, no duplicated smoke logic in YAML (NFR-08 / C-12).

**New tests this feature adds (all shell/static, Stage 3c):** the T1/T2/T3/T4 tests
enumerated in the per-component files below. These are feature-specific gate-logic
assertions, not harness-infrastructure changes — so no GH Issue for infra enhancement is
needed (USAGE-PROTOCOL "Adding New Tests" §1: validates existing SCOPE ACs, added in the
feature PR).

---

## Cross-Component Test Dependencies

- `smoke-amd64.md` and `smoke-arm64.md` share **one** gate-logic test surface (the jobs are
  near-identical: same `run_smoke_gate` bytes, differ only in runner + `-<arch>` suffix). The
  per-arch suffix correctness is asserted once in T2 (`-amd64` vs `-arm64`, no swap). Both
  arches MUST be present and gating (NFR-06 HARD RULE) — neither is silently dropped.
- `create-container-manifest.md` depends on both smoke components existing in the `needs:`
  list; its dispatch green-skip (`if: github.event_name != 'workflow_dispatch'`) is asserted
  in T4 and confirmed post-dispatch.
- `docker-http-posture-smoke.md` (AC-05) must keep `[783-smoke] ALL GATES PASSED` as the
  **last** emitted line — the marker the gate (smoke-amd64/arm64 T1) keys on. R-12 ties the
  AC-05 component back to the gate-logic component: a marker moved off the end silently breaks
  T1's positive case in the real run.

---

## Failure Triage (Stage 3c)

Per USAGE-PROTOCOL: a failure in the **new** gate-logic/tag-parity/AC-05 tests is **this
feature's bug → fix now**. A failure in an **unrelated** existing infra-001 suite surfaced by
the `pytest -m smoke` baseline is **pre-existing → file a GH Issue, `xfail`, continue** —
never fixed in this PR. The Docker-smoke's own pre-existing assertions (gates 1–3) are
inherited (R-13); a genuine arm64-specific defect found post-dispatch is **in-scope rework**
for this feature, not a third-party xfail.
