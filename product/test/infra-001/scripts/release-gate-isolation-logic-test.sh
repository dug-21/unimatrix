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

# Deterministic markers via RUN=t1 (derive_markers honors a pinned RUN). Markers carry
# the fixed feature-id token "1" (MARKER_FID_TOKEN, #859) so looks_like_feature_id holds.
RUN_NONCE="t1"
M_OBS_A="infra003-obs-a-1-${RUN_NONCE}"; M_OBS_B="infra003-obs-b-1-${RUN_NONCE}"
M_MCP_A="infra003-mcp-a-1-${RUN_NONCE}"; M_MCP_B="infra003-mcp-b-1-${RUN_NONCE}"
WARMUP_M="infra003-warmup-1-${RUN_NONCE}"   # C-WB throwaway warmup marker (infra-004)
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

# --- driver: run warmup_barrier() in an isolated child against the stubs --------
# run_warmup <name> <want_rc> <want_substr> -- <env assignments...>
# Sources the shipped gate (sourced-guard suppresses C1/C2/main), then drives the
# C-WB warmup_barrier against SMOKE_WRITE_CMD/SMOKE_READ_MARKER_CMD. On PRESENT it
# returns 0 and control reaches the post-marker; on INFRA it exits 2 (infra_fail).
run_warmup() {
  local name="$1" want_rc="$2" want_sub="$3"; shift 3
  [ "${1:-}" = "--" ] && shift
  local out rc
  out="$(
    env RUN="$RUN_NONCE" \
        SMOKE_WRITE_CMD="true" \
        SMOKE_READ_MARKER_CMD="bash $STUB_READ" \
        READ_POLL_SLEEP=1 \
        "$@" \
        bash -c 'set -uo pipefail; source "'"$GATE"'" >/dev/null 2>&1 || true; warmup_barrier && echo "__PROCEED_TO_MATRIX__"' 2>&1
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
      M_OBS_A="infra003-obs-a-1"
      M_OBS_B="infra003-obs-a-1-extra"   # M_OBS_A is a substring of this (both feature-id-shaped)
      M_MCP_A="infra003-mcp-a-1"; M_MCP_B="infra003-mcp-b-1"
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

echo "== C-WB R-01 warmup PRESENT is load-bearing: durable MCP round-trip => proceed =="
# Warmup marker present in A's store (same MCP entries predicate the real round trip
# uses) -> WTB=PRESENT -> warmup_barrier returns 0 and control reaches run_isolation_matrix.
run_warmup test_warmup_present_proceeds_to_matrix 0 "__PROCEED_TO_MATRIX__" -- \
  "WARMUP_DEADLINE_SECS=5" "STUB_PRESENT=${DA}::${WARMUP_M}"

echo "== C-WB AC-03/R-04 warmup TIMEOUT => INFRA (exit 2, never RED, never proceed) =="
test_warmup_timeout_is_infra_not_pass() {
  # Warmup marker omitted from the present-set + WARMUP_DEADLINE_SECS=0 -> own
  # marker never appears -> WTB=INFRA -> infra_fail exit 2 (never RED, never pass).
  run_warmup test_warmup_timeout_is_infra_not_pass 2 "INFRA" -- \
    "WARMUP_DEADLINE_SECS=0" "STUB_PRESENT=" || return
  if printf '%s' "$LAST_OUT" | grep -qF "ISOLATION BROKEN"; then
    oops "test_warmup_timeout_is_infra_not_pass" "warmup timeout wrongly RED: $LAST_OUT"
  elif printf '%s' "$LAST_OUT" | grep -qF "__PROCEED_TO_MATRIX__"; then
    oops "test_warmup_timeout_is_infra_not_pass" "warmup timeout vacuously proceeded: $LAST_OUT"
  elif ! printf '%s' "$LAST_OUT" | grep -qF "timed out"; then
    oops "test_warmup_timeout_is_infra_not_pass" "no diagnostic last-state logged on timeout (R-04): $LAST_OUT"
  else
    pass "test_warmup_timeout_is_infra_not_pass (INFRA exit 2; not RED, not proceed; diagnostic logged)"
  fi
}
test_warmup_timeout_is_infra_not_pass

echo "== C-WB R-01 PRESENT requires a durable read round-trip (read-fail => INFRA, not a shortcut) =="
test_warmup_present_requires_durable_read_roundtrip() {
  # The warmup read goes through the SAME SMOKE_READ_MARKER_CMD a real write uses:
  # force that read to return the INFRA sentinel -> write_then_barrier infra_fail's,
  # proving PRESENT is not a liveness shortcut (mirrors test_c6_missing_db_is_infra).
  run_warmup test_warmup_present_requires_durable_read_roundtrip 2 "store read failed" -- \
    "WARMUP_DEADLINE_SECS=5" "STUB_INFRA=${DA}::${WARMUP_M}" || return
  if printf '%s' "$LAST_OUT" | grep -qF "__PROCEED_TO_MATRIX__"; then
    oops "test_warmup_present_requires_durable_read_roundtrip" "PRESENT was a liveness shortcut despite a failed own-store read: $LAST_OUT"
  else
    pass "test_warmup_present_requires_durable_read_roundtrip (read-fail => INFRA, durable round-trip enforced)"
  fi
}
test_warmup_present_requires_durable_read_roundtrip

echo "== C-WB R-01 funnel (static): WTB is consumed in a CASE that gates proceed-to-matrix =="
test_warmup_result_is_consumed() {
  local body; body="$(awk '/^warmup_barrier\(\)/{f=1} f{print} f&&/^}/{exit}' "$GATE")"
  if printf '%s' "$body" | grep -qF 'case "$WTB"' && printf '%s' "$body" | grep -qF 'infra_fail'; then
    pass "test_warmup_result_is_consumed (WTB consumed in a CASE; not computed-and-discarded)"
  else
    oops "test_warmup_result_is_consumed" "WTB result not consumed to gate proceed (computed-and-discarded?)"
  fi
}
test_warmup_result_is_consumed

echo "== C-WB AC-02 (static): warmup uses write_then_barrier, NOT a store_size liveness poll =="
test_warmup_uses_write_then_barrier_not_store_size() {
  local body; body="$(awk '/^warmup_barrier\(\)/{f=1} f{print} f&&/^}/{exit}' "$GATE")"
  if printf '%s' "$body" | grep -qF 'write_then_barrier' \
     && ! printf '%s' "$body" | grep -qE 'store_size|wait_for_http_active'; then
    pass "test_warmup_uses_write_then_barrier_not_store_size (no new readiness mechanism)"
  else
    oops "test_warmup_uses_write_then_barrier_not_store_size" "warmup uses store_size/wait_for_http_active or omits write_then_barrier"
  fi
}
test_warmup_uses_write_then_barrier_not_store_size

echo "== C-WB R-02 warmup marker non-substring vs cell markers asserted (collision => INFRA) =="
test_warmup_marker_non_substring_asserted() {
  # Shadow derive_markers to plant a cell marker that the warmup marker
  # (infra003-warmup-t1) is a substring of -> the in-barrier R-02 guard must trip
  # (mirrors test_c7_substring_markers_fail_infra; targets derive_markers->assert).
  local out rc
  out="$(
    bash -c '
      set -uo pipefail
      source "'"$GATE"'" >/dev/null 2>&1 || true
      RUN="'"$RUN_NONCE"'"
      derive_markers() {
        M_OBS_A="infra003-warmup-1-'"$RUN_NONCE"'-collide"   # warmup marker (incl. fixed token) is a substring of this
        M_OBS_B="infra003-obs-b-1-'"$RUN_NONCE"'"
        M_MCP_A="infra003-mcp-a-1-'"$RUN_NONCE"'"; M_MCP_B="infra003-mcp-b-1-'"$RUN_NONCE"'"
        SLUG_DIR_A="'"$DA"'"; SLUG_DIR_B="'"$DB"'"
      }
      warmup_barrier
    ' 2>&1
  )"
  rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qF "non-substring invariant broken"; then
    pass "test_warmup_marker_non_substring_asserted"
  else
    oops "test_warmup_marker_non_substring_asserted" "rc=$rc out=$out (want INFRA exit 2 on collision)"
  fi
}
test_warmup_marker_non_substring_asserted

echo "== C-WB R-02 warmup row is inert to the matrix negatives (matches no cell predicate) =="
# Warmup marker present in BOTH stores alongside the four positives; negative cells
# query specific FOREIGN cell markers, none of which is the warmup marker => GREEN.
run_matrix test_warmup_row_inert_to_negatives 0 "ALL GATES PASSED" -- \
  "STUB_PRESENT=$ALL_POS ${DA}::${WARMUP_M} ${DB}::${WARMUP_M}"

echo "== C-WB R-03/AC-01 (static): assert_routes_live < warmup_barrier < run_isolation_matrix =="
test_assert_routes_live_precedes_barrier() {
  local lr lw lm
  lr="$(grep -nE '^assert_routes_live[[:space:]]' "$GATE" | cut -d: -f1 | tail -1)"
  lw="$(grep -nE '^warmup_barrier[[:space:]]' "$GATE" | cut -d: -f1 | tail -1)"
  lm="$(grep -nE '^run_isolation_matrix[[:space:]]' "$GATE" | cut -d: -f1 | tail -1)"
  if [ -n "$lr" ] && [ -n "$lw" ] && [ -n "$lm" ] && [ "$lr" -lt "$lw" ] && [ "$lw" -lt "$lm" ]; then
    pass "test_assert_routes_live_precedes_barrier (routes<warmup<matrix: $lr<$lw<$lm)"
  else
    oops "test_assert_routes_live_precedes_barrier" "call order wrong: routes=$lr warmup=$lw matrix=$lm"
  fi
}
test_assert_routes_live_precedes_barrier

echo "== C-WB R-03/AC-01 (static): WARMUP_DEADLINE_SECS default 180 documented as #767 derivation =="
test_warmup_bound_default_documented() {
  if grep -qE 'WARMUP_DEADLINE_SECS="\$\{WARMUP_DEADLINE_SECS:-180\}"' "$GATE" \
     && grep -qF '#767' "$GATE" && grep -qF 'READY_TIMEOUT_SECS' "$GATE"; then
    pass "test_warmup_bound_default_documented (default 180; #767 READY_TIMEOUT_SECS derivation cited)"
  else
    oops "test_warmup_bound_default_documented" "WARMUP_DEADLINE_SECS default/derivation not documented in the diff"
  fi
}
test_warmup_bound_default_documented

echo "== C-WB AC-05 stub-seam compatibility preserved post-barrier: full verdict truth table still drives =="
# After adding the barrier, the EXISTING run_isolation_matrix truth table still
# drives through the seam off-Docker (GREEN 0 / RED 1 / INFRA 2; SKIP 3 via the
# C1 docker-absent case above) — no regression to the stub seam.
run_matrix test_post_barrier_green_still_drives 0 "ALL GATES PASSED" -- "STUB_PRESENT=$ALL_POS"
run_matrix test_post_barrier_red_still_drives   1 "ISOLATION BROKEN" -- "STUB_PRESENT=$ALL_POS ${DA}::${M_MCP_B}"
run_matrix test_post_barrier_infra_still_drives 2 "INFRA" -- \
  "STUB_PRESENT=${DB}::${M_OBS_B} ${DA}::${M_MCP_A} ${DB}::${M_MCP_B}"

echo "== (c) #859 nonce is construction-safe: default path + adversarial battery never form PII shapes =="
# Cases factored into fixtures/isolation-nonce-logic-cases.sh to keep THIS file <=500
# lines (workspace rule); they consume pass()/oops() defined above and drive the REAL
# default derivation path (RUN UNSET) through the PID_OVERRIDE/EPOCH_OVERRIDE seam.
# shellcheck source=fixtures/isolation-nonce-logic-cases.sh
. "${SCRIPT_DIR}/fixtures/isolation-nonce-logic-cases.sh"
run_nonce_safety_cases

echo
echo "release-gate-isolation-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
