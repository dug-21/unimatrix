# Gate 3c Report: nan-020

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-20
> Branch validated: feature/nan-020 @ HEAD (fafa46a8)
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | All 16 risks mapped; every PRE-MERGE-PROVABLE risk green; R-07 negative control non-vacuous (asserted, not PENDING). All 30 non-negotiable test names exist in source (grep-verified, not report-only) |
| 2. Test coverage completeness | PASS | 45/45 shell gate-logic tests pass (19+12+14); critical R-01/R-03/R-07 + integration seams covered; PENDING items correctly limited to live container round-trip |
| 3. Specification compliance | PASS | AC-01..AC-09 verified; AC-03/AC-09 live half correctly PENDING-post-tag; AG-1 legacy `--remote` accepted residual |
| 4. Architecture compliance | PASS | ADR-001..005 honored; `release-gate-lib.sh` byte-unchanged vs main; extend-in-place; process-boundary hermeticity |
| 5. Knowledge stewardship | PASS | Test-phase stewardship block present in RISK-COVERAGE-REPORT.md (Queried + Stored-with-reason) |

### Integration-test validation (mandatory, adapted — no app code)

| Item | Status | Evidence |
|------|--------|----------|
| Stub-driven gate-logic suites ran + passed (the load-bearing "integration") | PASS | bundle-logic 19/0, bundle-static 12/0, nan-019 regression 14/0 — re-run by this gate, all rc=0 |
| `pytest -m smoke` baseline ran; single error triaged as env cold-start flake | PASS (accepted) | Report: 23 passed / 1 error (`test_contradiction_detected`, server-init 10s timeout); re-ran green in isolation (8.29s); in a suite nan-020 does not touch; no GH issue / no xfail per USAGE-PROTOCOL — correct |
| R-07 hermeticity negative control PRE-MERGE (not PENDING) + non-vacuous discrimination twin | PASS | `test_hermeticity_negative_control_still_red` (poison cred + `STUB_INIT_WRITE_CRED=0` broken attach → exit 1 + store-no-grow) PLUS `test_hermeticity_discrimination_unisolated_would_green` (non-isolated path grows store 0→1, proving the control would catch a vacuous green). Inspected bodies — genuinely discriminating |
| Live container round-trip classified PENDING-post-tag (not silently dropped) | PASS | RISK-COVERAGE-REPORT "PENDING-post-tag" section enumerates the live legs (#5189/#4796); C15 stays `partial` until post-tag |
| No integration/gate-logic tests deleted or commented out | PASS | No commented-out test functions in any suite; nan-019 marker + Gates intact; Gates 5–7 appended |
| `release-gate-lib.sh` byte-unchanged | PASS | `git diff main...HEAD` empty for the file; anchored marker grep `[783-smoke] ALL GATES PASSED.*` intact at lib:57 |
| RISK-COVERAGE-REPORT includes stub + integration counts | PASS | "45 passed, 0 failed" shell total + smoke 23+1/24 documented |
| Cargo OOM (linker signal 9) is environmental, not a feature regression | PASS | `git diff main...HEAD` shows 0 `.rs`/`crates/` files; nan-020 ships zero Rust — OOM cannot be a feature regression; release binary present (`target/release/unimatrix`, 16:10) |

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS

Per the standing Gate-3c lesson (#2758: report may list test names that don't exist in code), I
grepped **all 30 non-negotiable test function names** from the RISK-COVERAGE-REPORT against the
actual `.sh` source files. **Every one resolves to a real function** in
`release-gate-bundle-logic-test.sh` or `release-gate-bundle-static-test.sh` — none is report-only.

Critical risks:
- **R-01** (silent false-pass): full ADR-001 truth table (`test_gate5_emit_rc_nonzero_fails`,
  `..._empty_blob_fails`, `..._wrong_prefix_blob_fails`, `test_gate6_init_rc_nonzero_fails`,
  `test_gate7_observe_non204_fails`, `test_gate7_store_no_grow_fails`) + happy path is the only
  exit-0 + `test_marker_suppressed_on_failure_red` (no early `exit 0`). All green.
- **R-03** (nan-019 regression): `test_run_smoke_gate_byte_unchanged`, `test_append_only_ordering`,
  `test_single_terminal_marker`, plus the full nan-019 truth table (14/14). `release-gate-lib.sh`
  byte-unchanged vs main (diff empty).
- **R-07** (stale-credstore false-green) — the load-bearing one: the REQUIRED negative control is
  **present, green, and non-vacuous**. I read both bodies: the control poisons `~/.unimatrix/<hash>/
  remote.json` with a STALE cred AND forces a broken attach (`STUB_INIT_WRITE_CRED=0`), then asserts
  exit 1 + `"bundle-path observe did not land in per-slug store"`. The discrimination twin drives the
  non-isolated Gate-7 path and proves it WOULD have greened (store delta `before->after` grows from
  the stale cred). This is the vnc-041 AC-06 shape, asserted **pre-merge** — classifying it PENDING
  would have been a gap; it is not.

### Check 2 — Test coverage completeness
**Status**: PASS

Re-ran all three suites in this gate (Docker/node/network-free): bundle-logic **19/0**, bundle-static
**12/0**, nan-019 regression **14/0** = **45/0**, matching the report exactly. The High/Medium
gate-logic + doc-grep risks (R-02, R-04, R-05, R-06, R-08..R-13, R-15) each map to a present, passing
test. The only PENDING coverage is the live hosted-runner container round-trip — correctly labeled
POST-TAG-CONFIRMABLE (#5189/#4796), not a pre-merge gap.

### Check 3 — Specification compliance (AC-01..AC-09)
**Status**: PASS

Independently verified the grep-based ACs against shipped files:
- **AC-01**: `grep -cE '501|W2-7' docs/client-setup.md` → 0; zero `curl .../observe` blocks; positive
  `init --bundle` ×4 + observe route ×3 present.
- **AC-02**: zero `init --remote unimatrix-bundle:` (both files); `--remote` "legacy" marker present
  ×4 each; **zero `init --bundle ... --slug` invocations** (OQ-A/R-09 satisfied). The single
  `--slug`/`--bundle` grep co-occurrence at client-setup.md:35 is **prose stating the bundle path
  takes _no_ `--slug`** — the correctness statement itself, NOT a violation (verified by reading the
  line). The report's "zero `--slug` paired with `--bundle`" claim is accurate.
- **AC-03 / AC-09**: gate-logic green pre-merge; live round-trip PENDING-post-tag (accepted).
- **AC-04**: `test_no_new_smoke_script` green; round-trip inside `docker-http-posture-smoke.sh`.
- **AC-05/AC-06**: hard-fail skip paths + anchored terminal marker asserted.
- **AC-07**: uni-docs.md widened to `docs/` (×18) with blast-radius operational definition (line 19),
  full-tree-audit non-goal (line 20), Feature-2 fence (lines 134/163); `setup-node@v4` node 24 on
  BOTH smoke jobs (release.yml:413, 439).
- **AC-08**: N5 framing human-owned, bound to `--bundle` chain (inspection).
- **AG-1 / R-16**: legacy `--remote` documented-but-not-doc-tested — consciously accepted; sole owed
  mitigation (legacy marker) present.

### Check 4 — Architecture compliance (ADR-001..005)
**Status**: PASS

- **ADR-001**: extend-in-place; `release-gate-lib.sh` byte-unchanged; new failures fold into `fail()`
  exit 1 with distinct messages; no exit 5/6/7.
- **ADR-002**: emit (Rust) in throwaway container, consume (`init --bundle`, JS) host-side via
  repo-checkout client; `setup-node@v4` pinned. No `.rs`/image change (0 Rust files in diff).
- **ADR-003**: canonical chain only — `test_no_new_smoke_script`, single chain in the smoke script.
- **ADR-004**: uni-docs authorship-text-only widen; no drift-checker/CI-gate/Phase-4-trigger.
- **ADR-005**: process-boundary HOME isolation (HOME set on spawned child; `test_no_inprocess_home_
  mutation` static), clean-on-entry, proven by the non-vacuous negative control. No architectural
  drift.

### Check 5 — Knowledge stewardship (test phase)
**Status**: PASS

The test-phase stewardship block lives in `testing/RISK-COVERAGE-REPORT.md` (the tester's
deliverable): **Queried** — `context_briefing` surfacing #5180/#5183/#5189/#5192/#4977/#840 plus
the test-named-but-not-implemented lessons (#4202/#2656/#3548), all applied to verify the load-bearing
suites exist + are green and the R-07 control is non-vacuous; **Stored** — "nothing novel" with a
specific reason (patterns already at #5180/#5183/#5189/#5192/#4977 and vnc-041 AC-06/AC-02; faithful
reuse, re-storing would duplicate provenance). Reason present → PASS, not WARN.

## Notes on the environmental failures (both confirmed non-feature)

1. **Cargo workspace `ld ... signal 9 [Killed]`**: `git diff main...HEAD` shows **0** `.rs`/`crates/`
   files. nan-020 ships zero Rust code, so an OOM during link of `unimatrix-server` cannot be a
   nan-020 regression. The shipped release binary already exists and the gate logic ran green against
   stubs. Environmental memory exhaustion — correctly recorded as a baseline limitation, not a defect.
2. **`pytest -m smoke` single error** (`test_contradiction_detected`): server-init 10s ready-wait
   timeout under parallel load; passes in 8.29s in isolation; in the `contradiction` suite which
   nan-020 does not touch (no MCP/route/schema/tool change). USAGE-PROTOCOL triage to environmental
   flake (no GH issue, no xfail) is correct — NOT masking a feature bug. Effective smoke: 24/24
   behavioral pass.

## Rework Required

None.

## Scope Concerns

None. The pre-merge-provable surface is fully proven; the live container round-trip is the only
PENDING item and is correctly accepted as POST-TAG-CONFIRMABLE per #5189. C15 remains `partial`
pending the post-tag live run, as designed.
