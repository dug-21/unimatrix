# Risk Coverage Report: nan-020 — Product Documentation Currency (Doc-Test Enforcement)

> Stage 3c execution, 2026-06-20. nan-020 ships **NO Rust/JS application code** — it is a
> release-smoke extension (bash Gates 5–7 + hermeticity sandbox), `release.yml` YAML
> (`setup-node@v4` pin), doc rewrites (`docs/client-setup.md`, `README.md`), and one agent-def
> edit (`uni-docs.md`). The load-bearing coverage is the **stub-driven shell gate-logic suites**
> (#5189 pre-merge-provable approach). Per the Stage 3a plan the infra-001 **Python integration
> suites do not apply** (no MCP-visible behavior change); `pytest -m smoke` is run only as a
> no-regression baseline.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| **R-01** | New bundle-attach failure greens the gate (silent false-pass) — CRITICAL | `test_gate567_happy_path_exit0`, `test_gate5_emit_rc_nonzero_fails`, `test_gate5_empty_blob_fails`, `test_gate5_wrong_prefix_blob_fails`, `test_gate6_init_rc_nonzero_fails`, `test_gate7_observe_non204_fails`, `test_gate7_store_no_grow_fails` (bundle-logic) | PASS | Full (every ADR-001 row → fail()/exit-1; happy path is the only exit-0) |
| R-02 | Mis-attributed failure (distinctness is sole attributability) — HIGH | `test_msg_emit_vs_attach_distinct_emit`, `test_msg_emit_vs_attach_distinct_attach`; per-row distinct messages asserted across the truth table | PASS | Full |
| **R-03** | Regress nan-019 Gates 1–4 / load-bearing contract — CRITICAL | nan-019 regression suite `release-gate-logic-test.sh` (14/14, truth table {0,1,3,4} invariant); `test_run_smoke_gate_byte_unchanged` (sha256 == HEAD); `test_append_only_ordering`; `test_single_terminal_marker`; `test_marker_suppressed_on_failure_red` | PASS | Full (invariance proven; wrapper byte-unchanged) |
| R-04 | Host node absent / JS version drift — HIGH | `test_node_absent_hard_fails_exit1` (exit 1, NOT 3, distinct msg); `test_gate6_invokes_repo_checkout_client`; `test_setup_node_present_both_smoke_jobs`, `test_setup_node_version_pinned_24`, `test_setup_node_ordering` | PASS | Full pre-merge (live node version-compat POST-TAG) |
| R-05 | Bundle blob handoff corruption (container→host) — HIGH | `test_capture_stdout_only_not_stderr`, `test_stdout_only_no_token_leak`, `test_blob_quoting_safe`, `test_gate5_empty_blob_fails`, `test_gate5_wrong_prefix_blob_fails` | PASS | Full pre-merge (real blob POST-TAG) |
| R-06 | `client-bundle` rename/absence in shipped image — MEDIUM | `test_gate5_emit_rc_nonzero_fails` (names `client-bundle`); `test_gate6_invokes_repo_checkout_client` (pinned invocation, static) | PASS | Full rename-detection (live image-presence POST-TAG) |
| **R-07** | Non-hermetic CI / stale-credstore false-green — CRITICAL | **REQUIRED negative control** `test_hermeticity_negative_control_still_red` (poison stale cred + broken attach → STILL exit 1); `test_hermeticity_discrimination_unisolated_would_green` (proves the control non-vacuous, delta 0→1); `test_hermeticity_positive_twin_run`; `test_store_grew_non_flaky` (5/5); static: `test_gate6_runs_under_isolated_home`, `test_no_inprocess_home_mutation`, `test_sandbox_clean_on_entry`, `test_sandbox_trap_teardown` | PASS | Full pre-merge (negative control proven non-vacuous; live round-trip POST-TAG) |
| R-08 | Executable-claim classification rots (#768 class) — HIGH | AC-01 greps (zero 501/W2-7/curl-observe; positive `init --bundle`+`/v1/{slug}/observe`); canonical-chain == Gates 5/6/7 (claim-for-claim) | PASS | Full (grep + chain conformance) |
| R-09 | `--slug` on bundle path the CLI rejects — HIGH | AC-02 grep: zero `--slug` paired with `--bundle` in both files; Gate 6 invokes `init --bundle` no `--slug` | PASS | Full |
| R-10 | README multi-occurrence miss (OQ-B) — HIGH | AC-02 regex grep (multi-occurrence, not line-pinned): zero `init --remote unimatrix-bundle:` AND zero `init --remote <bundle>` bundle-fed forms; canonical present in both | PASS | Full |
| R-11 | Gate logic un-provable pre-merge → swallow ships — HIGH | Sourceable-spine reuse (both suites `source` the shipped `docker-http-posture-smoke.sh` / `release-gate-lib.sh` — never re-typed); RC-survival by execution; `test_store_grew_non_flaky` (no `\|\| retry`, 5/5) + discrimination twin | PASS | Full pre-merge |
| R-12 | AC-01 grep passes while drift remains — MEDIUM | AC-01 literal greps + obsolete-model sweep (no curl-hook telemetry prose) | PASS | Full |
| R-13 | uni-docs remit scope creep / blast radius — MEDIUM | AC-07 grep: `docs/` widening (18×), blast-radius operational definition, full-tree-audit NON-GOAL stated, narrow source-read relaxation, retained injection defense + "document only what is shipped", **Feature-2 fence** (drift-checker/CI-gate/Phase-4-trigger explicitly forbidden, lines 134/163) | PASS | Full (inspection) |
| R-14 | N5 / remit text human-owned, no automated coverage — LOW | Inspection (AC-08) — human-owned by design (C-3); not machine-checked | N/A (human-owned) | By design |
| R-15 | Silent second image build / divergent boot — LOW | `test_no_new_smoke_script` (exactly one smoke script); reuse-single-boot (static) | PASS | Full |
| R-16 | Legacy `--remote` documented-but-NOT-doc-tested — **ACCEPTED RESIDUAL** | AC-02 grep: `--remote <url> --token` present AND marked **"legacy"** in both files (the sole owed mitigation) | PASS (mitigation only) | Accepted gap — no round-trip coverage owed |

## Test Results

### Unit / Stub-Driven Shell Gate-Logic Tests (the load-bearing level)

| Suite | Result |
|-------|--------|
| `release-gate-bundle-logic-test.sh` (Gate 5–7 truth table + R-07 negative control) | **19 passed, 0 failed** (exit 0) |
| `release-gate-bundle-static-test.sh` (source/YAML grep: isolation, ordering, marker, byte-unchanged, setup-node, no-new-script) | **12 passed, 0 failed** (exit 0) |
| `release-gate-logic-test.sh` (nan-019 regression — R-03 invariance) | **14 passed, 0 failed** (exit 0) |
| **Total shell gate-logic** | **45 passed, 0 failed** |

- The **REQUIRED hermeticity negative control (R-07 / AC-09)** — `test_hermeticity_negative_control_still_red` — is present and **green**, proven non-vacuous by its discrimination twin (`test_hermeticity_discrimination_unisolated_would_green`, delta 0→1: a non-isolated run WOULD false-green). Classifying it PENDING would have been a gap (#5189); it is proven pre-merge here.
- Sourceable-spine integrity (R-11): both nan-020 suites `source` the shipped bytes; no paraphrased gate copy.

### Cargo Workspace Baseline (no-regression)

`cargo test --workspace` did **NOT complete**: `ld terminated with signal 9 [Killed]` (linker OOM-killed
by the host) during link of `unimatrix-server`. This is an **environment memory-exhaustion** constraint,
NOT a compilation/test-logic failure and **NOT attributable to nan-020** (which ships zero Rust code —
no `crates/` or `src/` files modified). The shipped release binary already exists
(`target/release/unimatrix`, built 16:10) and the integration smoke ran against it successfully. Recorded
as an environmental baseline limitation, not a feature defect or a regression.

### Integration Tests (infra-001 Python suites)

Per the Stage 3a plan, the Python suites **do not apply** — nan-020 changes no application code, tool,
route, or schema, so there is no MCP-visible behavior to validate. `pytest -m smoke` was run **only as a
no-regression baseline**:

| Run | Result |
|-----|--------|
| `pytest suites/ -m smoke --timeout=60` (full smoke pass) | 23 passed, 382 deselected, **1 error** in 239.5s |
| `test_contradiction_detected` re-run in isolation | **PASSED in 8.29s** |

- The single error (`test_contradiction.py::test_contradiction_detected`) was a **server-init timeout**
  (10s ready-wait exceeded) — the same resource-starvation class that OOM-killed the cargo linker. In
  isolation the test passes in 8.29s, confirming a **cold-start/resource flake under parallel load**, not
  a behavioral failure and not caused by nan-020 (zero server-code change).
- **Triage (per USAGE-PROTOCOL decision tree):** not in code nan-020 changed; not a pre-existing *code*
  bug (passes on retry); not a bad assertion → environmental flake. **No GH Issue filed, no `xfail`
  added.** Effective smoke result: 24/24 behavioral pass. The binary and MCP surface are untouched and
  healthy.
- Suites NOT run (correctly, not a gap): `tools`, `lifecycle`, `confidence`, `security`,
  `contradiction` (full), `volume`, `protocol`, `edge_cases`, `adaptation` — nan-020 touches none of
  their surfaces.

## Gaps

**No coverage gaps.** Every pre-merge-provable risk (R-01, R-03, R-07, R-11 and the High/Medium
gate-logic + doc-grep risks) is green pre-merge. The R-07 negative control — the one item whose PENDING
status would constitute a gap (#5189) — is proven here.

### PENDING-post-tag (accepted, NOT a gap)

The live container round-trip is **POST-TAG-CONFIRMABLE** — configured + locally verifiable against
stubs; GH execution confirmed only when the release tag fires:

- Real `client-bundle` emit from the **actual shipped image** (R-06 live image-presence).
- Live host `init --bundle` decode/pin/Ping against the **real container** (R-04 version-compat, R-05
  real blob).
- The **live** hermetic round-trip landing a real write in the per-slug store (R-07 live half; C15
  MCP-round-trip + observe-landing legs).

This is explicitly accepted per #5189/#4796 — the gate *logic* and the hermeticity sentinel are proven
correct + non-vacuous pre-merge against stubs; only the live wall-clock execution awaits the tag. C15
stays `partial` until the post-tag live run greens both legs.

### Accepted residual

- **R-16 / AG-1** — legacy `--remote` mode is documented-but-NOT-doc-tested by conscious design. The
  sole owed mitigation (the inspectable **"legacy" marker** in README + `docs/client-setup.md`) is
  present and verified under AC-02. No `--remote` round-trip coverage is owed.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | **PASS** | `grep -cE '501\|W2-7' docs/client-setup.md` → 0; no `curl .*/observe` fenced block; `init --bundle` ×4, `/v1/{slug}/observe` ×3 present; obsolete curl-hook telemetry prose gone |
| AC-02 | **PASS** | Both files: `init --bundle <blob>` present (client-setup ×4, README ×7); `--remote <url> --token` present (×3 each) + marked **"legacy"** (×4 each); zero `init --remote unimatrix-bundle:`; zero `init --remote <bundle>` bundle-fed forms; zero `--slug` paired with `--bundle` |
| AC-03 | **PASS (gate-logic) / PENDING (live)** | Gate 5–7 stub truth table green (`release-gate-bundle-logic-test.sh` 19/19); live container round-trip POST-TAG-CONFIRMABLE |
| AC-04 | **PASS** | `test_no_new_smoke_script` (exactly one smoke script); `test_append_only_ordering` (round-trip inside `docker-http-posture-smoke.sh`); no new CI job |
| AC-05 | **PASS** | Docker-absent exit-3 hard-fail preserved (nan-019 `test_gate_skip_exit3_hard_fail`); each new skip path → distinct `fail()` exit 1 (node-absent, emit-fail, empty blob, observe non-204, store-no-grow) |
| AC-06 | **PASS** | `test_marker_suppressed_on_failure_red` (forced failure → marker NOT printed → run_smoke_gate RED); `test_single_terminal_marker` (exactly one, last line); nan-019 `test_gate_early_exit0_marker_absent` |
| AC-07 | **PASS** | uni-docs.md: `docs/` widening (×18), blast-radius operational definition + full-tree-audit NON-GOAL, narrow source-read relaxation, retained prompt-injection defense + "document only what is shipped", Feature-2 fence (drift-checker/CI-gate/Phase-4-trigger forbidden); release.yml `setup-node@v4` ×3 + `node-version: '24'` ×3 on both smoke jobs, ordered checkout < setup-node < gate |
| AC-08 | **PASS (inspection, human-owned)** | N5 "usable-as-documented" framing bound to `--bundle` chain; doc-test named as docs-layer guard; no new NFR minted (R-14, human-owned by design) |
| **AC-09** | **PASS** | **REQUIRED pre-merge negative control green** (`test_hermeticity_negative_control_still_red`); discrimination twin proves non-vacuous (delta 0→1); clean-on-entry + trap teardown + process-boundary HOME isolation (no in-process mutation) all asserted static; fresh-write delta proven (live round-trip POST-TAG) |

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5180/#5183/#5189/#5192 (self-skip→hard-fail, verify-by-name contract, pre-merge-provable shell-gate plan, sourceable-spine), #4977 (vacuous-pass / assert non-skip), #840 (USAGE-PROTOCOL), plus delivery-process lessons on test-named-but-not-implemented (#4202/#2656/#3548). All applied: confirmed the load-bearing stub-driven suites exist + green, verified the R-07 negative control is non-vacuous (not just present), and triaged the cargo/smoke resource failures against the "named test actually runs" discipline.
- Stored: nothing novel — the patterns exercised here (stub-driven self-skipping CI gate must hard-fail; negative control proves a hermeticity sentinel non-vacuous; Rust-2024 process-boundary isolation) are already captured at #5180/#5183/#5189/#5192/#4977 and vnc-041's AC-06/AC-02. nan-020 is a faithful reuse, not a new cross-feature pattern; re-storing would duplicate provenance.
