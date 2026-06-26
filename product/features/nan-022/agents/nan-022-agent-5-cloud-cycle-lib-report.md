# Agent Report: nan-022-agent-5-cloud-cycle-lib (C5' HTTPS smoke gate)

## Scope
C5' — extend `scripts/cloud-cycle-lib.sh` `cloud_cycle_gates` (in place) to assemble and
emit the six-key `{run_token, dimension_bundle:{...}}` to `$HTTPS_VECTOR_OUT` instead of the
nan-021 `{run_token, metric_vector}`. Build the off-Docker stub-drive logic test proving bundle
assembly + barrier ordering + missing-capture->error WITHOUT a live daemon.

## Files created / modified
- **Modified** `product/test/infra-001/scripts/cloud-cycle-lib.sh` (373 lines) — sources the new
  bundle lib; gate-8 emit generalized: review step yields the C2' bridge-surface fragment,
  shell captures assembled POST-barrier (R-04), then the six-key bundle emitted. Stub seam
  generalized (SMOKE_BUNDLE_FRAGMENT preferred; SMOKE_REVIEW_VECTOR back-compat synthesizes a
  full fragment).
- **Created** `product/test/infra-001/scripts/cloud-bundle-lib.sh` (272 lines) — the shell-owned
  /observe-surface + container-side captures (behavioral D2, isolation D6, precompact D5),
  `assemble_shell_captures`, and `emit_dimension_bundle` (the node never-empty guard). Split out
  to keep both libs <=500 lines (nan-021 lib-split precedent).
- **Modified** `product/test/infra-001/scripts/release-gate-cloud-cycle-logic-test.sh` (499 lines)
  — Part C happy path now asserts the emitted out-file is a contract-shaped six-key bundle
  (run_token, all 6 capture_keys, analytics.metric_vector, named precompact gap, isolation
  booleans); supplies shell captures via the SMOKE_SHELL_CAPTURES stub seam.
- **Created** `product/test/infra-001/scripts/release-gate-bundle-assembly-logic-test.sh` (255
  lines) — the R-09/R-04 dimension-bundle assembly scenarios (split out of the cycle logic test
  to respect the 500-line rule).

## Tests (off-Docker stub-drive)
- `release-gate-cloud-cycle-logic-test.sh`: **21 passed, 0 failed** (was 18 in nan-021; +3 nan-022
  bundle-shape/control-flow checks).
- `release-gate-bundle-assembly-logic-test.sh`: **11 passed, 0 failed** — green six-key bundle;
  RED rows for empty retrieval, missing proactive capture_2, empty metric_vector, empty
  behavioral, unnamed precompact gap, illegal null payload, missing isolation booleans (each
  asserts NO bundle leaked); barrier-ordering source check (R-04); single-source check (C-5);
  SMOKE_SHELL_CAPTURES stub-seam honoured.
- **Total: 32 passed, 0 failed.** shellcheck clean on both libs (only SC1090 non-constant-source
  info on the variable-sourced test, matching the existing sibling tests).

## Contract alignment with C2' (agent-4, already landed in the shared checkout)
The C2' `bridge-cycle-driver.js` (+ `bridge-cycle-capture.js`) emit exactly the fragment my
`emit_dimension_bundle` consumes from `$REVIEW_OUT`: `{ok, metric_vector, retrieval:{queries,
capture_2}, proactive:{briefing_ids,briefing_scores,injection_set,capture_2}, informs_edges,
phase_signal}` — and only on the REVIEW_INLINE invocation, which is what step 6 reads. Single
source per dimension is preserved: the driver owns the MCP-bridge surface; this lib owns the
/observe surface (behavioral, precompact, isolation).

## Issues / adjacent breakage / contract-seam flags
- **CONTRACT SEAM (Gate-8 wiring, flagged per brief):** `docker-http-posture-smoke.sh` Gate-8
  wiring is UNCHANGED — same env trio (MANIFEST_PATH/RUN_TOKEN/HTTPS_VECTOR_OUT), same
  `cloud_cycle_gates` call, same `cloud-cycle-lib.sh` source line. ONLY the out-file *payload*
  widened (metric_vector -> dimension_bundle). I did NOT edit the smoke. The widened payload is
  the seam ORCH (test_https_uds_parity.py) + K5 `load_https_bundle` must ingest — the Python
  side is owned by other agents; the cross-language shape is the `parity_bundle_contract.md`
  fixture both sides assert against.
- **Stage-3c flag — host sqlite3 for D2 behavioral read.** The live behavioral capture
  (`capture_behavioral_topic_signals`) `vol cat`s the per-slug db out and queries DISTINCT
  topic_signal HOST-SIDE via `sqlite3`. That binary is NOT on the distroless image NOR guaranteed
  on a runner. I made its absence a HARD INFRA fail (provision sqlite3 like node), never a silent
  empty capture. The **release lane (release.yml nan-021-https-uds-parity job) must provision
  sqlite3** for the live D2 read, OR the live behavioral capture should instead read the rows over
  the bridge/analytics surface. This is a live-leg (Stage 3c) provisioning decision — flagging,
  not fixing (out of my scope).
- **Stage-3c flag — D5 precompact + D6 isolation are conservative/documented, not live-proven
  here.** PreCompact restoration is a documented host-side gap (ADR-006/OQ-2: measurable=false,
  gap NAMED — never a vacuous pass). Isolation records on-disk landing booleans; a live cross-slug
  READ probe is the UDS-leg symmetry's job. Both shapes are emitted and contract-valid; their live
  semantics get scrutinized at first Docker run (OQ-2/OQ-4).
- No production code touched. Diff confined to `product/test/infra-001/scripts/`. No new
  transport/cert/credstore/spawn path (R-16): the /observe captures reuse the pinned-curl idiom,
  the DB reads reuse the `vol` busybox sidecar.
- Did NOT run any git commands (Delivery Leader owns git). Did NOT touch integration tests.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (decision/nan-022) + `context_briefing` — surfaced the
  nan-022 ADR set (#5305 ADR-001, #5313 ADR-002 four-valued outcome model, #5307 ADR-003) and the
  nan-021 precedents (#5299 false-green HTTPS leg, #5258 sourceable-fn stub seam, #5298 byte-
  identical RecordEvent sequence, #5193 WAL-grow signal). Applied: barrier symmetry, fixture-file
  stub seam, never-empty guard, append-only fail()/exit-1.
- Stored: entry #5321 "nan-022 C5': dimension-bundle assembly in a split shell lib with a fixture
  stub seam + node never-empty guard" via `/uni-store-pattern` (topic infra-001).
