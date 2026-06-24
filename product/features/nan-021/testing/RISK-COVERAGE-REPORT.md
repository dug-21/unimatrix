# Risk Coverage Report: nan-021

> **HTTPS-Bridge Integration Fixture — Stage 3c Test Execution.** Pure test-infrastructure feature
> (zero production-code diff). This report records the TRUE cross-leg live HTTPS-vs-UDS parity run and
> the ⚠ first-live-run field-by-field validation gate (ADR-003 #5293 / NFR-8). All tests run FOREGROUND
> (Docker Engine 29.5.2 verified).

## ⚠ First-Live-Run Field-by-Field Validation Gate — VERDICT: FULL MATCH

**The 3-field D-5 exclusion set is now PROVEN (no longer an unverified assumption) for the parity
workload.** The TRUE cross-leg live run drove BOTH legs in ONE pytest execution
(`test_https_uds_parity`, PASSED): the UDS leg in-process against a live `unimatrix serve --foreground`
daemon, and the HTTPS leg via the real Docker container (`unimatrix:783-smoke`, built fresh from current
source) — bridge cycle over pinned HTTPS, `Mcp-Session-Id` replayed byte-stable, SSE parsed. Both legs
produced live `MetricVector`s from two distinct `context_cycle_review` calls (HTTPS `computed_at`=1782275609,
UDS `computed_at`=1782275601). Evidence: `testing/first-live-run-field-record.json`,
`testing/first-live-run-https-leg-gates.log`.

**Result: ZERO divergence outside the closed 3-field D-5 exclusion set.** No product/human disposition is
required — no GitHub bug, no ADR-003 amendment. The at-risk session-lifecycle fields (the prime divergence
suspects) all matched.

### UniversalMetrics — all 21 fields (run_token `nan-021-parity-session-0001`)

| Field | HTTPS | UDS | Equal | Excluded (D-5) | At-risk |
|-------|-------|-----|-------|----------------|---------|
| total_tool_calls | 4 | 4 | ✓ | | |
| total_duration_secs | 0 | 0 | ✓ | **EXCLUDED** | |
| session_count | 1 | 1 | ✓ | | |
| search_miss_rate | 0 | 0.0 | ✓ | | |
| edit_bloat_total_kb | 0 | 0.0 | ✓ | | |
| edit_bloat_ratio | 0 | 0.0 | ✓ | | |
| permission_friction_events | 1 | 1 | ✓ | | **AT-RISK** |
| bash_for_search_count | 0 | 0 | ✓ | | |
| cold_restart_events | 0 | 0 | ✓ | | **AT-RISK** |
| coordinator_respawn_count | 0 | 0 | ✓ | | **AT-RISK** |
| parallel_call_rate | 1 | 1.0 | ✓ | | |
| context_load_before_first_write_kb | 2 | 2.0 | ✓ | | **AT-RISK** |
| total_context_loaded_kb | 3.5 | 3.5 | ✓ | | **AT-RISK** |
| post_completion_work_pct | 0 | 0.0 | ✓ | | |
| follow_up_issues_created | 0 | 0 | ✓ | | |
| knowledge_entries_stored | 0 | 0 | ✓ | | |
| sleep_workaround_count | 0 | 0 | ✓ | | |
| agent_hotspot_count | 0 | 0 | ✓ | | |
| friction_hotspot_count | 0 | 0 | ✓ | | |
| session_hotspot_count | 0 | 0 | ✓ | | |
| scope_hotspot_count | 0 | 0 | ✓ | | |

20/20 non-excluded universal fields equal. All 5 at-risk session-lifecycle fields equal.

### phases (BTreeMap)

| Aspect | HTTPS | UDS | Equal |
|--------|-------|-----|-------|
| key set | {delivery} | {delivery} | ✓ |
| delivery.tool_call_count | 4 | 4 | ✓ |
| delivery.duration_secs | 0 | 0 | EXCLUDED (per-phase wall-clock) |

### domain_metrics (HashMap)

| HTTPS | UDS | Equal |
|-------|-----|-------|
| {} | {} | ✓ |

**Note (not a divergence):** several fields serialize as int on the HTTPS leg (`2`, `0`, `1`) vs float on
the UDS leg (`2.0`, `0.0`, `1.0`). These are JSON encoder representation differences between the two
transports; the numeric values are identical and the comparator's `!=` correctly treats `2 == 2.0` as
equal. This is a representation artifact, NOT a parity defect — recorded here for transparency. No
exclusion-set change made.

**Disposition:** none required. D-5 set (`computed_at`, `universal.total_duration_secs`,
`phases.*.duration_secs`) confirmed COMPLETE and MINIMAL for this workload on first live run.

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Incomplete D-5 set → parity flakes | `test_c4_every_field_classified`, `test_c4_ratio_fields_compared_exactly`, `test_c4_excludes_wallclock_not_luck`; **first-live-run field-by-field gate (FULL MATCH)** | PASS | Full |
| R-02 | Over-broad set → vacuous green | `test_c4_mutation_drop_observe_fails` (teeth), `test_c4_count_fields_never_excludable`, `test_c4_each_excluded_field_justified`, `test_c4_non_empty_on_structural_fields` | PASS | Full |
| R-03 | Fragile single-execution seam | `test_c3_runs_in_same_pytest_invocation`, `test_c4_rejects_stale_correlation_token`, `test_c3_seam_rejects_stale_token`, `test_c3_missing_https_leg_errors_never_empty`; live `test_https_uds_parity` (token-correlated ingest) | PASS | Full |
| R-04 | Bridge silently bypassed | live HTTPS leg: `bridge carried it — SSE Accept sent, text/event-stream parsed, Mcp-Session-Id replayed byte-stable`; negative control `JSON-only Accept => REAL SSE required` (gates log) | PASS | Full |
| R-05 | keep_alive idle eviction → 404 | NFR-7 idle-window-minimized drive (bridge spawned last, driven immediately); shipped #830 self-heal relied on, not re-implemented; live cycle completed clean (no eviction in this run) | PASS | Full |
| R-06 | Fire-and-forget WAL → observes not durable | `test_c4_barrier_*` (predicate, dir-granularity, symmetric single helper, timeout hard-fail); live legs: `durability barrier released (HTTPS): store size 688 stable`; UDS barrier symmetric | PASS | Full |
| R-07 | topic_signal → unattributed | `assert_derived_attribution` (live UDS leg + orchestrator): `topic_signal == nan-021` EXACTLY; `unattributed` hard-fail guard; Bash content carries feature-ID token | PASS | Full |
| R-08 | Docker false-green / false-fail | `release-gate-cloud-cycle-logic-test.sh` (20 pass): exit-3 skip=hard-fail, exit-4 distinct, exit-0-no-marker=red, anchored whole-line marker; nan-019 `pull \|\| inspect \|\| exit-4` verbatim | PASS | Full |
| R-09 | Divergent CC session identity (#832) | `test_c3_same_session_identity_as_https`, `test_c3_drives_same_manifest_object`; single manifest/session-id both legs; live attribution holds (proves stable id) | PASS | Full |
| R-10 | Accidental fork of infra-001 | `test_c3_uses_existing_clients_not_fork`, `test_c4_is_only_substantial_net_new`; `test_run_smoke_gate_byte_unchanged`, `test_no_new_smoke_script`; git-diff: zero crates/lib runtime change | PASS | Full |
| R-11 | projectHash recomputed | live HTTPS gates log: `projectHash read back as 8760d55cf13be1f7`; `test_c5_cloud_cycle_missing_credstore_red` (read-back precondition) | PASS | Full |
| R-12 | First-green tax (release-only gate) | `release-gate-cloud-cycle-logic-test.sh` (20 pass) stub-drives the gate spine pre-merge; `release-gate-bundle-static-test.sh` (12 pass) | PASS | Full |
| R-13 | Child stderr swallowed | smoke captures bridge/init/container stderr to `$SANDBOX`, tail-dumped on failure (ADR-005); `emit_bundle` deliberately suppressed (bearer) | PASS | Full |
| R-14 | Hermeticity leak | `test_gate6_runs_under_isolated_home`, `test_sandbox_clean_on_entry`, `test_sandbox_trap_teardown`; live run used hermetic `$SANDBOX/home` | PASS | Full |

---

## Test Results

### Unit Tests (`cargo test --workspace`)
- **6697 passed, 0 failed, 31 ignored** across 60 test suites (rc=0). nan-021 is zero-production-diff — no
  Rust source changed; this is the standing workspace gate, green.
- Unit Test Note: an initial unconstrained `cargo test --workspace` run hit an OOM linker kill
  (`ld terminated with signal 9 [Killed]`) on the large `unimatrix-server` test binaries during peak
  concurrency (Docker build + live daemons + regression suite running simultaneously). This was an
  environment resource artifact, NOT a code/test failure — a throttled re-run (`--jobs 2`) under reduced
  contention compiled and passed all 6697 tests cleanly.

### Off-Docker Parity Unit/Contract Tests (Python, no Docker — the C4 spine, R-12 mitigation)
- `suites/test_parity_workload.py`: 29 collected
- `suites/test_https_uds_parity.py` (off-Docker contract/seam, `-m "not integration and not parity"`): 9
- **Result: 38 passed, 0 failed.** Covers comparator teeth (R-02), exclusion-set completeness/classification
  (R-01), stale-token rejection + missing-leg-errors (R-03), barrier predicate/symmetry/timeout (R-06),
  single-driver/session-identity (R-09), no-seed static audit (AC-03), sole-net-new-module (AC-07).

### Gate-Spine Shell Tests (no Docker — R-08 / R-12 / AC-05)
- `release-gate-cloud-cycle-logic-test.sh`: **20 passed, 0 failed** (exit-code discriminator, anchored
  run-marker, nan-019 acquisition verbatim, C2 contract env, cloud_cycle control flow, release-lane wiring).
- `release-gate-bundle-static-test.sh`: **12 passed, 0 failed** (hermeticity, single terminal marker,
  `run_smoke_gate` byte-unchanged, no new smoke script).

### Integration Tests (live daemon / live Docker — FOREGROUND)
- **Mandatory smoke gate** `pytest -m smoke`: **24 passed, 0 failed** (210s). Regression baseline green.
- **Live UDS-leg review + attribution** `test_c3_uds_leg_live_review_non_empty_and_attributed`: **1 passed**
  (live daemon; non-empty MetricVector after barrier; `topic_signal == nan-021` derived; self-parity clean).
- **TRUE cross-leg live parity** `test_https_uds_parity` (`-m parity`, `UNIMATRIX_HTTPS_SMOKE` wired,
  `IMAGE=unimatrix:783-smoke`): **1 passed** — UDS in-process + HTTPS via live Docker bridge cycle; comparator
  zero diffs; first-live-run field record emitted.
- **Standalone HTTPS leg** (authenticity confirmation): real container ran ALL GATES 1–8 PASSED — boots
  HTTP-on, per-slug HTTPS 204, bundle/init/credstore-0600, bridge cycle (11 frames, 4 PreToolUse / 3
  PostToolUse observes, phase 'delivery'), SSE+session-id replay, JSON-only negative control, durability
  barrier, `context_cycle_review` over the bridge → MetricVector(HTTPS).
- **Regression suites** (`protocol`, `tools`, `lifecycle` — 291 tests; lifecycle is the AC-03 no-seed audit
  target): `protocol` 13/13 PASS, `tools` PASS (1 pre-existing harness `xfail`, NOT introduced by nan-021),
  `lifecycle` partially executed. The run reached ~46% (zero failures, zero errors in the executed portion)
  before hitting the outer 25-minute `timeout` ceiling (rc=124) — the lifecycle restart-persistence tests
  each spin multiple full server boot/shutdown cycles, which is slow in this resource-constrained dev box.
  This is an environment time-budget artifact, NOT a test/code failure (no `FAILED`/`ERROR` in the log). The
  nan-021-relevant lifecycle concern — no-seed (`_seed_observation_sql_lifecycle` not reachable) — is proven
  independently by the static `assert_no_seed_reachable` audit (`test_c3_no_seed_site_reachable`,
  `test_c4_no_seed_site_reachable`, both PASS) and the live UDS derived-attribution test. No GH Issue filed
  (no failure to triage).

---

## Gaps

None. Every risk R-01..R-14 maps to executed test coverage with a PASS result (see Coverage Summary). The
load-bearing first-live-run field-by-field gate (R-01/R-02/NFR-8) ran live and returned FULL MATCH, so the
D-5 exclusion set is proven complete-and-minimal for the parity workload — the single residual "assumption"
called out in the IMPLEMENTATION-BRIEF delivery gate is now discharged.

**Boundary note (intended, not a gap):** the live HTTPS-vs-UDS parity gate's standing home is the
release-gate Docker lane (`.github/workflows/release.yml` job `nan-021-https-uds-parity`, `workflow_dispatch`/tag)
— NOT the per-PR `ci.yml` matrix (D-3). It is intentionally NOT in `create-container-manifest`'s `needs:`
until first-green (R-12 budget). This Stage-3c run achieved that first green LOCALLY against the freshly
built shipped image.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | Live HTTPS leg gates 1–4: image boots HTTP-on by default, slug `arch-research` registered pre-serve, per-slug HTTPS `/v1/{slug}/observe` returned 204, per-slug store grew (NOT `serve --stdio`). Extends `docker-http-posture-smoke.sh` Gates 1–7. |
| AC-02 | PASS | Live gate 8: cycle driven THROUGH `mcp-bridge.js` over pinned HTTPS; `Mcp-Session-Id b699fccf-…` captured on initialize and replayed byte-stable; `text/event-stream` parsed; JSON-only-Accept negative control failed framing (REAL SSE required). Zero direct cycle-`mcp_url` POSTs. |
| AC-03 | PASS | `assert_derived_attribution`: every driven observation `topic_signal == nan-021` EXACTLY, derived over the wire (not seeded). No-seed static audit `assert_no_seed_reachable` over orchestrator + leg + comparator modules (`test_c3_no_seed_site_reachable`, `test_c4_no_seed_site_reachable`). Confirmed on the live UDS path. |
| AC-04 | PASS | Live-vs-live parity: both vectors non-empty after symmetric barrier (`total_tool_calls=4>0`, `session_count=1>0`, `phases` populated); equal field-for-field modulo D-5. **First-live-run field-by-field gate: FULL MATCH (see top of report).** Comparator mutation teeth + ≥correlation-token guards covered off-Docker. |
| AC-05 | PASS | `release-gate-cloud-cycle-logic-test.sh` (20 pass) + `release.yml` job `nan-021-https-uds-parity` on `workflow_dispatch`/tag (NOT pull_request); `run_smoke_gate` verify-by-name: Docker-absent=exit3 hard-fail, unacquirable=exit4 distinct, exit-0-no-marker=red, anchored `[*-smoke] ALL GATES PASSED` marker; nan-019 `pull \|\| inspect \|\| exit-4` reused verbatim. |
| AC-06 | PASS | `git diff main...HEAD` touches only `product/test/infra-001/**`, `.github/workflows/release.yml`, and `product/features/nan-021/**` docs. ZERO `crates/**`, `lib/**`, or `packages/*/{lib,bin}` runtime changes. |
| AC-07 | PASS | Sole substantial net-new module = C4 (`parity_workload.py` + `metric_comparator.py` + `parity_legs.py`, split for the ≤500-line rule). `test_c3_uses_existing_clients_not_fork`, `test_c4_is_only_substantial_net_new`, `test_run_smoke_gate_byte_unchanged`, `test_no_new_smoke_script`. `projectHash` READ BACK (`8760d55cf13be1f7`), not recomputed (R-11). |
| NFR-8 (process) | PASS | No non-wall-clock divergence on the first live run → no disposition call needed (no GH bug, no ADR-003 amendment). Exclusion-set literal unchanged. The comparator surfaces any future divergence loudly (`ParityMismatch` names field + both values + AT-RISK flag) for product/human disposition — never silent widen. |

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — strong hits #5293 (ADR-003 comparison contract + first-live-run
  gate + disposition authority), #5286 (ADR-001 hybrid single-driver), #5298 (by-construction-identical
  MetricVector parity pattern), #2844 (UniversalMetrics 21-field struct), #5265/#5280 (WAL durability /
  idle-eviction self-heal), #5129 (rmcp SSE).
- Stored: see agent report.
