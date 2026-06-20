#!/usr/bin/env bash
# release-gate-bundle-logic-test.sh — pre-merge HARD gate for the nan-020 Gates 5–7
# documented-bundle-attach extension (R-01/R-02/R-03/R-04/R-05/R-07 + AC-09).
#
# Companion to release-gate-logic-test.sh (nan-019 Gates 1–4 spine). This file
# proves the Gate 5–7 LOGIC by EXERCISING THE EXACT SHIPPED BYTES: it `source`s
# docker-http-posture-smoke.sh (whose sourced-guard suppresses Gates 1–4) and
# drives the real bundle_attach_gates() against env-injected stubs — never Docker,
# never node, never the network. A paraphrased copy of the gate would test nothing
# (R-01); sourcing the shipped script is the single source of truth (#5192).
#
# It ALSO drives the FULL run_smoke_gate (sourced from release-gate-lib.sh) against
# a wrapper smoke so marker-suppression-on-failure and the byte-unchanged wrapper
# diff are proven end-to-end.
#
# Covers: the Gate 5–7 exit-code truth table, each distinct failure message,
# marker-suppressed-on-failure, run_smoke_gate byte-unchanged, and the REQUIRED
# hermeticity NEGATIVE CONTROL (poison stale cred at a fake REAL ~/.unimatrix +
# broken attach => Gate 7 STILL FAILS) with a positive twin and a >=5x non-flaky /
# discriminating store-grew proof. ADR-005: classifying the negative control
# PENDING IS a gap — it is proven here pre-merge.
#
# Fully local + deterministic: no Docker, no network, no tag push, no node.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/release-gate-lib.sh"
SMOKE="${SCRIPT_DIR}/docker-http-posture-smoke.sh"
FIX="${SCRIPT_DIR}/fixtures"

STUB_EMIT="${FIX}/stub-client-bundle.sh"
STUB_INIT="${FIX}/stub-init-bundle.sh"
STUB_HOOK="${FIX}/stub-hook-fire.sh"
STUB_STORE="${FIX}/stub-store-size.sh"

MARKER='[783-smoke] ALL GATES PASSED'

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# ---- source the SHIPPED smoke bytes (single source of truth) ----------------
# The sourced-guard in the smoke stops before the Docker preflight, so sourcing
# defines bundle_attach_gates() + the seam helpers WITHOUT running Gates 1–4.
# Save/restore shell opts because the smoke runs `set -euo pipefail` at file top.
set +e
# shellcheck source=docker-http-posture-smoke.sh
source "$SMOKE"
set +e; set +u 2>/dev/null; set -uo pipefail   # restore THIS harness's opts
if ! declare -F bundle_attach_gates >/dev/null; then
  echo "FATAL: bundle_attach_gates not found after sourcing $SMOKE" >&2
  exit 1
fi

# ---- driver: run bundle_attach_gates() in an isolated subshell against stubs --
# Returns the gate rc and captures stdout+stderr (the PASS log + fail() message).
# Each call gets a fresh SANDBOX-like temp + a fresh isolated/real HOME pair so
# the truth-table rows do not cross-contaminate.
#
# run_bundle_case <name> <expect_rc> <expect_substr> -- <env assignments...>
run_bundle_case() {
  local name="$1" want_rc="$2" want_sub="$3"; shift 3
  [ "${1:-}" = "--" ] && shift
  local tmp out got_rc
  tmp="$(mktemp -d)"
  # Defaults that make the happy path green; rows override via the trailing env.
  out="$(
    env \
      SLUG_DIR="/data/.unimatrix/arch-research" \
      SMOKE_EMIT_CMD="bash $STUB_EMIT" \
      SMOKE_INIT_CMD="bash $STUB_INIT" \
      SMOKE_HOOK_CMD="bash $STUB_HOOK" \
      SMOKE_STORE_SIZE_CMD="bash $STUB_STORE" \
      STUB_HOOK_STORE_FILE="$tmp/store" \
      "$@" \
      bash -c '
        set -uo pipefail
        source "'"$SMOKE"'" >/dev/null 2>&1 || true
        bundle_attach_gates
      ' 2>&1
  )"
  got_rc=$?
  rm -rf "$tmp"

  if [ "$got_rc" -ne "$want_rc" ]; then
    oops "$name (rc)" "expected rc=$want_rc got=$got_rc; output: $out"
    return
  fi
  if [ -n "$want_sub" ] && ! printf '%s' "$out" | grep -qF "$want_sub"; then
    oops "$name (substr)" "expected '$want_sub' in: $out"
    return
  fi
  pass "$name"
}

echo "== nan-020 Gate 5–7 truth table (sourcing shipped docker-http-posture-smoke.sh) =="

# Happy path: the ONLY exit-0 combination.
run_bundle_case test_gate567_happy_path_exit0 0 "PASS gate 7" --

# R-01: every new failure mode => exit 1, distinct message.
run_bundle_case test_gate5_emit_rc_nonzero_fails 1 \
  "client-bundle emit failed (rc=7)" -- STUB_EMIT_RC=7

run_bundle_case test_gate5_empty_blob_fails 1 \
  "client-bundle produced no/invalid bundle blob" -- STUB_EMIT_STDOUT=

run_bundle_case test_gate5_wrong_prefix_blob_fails 1 \
  "client-bundle produced no/invalid bundle blob" -- STUB_EMIT_STDOUT=not-a-bundle

run_bundle_case test_gate6_init_rc_nonzero_fails 1 \
  "init --bundle failed (rc=9) — bundle attach broken" -- STUB_INIT_RC=9

run_bundle_case test_gate7_observe_non204_fails 1 \
  "documented bundle attach observe returned HTTP 500 (expected 204)" -- \
  STUB_HOOK_OBSERVE_CODE=500

run_bundle_case test_gate7_store_no_grow_fails 1 \
  "bundle-path observe did not land in per-slug store" -- STUB_INIT_WRITE_CRED=0

echo "== R-02 distinct attributable messages (no two share) =="
# Gate-5 emit-fail vs Gate-6 attach-fail attribution is distinct (ties R-05).
run_bundle_case test_msg_emit_vs_attach_distinct_emit 1 \
  "client-bundle emit failed" -- STUB_EMIT_RC=1
run_bundle_case test_msg_emit_vs_attach_distinct_attach 1 \
  "init --bundle failed" -- STUB_INIT_RC=1

echo "== R-05 blob handoff (stdout-only, quoting-safe) =="
# stdout-only: the stub emits a token-redacted echo on stderr; assert it is NOT
# folded into the blob and the gate still greens (the blob came from stdout only).
run_bundle_case test_capture_stdout_only_not_stderr 0 "PASS gate 5" -- \
  STUB_EMIT_STDERR="token=SECRET-SHOULD-NOT-LEAK"
# And the leaked token must never appear in the captured gate output.
test_stdout_only_no_token_leak() {
  local tmp out
  tmp="$(mktemp -d)"
  out="$(
    env SLUG_DIR="/data/.unimatrix/arch-research" \
      SMOKE_EMIT_CMD="bash $STUB_EMIT" SMOKE_INIT_CMD="bash $STUB_INIT" \
      SMOKE_HOOK_CMD="bash $STUB_HOOK" SMOKE_STORE_SIZE_CMD="bash $STUB_STORE" \
      STUB_HOOK_STORE_FILE="$tmp/store" \
      STUB_EMIT_STDERR="token=SECRET-SHOULD-NOT-LEAK" \
      bash -c 'set -uo pipefail; source "'"$SMOKE"'" >/dev/null 2>&1 || true; bundle_attach_gates' 2>&1
  )"
  rm -rf "$tmp"
  if printf '%s' "$out" | grep -qF "SECRET-SHOULD-NOT-LEAK"; then
    oops "test_stdout_only_no_token_leak" "stderr token leaked into gate output: $out"
  else
    pass "test_stdout_only_no_token_leak"
  fi
}
test_stdout_only_no_token_leak

# Quoting-safe: a blob carrying shell-significant chars + trailing content reaches
# init --bundle intact (passed quoted, no word-splitting, no eval).
run_bundle_case test_blob_quoting_safe 0 "PASS gate 6" -- \
  STUB_EMIT_STDOUT='unimatrix-bundle:v2 a;b $x `y` "z"'

echo "== R-04 node-absent hard-fails exit 1 (NOT exit 3) =="
# Force command -v node to miss by shadowing PATH with an empty dir + a 'command'
# that reports node absent. Simplest portable approach: run with a PATH that has
# no node and assert the node-preflight fires. We point PATH at a dir with only
# the tools the gate needs (bash builtins suffice for the preflight branch).
test_node_absent_hard_fails_exit1() {
  local tmp out rc
  tmp="$(mktemp -d)"
  # A PATH dir containing a fake `command`-less env: we cannot easily strip the
  # `command` builtin, so instead stub node-absence by overriding the seam:
  # set PATH to a dir with NO node so `command -v node` returns non-zero.
  mkdir -p "$tmp/bin"
  out="$(
    env -i PATH="$tmp/bin:/usr/bin:/bin" HOME="$tmp/home" \
      SLUG_DIR="/data/.unimatrix/arch-research" \
      bash -c '
        set -uo pipefail
        # Ensure node really is not resolvable on this PATH.
        if command -v node >/dev/null 2>&1; then echo "PRECOND-FAIL: node present"; exit 99; fi
        source "'"$SMOKE"'" >/dev/null 2>&1 || true
        bundle_attach_gates
      ' 2>&1
  )"
  rc=$?
  rm -rf "$tmp"
  if [ "$rc" -eq 99 ]; then
    oops "test_node_absent_hard_fails_exit1" "could not construct node-absent PATH (node leaked in)"
    return
  fi
  if [ "$rc" -ne 1 ]; then
    oops "test_node_absent_hard_fails_exit1 (rc)" "expected exit 1 (NOT 3), got $rc: $out"
    return
  fi
  if ! printf '%s' "$out" | grep -qF "node not available — the documented init --bundle path cannot be exercised"; then
    oops "test_node_absent_hard_fails_exit1 (msg)" "missing node-absent message: $out"
    return
  fi
  pass "test_node_absent_hard_fails_exit1"
}
test_node_absent_hard_fails_exit1

echo "== R-04/R-07/R-03/R-15 static (source/YAML grep) assertions =="
echo "   -> see release-gate-bundle-static-test.sh (split for the 500-line limit)"

echo "== R-07 REQUIRED NEGATIVE CONTROL (poison + break => STILL RED) =="
# Poison: pre-seed a stale, valid-LOOKING credential where a NON-isolated run
# would read it — a FAKE "real" HOME's ~/.unimatrix (never the developer's real
# home). Break: stub-init writes NO fresh cred into the isolated $SANDBOX/home.
# Assert STILL-RED: with isolation working the stale cred is unreachable (wrong
# HOME) so Gate 7 sees no fresh write and fails.
test_hermeticity_negative_control_still_red() {
  local fakehome out rc poison
  fakehome="$(mktemp -d)"
  poison="$fakehome/.unimatrix/stub-hash/remote.json"
  mkdir -p "$(dirname "$poison")"
  printf '{"observe_url":"https://localhost:18443/v1/arch-research/observe","token":"STALE","fingerprint":"sha256:stale"}\n' > "$poison"
  local tmp; tmp="$(mktemp -d)"
  out="$(
    env HOME="$fakehome" \
      SLUG_DIR="/data/.unimatrix/arch-research" \
      SMOKE_EMIT_CMD="bash $STUB_EMIT" SMOKE_INIT_CMD="bash $STUB_INIT" \
      SMOKE_HOOK_CMD="bash $STUB_HOOK" SMOKE_STORE_SIZE_CMD="bash $STUB_STORE" \
      STUB_HOOK_STORE_FILE="$tmp/store" \
      STUB_INIT_WRITE_CRED=0 \
      bash -c 'set -uo pipefail; source "'"$SMOKE"'" >/dev/null 2>&1 || true; bundle_attach_gates' 2>&1
  )"
  rc=$?
  rm -rf "$fakehome" "$tmp"
  if [ "$rc" -eq 1 ] \
     && printf '%s' "$out" | grep -qF "bundle-path observe did not land in per-slug store"; then
    pass "test_hermeticity_negative_control_still_red (poison+break => RED)"
  else
    oops "test_hermeticity_negative_control_still_red" "expected exit 1 + store-no-grow; rc=$rc out=$out"
  fi
}
test_hermeticity_negative_control_still_red

# Discrimination proof: a harness WITHOUT HOME isolation WOULD PASS the same
# poison+break scenario (the stale cred satisfies observe). We model "no
# isolation" by pointing the hook stub's HOME at the poisoned fake-real-HOME so
# the stale cred IS reachable => the fire writes => delta>0 => it would GREEN.
# This proves the negative control flips the ONLY thing that could false-green.
test_hermeticity_discrimination_unisolated_would_green() {
  local fakehome out rc tmp
  fakehome="$(mktemp -d)"
  mkdir -p "$fakehome/.unimatrix/stub-hash"
  printf '{"observe_url":"x","token":"STALE","fingerprint":"y"}\n' > "$fakehome/.unimatrix/stub-hash/remote.json"
  tmp="$(mktemp -d)"
  # Drive ONLY the Gate 7 fire+sample logic the way a NON-isolated harness would:
  # HOME points at the poisoned home (the stale cred is reachable), broken attach
  # (no fresh write). A non-isolated run sees the stale cred => store grows.
  local before after
  before="$(STUB_HOOK_STORE_FILE="$tmp/store" bash "$STUB_STORE" "/data/.unimatrix/arch-research")"
  HOME="$fakehome" STUB_HOOK_STORE_FILE="$tmp/store" bash "$STUB_HOOK" <<<'{}' >/dev/null 2>&1
  after="$(STUB_HOOK_STORE_FILE="$tmp/store" bash "$STUB_STORE" "/data/.unimatrix/arch-research")"
  rm -rf "$fakehome" "$tmp"
  if [ "$after" -gt "$before" ]; then
    pass "test_hermeticity_discrimination_unisolated_would_green (delta $before->$after)"
  else
    oops "test_hermeticity_discrimination_unisolated_would_green" \
      "non-isolated scenario did NOT grow ($before->$after) — the negative control would be vacuous"
  fi
}
test_hermeticity_discrimination_unisolated_would_green

echo "== R-07 positive twin + non-flaky (>=5) store-grew =="
test_hermeticity_positive_twin() {
  # Fresh attach into the isolated sandbox is the ONLY green: delta>0 from THIS run.
  run_bundle_case test_hermeticity_positive_twin_run 0 "PASS gate 7" --
}
test_hermeticity_positive_twin

test_store_grew_non_flaky() {
  local i ok=1
  for i in 1 2 3 4 5; do
    local tmp out rc
    tmp="$(mktemp -d)"
    out="$(
      env SLUG_DIR="/data/.unimatrix/arch-research" \
        SMOKE_EMIT_CMD="bash $STUB_EMIT" SMOKE_INIT_CMD="bash $STUB_INIT" \
        SMOKE_HOOK_CMD="bash $STUB_HOOK" SMOKE_STORE_SIZE_CMD="bash $STUB_STORE" \
        STUB_HOOK_STORE_FILE="$tmp/store" \
        bash -c 'set -uo pipefail; source "'"$SMOKE"'" >/dev/null 2>&1 || true; bundle_attach_gates' 2>&1
    )"
    rc=$?
    rm -rf "$tmp"
    if [ "$rc" -ne 0 ] || ! printf '%s' "$out" | grep -qF "PASS gate 7"; then
      ok=0; break
    fi
  done
  if [ "$ok" -eq 1 ]; then
    pass "test_store_grew_non_flaky (5/5 green)"
  else
    oops "test_store_grew_non_flaky" "positive twin was not green on all 5 runs"
  fi
}
test_store_grew_non_flaky

echo "== R-03 marker-suppressed-on-failure + run_smoke_gate integration =="
# Drive the FULL run_smoke_gate (sourced lib) against a wrapper smoke that runs
# the real bundle_attach_gates then prints the terminal marker only if it returns
# 0. On a forced Gate-5–7 failure the marker must NOT print => run_smoke_gate RED.
source "$LIB"
WRAP="$(mktemp)"
cat > "$WRAP" <<WRAPEOF
#!/usr/bin/env bash
set -uo pipefail
source "$SMOKE" >/dev/null 2>&1 || true
SLUG_DIR="/data/.unimatrix/arch-research"
bundle_attach_gates
printf '%s — wrapper terminal marker\n' '$MARKER'
WRAPEOF
chmod +x "$WRAP"

marker_case() {
  local name="$1" want_rc="$2" want_diag="$3"; shift 3
  local out got_rc tmp
  tmp="$(mktemp -d)"
  # run_smoke_gate is a sourced shell function; it cannot be invoked via `env`.
  # Export the stub seam (the wrapper smoke is a separate child that reads them
  # from its environment) plus any per-row overrides ("$@"), then call directly.
  export SMOKE_EMIT_CMD="bash $STUB_EMIT" SMOKE_INIT_CMD="bash $STUB_INIT" \
         SMOKE_HOOK_CMD="bash $STUB_HOOK" SMOKE_STORE_SIZE_CMD="bash $STUB_STORE" \
         STUB_HOOK_STORE_FILE="$tmp/store"
  local kv
  for kv in "$@"; do export "${kv?}"; done
  out="$(run_smoke_gate "irrelevant-image" bash "$WRAP" 2>&1)"
  got_rc=$?
  # Unexport the per-row overrides so rows don't leak into each other.
  for kv in "$@"; do unset "${kv%%=*}"; done
  unset SMOKE_EMIT_CMD SMOKE_INIT_CMD SMOKE_HOOK_CMD SMOKE_STORE_SIZE_CMD STUB_HOOK_STORE_FILE
  rm -rf "$tmp"
  if [ "$got_rc" -ne "$want_rc" ]; then
    oops "$name (gate rc)" "expected $want_rc got $got_rc; out: $out"; return
  fi
  if [ -n "$want_diag" ] && ! printf '%s' "$out" | grep -qF "$want_diag"; then
    oops "$name (diag)" "missing '$want_diag' in: $out"; return
  fi
  pass "$name"
}

# Happy: marker prints => gate GREEN.
marker_case test_marker_printed_on_success_green 0 ""
# Forced Gate-5 failure: marker NOT printed => run_smoke_gate RED (early-exit-1
# path; the wrapper exits 1 from fail() before the marker line).
marker_case test_marker_suppressed_on_failure_red 1 "first-run path is broken" \
  STUB_EMIT_RC=4
rm -f "$WRAP"

echo
echo "release-gate-bundle-logic-test: ${PASS} passed, ${FAIL} failed"
echo "(static source/YAML-grep assertions: run release-gate-bundle-static-test.sh)"
[ "$FAIL" -eq 0 ]
