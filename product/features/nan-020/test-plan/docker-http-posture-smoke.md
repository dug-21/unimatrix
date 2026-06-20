# Test Plan — `docker-http-posture-smoke.sh` (Gates 5–7 extension)

> Component: in-place extension of the nan-019 smoke with Gates 5–7 (in-container bundle emit,
> host hermetic `init --bundle` consume, hook-fire observe round-trip into the per-slug store).
> Risks: **R-01 (Crit)**, R-02, **R-03 (Crit)**, R-04, R-05, R-06, R-15. ACs: AC-03/04/05/06 (+09 split).
> All gate-logic assertions are **stub-driven, pre-merge-provable** via the existing
> `scripts/release-gate-logic-test.sh` convention; the live round-trip is POST-TAG-CONFIRMABLE.

## Test Vehicle

Extend `scripts/release-gate-logic-test.sh` (cumulative). New stub fixtures:
`fixtures/stub-client-bundle.sh`, `fixtures/stub-init-bundle.sh`, observe/store stub or env hooks.
Delivery MUST factor the Gate 5–7 logic so the external commands (`docker run … client-bundle`,
`node … init --bundle`, the observe POST + store `du`) are env-injectable (the nan-019
`run_smoke_gate SMOKE_CMD…` indirection pattern, #5192). No new test framework; no Docker in these.

---

## R-01 (Critical) — New bundle-attach failure must NOT green the gate

Each row drives one forced condition through the Gate 5–7 logic and asserts: reaches `fail()` →
**exit 1**, prints **NO** `[783-smoke] ALL GATES PASSED` marker, and `run_smoke_gate` returns non-zero.

| Test name | Forced condition | Assert |
|-----------|------------------|--------|
| `test_gate5_emit_rc_nonzero_fails` | stub-client-bundle rc≠0 | exit 1, no marker |
| `test_gate5_empty_blob_fails` | stub emits empty stdout | exit 1, no marker |
| `test_gate5_wrong_prefix_blob_fails` | stub emits text without `unimatrix-bundle:` | exit 1, no marker |
| `test_gate6_init_rc_nonzero_fails` | stub-init-bundle rc≠0 | exit 1, no marker |
| `test_gate7_observe_non204_fails` | observe stub returns 200/404/500/501 | exit 1, no marker |
| `test_gate7_store_no_grow_fails` | store delta = 0 | exit 1, no marker |

- `test_happy_path_is_only_green` (**negative-control / discrimination**): all stubs succeed +
  blob valid + observe 204 + store delta>0 ⇒ exit 0 AND marker. Then flip EACH single condition and
  assert green→red. The happy combo is the ONLY exit-0+marker cell.
- `test_no_early_exit0_before_marker`: inject a forced `exit 0` after Gate 4 but before Gate 7
  completes; assert `run_smoke_gate`'s `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` fails the gate
  (AC-06). The marker must not be reachable on any partial path.

**Coverage requirement:** every ADR-001 new-failure-mode row has a stub test proving exit-1 +
marker-suppressed; zero new modes reach exit 0. (R-01 — PENDING here IS a gap.)

---

## R-02 (High) — Distinct, attributable fail messages (the only attributability mechanism)

For each forced condition, assert the emitted `[783-smoke] FAIL:` message contains its **unique**
ADR-001 prefix; assert no two share a message.

| Test name | Condition | Required message substring |
|-----------|-----------|----------------------------|
| `test_msg_emit_failed` | emit rc≠0 | `client-bundle emit failed (rc=` AND names `client-bundle` (SR-02) |
| `test_msg_invalid_blob` | empty/wrong-prefix | `client-bundle produced no/invalid bundle blob` |
| `test_msg_init_failed` | init rc≠0 | `init --bundle failed (rc=` |
| `test_msg_observe_code` | observe 500 | `observe returned HTTP 500 (expected 204)` (distinguishes doc-drift from route change — SR-09) |
| `test_msg_store_no_grow` | delta 0 | `bundle-path observe did not land in per-slug store` |
| `test_msg_emit_vs_attach_distinct` | run emit-fail then attach-fail | the two messages differ (Gate-5 emit vs Gate-6 attach attribution; ties R-05) |

---

## R-03 (Critical) — Do NOT regress nan-019 Gates 1–4 / the load-bearing contract

- `test_nan019_truth_table_invariant`: re-run the existing nan-019 {0,1,3,4}×{marker present/absent}
  truth table against the EXTENDED script's gate path; only (0, marker) green; 3→HARD-fail,
  4→HARD-fail (`IMAGE=` arm, #5208), 1→fail, early-exit-0→fail, substring-marker→fail. All
  existing rows in `release-gate-logic-test.sh` must still pass post-extension.
- `test_rc_survives_capture_by_execution`: drive the new gate path through the lib's
  `set +e; out="$(… 2>&1)"; rc=$?; set -e` capture and assert exit 1 reads **1**, exit 3 reads **3**
  — by RUNNING, not reading YAML (the #4873 swallow class; only execution catches it).
- `test_append_only_gate4_precondition`: force a Gate-4 failure and assert the script fails AT
  Gate 4 with the existing nan-019 message, BEFORE any Gate-5 code runs. Gates 5–7 execute only
  after Gate 4 passes.
- `test_single_terminal_marker`: assert `[783-smoke] ALL GATES PASSED` appears **exactly once**, is
  the **last** line, and the new gates print BEFORE it (no second marker).
- `test_run_smoke_gate_byte_unchanged`: diff-assert `release-gate-lib.sh::run_smoke_gate` is
  byte-identical to the nan-019 baseline (ADR-001: wrapper not modified). See `release-yml-setup-node.md`
  note — this is a static diff in the test script; the lib file is in `Files to Modify` as UNCHANGED.

---

## R-04 (High) — Host node absence / client provenance (live version-compat is PENDING)

- `test_node_absent_hard_fails_exit1`: force `command -v node` to miss; assert `fail()` exit 1 with
  `node not available — the documented init --bundle path cannot be exercised`. **NOT exit 3** —
  node-absence is a mis-provisioned lane, same class as Docker-absent (#5180: a missing prerequisite
  hard-fails, never greens). (The `setup-node@v4` pin is the first defense — see
  `release-yml-setup-node.md`; this preflight is the backstop. Defense in depth.)
- `test_gate6_invokes_repo_checkout_client`: assert Gate 6 invokes
  `node packages/unimatrix/bin/unimatrix.js init …` (the repo-checkout shipped bytes, NFR-4), not a
  global/npm-installed `unimatrix`. Static assertion on the smoke source.
- **PENDING-post-tag:** node-present-but-wrong-major-version compatibility against the live `init`
  client — confirmed on the hosted runner with pinned `setup-node@v4` node 24, never asserted pre-merge.

---

## R-05 (High) — Bundle blob handoff corruption across container→host

- `test_capture_stdout_only_not_stderr`: stub emits the blob on stdout AND a token-redacted
  URL/fingerprint echo on stderr; assert only stdout is captured into `BUNDLE` — the stderr echo is
  NOT folded into the blob (preserves token redaction; security surface §Security).
- `test_blob_prefix_validated`: stub emits non-`unimatrix-bundle:` stdout; assert Gate-5 hard-fails
  with `produced no/invalid bundle blob` (shares the R-01 invalid-blob row, asserted from the
  handoff-validation angle).
- `test_blob_quoting_safe`: stub emits a blob containing shell-significant chars (`$`, spaces,
  `;`) and a trailing newline; assert it reaches `init --bundle "$BUNDLE"` intact — passed quoted,
  no word-splitting, no `eval`.
- `test_empty_capture_guard`: stub emits empty stdout; assert hard-fail at Gate 5, NOT an empty
  string passed to `init` that errors generically (attribution stays at the emit step).

**Coverage requirement:** blob validated (prefix + non-empty) at the boundary; stderr excluded;
handoff quoting-safe; a corrupt handoff fails with a **Gate-5 (emit)** message, not a Gate-6 message.

---

## R-06 (Medium) — `client-bundle` rename/absence (coupling nan-020 doesn't own)

- `test_pinned_invocation_form`: static assert the smoke invokes the exact verified form
  `unimatrix --project-dir /data client-bundle <slug>` (main.rs:293,437; A1 CONFIRMED).
- `test_rename_named_failure`: stub-client-bundle rc≠0; assert the failure NAMES `client-bundle`
  (correct, attributable red on a future server-side rename — not a confusing attach error).
- **PENDING-post-tag:** that the subcommand actually exists in the **shipped image** (image-presence)
  — confirmed by the live emit on the tag run.

---

## R-15 (Low) — No silent second image build / no new script

- `test_reuse_single_boot`: assert Gates 5–7 reuse the SAME `$CNAME`/`$VOL`/`$SLUG`/`$PORT`/token/cert
  as Gates 1–4 — no second `docker run -d`/`docker build` for the round-trip (D-2; NFR-7). Static
  assertion on the smoke source (the emit container is a `--rm` throwaway off the same image/volume,
  which is permitted; a second *booted* server is not).
- `test_no_new_script_filecount` (AC-04): assert file-count delta under `scripts/` is **0 new
  scripts** beyond the existing test/fixtures additions; the round-trip lives inside
  `docker-http-posture-smoke.sh`. A sibling is allowed ONLY if the FR-10 divergence caveat triggers
  AND is documented (it does not here).

---

## AC-05 — Docker absent + every new skip path hard-fails

- `test_docker_absent_exit3` (inherited nan-019): Docker-absent ⇒ smoke exit 3 ⇒ `run_smoke_gate`
  HARD-fails (`::error::smoke SKIPPED (exit 3) … HARD failure`). Unchanged; assert still holds.
- Each new skip path (node absent, emit absent/empty blob, observe non-204, store-no-grow) yields a
  distinct `fail()` exit 1 + failing gate — covered by R-01/R-02/R-04 rows above.

## Live round-trip (AC-03) — POST-TAG-CONFIRMABLE

The two-leg done_when (ACCEPTANCE-MAP §AC-03→C15): Gate 6 pinned `Ping` (MCP-round-trip leg) +
Gate 7 204 + per-slug store delta (observe-landing leg). Both run **hermetically** (see
`hermeticity-negative-control.md`). Pre-merge: gate logic proven via stubs (above). Live:
*"configured + verified locally; GH execution confirmed post-tag."* Never asserted as executed
pre-merge (#4796). C15 stays `partial` until the post-tag live run greens both legs.

## Self-Check

- [x] Every ADR-001 new-failure row has a stub test (exit-1 + marker-suppressed) — R-01.
- [x] Each row's distinct message asserted; no two share — R-02.
- [x] nan-019 truth table + RC-survival + append-only + single-marker + wrapper-unchanged — R-03.
- [x] node-absent→exit-1 (not 3) + repo-checkout client — R-04.
- [x] stdout-only capture + blob validation + quoting-safe + empty guard — R-05.
- [x] Live round-trip labeled POST-TAG-CONFIRMABLE, never pre-merge-asserted.
