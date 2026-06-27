# Gate 3b Report: infra-003

> Gate: 3b (Code Review)
> Date: 2026-06-27
> Result: PASS
> Validator: infra-003-gate-3b

Test-only feature, SHELL deliverable. Artifacts reviewed (committed on
`feature/infra-003` HEAD `9dc19db1`):
- `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` (C1–C7 gate)
- `product/test/infra-001/scripts/isolation-probe-lib.sh` (C3/C4 probes)
- `product/test/infra-001/scripts/fixtures/stub-read-marker.sh` (read-seam stub)
- `product/test/infra-001/scripts/release-gate-isolation-logic-test.sh` (tier-1 teeth)
- `product/test/infra-001/scripts/release-gate-bundle-static-test.sh` (R-15 change)

Validated against ARCHITECTURE.md (+ ADR-001..004), SPECIFICATION.md,
RISK-TEST-STRATEGY.md (18 risks), and pseudocode/ + test-plan/ (C1–C7 + R-15).
"Compiles" for shell = `bash -n` clean + `shellcheck -S warning` clean — verified
directly. Both teeth tests were run by the validator.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Code implements C1–C7 + R-15 1:1; functions/data flow match OVERVIEW + ARCH "Data Flow"; uses the authoritative correct query form (not the C4-prose typo flagged in 3a) |
| 2. Architecture compliance | PASS | ADR-001 standalone shell gate (sources define-on-source lib); ADR-002 read-as-barrier + non-substring 2×2; ADR-003 per-route MCP session; ADR-004 single-restart + liveness-precondition. Seam exercised, no `crates/` change |
| 3. Interface implementation | PASS | Markers/slugs/SID_A/SID_B/POS_*/NEG_* coherent across files; observe `topic_signal = '<marker>'` exact, MCP `content LIKE '%marker%' OR topic = '<marker>'`; tri-state exit contract correct |
| 4. Test case alignment | PASS | Tier-1 logic test (25 cases) + R-15 static test (12 cases) cover the C5/C6/C7 truth table and all 18 risks per the test plans |
| 5. Code quality | PASS | All files ≤500 lines (max 430); no TODO/FIXME/unimplemented/placeholder; `bash -n` clean; `shellcheck -S warning` clean on shipped gate/lib/stub/logic-test (one pre-existing SC2010 in the R-15 file — not introduced by infra-003) |
| 6. Security | PASS | Token pulled from `vol` (no hardcoded secret); markers + RUN constrained to `[a-z0-9-]` (R-12) with non-substring self-check (R-18); `vol` mount `:ro`; no `docker exec`; node-built JSON bodies; no path traversal (fixed slug constants) |
| 7. Knowledge stewardship | PASS | `agent-3-gate-script-report.md` has `## Knowledge Stewardship` with `Queried:` and `Stored:` (context_correct #5194→#5345) |

### TEETH verification (validator ran both tests)

| Requirement | Evidence | Verdict |
|-------------|----------|---------|
| Tier-1 gate-logic test fails RED on a planted wrong-store marker (no vacuous GREEN) | `release-gate-isolation-logic-test.sh`: `test_c7_planted_leak_{mcp_b_in_a,mcp_a_in_b,obs_b_in_a,obs_a_in_b}_is_red` all PASS (expect rc=1 / "ISOLATION BROKEN"), 4 directions | PASS |
| Own-store timeout classified INFRA (never RED, never GREEN) | `test_c5_own_timeout_is_infra_not_red` PASS (rc=2 / INFRA, asserts not RED and not GREEN); `test_c7_red_dominates_infra` PASS | PASS |
| Whole tier-1 suite green | 25 passed, 0 failed, RC=0 | PASS |
| R-15: #815 invariant registers the new script | `multi-tenant-isolation-smoke.sh` in `KNOWN_SMOKE_SCRIPTS`; `test_no_new_smoke_script` PASS | PASS |
| R-15: invariant retains teeth | Validator planted `synthetic-fork-smoke.sh`; suite went RED (rc=1, "FORK smell — unknown smoke script(s) added"); removed → green (rc=0); tree clean | PASS |
| R-15 static suite green | 12 passed, 0 failed, RC=0 | PASS |

### Load-bearing invariants (spawn-prompt focus) — all implemented, no contradictions

| Invariant | Implemented in | Verdict |
|-----------|----------------|---------|
| Bidirectional 2×2; each store asserted to hold ONLY its own marker both directions | `run_cells` (4 positives) + `run_negatives` (4 cross cells: B-obs in A, A-obs in B, B-mcp in A, A-mcp in B); `verdict` scans all 4 NEG + all 4 POS | PASS |
| Own-store positive absent AT deadline = INFRA (never RED); RED reserved for cross-store wrong-store presence; positive-gates-negative per direction; NO aggregate du barrier | `write_then_barrier` (timeout→`WTB=INFRA`, l.286-291); `negative_cell` (foreign present→`NEG=RED` independent of own; absent+own-PRESENT→ABSENT, absent+own-INFRA→SKIPPED); `store_size` defined but never on the barrier path | PASS |
| Markers mutually NON-SUBSTRING `infra003-{obs,mcp}-{a,b}-<run>` with runtime pairwise self-check; exact `topic_signal = '<marker>'` + `content LIKE '%marker%'` | `derive_markers` + `assert_markers_distinct` (charset + pairwise non-substring, fail INFRA); `query_for` predicates | PASS |
| Per-route MCP session isolation (distinct SID_A/SID_B, never crossed); handshake/session failure = INFRA per route, wrong-store = RED | `mcp_handshake` (per-route session mint, missing id→INFRA); `mcp_write` binds `SID_A`/`SID_B` per route; no shared session var | PASS |
| Tri-state exit GREEN=0/RED=1/INFRA=2/SKIP=3 (no collision with posture-smoke exit 4); RED dominates INFRA dominates GREEN | `fail`=1, `infra_fail`=2, SKIP=3, GREEN=0; `verdict` checks RED first, then INFRA, then GREEN | PASS |
| sqlite3 hard-fail INFRA; `vol cat` db+`-wal`+`-shm` read-only; never `docker exec` | `preflight` sqlite3 absent→INFRA; `read_marker` copies db+wal+shm, missing main db→INFRA; `vol()` `:ro`; no `docker exec` anywhere (only a comment mentions it) | PASS |
| Terminal success marker matches verify-by-name grep contract (#5180) | `verdict` emits `[infra003-smoke] ALL GATES PASSED` only on GREEN; `test_c7_terminal_marker_matches_grep` confirms the anchored `\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*` match | PASS |

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status**: PASS
**Evidence**: The shipped gate maps 1:1 onto the C1–C7 + R-15 decomposition:
`preflight`(C1) → `setup_container`/`register_both_and_restart`/`assert_routes_live`(C2)
→ `observe_write`(C3) → `mcp_handshake`/`mcp_write`/`parse_sse_jsonrpc`(C4) →
`write_then_barrier`/`run_cells`(C5) → `read_marker`/`query_for`/`negative_cell`/
`run_negatives`(C6) → `verdict`(C7), with R-15 the one-line `KNOWN_SMOKE_SCRIPTS`
addition. The data-flow order (preflight → boot/register/single-restart →
liveness precondition → per-cell sequential write+barrier → negatives → verdict)
mirrors ARCHITECTURE "Data Flow" exactly. The gate uses the authoritative correct
read predicate (`topic = '<marker>'` exact, no stray `%`), so the C4-prose typo
that Gate 3a flagged has no behavioral carry-over.

### Check 2 — Architecture compliance
**Status**: PASS
**Evidence**: ADR-001 — a SEPARATE top-level smoke (`multi-tenant-isolation-smoke.sh`)
with a sourced-on-source probe lib (`isolation-probe-lib.sh`), self-contained
assertions (SR-12), no graft onto posture-smoke Gates 1–8. ADR-002 — marker-keyed
read-as-barrier (retry-until-present) is the durability proof; aggregate `store_size`
barrier absent (`store_size` retained only as a primitive, never called on the
barrier path). ADR-003 — bidirectional MCP probe, SSE `Accept`, per-route own
`Mcp-Session-Id`. ADR-004 — both slugs registered before one restart; route-liveness
is a precondition only. `git show` confirms no `crates/` change; slug literals come
from `SLUG_A`/`SLUG_B` globals (not re-typed ADR-004 regex).

### Check 3 — Interface implementation
**Status**: PASS
**Evidence**: `read_marker(store_dir, table, predicate)` and `query_for(surface,
marker)` are the shared two-store-read seam used by both C5 positive and C6
negative. Result tokens are consistent: `WTB ∈ {PRESENT, INFRA}`, `NEG ∈ {ABSENT,
RED, SKIPPED}`, consumed by `verdict` via `${!cell}` indirection. Observe →
`observations.topic_signal = '<marker>'`; MCP → `entries` `content LIKE '%marker%'
OR topic = '<marker>'` (matches AC-07 canonical). Exit helpers `fail`(1)/
`infra_fail`(2) and SKIP(3)/GREEN(0) are the documented tri-state contract.

### Check 4 — Test case alignment
**Status**: PASS
**Evidence**: `release-gate-isolation-logic-test.sh` sources the shipped bytes
(single source of truth, #5192) and drives the verdict truth table through the
`SMOKE_READ_MARKER_CMD`/`SMOKE_WRITE_CMD` seams: 4-direction planted-leak→RED,
own-timeout→INFRA, RED-dominates-INFRA, missing-db→INFRA, retry-until-present
(polls ≥3× then green), tri-state distinct exit codes, R-18 substring self-check,
C1 SKIP/INFRA, and static checks for the read-as-barrier (no `store_size`), no
count heuristic, per-route session, SSE Accept, no warn+continue, no overclaim.
25 cases, all green. R-15 static suite (12 cases) green. Coverage maps to the test
plans and the 18-risk register.

### Check 5 — Code quality
**Status**: PASS
**Evidence**: `wc -l` — 430 / 107 / 68 / 323 / 231 (all ≤500). `bash -n` clean on
all 5. `shellcheck -S warning -x` clean on the shipped gate, probe lib, stub, and
tier-1 logic test. No TODO/FIXME/unimplemented/placeholder in the shipped gate or
lib. The shell idiom precludes `.unwrap()`.

### Check 6 — Security
**Status**: PASS
**Evidence**: Bearer token is read from `vol cat "$HASH_DIR/token"` (never
hardcoded). RUN nonce and all four markers are constrained to `[a-z0-9-]`
(`derive_markers`/`assert_markers_distinct`, R-12) so no LIKE wildcard or quote can
reach the host `sqlite3` predicate; pairwise non-substring self-check fails LOUD
(R-18). JSON bodies are node-built, so a marker cannot break quoting. The `vol`
sidecar mounts `:ro` (a read cannot mutate the property it measures); no
`docker exec` into the distroless runtime (NFR-02). Slugs are fixed constants (no
path-traversal surface). Malformed `sqlite3 -json` output is coerced to 0 only
after the `INFRA` sentinel branch, so a failed read is INFRA, never a silent
0-row pass.

### Check 7 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: `infra-003-agent-3-gate-script-report.md` (the 3b implementation
agent) carries a `## Knowledge Stewardship` block with a `Queried:` entry
(`context_search` → ADR #5335/#5342/#5343/#5344, stub-smoke/verify-by-name
#5258/#5192, WAL-robust #5193, sourceable-gate harness #5194) and a `Stored:`
entry (`context_correct` #5194→#5345, augmenting the existing pattern rather than
duplicating). Well-formed; no missing block.

## Non-blocking notes (carried for delivery, not gate failures)

- **Pre-existing SC2010** at `release-gate-bundle-static-test.sh:206`
  (`ls "$SCRIPT_DIR"/*smoke*.sh | grep -v ...`). Verified via `git show 03a31f36`
  that infra-003's only change to this file is the one-line `KNOWN_SMOKE_SCRIPTS`
  array addition — the `ls | grep` line predates infra-003. Zero NEW shellcheck
  codes introduced by this feature. Opportunistic cleanup possible later; not in
  scope and not a gate failure.
- **Mode change 755→644** on `release-gate-bundle-static-test.sh` (git tracks
  `old mode 100755 / new mode 100644`). Benign: the file is invoked via `bash …`,
  and the sibling smokes (`docker-http-posture-smoke.sh`,
  `docker-embed-readiness-smoke.sh`) are also 644. Cosmetic only.
- **`store_size()` defined but uncalled** in the flow — intentionally retained as a
  liveness primitive (boot/liveness use `wait_for_http_active` via log-grep), and
  explicitly documented as NOT the barrier. Consistent with ARCH intent.
- **R-16 standing-lane / R-15 #815 cross-link** are delivery-coordination
  obligations (CI wiring + issue comments), confirmed out of in-gate logic scope;
  the gate is correctly NOT wired into `release.yml` (N5/#788 lane out of scope).

## Rework Required

None.
