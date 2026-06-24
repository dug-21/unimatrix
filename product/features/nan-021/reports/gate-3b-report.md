# Gate 3b Report: nan-021

> Gate: 3b (Code Review)
> Date: 2026-06-24
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | C1–C5 match OVERVIEW + per-component pseudocode; one documented, justified departure (manifest tool_calls not replayed as MCP calls — server has no such tools). |
| 2. Architecture compliance | PASS | Component boundaries held; ADR-001..006 followed; AC-06 zero-prod-diff verified empty. |
| 3. Interface implementation | PASS | Exact integration-surface signatures used (UnimatrixUdsClient/HookClient, mcp-bridge.js, MetricVector dict, run_smoke_gate). No invented APIs. |
| 4. Test case alignment | PASS | 71 off-Docker tests pass (29 C4 + 10 C3 + 20 C5 cloud-cycle + 12 bundle-static); map 1:1 to component test plans. |
| 5. Code quality | PASS | All 10 source files < 500 lines; no todo!/unimplemented!/TODO/FIXME/unsafe; no `.unwrap()` (no Rust changed). |
| 6. Security | PASS | No secrets; witness never logs Authorization (NFR-06); pinned-flush exercised; credstore mode-0600 asserted; no new attack surface. |
| 7. Knowledge stewardship | PASS | All 3 impl agents (C3/C4/C5) have `## Knowledge Stewardship` with `Queried:` + `Stored:`/justified-nothing-novel. |
| ZERO production-code diff (AC-06/NFR-1) | PASS | `git diff main...feature/nan-021 -- crates/ lib/ packages/` is EMPTY. |
| AC-03 no-seed (3 forbidden sites) | PASS | `assert_no_seed_reachable` enumerates all 3; call/import-shaped detection; reachability test has teeth. |
| AC-02/FR-9 bridge-in-path | PASS | Cycle `context_cycle(start/stop/review)` driven THROUGH mcp-bridge.js; SSE + session-id replay witnessed; only `mcp_url` POST is the JSON-only negative control. |
| SR-05 cross-leg workload identity | WARN | Sequences are identical, but hand-duplicated across node (C2) + Python (C3) with no single-source guard beyond the shared manifest + a duplicated phase literal. Parity-drift maintenance risk. |
| ADR-006/FR-10 symmetric barrier | PASS | One C4 helper gates BOTH legs (UDS in-process; HTTPS via same predicate/cadence constants); stabilize-across-two-polls, DIR-incl-`-wal`. |
| ADR-005/AC-05 false-green | PASS | nan-019 exit-code discriminator + verify-by-name marker reused verbatim; Docker-absent=exit-3 HARD fail; release.yml lane is workflow_dispatch/tag, NOT pull_request. |
| C2 PostToolUse-frame topic_signal DERIVED | PASS | C2 emits plain `event_type:"PostToolUse"` RecordEvent (token-free body); topic_signal derived server-side from response_snippet feature-ID token; no seed. |
| NFR-8 comparator emits raw vectors + per-field table | PASS | `field_by_field_record` emits both raw vectors + 21-field table + at-risk flags; D-5 set stays closed (3 fields). |
| `_project_hash` in conftest (R-11/SR-04) | WARN | conftest `daemon_server` recomputes the project hash for UDS socket-dir discovery (not the bridge credstore path). Bridge path correctly reads-back. Minor fork-smell; drift fails loud (socket-not-found). |

**Result: PASS.** Both WARNs are non-blocking and belong to Stage-3c live-run watch items; no FAIL.

## Detailed Findings

### 1. Pseudocode fidelity
**Status**: PASS
**Evidence**: C4 `parity_workload.py` + `metric_comparator.py` implement the OVERVIEW manifest type,
the 3-field closed D-5 `EXCLUDED` set, the symmetric `durability_barrier`, the stale-token
`load_https_vector`, and the no-seed audit exactly as the OVERVIEW MetricVector/barrier contract
specifies. C2 `cloud-cycle-lib.sh` / `bridge-cycle-driver.js` follow c2-bridge-cycle.md (read-back
projectHash, spawn-bridge-last, drive-immediately, barrier-before-review, emit `{run_token,
metric_vector}`). C3 `parity_legs.py` follows c3 (drive manifest, barrier, review).
**Documented departure (justified)**: c2 pseudocode L46–48 implied the manifest tool_calls would be
"issued via the bridge". The implementation correctly does NOT replay Read/Bash/Grep as MCP
`tools/call` — the live server exposes only `context_*` tools (an MCP call for "Read" returns -32602).
The driver documents this (L204–217): the bridge carries `context_cycle(start)→(stop)→review` (where
SSE + session-id replay are proven), and the workload tool calls manifest as the SHELL's pinned
`/observe` PostToolUse hooks. This matches FR-2's split ("MCP traffic through the bridge; hook
observations over pinned `/observe`") and avoids inventing non-existent server tools (NFR-2). Not a defect.

### 2. Architecture compliance
**Status**: PASS
**Evidence**: `git diff main...feature/nan-021 -- crates/ lib/ packages/` returns EMPTY (AC-06/NFR-1
hard-verified). The diff touches only `product/test/infra-001/**`, `.github/workflows/release.yml`
(+67 lines, one new job), and `product/features/nan-021/**` docs. ADR-001 pytest-as-orchestrator seam
present (`test_https_uds_parity.py` owns both legs in one invocation). ADR-002 (spawn-bridge-last,
drive-immediately, no interposed wait, rely on #830 self-heal, no re-authored reconnect) present in
`bridge-cycle-driver.js`. ADR-005 false-green discriminator reused via `cloud-cycle-https-leg.sh` →
`run_smoke_gate`. `release-gate-lib.sh` is sha256-byte-unchanged (bundle-static-test PASS).

### 3. Interface implementation
**Status**: PASS
**Evidence**: C3 uses the exact `UnimatrixUdsClient` / `UnimatrixHookClient` surfaces from the
integration table (`session_register`, `record_cycle_start/stop`, `record_pre/post_tool_use`,
`context_cycle_review(format="json")`). The comparator reads the parsed dict
(`.universal/.phases/.domain_metrics`), never the Rust struct, per the surface note. The bridge driver
sends `context_cycle` with arg key `type` (not `cycle_type`) — the driver comment documents this was
verified live against the server's actual MCP arg name (`harness/client.py:693`). `run_smoke_gate
IMAGE SMOKE_CMD...` consumed as-is. No invented APIs.

### 4. Test case alignment
**Status**: PASS
**Evidence**: Ran off-Docker (the live HTTPS/Docker legs are Stage-3c):
- `pytest suites/test_parity_workload.py` → **29 passed** (R-01 completeness, R-02 teeth/mutation,
  R-06 barrier predicate+timeout, R-03 token guard, NFR-8 field table, AC-03 no-seed-with-teeth,
  AC-07 sole-net-new).
- `pytest suites/test_https_uds_parity.py -m "not integration"` → **10 passed** (C3 contract: same
  manifest object, same session identity, existing-clients-not-fork, barrier-before-review symmetry,
  one-invocation, missing/stale-leg errors, off-Docker seam wiring proof).
- `release-gate-cloud-cycle-logic-test.sh` → **20 passed** (false-green truth table through the C5
  wrapper, anchored marker, nan-019 verbatim acquisition/regex, C2 control-flow via SMOKE_CYCLE_CMD,
  release.yml lane=tag/dispatch).
- `release-gate-bundle-static-test.sh` → **12 passed** (release-gate-lib.sh byte-unchanged, append-only
  ordering, single terminal marker, no-new-smoke-script allow-list).

### 5. Code quality
**Status**: PASS
**Evidence**: Line counts (all < 500): parity_workload 452, metric_comparator 225, parity_legs 302,
cloud-cycle-lib 338, bridge-cycle-driver 285, bridge-witness 81, cloud-cycle-https-leg 60,
release-gate-cloud-cycle-logic-test 430, test_https_uds_parity 315, test_parity_workload 487.
No `todo!()`/`unimplemented!()`/`TODO`/`FIXME`/`unsafe` in the diff. No Rust changed → no `.unwrap()`
concern. The `__main__` CLI shim and bridge teardown handle errors explicitly.

### 6. Security
**Status**: PASS
**Evidence**: No hardcoded secrets (the only "Bearer"/"token" hits are NFR-06 comments and the pinned
`Authorization: Bearer ${TOKEN}` reading the volume-sourced token). `bridge-witness.js` explicitly
never reads `Authorization` (only the non-secret server-minted session-id + content-type). The
`emit_bundle` child stays the single suppressed stderr (bearer-bearing blob). Credstore mode-0600 is
asserted at the C1→C2 boundary. The negative control proves SSE is required (TLS trust boundary
EXERCISED, not shape-asserted — #4970). No new route/deserialization/path-traversal sink. `cargo audit`
N/A — zero Rust/dependency change.

### 7. Knowledge stewardship
**Status**: PASS
**Evidence**: C4 agent — Queried (#5286/#5293/#5291/#4907) + "nothing novel, #4907 covers the
self-reference trap". C2 agent — Queried (#5294/#5291/#5290/#5129/#5115) + Stored #5298 + #5296
(patterns). C5 agent — Queried (#5290/#5258/#5192/#5183/#5208) + Stored #5299 (pattern, edged
Supports). All three blocks present with `Queried:` and `Stored:`/justified entries.

## WARN findings (non-blocking; Stage-3c watch items)

### W1 — SR-05 cross-leg workload identity is hand-duplicated in two languages
**Status**: WARN
**Evidence**: C2 (`cloud-cycle-lib.sh:_fire_observe_hooks`, node) and C3
(`parity_legs.py:drive_uds_leg`, Python) BOTH hand-build the drive-event sequence:
`SessionRegister(tester) → cycle_start(phase) → PreToolUse(TaskCreate subject="delivery: drive the
parity workload") → per-observe Pre+Post(name,response_size,response_snippet,tool_input) →
cycle_stop → SessionClose(completed, duration_secs=1)`. The two are currently **byte-equivalent** and
both consume the same C4 manifest and the same `PARITY_PHASE="delivery"` value. **However** the phase
literal AND the phase-setting subject string `"{phase}: drive the parity workload"` are duplicated
verbatim in both files with no single-source mechanism; only `expected_observe_count` and the manifest
contents are truly single-sourced. C2's own comment names this ("C3 is the SOURCE OF TRUTH; C2 conforms
to it"). A future edit to one leg's frame order/subject/fields silently breaks live-vs-live parity
(R-09 class). This is exactly the SR-05 parity-drift risk. **Recommendation (Stage-3c / future):**
either factor the RecordEvent frame sequence into the C4 manifest (emit the ordered frame list once,
both legs replay), or add a cross-language equivalence test that diffs the two emitted frame arrays.
Not a blocker — the first live dual-transport run (Stage-3c) is itself the drift detector (any
divergence surfaces as a `phases`/`total_tool_calls` parity mismatch, caught loudly by the comparator).

### W2 — conftest `_project_hash` recomputes the project hash (R-11/SR-04 adjacency)
**Status**: WARN
**Evidence**: `harness/conftest.py:410` (`_project_hash`) reimplements production
`compute_project_hash` (SHA-256, first-16-hex) to locate the daemon's data dir / sockets for the UDS
`daemon_server` fixture. The risk strategy's R-11 ("projectHash recomputed instead of read-back; no
hashing primitive in the fixture") targets the **bridge-spawn path** — and that path is clean: C2
(`cloud-cycle-lib.sh`) reads the projectHash back by listing the single dir under
`$SANDBOX/home/.unimatrix/`, invoking no hashing. The conftest hash is a UDS-fixture convenience
(socket discovery), not the bridge credstore derivation, and a production hash-algo drift would fail
LOUD (sockets not found within deadline), not silently mis-attach. **Recommendation:** acceptable as-is
for the UDS socket-discovery use; if production exposes a data-dir/socket path query, prefer read-back
to fully close SR-04. Non-blocking.

## Rework Required

None. (Both findings are WARN; no FAIL.)

## Stage-3c carry-forward (informational, not gate-3b blockers)

- ⚠ **First-live-run field-by-field validation gate (AC-04 / NFR-8):** the 3-field D-5 set is a
  load-bearing ASSUMPTION until the first live dual-transport run is examined across all 18 non-excluded
  `UniversalMetrics` fields. The comparator already emits the raw vectors + per-field table to support
  this; the tester/leader must run the gate and disposition any divergence as product/human (defect →
  GH bug OR product-signed ADR-003 amendment) — never silently widen.
- First-green tax (R-12): the live bridge/cert/SSE path runs green for the first time only on a tag;
  budget multiple tag rounds. The lane is intentionally NOT in `create-container-manifest.needs:` until
  first-green (release.yml comment documents this).
- W1 cross-leg drift is observable by the live comparator on the first parity run.
