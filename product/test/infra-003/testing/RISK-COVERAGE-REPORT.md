# Risk Coverage Report: infra-003

> Stage 3c execution of the standalone bidirectional multi-tenant HTTP isolation
> smoke gate (`multi-tenant-isolation-smoke.sh`, #853) and its tier-1 off-Docker
> gate-logic test. Test-only, SHELL deliverable; no `crates/` change (AC-13
> confirmed by `git diff`). GH issue #853.
>
> Two-tier strategy executed exactly as planned in `test-plan/OVERVIEW.md`:
> **(tier 1)** off-Docker stub-driven gate-logic test = the load-bearing teeth
> proof (a smoke gate's dominant failure is the false-GREEN/vacuous pass);
> **(tier 2)** the live Docker leg = the point-in-time bidirectional 2×2 property
> proof. Both observed. The R-15 #815 invariant update was executed with teeth
> re-verified. The infra-001 pytest `-m smoke` set was run as the unrelated
> regression baseline.

## Execution Summary (with exit codes)

| Leg | Command | Result | Exit |
|-----|---------|--------|------|
| Tier-1 gate-logic test (MANDATORY) | `release-gate-isolation-logic-test.sh` | **PASS** — 25/25 | 0 |
| R-15 #815 invariant test (MANDATORY) | `release-gate-bundle-static-test.sh` | **PASS** — 12/12 | 0 |
| R-15 teeth (negative controls) | synthetic fork `*smoke*.sh` + known-script removal | **PASS** — both trip RED | 1, 1 |
| Live Docker leg (integration smoke) | `multi-tenant-isolation-smoke.sh` (IMAGE=unimatrix:783-smoke) | **GREEN observed** — "ALL GATES PASSED" full 2×2 both surfaces | 0 |
| Live Docker leg — tri-state discrimination | same gate, 2 prior runs | **INFRA observed** (transient MCP warmup race) — never RED, never false-GREEN | 2 |
| Unrelated regression baseline | `pytest suites/ -m smoke --timeout=60` | **PASS** — 24/24 | 0 |

Tri-state exit contract observed live and in tier-1: GREEN=0 / RED=1 / INFRA=2 /
SKIP=3, all distinct, no non-GREEN rounds to 0 (R-10/#5180).

## Live-Leg Disposition (explicit, honest)

The live Docker leg was **runnable in this environment** (docker present, info OK;
sqlite3/node/busybox/curl present) and a **full GREEN run was directly observed**:

```
[infra003-smoke] observe: A has obs-a not obs-b; B has obs-b not obs-a => observe GREEN
[infra003-smoke] mcp: A has mcp-a not mcp-b; B has mcp-b not mcp-a => mcp GREEN
[infra003-smoke] point-in-time proof only: advances N3 (#5161), does not close it (N5/#788 lane unwired).
[infra003-smoke] ALL GATES PASSED — bidirectional 2x2 isolation holds on both surfaces.   (exit 0)
```

All four positive controls reached PRESENT in their own store (obs-a→A, obs-b→B,
mcp-a→A, mcp-b→B), and all four cross-cells read absent — the load-bearing
B-direction included. Each MCP route used its **own** `Mcp-Session-Id`
(distinct UUIDs logged per slug; never crossed — R-17/AC-15).

**Caveat (image provenance):** the GREEN was observed against the prebuilt
distroless image `unimatrix:783-smoke` (server v0.8.9, `UNIMATRIX_HTTP_ENABLED=true`,
`project routing active slug_count=2`), not a fresh build from HEAD's Dockerfile.
Because infra-003 is test-only with **zero `crates/` change** (AC-13 verified), the
prebuilt image's server binary is representative of HEAD production behavior. A
fresh `docker build` GREEN is the gold standard for a release run and is deferred
to the Docker-capable CI lane.

**Observed live tri-state discrimination (gate behaving correctly):** on 2 earlier
runs the gate exited **INFRA (2)** at the first MCP `context_store` after restart —
a transient embedding-model warmup race (the first `entries` write races HNSW/model
readiness). The gate correctly classified this as INFRA ("durability not
established"), **never RED and never a false-GREEN**, and never reported the
cross-store cleanliness as a pass. Root cause was confirmed by reproducing the exact
gate code path standalone (succeeds once the model is warm) and by an instrumented
run that dumped a clean `isError:false` `context_store` SSE result and reached full
GREEN. This is the gate's tri-state discipline working as designed under a real
async-readiness window — not a correctness defect and not a server bug.

## Coverage Summary — all 18 risks

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | MCP streamable-HTTP handshake mis-built (×2 dir); failure = INFRA | live handshake both routes (own SID, SSE Accept); `test_c4_accept_advertises_sse`; lib `mcp_handshake` INFRA-on-no-session | PASS | Full |
| R-02 | Load-bearing MCP probe passes vacuously | live: both MCP positives PRESENT via genuine `entries` content read; `test_c7_planted_leak_mcp_*` (MCP cells → RED) | PASS | Full |
| R-03 | Positive-gates-negative inversion (4 positives) | `test_c5_own_timeout_is_infra_not_red`; `negative_cell` SKIPPED-on-INFRA path; live GREEN gated on PRESENT | PASS | Full |
| R-04 | WAL pre-checkpoint false-empty (both stores) | `read_marker` copies db+`-wal`+`-shm` (inspection); live positives durable; `test_c6_missing_db_is_infra` | PASS | Full |
| R-05 | Durability-barrier soundness (read-as-barrier) | `test_c5_positive_is_retry_until_present` (≥3 polls→PRESENT); `test_c5_own_timeout_is_infra_not_red`; live INFRA on warmup race (own-timeout→INFRA) | PASS | Full |
| R-06 | Read-dependency absent → empty-pass | `test_c1_sqlite3_absent_is_infra`; `test_c1_no_warn_continue`; live preflight OK | PASS | Full |
| R-07 | Liveness-as-verdict / missing-B INFRA | `test_c6_missing_db_is_infra`; live "non-404 != isolated" precondition logs; per-slug db existence asserted | PASS | Full |
| R-08 | Stale store / non-unique markers (×4) | `test_c7_real_markers_are_non_substring`; per-run nonce in `derive_markers`; live markers carry PID+ts nonce | PASS | Full |
| R-09 | Marker round-trip into column | live: obs→`observations.topic_signal`, mcp→`entries.content`/`topic` both PRESENT (one read = mapping proof + barrier) | PASS | Full |
| R-10 | INFRA/RED/GREEN tri-state collapse | `test_c7_tristate_exit_codes` (0/1/2 distinct); `test_c1_docker_absent_skips_exit3` (SKIP=3); live exits 0 and 2 observed distinct | PASS | Full |
| R-11 | Slug-B literal collision (resolved by design) | `SLUG_B=isolation-b` literal; live fresh per-run volume; nonce isolates runs | PASS | Full |
| R-12 | Marker SQL/LIKE metacharacters | `derive_markers` rejects non-`[a-z0-9-]`; `assert_markers_distinct` charset gate | PASS | Full |
| R-13 | Cumulative coupling to posture-smoke libs | separate top-level script, self-contained assertions; `release-gate-lib.sh` byte-unchanged (`test_run_smoke_gate_byte_unchanged`) | PASS | Full |
| R-14 | Overclaim / parity reintroduction | `test_no_overclaim_point_in_time` ("does not close N3", no parity/UDS); live "advances N3, does not close it" log | PASS | Full |
| R-15 | New-smoke-script invariant trip (#815) | `release-gate-bundle-static-test.sh` 12/12 incl. `test_no_new_smoke_script`; **teeth re-verified** (fork→RED, removal→RED) | PASS | Full |
| R-16 | Standing-gate orphan (#788) | delivery-coordination action (durable #788 adoption linkage) — leader-owned; gate emits point-in-time wording | N/A (delivery action) | Tracked |
| R-17 | Crossed/reused `Mcp-Session-Id` | `test_c4_session_captured_per_route` (distinct SID_A/SID_B, "never crossed"); live: distinct UUID per slug logged | PASS | Full |
| R-18 | Marker substring collision under `LIKE` | `test_c7_substring_markers_fail_infra` (violation→INFRA); `test_c7_real_markers_are_non_substring`; live "pairwise non-substring PASS" | PASS | Full |

All 18 risks covered. Every Critical/High risk has a named teeth or
INFRA-discrimination test that was executed and passed. R-16 is a
delivery-coordination obligation (not in-gate logic), owned by the leader.

## Test Results

### Unit / Tier-1 (off-Docker, deterministic — the teeth proof)
- `release-gate-isolation-logic-test.sh`: **25 passed, 0 failed** (exit 0)
  - Planted-leak teeth: 4/4 directions (mcp-b→A, mcp-a→B, obs-b→A, obs-a→B) → RED (exit 1)
  - Own-store read-as-barrier timeout → INFRA (exit 2), never RED, never GREEN
  - RED dominates INFRA (leak surfaces RED even when own positive timed out)
  - Missing main db → INFRA; retry-until-present (≥3 polls then PRESENT)
  - Tri-state exit codes distinct (GREEN 0 / RED 1 / INFRA 2 / SKIP 3)
  - Non-substring self-check fails LOUD (INFRA) on violation
- `release-gate-bundle-static-test.sh` (R-15 #815): **12 passed, 0 failed** (exit 0)
  - Teeth re-verified out of band: synthetic unaccounted `*smoke*.sh` → RED;
    removal of a known smoke → RED (closed-set discipline intact)

### Integration (live Docker)
- Live isolation gate: **1 full GREEN observed** (exit 0, "ALL GATES PASSED",
  full bidirectional 2×2 on observe + MCP) + **2 INFRA observed** (exit 2,
  transient MCP warmup race, correct tri-state)
- infra-001 pytest `-m smoke` regression baseline: **24 passed, 0 failed**
  (exit 0, 207.7s)

## Gaps

None. All 18 risks mapped to executed tests; all 15 ACs verified (below). No risk
lacks coverage. R-16 (standing-gate orphan) is a leader-owned delivery-coordination
action (durable #788 adoption linkage), explicitly out of in-gate logic scope.

## xfail / GH Issues filed

None. No genuine integration failure was encountered. The live INFRA runs were the
gate's correct tri-state degradation under a transient async-readiness window (not a
RED isolation failure, not a pre-existing server bug). No test was marked `xfail`;
no test was deleted or commented out.

**Non-blocking robustness recommendation (not a defect):** to make the live leg
deterministically GREEN rather than occasionally INFRA, the gate could add a warmup
barrier before the load-bearing writes — e.g. wait for `embedding model loaded
successfully` in `docker logs`, or issue one discard `context_store` and poll until
it succeeds, before the marked MCP writes. Optional follow-up; the current INFRA
behavior is correct and never produces a false verdict.

## Acceptance Criteria Verification (AC-01…AC-15)

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | Live: both slugs registered before single restart; all 4 routes non-404 ("PASS C2"); recorded as precondition only |
| AC-02 | PASS | Live: "observe write arch-research accepted (204)" + isolation-b (204) |
| AC-03 | PASS | Live: both observe positive controls PRESENT via marker-keyed retry-until-present read |
| AC-04 | PASS | Live: "A has obs-a not obs-b; B has obs-b not obs-a"; tier-1 planted-leak obs (4 dir) → RED |
| AC-05 | PASS | Tier-1 `test_c5_own_timeout_is_infra_not_red` + `negative_cell` SKIPPED-on-INFRA (no vacuous GREEN) |
| AC-06 | PASS | Live: both MCP writes succeeded (JSON-RPC `isError:false`, own session per route) |
| AC-07 | PASS | Live: both MCP positive controls PRESENT via `content LIKE … OR topic =` read-as-barrier |
| AC-08 | PASS | Live: "A has mcp-a not mcp-b; B has mcp-b not mcp-a"; tier-1 planted-leak mcp (4 dir) → RED |
| AC-09 | PASS | Tier-1 MCP planted-leak teeth → RED; positive-gates-negative per direction |
| AC-10 | PASS | Tier-1 `test_c5_barrier_is_read_marker_not_store_size` (no aggregate `store_size` barrier); retry-until-present |
| AC-11 | PASS | Tier-1 `test_c1_sqlite3_absent_is_infra`; `read_marker` copies `-wal`/`-shm` with each db |
| AC-12 | PASS | Inspection: each read via `vol` sidecar on that slug's `unimatrix.db`; one bearer token, slug-in-path; no cross-credential read |
| AC-13 | PASS | `git diff main...HEAD`: **no `crates/` change**; all changes under `product/test/` |
| AC-14 | PASS | Tier-1 `test_no_overclaim_point_in_time` (no parity/UDS re-run); ADR-006 referenced only |
| AC-15 | PASS | Tier-1 `test_c4_session_captured_per_route` (distinct SID_A/SID_B, never crossed); live: distinct `Mcp-Session-Id` UUID per slug |

All 15 acceptance criteria PASS.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced the nan-019/nan-020
  stub-driven Docker-smoke precedent (#5258/#5192), verify-by-name/exit-code
  contract (#5183/#5180), and the infra-003 ADRs (#5335). Applied directly:
  the tier-1 sourced-bytes gate-logic test and the live tri-state triage.
- Stored: nothing novel to store -- the patterns exercised (sourced-bytes
  gate-logic test, verify-by-name marker, tri-state INFRA discrimination) are
  already in Unimatrix (#5192/#5258/#5183). The live embedding-warmup-race →
  INFRA observation is a gate-robustness note captured in this report, not a
  reusable cross-feature pattern.
