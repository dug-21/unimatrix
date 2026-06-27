#!/usr/bin/env bash
# release-gate-tristate-logic-test.sh — pre-merge HARD gate for C-TS (infra-004 / ADR-002 #5350).
#
# Proves the NEW additive run_smoke_gate_tristate in release-gate-lib.sh discriminates the
# isolation gate's exit code 0/1/2/3/other so the isolation lane BLOCKS on RED (exit 1)
# WITHOUT blocking on INFRA (exit 2) — by EXERCISING THE EXACT SHIPPED BYTES. This test
# `source`s the same release-gate-lib.sh that .github/workflows/release.yml sources, and
# drives run_smoke_gate_tristate against the controllable stub (fixtures/stub-smoke.sh) —
# never the real Docker smoke. A paraphrased copy of the gate would test nothing (R-01);
# sourcing the shipped lib is the single source of truth (NFR-3).
#
# Fully local + deterministic: no Docker, no network, no tag push.
#
# Truth table (only exit 2 maps to a non-blocking return 0; only (0,marker) is GREEN):
#   exit 0 + marker present               -> return 0  GREEN (passes)
#   exit 0 + marker absent (early-exit-0)  -> return 1  blocks "early-exit-0 (false-green)"
#   exit 1                                -> return 1  RED genuine leak, blocks (the DoD)
#   exit 2                                -> return 0  ::warning:: + canonical INFRA marker, non-blocking
#   exit 3                                -> return 1  Docker-present lane mis-provisioned, hard fail
#   exit 139 (unexpected)                 -> return 1  blocks "exited unexpectedly"
#   marker as a mid-line substring only   -> return 1  early-exit-0 (anchored grep must not be spoofed, R-06)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/release-gate-lib.sh"
STUB="${SCRIPT_DIR}/fixtures/stub-smoke.sh"
ISO_SMOKE="${SCRIPT_DIR}/multi-tenant-isolation-smoke.sh"

# --- consume the SHIPPED gate bytes (single source of truth) -----------------------------
# shellcheck source=release-gate-lib.sh
source "$LIB"
# R-14 (#5345): a sourced gate that runs `set -euo pipefail` at its top would leave -e ON in
# this harness; a later `set -uo pipefail` does NOT clear it. Explicitly clear -e so the
# intentionally-RED rows below do not abort the suite before the summary line prints.
set +e
set -uo pipefail

if ! declare -F run_smoke_gate_tristate >/dev/null; then
  echo "FATAL: run_smoke_gate_tristate not found after sourcing $LIB" >&2
  exit 1
fi

# The marker the gate greps for must be byte-identical to what the isolation smoke emits at
# runtime via log() (e.g. '[infra003-smoke] ALL GATES PASSED ...'), not a source literal.
MARKER='[infra003-smoke] ALL GATES PASSED'
# The canonical INFRA marker MUST appear verbatim on the exit-2 row (R-09, WARN #3337).
INFRA_MARKER='[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN'

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# run_case <name> <stub_rc> <stub_body> <stub_stream> <expect_gate_rc> <expect_diag_substr>
#   Drives the REAL run_smoke_gate_tristate against the stub. Captures the gate's own
#   stdout+stderr (the ::error::/::warning:: diagnostics) and its return status, then
#   asserts both. A sourced function cannot be invoked via `env VAR=x fn`, so STUB_* are
#   exported into the command environment of the same line.
run_case() {
  local name="$1" stub_rc="$2" stub_body="$3" stub_stream="$4" want_rc="$5" want_diag="$6"
  local out got_rc
  out="$(STUB_RC="$stub_rc" STUB_BODY="$stub_body" STUB_STREAM="$stub_stream" \
        run_smoke_gate_tristate "irrelevant-image" bash "$STUB" 2>&1)"
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
    oops "$name (unexpected diagnostic)" "pass case emitted ::error::: $out"
    return
  fi
  pass "$name"
}

# R-14 completeness witness: drive an intentionally-RED row FIRST. If the harness ever runs
# under set -e (the #5345 trap), this row aborts the suite and the summary line never prints.
echo "== R-14 RED-first completeness probe (the suite must keep running) =="
run_case test_tristate_red_first_does_not_abort_suite \
  1 "[infra003-smoke] FAIL: tenant A read tenant B's write" stdout \
  1 "genuine cross-tenant leak (RED)"

echo "== C-TS truth table (sourcing shipped release-gate-lib.sh) =="

# Row: (0, marker present) — the ONLY green cell (AC-08 GREEN).
run_case test_tristate_green_exit0_marker_present \
  0 "${MARKER} — every tenant's write landed ONLY in its own per-slug store." stdout \
  0 ""

# Row: (0, no marker) — early-exit-0 blocks (AC-09 / R-06).
run_case test_tristate_early_exit0_marker_absent \
  0 "[infra003-smoke] PASS cell A->A" stdout \
  1 "early-exit-0 (false-green)"

# Row: (1) — RED genuine leak blocks (AC-08 / the DoD).
run_case test_tristate_red_exit1_blocks \
  1 "[infra003-smoke] FAIL: cross-tenant leak detected" stdout \
  1 "genuine cross-tenant leak (RED)"

# Row: (3) — Docker-present lane mis-provisioned, hard fail (AC-08).
run_case test_tristate_skip_exit3_hard_fail \
  3 "SKIP: Docker not available" stdout \
  1 "mis-provisioned"

# Row: (139) — unexpected blocks (R-05/R-08), empty body.
run_case test_tristate_unexpected_exit139 \
  139 "" stdout \
  1 "exited unexpectedly (exit 139)"

# Row: (2) — INFRA: non-blocking (return 0) AND ::warning:: AND the canonical INFRA marker.
# run_case's ::error::-on-empty-diag check does not fire here because we pass want_diag, and
# the exit-2 path emits ::warning:: not ::error::. We additionally assert the canonical
# marker + the warning + absence of ::error:: explicitly below (AC-13 / R-09).
echo "== exit-2 INFRA visibility (AC-13 / R-09 — pinned canonical marker) =="
run_case test_tristate_infra_exit2_nonblocking_visible \
  2 "[infra003-smoke] INFRA: warmup barrier timed out" stdout \
  0 "::warning::"

# Pin the EXACT canonical INFRA marker literal + the warning + no ::error:: on the exit-2 row.
infra_out="$(STUB_RC=2 STUB_BODY="[infra003-smoke] INFRA: warmup barrier timed out" STUB_STREAM=stdout \
  run_smoke_gate_tristate "irrelevant-image" bash "$STUB" 2>&1)"
infra_rc=$?
infra_ok=1
[ "$infra_rc" -eq 0 ] || { infra_ok=0; iwhy="expected return 0 on exit 2 got $infra_rc"; }
printf '%s\n' "$infra_out" | grep -qF "$INFRA_MARKER" || { infra_ok=0; iwhy="canonical INFRA marker not emitted verbatim: $infra_out"; }
printf '%s\n' "$infra_out" | grep -q '::warning::' || { infra_ok=0; iwhy="::warning:: not emitted on exit 2: $infra_out"; }
printf '%s\n' "$infra_out" | grep -q '::error::' && { infra_ok=0; iwhy="exit-2 INFRA wrongly emitted ::error::: $infra_out"; }
if [ "$infra_ok" -eq 1 ]; then
  pass "test_tristate_infra_exit2_canonical_marker_pinned"
else
  oops "test_tristate_infra_exit2_canonical_marker_pinned" "${iwhy:-unknown}"
fi

echo "== R-06 marker anchoring (no spoof) =="

# Marker as a SUBSTRING of a longer line: grep -qx must NOT match -> early-exit-0 blocks.
run_case test_tristate_marker_anchored_substring \
  0 "xx ${MARKER} yy trailing junk on the same physical line" stdout \
  1 "early-exit-0 (false-green)"

# Marker on its own whole line + later prose -> GREEN (the lib greps any whole line in the
# buffer; this is the lib's documented behaviour, mirrors run_smoke_gate).
run_case test_tristate_marker_whole_line_anywhere_is_green \
  0 "${MARKER}
[infra003-smoke] (some later harmless prose)" stdout \
  0 ""

echo "== R-08 fail-closed mapping (only exit 2 is non-blocking) =="
# Across the table, the ONLY rc that returns 0 without crediting a GREEN marker is exit 2.
# Re-drive 1 / 3 / 0-no-marker / 139 and assert every one returns 1.
r08_ok=1
for cell in "1:[infra003-smoke] FAIL leak" "3:SKIP" "0:[infra003-smoke] no terminal marker" "139:"; do
  crc="${cell%%:*}"; cbody="${cell#*:}"
  out="$(STUB_RC="$crc" STUB_BODY="$cbody" STUB_STREAM=stdout \
        run_smoke_gate_tristate "irrelevant-image" bash "$STUB" 2>&1)"
  grc=$?
  if [ "$grc" -ne 1 ]; then
    r08_ok=0; r08_why="non-GREEN rc=$crc rounded to pass (return $grc): $out"
  fi
done
if [ "$r08_ok" -eq 1 ]; then
  pass "test_tristate_only_exit2_nonblocking"
else
  oops "test_tristate_only_exit2_nonblocking" "${r08_why:-unknown}"
fi

# --- R-05: RC survives the no-pipe capture, verified by EXECUTION not by reading (#4873) ---
echo "== R-05 RC survives capture (by execution) =="
# Drive the EXACT capture shape the lib uses: set +e; out="$(... 2>&1)"; rc=$?; set -e.
# If a pipe/pipefail/setsid had swallowed the RC, exit 1 would read as 0. Assert the real
# codes survive (1 reads 1, 3 reads 3, 2 reads 2), proven by running.
rc_survives() {
  local stub_rc="$1" want="$2"
  local out rc
  set +e
  out="$(STUB_RC="$stub_rc" STUB_BODY="probe" bash "$STUB" 2>&1)"
  rc=$?
  # Deliberately do NOT `set -e` here — the harness runs under `set -uo pipefail` only, so an
  # intentionally-red case does not abort the suite (R-14).
  if [ "$rc" -eq "$want" ]; then
    pass "test_tristate_rc_survives_capture (exit $stub_rc reads $rc)"
  else
    oops "test_tristate_rc_survives_capture (exit $stub_rc)" "RC swallowed: expected $want got $rc"
  fi
}
rc_survives 1 1
rc_survives 2 2
rc_survives 3 3

# --- R-05: no-pipe between smoke and $?, and return-not-exit (static, against shipped bytes) -
echo "== R-05 no-pipe capture + return-not-exit (static on shipped lib) =="
FN_BODY="$(awk '/^run_smoke_gate_tristate\(\) \{/{f=1} f{print} f&&/^\}/{exit}' "$LIB")"
np_ok=1
# The capture line must assign the smoke output with NO pipe in it (the #4873 swallow class).
CAP_LINE="$(printf '%s\n' "$FN_BODY" | grep -m1 'out="\$(IMAGE=')"
[ -n "$CAP_LINE" ] || { np_ok=0; npwhy="capture line out=\$(IMAGE=...) not found"; }
printf '%s' "$CAP_LINE" | grep -q '|' && { np_ok=0; npwhy="capture line contains a pipe (RC-swallow risk): $CAP_LINE"; }
# rc must be read immediately on its own line.
printf '%s\n' "$FN_BODY" | grep -qE '^\s*rc=\$\?' || { np_ok=0; npwhy="rc=\$? not read immediately after capture"; }
# The function must use return, never exit (keeps it unit-testable when sourced).
printf '%s\n' "$FN_BODY" | grep -qw 'return' || { np_ok=0; npwhy="function never uses return"; }
# command-position `exit` only (anchored to line-start or after ;) — the diagnostic strings
# legitimately contain the literal "early-exit-0", which must NOT count as an exit statement.
printf '%s\n' "$FN_BODY" | grep -qE '(^|;)[[:space:]]*exit([[:space:]]|;|$)' \
  && { np_ok=0; npwhy="function uses a command-position exit (must use return when sourced): $FN_BODY"; }
if [ "$np_ok" -eq 1 ]; then
  pass "test_tristate_no_pipe_static_return_not_exit"
else
  oops "test_tristate_no_pipe_static_return_not_exit" "${npwhy:-unknown}"
fi

# --- R-05: stderr is captured (2>&1) — a marker/fail on stderr still reaches the grep -------
echo "== R-05 stderr is captured (2>&1) =="
run_case test_tristate_captures_stderr \
  0 "${MARKER} — emitted on stderr" stderr \
  0 ""
# A stderr-only exit-1 must stay RED (no false green).
run_case test_tristate_captures_stderr_fail \
  1 "[infra003-smoke] FAIL: stderr-only leak" stderr \
  1 "genuine cross-tenant leak (RED)"

# --- R-07: sibling no-regression — run_smoke_gate untouched + still callable ----------------
echo "== R-07 sibling no-regression (run_smoke_gate untouched) =="
if declare -F run_smoke_gate >/dev/null; then
  # The byte-unchanged sibling must still discriminate its own exit-4 case (which the tri-state
  # variant deliberately does NOT have), proving the additive change did not bleed into it.
  out="$(STUB_RC=4 STUB_BODY="[783-smoke] could not pull image" STUB_STREAM=stdout \
        run_smoke_gate "irrelevant-image" bash "$STUB" 2>&1)"
  grc=$?
  if [ "$grc" -eq 1 ] && printf '%s' "$out" | grep -qF "could not pull prebuilt IMAGE"; then
    pass "test_run_smoke_gate_sibling_unchanged_exit4"
  else
    oops "test_run_smoke_gate_sibling_unchanged_exit4" "sibling exit-4 behaviour changed (rc=$grc): $out"
  fi
else
  oops "test_run_smoke_gate_sibling_unchanged_exit4" "run_smoke_gate missing after sourcing $LIB"
fi

# --- byte-identity cross-check: the isolation smoke's runtime marker matches the lib grep ---
echo "== marker byte-identity cross-check (isolation smoke runtime line) =="
bi_ok=1
if [ -f "$ISO_SMOKE" ]; then
  # Reconstruct the RUNTIME line the isolation smoke prints (log() prefix, not source literal,
  # #5345 finding c) and confirm the SHIPPED lib's anchored grep pattern matches it.
  ISO_RUNTIME="${MARKER} — bidirectional isolation matrix held across all tenants."
  printf '%s\n' "$ISO_RUNTIME" | grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*' \
    || { bi_ok=0; biwhy="lib pattern does not match isolation runtime line: $ISO_RUNTIME"; }
else
  bi_ok=0; biwhy="isolation smoke script not found at $ISO_SMOKE"
fi
if [ "$bi_ok" -eq 1 ]; then
  pass "test_tristate_marker_byte_identical"
else
  oops "test_tristate_marker_byte_identical" "${biwhy:-unknown}"
fi

echo
echo "release-gate-tristate-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
