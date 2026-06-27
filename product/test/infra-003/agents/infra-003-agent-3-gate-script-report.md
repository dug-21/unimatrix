# Agent Report — infra-003-agent-3-gate-script

**Task:** Implement the standalone multi-tenant HTTP isolation smoke gate (C1–C7)
as a bash script extending the infra-001 harness, plus its tier-1 off-Docker
gate-logic test. Test-only, shell-only. No `crates/` change.

## Files created / modified (absolute paths)

- `/workspaces/unimatrix/product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` — CREATE. The standalone C1–C7 gate (preflight, two-slug registration + single restart + route-liveness precondition, read-as-barrier positive control, cross-store negative + two-store read primitive, verdict). 427 lines.
- `/workspaces/unimatrix/product/test/infra-001/scripts/isolation-probe-lib.sh` — CREATE. C3 observe + C4 MCP-write probes, factored out so the gate stays under the 500-line rule (mirrors the cloud-cycle-lib.sh split). Sourced by the gate. 107 lines.
- `/workspaces/unimatrix/product/test/infra-001/scripts/fixtures/stub-read-marker.sh` — CREATE. Off-Docker stub for the `SMOKE_READ_MARKER_CMD` read seam (present/absent/leak/INFRA/retry, keyed on store_dir + marker-substring-of-predicate). 68 lines.
- `/workspaces/unimatrix/product/test/infra-001/scripts/release-gate-isolation-logic-test.sh` — CREATE. Tier-1 off-Docker gate-logic test; sources the shipped gate and drives the verdict truth table. 323 lines.
- `/workspaces/unimatrix/product/test/infra-001/scripts/release-gate-bundle-static-test.sh` — MODIFY (R-15). Registered `multi-tenant-isolation-smoke.sh` in `KNOWN_SMOKE_SCRIPTS` (#853) so the #815 invariant does not trip; guard teeth preserved.

Committed: `03a31f36` on `feature/infra-003`.

## Tests — tier-1 gate-logic (off-Docker): 25 passed, 0 failed

Required teeth + INFRA discrimination cases (all passing):
- `test_c7_planted_leak_{mcp_b_in_a,mcp_a_in_b,obs_b_in_a,obs_a_in_b}_is_red` — foreign marker planted in the wrong store -> **RED exit 1**, all four directions on both surfaces.
- `test_c5_own_timeout_is_infra_not_red` — own-store read-as-barrier deadline miss -> **INFRA exit 2**, asserted NOT RED and NOT GREEN.
- `test_c7_red_dominates_infra` — leak in A while A's own positive timed out INFRA -> **RED** (mis-route never masked).
- `test_c6_missing_db_is_infra` — read primitive INFRA sentinel -> **INFRA**, never a 0-row clean pass (R-07).
- `test_c5_positive_is_retry_until_present` — stub absent×2 then present -> polled 3× then PRESENT -> GREEN (proves bounded retry, not a fixed sleep / single read).
- `test_c7_tristate_exit_codes` — GREEN=0 / RED=1 / INFRA=2 distinct; SKIP=3 below.
- `test_c1_docker_absent_skips_exit3` / `test_c1_sqlite3_absent_is_infra` — SKIP exit 3 vs INFRA exit 2 via a fakebin PATH (no Docker).
- `test_c7_substring_markers_fail_infra` / `test_c7_real_markers_are_non_substring` — R-18 runtime self-check.
- `test_c7_terminal_marker_matches_grep` — GREEN emits a single `[infra003-smoke] ALL GATES PASSED` line matching `release-gate-lib.sh:59` `\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*`.
- Static greps: read-as-barrier keys on `read_marker` not `store_size` (no aggregate barrier, C-08); negative read is a content predicate not a count heuristic (C-11); distinct `SID_A`/`SID_B` never crossed (R-17); SSE `Accept` advertised; no warn+continue; point-in-time-only overclaim guard.

Also verified: `release-gate-bundle-static-test.sh` 12/12 (R-15 invariant passes with the new script registered and teeth intact); `shellcheck -S warning` clean on all four shell files; `bash -n` clean. No `crates/` change (AC-13). No full `cargo` run performed — this is a shell-only deliverable with zero Rust edits.

## Issues / resolved open questions

- **INFRA exit code value:** pinned **2** — distinct from RED(1), SKIP(3), and posture-smoke's exit 4 ("IMAGE= prebuilt tag unavailable"). No collision when both run in one lane.
- **Read deadline value:** `READ_DEADLINE_SECS` default **10s** (mirrors the ~10s store-grow wait in `docker-http-posture-smoke.sh`, arm64-CI headroom); env-overridable, and a bounded deadline-poll (never a fixed sleep). Poll interval `READ_POLL_SLEEP` default 1s.
- **Log prefix / terminal marker:** `[infra003-smoke]` — keeps the c1/c7 "infra003" identity while satisfying the `-smoke]` verify-by-name grep (#5180). Terminal `ALL GATES PASSED` emitted only on GREEN.
- **Stub seam exposed (test-plan requirement):** `SMOKE_READ_MARKER_CMD` (argv `CMD <store_dir> <table> <predicate>`, prints row-count or the `INFRA` sentinel) is the load-bearing C5/C6 read seam, mirroring infra-001's `SMOKE_STORE_SIZE_CMD`. Companion `SMOKE_WRITE_CMD` (argv `CMD <surface> <slug> <marker>`) neutralizes the C3/C4 writes off-Docker. `read_marker` is pure-return (never exits) so its INFRA classification is not swallowed by a command-substitution subshell; callers raise `infra_fail`.
- **MCP `tools/call` args / minimal `ImplantEvent` shape:** the pseudocode defers exact bytes to Stage 3c. Used `context_store {content, topic, category:"pattern"}` and `RecordEvent{event_type:"tool_use",session_id,timestamp:0,payload:{},topic_signal}` (per `wire.rs:251-267`, RecordEvent flattens ImplantEvent). Live-only path; 3c confirms the exact persisting bytes. NOT a stub/placeholder — concrete executable best-effort, flagged for 3c confirmation only.
- **c4 prose typo confirmed:** used the authoritative c5 MCP predicate `content LIKE '%<marker>%' OR topic = '<marker>'` (exact topic), not c4's stray-`%` form.
- **Did NOT run the live Docker leg** (Stage 3c).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (decision/infra-003 + pattern) — surfaced ADR-001/002/003/004 (#5335/#5342/#5343/#5344), the stub-driven Docker-smoke / verify-by-name patterns (#5258/#5192), WAL-robust store-grew (#5193), and the sourceable-gate-test harness pattern (#5194). Applied: the off-Docker stub-seam tier, the `-smoke]` marker contract, and the WAL-complete `vol cat` read.
- Stored: corrected #5194 -> **#5345** "Testing a sourceable shell gate lib: run the harness under set -uo pipefail only, never set -e" via `context_correct` — augmented with the infra-003 source-time `-e` re-enable vector (sourcing a gate that runs `set -euo pipefail` re-enables `-e`; a later `set -uo pipefail` does not clear it; needs an explicit `set +e`). Preferred augmenting the existing pattern over a duplicate.
