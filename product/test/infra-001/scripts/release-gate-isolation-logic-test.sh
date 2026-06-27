#!/usr/bin/env bash
# release-gate-isolation-logic-test.sh — infra-003 (#853) tier-1 off-Docker
# gate-logic test for multi-tenant-isolation-smoke.sh.
#
# A smoke gate's dominant failure is the FALSE-GREEN / vacuous pass (a gate that
# greens while isolation is broken is worse than no gate). This test proves the
# gate's TEETH WITHOUT Docker by SOURCING THE SHIPPED BYTES (the sourced-guard
# suppresses C1/C2/main) and driving the C5/C6/C7 verdict truth table through the
# injectable read seam (SMOKE_READ_MARKER_CMD) + the write seam (SMOKE_WRITE_CMD).
# A paraphrased copy would test nothing (#5192); sourcing the real script is the
# single source of truth.
#
# Load-bearing cases: a marker planted in the WRONG store -> RED (both surfaces,
# both directions); an own-store read-as-barrier timeout -> INFRA (never RED,
# never GREEN); RED dominates INFRA; a missing main db -> INFRA; four distinct,
# non-GREEN-coercible exit states (GREEN 0 / RED 1 / INFRA 2 / SKIP 3).
#
# Fully local + deterministic: no Docker, no network, no real sqlite3.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="${SCRIPT_DIR}/multi-tenant-isolation-smoke.sh"
PROBE_LIB="${SCRIPT_DIR}/isolation-probe-lib.sh"   # C3/C4 write probes (sourced by GATE)
STUB_READ="${SCRIPT_DIR}/fixtures/stub-read-marker.sh"

# Deterministic markers via RUN=t1 (derive_markers honors a pinned RUN).
RUN_NONCE="t1"
M_OBS_A="infra003-obs-a-${RUN_NONCE}"; M_OBS_B="infra003-obs-b-${RUN_NONCE}"
M_MCP_A="infra003-mcp-a-${RUN_NONCE}"; M_MCP_B="infra003-mcp-b-${RUN_NONCE}"
DA="/data/.unimatrix/arch-research"   # SLUG_DIR_A default
DB="/data/.unimatrix/isolation-b"     # SLUG_DIR_B default

# The four own-store positives (the all-green present-set).
ALL_POS="${DA}::${M_OBS_A} ${DB}::${M_OBS_B} ${DA}::${M_MCP_A} ${DB}::${M_MCP_B}"

PASS=0; FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# Sanity: the shipped gate must source cleanly and expose the orchestration seam.
set +e
# shellcheck source=multi-tenant-isolation-smoke.sh
source "$GATE" >/dev/null 2>&1
set +e; set -uo pipefail   # the gate ran `set -euo pipefail` on source — drop -e for THIS harness
if ! declare -F run_isolation_matrix >/dev/null; then
  echo "FATAL: run_isolation_matrix not found after sourcing $GATE" >&2
  exit 1
fi

# --- driver: run run_isolation_matrix() in an isolated child against the stubs --
# run_matrix <name> <want_rc> <want_substr> -- <env assignments...>
LAST_OUT=""
run_matrix() {
  local name="$1" want_rc="$2" want_sub="$3"; shift 3
  [ "${1:-}" = "--" ] && shift
  local out rc
  out="$(
    env RUN="$RUN_NONCE" \
        SMOKE_WRITE_CMD="true" \
        SMOKE_READ_MARKER_CMD="bash $STUB_READ" \
        READ_DEADLINE_SECS=0 READ_POLL_SLEEP=1 \
        "$@" \
        bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; run_isolation_matrix' 2>&1
  )"
  rc=$?
  LAST_OUT="$out"
  if [ "$rc" -ne "$want_rc" ]; then
    oops "$name (rc)" "expected rc=$want_rc got=$rc; out: $out"; return 1
  fi
  if [ -n "$want_sub" ] && ! printf '%s' "$out" | grep -qF "$want_sub"; then
    oops "$name (substr)" "expected '$want_sub' in: $out"; return 1
  fi
  pass "$name"; return 0
}

echo "== C7 happy path: all 4 positives PRESENT + all cross-cells ABSENT => GREEN =="
run_matrix test_c7_all_green_exit0 0 "ALL GATES PASSED" -- "STUB_PRESENT=$ALL_POS"

echo "== C7 terminal marker matches the release-gate-lib verify-by-name grep (#5180) =="
test_c7_terminal_marker_matches_grep() {
  run_matrix test_c7_all_green_marker_probe 0 "ALL GATES PASSED" -- "STUB_PRESENT=$ALL_POS" || return
  # The terminal line must match  \[[a-z0-9-]+-smoke\] ALL GATES PASSED.*
  if printf '%s\n' "$LAST_OUT" | grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'; then
    pass "test_c7_terminal_marker_matches_grep"
  else
    oops "test_c7_terminal_marker_matches_grep" "no line matched the anchored verify-by-name regex: $LAST_OUT"
  fi
}
test_c7_terminal_marker_matches_grep

test_c7_marker_only_on_green() {
  # A non-green run must NOT emit the terminal marker (no early-exit-0 false green).
  run_matrix test_c7_red_no_marker_probe 1 "ISOLATION BROKEN" -- \
    "STUB_PRESENT=$ALL_POS ${DA}::${M_MCP_B}" || return
  if printf '%s' "$LAST_OUT" | grep -qF "ALL GATES PASSED"; then
    oops "test_c7_marker_only_on_green" "terminal marker leaked on a RED run: $LAST_OUT"
  else
    pass "test_c7_marker_only_on_green"
  fi
}
test_c7_marker_only_on_green

echo "== C6/C7 PLANTED-LEAK TEETH: foreign marker in the wrong store => RED (4 dir) =="
# MCP B->A : B's mcp marker planted in A's store.
run_matrix test_c7_planted_leak_mcp_b_in_a_is_red 1 "ISOLATION BROKEN" -- \
  "STUB_PRESENT=$ALL_POS ${DA}::${M_MCP_B}"
# MCP A->B : A's mcp marker planted in B's store.
run_matrix test_c7_planted_leak_mcp_a_in_b_is_red 1 "ISOLATION BROKEN" -- \
  "STUB_PRESENT=$ALL_POS ${DB}::${M_MCP_A}"
# Observe B->A : B's obs marker planted in A's store.
run_matrix test_c7_planted_leak_obs_b_in_a_is_red 1 "ISOLATION BROKEN" -- \
  "STUB_PRESENT=$ALL_POS ${DA}::${M_OBS_B}"
# Observe A->B : A's obs marker planted in B's store.
run_matrix test_c7_planted_leak_obs_a_in_b_is_red 1 "ISOLATION BROKEN" -- \
  "STUB_PRESENT=$ALL_POS ${DB}::${M_OBS_A}"

echo "== C5 own-store read-as-barrier TIMEOUT => INFRA (never RED, never GREEN) =="
# Omit A's obs own-positive from the present-set: it never appears -> INFRA exit 2.
test_c5_own_timeout_is_infra_not_red() {
  run_matrix test_c5_own_timeout_is_infra_not_red 2 "INFRA" -- \
    "STUB_PRESENT=${DB}::${M_OBS_B} ${DA}::${M_MCP_A} ${DB}::${M_MCP_B}" || return
  if printf '%s' "$LAST_OUT" | grep -qF "ISOLATION BROKEN"; then
    oops "test_c5_own_timeout_is_infra_not_red" "own-store timeout wrongly RED: $LAST_OUT"
  elif printf '%s' "$LAST_OUT" | grep -qF "ALL GATES PASSED"; then
    oops "test_c5_own_timeout_is_infra_not_red" "own-store timeout vacuously GREEN: $LAST_OUT"
  else
    pass "test_c5_own_timeout_is_infra_not_red (INFRA, not RED, not GREEN)"
  fi
}
test_c5_own_timeout_is_infra_not_red

echo "== C7 RED dominates INFRA: leak surfaces RED even when own positive timed out =="
# A-mcp own positive omitted (INFRA) AND B's mcp marker planted in A (leak).
run_matrix test_c7_red_dominates_infra 1 "ISOLATION BROKEN" -- \
  "STUB_PRESENT=${DA}::${M_OBS_A} ${DB}::${M_OBS_B} ${DB}::${M_MCP_B} ${DA}::${M_MCP_B}"

echo "== C6/C2 missing main db => INFRA, never a 0-row clean pass (R-07) =="
run_matrix test_c6_missing_db_is_infra 2 "INFRA" -- \
  "STUB_PRESENT=$ALL_POS" "STUB_INFRA=${DA}::${M_OBS_A}"

echo "== C5 read-as-barrier RETRIES until present (not a fixed sleep / single read) =="
test_c5_positive_is_retry_until_present() {
  local cf out rc
  cf="$(mktemp)"
  out="$(
    env RUN="$RUN_NONCE" SMOKE_WRITE_CMD="true" \
        SMOKE_READ_MARKER_CMD="bash $STUB_READ" \
        READ_DEADLINE_SECS=5 READ_POLL_SLEEP=1 \
        STUB_RETRY_COUNTER="$cf" \
        "STUB_RETRY=${DA}::${M_OBS_A}::2" \
        "STUB_PRESENT=${DB}::${M_OBS_B} ${DA}::${M_MCP_A} ${DB}::${M_MCP_B}" \
        bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; run_isolation_matrix' 2>&1
  )"
  rc=$?
  local reads; reads="$(cat "$cf" 2>/dev/null || echo 0)"; rm -f "$cf"
  if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -qF "ALL GATES PASSED" && [ "${reads:-0}" -ge 3 ]; then
    pass "test_c5_positive_is_retry_until_present (polled ${reads}x then PRESENT => GREEN)"
  else
    oops "test_c5_positive_is_retry_until_present" "rc=$rc reads=$reads (want green after >=3 polls); out: $out"
  fi
}
test_c5_positive_is_retry_until_present

echo "== R-10 tri-state exit codes are DISTINCT; no non-GREEN rounds to 0 =="
test_c7_tristate_exit_codes() {
  local g r i
  env RUN="$RUN_NONCE" SMOKE_WRITE_CMD="true" SMOKE_READ_MARKER_CMD="bash $STUB_READ" \
      READ_DEADLINE_SECS=0 READ_POLL_SLEEP=1 "STUB_PRESENT=$ALL_POS" \
      bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; run_isolation_matrix' >/dev/null 2>&1; g=$?
  env RUN="$RUN_NONCE" SMOKE_WRITE_CMD="true" SMOKE_READ_MARKER_CMD="bash $STUB_READ" \
      READ_DEADLINE_SECS=0 READ_POLL_SLEEP=1 "STUB_PRESENT=$ALL_POS ${DA}::${M_MCP_B}" \
      bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; run_isolation_matrix' >/dev/null 2>&1; r=$?
  env RUN="$RUN_NONCE" SMOKE_WRITE_CMD="true" SMOKE_READ_MARKER_CMD="bash $STUB_READ" \
      READ_DEADLINE_SECS=0 READ_POLL_SLEEP=1 "STUB_PRESENT=${DB}::${M_OBS_B} ${DA}::${M_MCP_A} ${DB}::${M_MCP_B}" \
      bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; run_isolation_matrix' >/dev/null 2>&1; i=$?
  if [ "$g" -eq 0 ] && [ "$r" -eq 1 ] && [ "$i" -eq 2 ] \
     && [ "$g" != "$r" ] && [ "$r" != "$i" ] && [ "$g" != "$i" ]; then
    pass "test_c7_tristate_exit_codes (GREEN=$g RED=$r INFRA=$i — distinct; SKIP=3 below)"
  else
    oops "test_c7_tristate_exit_codes" "GREEN=$g RED=$r INFRA=$i (want 0/1/2 distinct)"
  fi
}
test_c7_tristate_exit_codes

echo "== C7 marker non-substring self-check fails LOUD (INFRA) on a violation (R-18) =="
test_c7_substring_markers_fail_infra() {
  # Force a substring-violating marker set and call assert_markers_distinct: a
  # marker that is a substring of another must be rejected as INFRA.
  local out rc
  out="$(
    bash -c '
      set -uo pipefail
      source "'"$GATE"'" >/dev/null 2>&1 || true
      M_OBS_A="infra003-obs-a"
      M_OBS_B="infra003-obs-a-extra"   # M_OBS_A is a substring of this
      M_MCP_A="infra003-mcp-a"; M_MCP_B="infra003-mcp-b"
      assert_markers_distinct
    ' 2>&1
  )"
  rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qF "non-substring invariant broken"; then
    pass "test_c7_substring_markers_fail_infra"
  else
    oops "test_c7_substring_markers_fail_infra" "rc=$rc out=$out (want INFRA exit 2)"
  fi
}
test_c7_substring_markers_fail_infra

test_c7_real_markers_are_non_substring() {
  local out rc
  out="$(bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; RUN="'"$RUN_NONCE"'"; derive_markers; assert_markers_distinct' 2>&1)"
  rc=$?
  if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -qF "pairwise non-substring"; then
    pass "test_c7_real_markers_are_non_substring"
  else
    oops "test_c7_real_markers_are_non_substring" "rc=$rc out=$out"
  fi
}
test_c7_real_markers_are_non_substring

echo "== C1 preflight: Docker absent => SKIP exit 3; sqlite3 absent => INFRA exit 2 =="
# Build a minimal fakebin so the early REPO_ROOT computation (dirname) still works.
FAKEBIN="$(mktemp -d)"
ln -s "$(command -v dirname)" "$FAKEBIN/dirname"
BASH_BIN="$(command -v bash)"

test_c1_docker_absent_skips_exit3() {
  local out rc
  # No docker on PATH (fakebin has only dirname) -> command -v docker fails -> SKIP 3.
  out="$(PATH="$FAKEBIN" "$BASH_BIN" "$GATE" 2>&1)"; rc=$?
  if [ "$rc" -eq 3 ] && printf '%s' "$out" | grep -qF "SKIP: Docker not available"; then
    pass "test_c1_docker_absent_skips_exit3"
  else
    oops "test_c1_docker_absent_skips_exit3" "rc=$rc out=$out (want SKIP exit 3)"
  fi
}
test_c1_docker_absent_skips_exit3

test_c1_sqlite3_absent_is_infra() {
  local out rc
  # Provide a fake docker (info succeeds) but NO sqlite3 on PATH -> INFRA exit 2.
  printf '#!/bin/bash\nexit 0\n' > "$FAKEBIN/docker"; chmod +x "$FAKEBIN/docker"
  out="$(PATH="$FAKEBIN" "$BASH_BIN" "$GATE" 2>&1)"; rc=$?
  rm -f "$FAKEBIN/docker"
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qF "sqlite3 not provisioned"; then
    pass "test_c1_sqlite3_absent_is_infra (provision like node)"
  else
    oops "test_c1_sqlite3_absent_is_infra" "rc=$rc out=$out (want INFRA exit 2)"
  fi
}
test_c1_sqlite3_absent_is_infra
rm -rf "$FAKEBIN"

echo "== static: read-as-barrier uses read_marker (no aggregate store_size barrier, C-08) =="
test_c5_barrier_is_read_marker_not_store_size() {
  # write_then_barrier must poll read_marker, and store_size must NOT be the barrier.
  if grep -A20 '^write_then_barrier()' "$GATE" | grep -q 'read_marker' \
     && ! grep -A20 '^write_then_barrier()' "$GATE" | grep -q 'store_size'; then
    pass "test_c5_barrier_is_read_marker_not_store_size"
  else
    oops "test_c5_barrier_is_read_marker_not_store_size" "barrier does not key on read_marker, or uses store_size"
  fi
}
test_c5_barrier_is_read_marker_not_store_size

test_c6_no_count_heuristic() {
  # The negative read is a content WHERE predicate, never a du/dir-count/other_count.
  if grep -A18 '^negative_cell()' "$GATE" | grep -q 'read_marker' \
     && ! grep -qE 'other_count|du -s.*other|dir.?count' "$GATE"; then
    pass "test_c6_no_count_heuristic"
  else
    oops "test_c6_no_count_heuristic" "found a count-heuristic in the negative read"
  fi
}
test_c6_no_count_heuristic

echo "== static: MCP per-route session isolation (distinct SID_A/SID_B, SSE Accept) =="
test_c4_session_captured_per_route() {
  # Distinct SID_A and SID_B variables exist; bound per route, never crossed (R-17).
  if grep -q 'SID_A=' "$PROBE_LIB" && grep -q 'SID_B=' "$PROBE_LIB" \
     && grep -q 'never crossed (R-17)' "$PROBE_LIB"; then
    pass "test_c4_session_captured_per_route"
  else
    oops "test_c4_session_captured_per_route" "distinct per-route session variables not found"
  fi
}
test_c4_session_captured_per_route

test_c4_accept_advertises_sse() {
  if grep -q 'Accept: application/json, text/event-stream' "$PROBE_LIB"; then
    pass "test_c4_accept_advertises_sse"
  else
    oops "test_c4_accept_advertises_sse" "MCP requests do not advertise text/event-stream (rmcp forces SSE)"
  fi
}
test_c4_accept_advertises_sse

echo "== static: no warn+continue; every dep check ends in an exit (R-06/#4473) =="
test_c1_no_warn_continue() {
  # preflight must classify each absence to exit (SKIP/infra_fail), never fall through.
  if grep -A30 '^preflight()' "$GATE" | grep -q 'infra_fail' \
     && grep -A30 '^preflight()' "$GATE" | grep -q 'exit 3' \
     && ! grep -A30 '^preflight()' "$GATE" | grep -qiE 'warn.*continue|continue anyway'; then
    pass "test_c1_no_warn_continue"
  else
    oops "test_c1_no_warn_continue" "a dependency check may warn+continue"
  fi
}
test_c1_no_warn_continue

test_no_overclaim_point_in_time() {
  # Output is point-in-time only ("advances, does not close N3"); no parity/UDS claim.
  if grep -q 'does not close' "$GATE" && ! grep -qiE 'parity-matrix|FORBIDDEN_IN_LOCAL.*re-run' "$GATE"; then
    pass "test_no_overclaim_point_in_time"
  else
    oops "test_no_overclaim_point_in_time" "overclaim or parity/UDS reintroduction detected"
  fi
}
test_no_overclaim_point_in_time

echo
echo "release-gate-isolation-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
