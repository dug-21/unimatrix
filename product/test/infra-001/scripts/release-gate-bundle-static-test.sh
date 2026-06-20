#!/usr/bin/env bash
# release-gate-bundle-static-test.sh — pre-merge static (source/YAML grep) gate for
# the nan-020 Gates 5–7 extension. Companion to release-gate-bundle-logic-test.sh
# (the dynamic stub-driven truth table + hermeticity negative control). Split out
# to keep each file focused and under the 500-line modular limit.
#
# Verify-by-name discipline (#5180): assert the shipped script + release.yml carry
# the required structure — repo-checkout client, process-boundary HOME isolation,
# clean-on-entry + trap teardown, append-only ordering, single terminal marker,
# release-gate-lib.sh byte-unchanged, pinned setup-node@v4 on both smoke jobs, and
# no second smoke script. No Docker, no node, no network — pre-merge-provable.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/release-gate-lib.sh"
SMOKE="${SCRIPT_DIR}/docker-http-posture-smoke.sh"
RELEASE_YML="$(cd "$SCRIPT_DIR/../../../.." && pwd)/.github/workflows/release.yml"

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

echo "== R-04 Gate 6 invokes the repo-checkout client (static) =="
test_gate6_invokes_repo_checkout_client() {
  if grep -q 'packages/unimatrix/bin/unimatrix.js' "$SMOKE" \
     && grep -q 'packages/unimatrix/lib/hook-client/index.js' "$SMOKE"; then
    pass "test_gate6_invokes_repo_checkout_client"
  else
    oops "test_gate6_invokes_repo_checkout_client" "smoke does not invoke the repo-checkout JS entry points"
  fi
}
test_gate6_invokes_repo_checkout_client

echo "== R-07 hermeticity: structural isolation assertions (static) =="
test_gate6_runs_under_isolated_home() {
  # HOME must be threaded into the CHILD invocation (per-command prefix), never
  # via in-process export that outlives the child (vnc-041 AC-02).
  if grep -Eq 'HOME="\$SANDBOX/home"' "$SMOKE" \
     && grep -q 'SANDBOX="\$(mktemp -d)"' "$SMOKE"; then
    pass "test_gate6_runs_under_isolated_home"
  else
    oops "test_gate6_runs_under_isolated_home" "isolated HOME / mktemp -d sandbox not found"
  fi
}
test_gate6_runs_under_isolated_home

test_no_inprocess_home_mutation() {
  # No exported-then-reset HOME in the parent shell; HOME appears only as a
  # per-command prefix on the node invocations (process-boundary isolation).
  if grep -Eq '^[[:space:]]*export[[:space:]]+HOME=' "$SMOKE" \
     || grep -Eq '^[[:space:]]*HOME=[^ ]+$' "$SMOKE"; then
    oops "test_no_inprocess_home_mutation" "found a standalone HOME assignment (would mutate parent HOME)"
  else
    pass "test_no_inprocess_home_mutation"
  fi
}
test_no_inprocess_home_mutation

test_sandbox_clean_on_entry() {
  # rm -rf + mkdir of the sandbox subtree BEFORE Gate 6 (crashed-prior-run guard).
  if grep -q 'rm -rf "\$SANDBOX/home" "\$SANDBOX/proj"' "$SMOKE" \
     && grep -q 'mkdir -p "\$SANDBOX/home" "\$SANDBOX/proj"' "$SMOKE"; then
    pass "test_sandbox_clean_on_entry"
  else
    oops "test_sandbox_clean_on_entry" "clean-on-entry guard (rm -rf + mkdir) not found"
  fi
}
test_sandbox_clean_on_entry

test_sandbox_trap_teardown() {
  # cleanup() must rm -rf the sandbox so the trap removes it on exit / early fail.
  if grep -q 'rm -rf "\$SANDBOX"' "$SMOKE"; then
    pass "test_sandbox_trap_teardown"
  else
    oops "test_sandbox_trap_teardown" "cleanup() does not remove \$SANDBOX"
  fi
}
test_sandbox_trap_teardown

echo "== R-03 release-gate-lib.sh byte-unchanged (ADR-001) =="
test_run_smoke_gate_byte_unchanged() {
  # ADR-001: the wrapper lib must be byte-identical to its committed baseline.
  # Diff vs git HEAD turns any edit (which would widen the SR-04 blast radius)
  # RED at merge. Falls back to a structural-contract check if no git baseline.
  local actual expected
  expected="$(git -C "$SCRIPT_DIR" show HEAD:product/test/infra-001/scripts/release-gate-lib.sh 2>/dev/null | sha256sum | awk '{print $1}')"
  actual="$(sha256sum "$LIB" | awk '{print $1}')"
  if [ -z "$expected" ]; then
    if grep -q "grep -qx '\\\\\[783-smoke\\\\\] ALL GATES PASSED.\*'" "$LIB" \
       && grep -q '3) echo "::error::smoke SKIPPED (exit 3)' "$LIB"; then
      pass "test_run_smoke_gate_byte_unchanged (structural fallback — no git baseline)"
    else
      oops "test_run_smoke_gate_byte_unchanged" "wrapper structural contract changed and no git baseline to diff"
    fi
    return
  fi
  if [ "$actual" = "$expected" ]; then
    pass "test_run_smoke_gate_byte_unchanged (sha256 matches HEAD baseline)"
  else
    oops "test_run_smoke_gate_byte_unchanged" "release-gate-lib.sh changed vs HEAD (ADR-001 forbids touching it)"
  fi
}
test_run_smoke_gate_byte_unchanged

echo "== append-only: bundle_attach_gates called after Gate 4, before the single marker =="
test_append_only_ordering() {
  local g4 call marker
  g4="$(grep -n 'PASS gate 4 (AC-05)' "$SMOKE" | tail -1 | cut -d: -f1)"
  call="$(grep -n '^bundle_attach_gates$' "$SMOKE" | tail -1 | cut -d: -f1)"
  marker="$(grep -n 'log "ALL GATES PASSED' "$SMOKE" | tail -1 | cut -d: -f1)"
  if [ -n "$g4" ] && [ -n "$call" ] && [ -n "$marker" ] \
     && [ "$g4" -lt "$call" ] && [ "$call" -lt "$marker" ]; then
    pass "test_append_only_ordering (gate4 $g4 < call $call < marker $marker)"
  else
    oops "test_append_only_ordering" "gate4=$g4 call=$call marker=$marker (not append-only)"
  fi
}
test_append_only_ordering

test_single_terminal_marker() {
  local n
  n="$(grep -c 'log "ALL GATES PASSED' "$SMOKE")"
  if [ "$n" -eq 1 ]; then
    pass "test_single_terminal_marker (exactly one)"
  else
    oops "test_single_terminal_marker" "found $n terminal-marker emissions (expected 1)"
  fi
}
test_single_terminal_marker

echo "== release.yml pinned setup-node@v4 on BOTH smoke jobs (static) =="
# Extract the line range of a job block (from 'job-name:' to the next top-level
# job key at the same 2-space indent), so assertions are scoped per job.
job_block() {
  awk -v job="$1" '
    $0 ~ "^  " job ":" { inblk=1; print; next }
    inblk && /^  [a-zA-Z0-9_-]+:/ { inblk=0 }
    inblk { print }
  ' "$RELEASE_YML"
}

test_setup_node_present_both_smoke_jobs() {
  local a b
  a="$(job_block smoke-amd64 | grep -c 'uses: actions/setup-node@v4')"
  b="$(job_block smoke-arm64 | grep -c 'uses: actions/setup-node@v4')"
  if [ "$a" -ge 1 ] && [ "$b" -ge 1 ]; then
    pass "test_setup_node_present_both_smoke_jobs"
  else
    oops "test_setup_node_present_both_smoke_jobs" "amd64=$a arm64=$b setup-node steps (need >=1 each)"
  fi
}
test_setup_node_present_both_smoke_jobs

test_setup_node_version_pinned_24() {
  local a b
  a="$(job_block smoke-amd64 | grep -A2 'actions/setup-node@v4' | grep -c "node-version: '24'")"
  b="$(job_block smoke-arm64 | grep -A2 'actions/setup-node@v4' | grep -c "node-version: '24'")"
  if [ "$a" -ge 1 ] && [ "$b" -ge 1 ]; then
    pass "test_setup_node_version_pinned_24 (parity with package-npm)"
  else
    oops "test_setup_node_version_pinned_24" "amd64=$a arm64=$b node-version '24' (need >=1 each)"
  fi
}
test_setup_node_version_pinned_24

test_setup_node_ordering() {
  # setup-node must be AFTER checkout and BEFORE the run_smoke_gate step in each job.
  local job ok=1
  for job in smoke-amd64 smoke-arm64; do
    local blk co sn rg
    blk="$(job_block "$job")"
    co="$(printf '%s\n' "$blk" | grep -n 'uses: actions/checkout@v4' | head -1 | cut -d: -f1)"
    sn="$(printf '%s\n' "$blk" | grep -n 'uses: actions/setup-node@v4' | head -1 | cut -d: -f1)"
    rg="$(printf '%s\n' "$blk" | grep -n 'run_smoke_gate' | head -1 | cut -d: -f1)"
    if [ -z "$co" ] || [ -z "$sn" ] || [ -z "$rg" ] || [ "$co" -ge "$sn" ] || [ "$sn" -ge "$rg" ]; then
      ok=0
      oops "test_setup_node_ordering ($job)" "checkout=$co setup-node=$sn run_smoke_gate=$rg (need checkout<setup-node<gate)"
    fi
  done
  [ "$ok" -eq 1 ] && pass "test_setup_node_ordering (both jobs: checkout < setup-node < gate)"
}
test_setup_node_ordering

echo "== R-15 no new smoke script; round-trip lives in the extended smoke (static) =="
test_no_new_smoke_script() {
  # Gates 5–7 live inside docker-http-posture-smoke.sh — there must be no second
  # *smoke* script. (Fixtures + the gate-logic tests are permitted additions.)
  local n
  n="$(ls "$SCRIPT_DIR"/*smoke*.sh 2>/dev/null | grep -v 'stub-smoke.sh' | wc -l | tr -d ' ')"
  if [ "$n" -eq 1 ]; then
    pass "test_no_new_smoke_script (only docker-http-posture-smoke.sh)"
  else
    oops "test_no_new_smoke_script" "expected exactly 1 smoke script, found $n"
  fi
}
test_no_new_smoke_script

echo
echo "release-gate-bundle-static-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
