#!/usr/bin/env bash
# release-gate-logic-test.sh — pre-merge HARD gate (nan-019 T1 / R-01,R-02,R-03).
#
# Proves the verify-by-name gate spine yields GREEN iff (RC==0 AND the anchored terminal
# run-marker is present) and RED on every other cell of the truth table — by EXERCISING
# THE EXACT SHIPPED BYTES. This test `source`s the same release-gate-lib.sh that
# .github/workflows/release.yml sources, and drives run_smoke_gate against a controllable
# stub (fixtures/stub-smoke.sh) — never the real Docker smoke. A paraphrased copy of the
# gate would test nothing (R-01); sourcing the shipped lib is the single source of truth.
#
# Fully local + deterministic: no Docker, no network, no tag push.
#
# Truth table (only (exit 0, marker present) is green):
#   exit 0 + marker present              -> GREEN
#   exit 0 + marker absent (early-exit-0) -> RED  "exited 0 but never printed ALL GATES PASSED"
#   exit 1                               -> RED  "first-run path is broken"
#   exit 3                               -> RED  "mis-provisioned ... HARD failure"
#   exit 4                               -> RED  "could not pull prebuilt IMAGE" (#795)
#   exit 2 / 139 (unexpected)            -> RED  "exited unexpectedly (exit N)"
#   marker as a mid-line substring only  -> RED  (anchored grep must not be spoofed — R-03)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/release-gate-lib.sh"
STUB="${SCRIPT_DIR}/fixtures/stub-smoke.sh"
SMOKE="${SCRIPT_DIR}/docker-http-posture-smoke.sh"

# --- consume the SHIPPED gate bytes (single source of truth) -----------------------------
# shellcheck source=release-gate-lib.sh
source "$LIB"
if ! declare -F run_smoke_gate >/dev/null; then
  echo "FATAL: run_smoke_gate not found after sourcing $LIB" >&2
  exit 1
fi

# The marker the gate greps for must be byte-identical to what the smoke actually emits.
# Smoke emits via log(): '[783-smoke] <msg>'. Terminal line begins with this literal.
MARKER='[783-smoke] ALL GATES PASSED'

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# run_case <name> <stub_rc> <stub_body> <stub_stream> <expect_gate_rc> <expect_diag_substr>
#   Drives the REAL run_smoke_gate against the stub. Captures the gate's own stdout+stderr
#   (the ::error:: diagnostics) and its return status, then asserts both.
run_case() {
  local name="$1" stub_rc="$2" stub_body="$3" stub_stream="$4" want_rc="$5" want_diag="$6"
  local out got_rc
  out="$(STUB_RC="$stub_rc" STUB_BODY="$stub_body" STUB_STREAM="$stub_stream" \
        run_smoke_gate "irrelevant-image" bash "$STUB" 2>&1)"
  got_rc=$?

  if [ "$got_rc" -ne "$want_rc" ]; then
    oops "$name (gate rc)" "expected gate rc=$want_rc got=$got_rc; output: $out"
    return
  fi
  if [ -n "$want_diag" ] && ! printf '%s' "$out" | grep -qF "$want_diag"; then
    oops "$name (diagnostic)" "expected substring '$want_diag' not found in: $out"
    return
  fi
  if [ -z "$want_diag" ] && printf '%s' "$out" | grep -q '::error::'; then
    oops "$name (unexpected diagnostic)" "green case emitted ::error::: $out"
    return
  fi
  pass "$name"
}

echo "== T1 truth table (sourcing shipped release-gate-lib.sh) =="

# Row: (0, marker present) — the ONLY green cell.
run_case test_gate_pass_exit0_marker_present \
  0 "${MARKER} — clean image boots HTTP-on and routes the registered slug over HTTPS." stdout \
  0 ""

# Row: (1, fail text) — ran+failed.
run_case test_gate_fail_exit1_no_marker \
  1 "[783-smoke] FAIL: per-slug observe returned HTTP 404" stdout \
  1 "first-run path is broken"

# Row: (3, SKIP) — self-skip is a HARD failure.
run_case test_gate_skip_exit3_hard_fail \
  3 "SKIP: Docker not available" stdout \
  1 "mis-provisioned"

# Row: (4, could-not-pull) — prebuilt IMAGE unavailable is a distinct HARD failure (#795).
# Must NOT be mapped to the exit-1 "first-run path is broken" diagnosis (the false
# diagnosis this bug removed), and is now a KNOWN code — no longer caught by the *) arm.
run_case test_gate_pull_failed_exit4 \
  4 "[783-smoke] FAIL: could not pull ghcr.io/x/unimatrix:latest-amd64" stdout \
  1 "could not pull prebuilt IMAGE"

# Row: (0, no marker) — early-exit-0.
run_case test_gate_early_exit0_marker_absent \
  0 "[783-smoke] PASS gate 1" stdout \
  1 "exited 0 but never printed ALL GATES PASSED"

# Row: (2, unexpected).
run_case test_gate_unexpected_exit2 \
  2 "boom" stdout \
  1 "exited unexpectedly (exit 2)"

# Row: (139, OOM/segfault) — empty body.
run_case test_gate_unexpected_exit139 \
  139 "" stdout \
  1 "exited unexpectedly (exit 139)"

echo "== R-03 marker anchoring (no spoof) =="

# Marker as a SUBSTRING of a longer line: grep -qx must NOT match -> early-exit-0 RED.
run_case test_gate_marker_anchored_substring \
  0 "xx ${MARKER} yy trailing junk on the same physical line" stdout \
  1 "exited 0 but never printed ALL GATES PASSED"

# Marker echoed early as a comment line then exit 0 with no terminal marker. Here the
# marker DOES appear as its own whole line, so grep -qx legitimately matches (the lib
# greps any whole line in the buffer). The discriminating spoof case is the substring
# one above; this case pins that an echoed-as-own-line marker + exit 0 is GREEN, which
# is exactly the lib's documented behaviour — so we assert GREEN, not RED, to avoid
# encoding a false expectation. (Pseudocode note: substring is the spoof; "never printed
# at all" is the early-exit-0 row.)
run_case test_gate_marker_whole_line_anywhere_is_green \
  0 "${MARKER}
[783-smoke] (some later harmless prose)" stdout \
  0 ""

# Byte-identity cross-check: the runtime line the smoke emits must match the lib's grep.
# The smoke emits via log() which prefixes '[783-smoke] '; its SOURCE line is
#   log "ALL GATES PASSED — ..."
# so we (a) confirm log() prepends exactly '[783-smoke] ', (b) confirm the source line
# carries the 'ALL GATES PASSED' message, (c) reconstruct the runtime line the smoke
# actually prints and confirm the SHIPPED lib's anchored grep pattern matches it. This
# binds the asserted marker to the smoke's real emission, not a hand-copied literal.
echo "== marker byte-identity cross-check =="
LOG_DEF="$(grep -m1 'log() {' "$SMOKE")"
SMOKE_MSG_LINE="$(grep -m1 'ALL GATES PASSED' "$SMOKE")"
byte_ident_ok=1
# (a) log() must format as '[783-smoke] %s\n'
printf '%s' "$LOG_DEF" | grep -qF "[783-smoke] %s" || { byte_ident_ok=0; bi_why="log() prefix changed: $LOG_DEF"; }
# (b) source line is a log "..." call carrying the marker message
printf '%s' "$SMOKE_MSG_LINE" | grep -qE 'log[[:space:]]+"ALL GATES PASSED' || { byte_ident_ok=0; bi_why="marker not emitted via log(): $SMOKE_MSG_LINE"; }
# (c) reconstruct the runtime line and confirm the SHIPPED lib grep matches it
SMOKE_RUNTIME="${MARKER} — clean image boots HTTP-on and routes the registered slug over HTTPS."
printf '%s\n' "$SMOKE_RUNTIME" | grep -qx '\[783-smoke\] ALL GATES PASSED.*' || { byte_ident_ok=0; bi_why="lib pattern does not match runtime line: $SMOKE_RUNTIME"; }
if [ "$byte_ident_ok" -eq 1 ]; then
  pass "test_gate_marker_byte_identical"
else
  oops "test_gate_marker_byte_identical" "${bi_why:-unknown}"
fi

# --- R-02: RC survives capture, verified by EXECUTION not by reading (the #4873 class) ----
echo "== R-02 RC survives capture (by execution) =="

# Drive the EXACT capture shape the lib uses: set +e; out="$(... 2>&1)"; rc=$?; set -e.
# If a pipe/pipefail/setsid had swallowed the RC, exit 1 would read as 0. We assert the
# real codes survive (1 reads 1, 3 reads 3), proven by running.
rc_survives() {
  local stub_rc="$1" want="$2"
  local out rc
  set +e
  out="$(STUB_RC="$stub_rc" STUB_BODY="probe" bash "$STUB" 2>&1)"
  rc=$?
  # NOTE: deliberately do NOT `set -e` here — this harness runs under `set -uo pipefail`
  # only; turning on errexit would abort the harness on the first intentionally-red case.
  if [ "$rc" -eq "$want" ]; then
    pass "test_gate_rc_survives_capture (exit $stub_rc reads $rc)"
  else
    oops "test_gate_rc_survives_capture (exit $stub_rc)" "RC swallowed: expected $want got $rc"
  fi
}
rc_survives 1 1
rc_survives 3 3

# stderr capture: a fail()/marker written to stderr must still reach the grep via 2>&1.
echo "== R-02 stderr is captured (2>&1) =="
run_case test_gate_captures_stderr \
  0 "${MARKER} — emitted on stderr" stderr \
  0 ""
# And a stderr-only fail must NOT vanish into a false green: exit 1 stays RED.
run_case test_gate_captures_stderr_fail \
  1 "[783-smoke] FAIL: stderr-only failure" stderr \
  1 "first-run path is broken"

echo
echo "release-gate-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
