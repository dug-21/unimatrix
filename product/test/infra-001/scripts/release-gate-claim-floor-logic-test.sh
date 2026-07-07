#!/usr/bin/env bash
# release-gate-claim-floor-logic-test.sh — pre-merge HARD gate for the #915/#916 claim-
# floor gates: Gate 9 (client-works, #916/C15) + Gate 10 (compose boot, #915/C1).
# Companion to release-gate-bundle-logic-test.sh (Gates 5–7) and
# release-gate-cloud-cycle-logic-test.sh (Gate 8). It `source`s the SHIPPED
# docker-http-posture-smoke.sh (whose sourced-guard suppresses the Docker preflight +
# Gates 1–4) to get the REAL client_works_gate / compose_boot_gate, then drives them
# off-Docker: Gate 9 via the SMOKE_CLIENT_CALL_CMD stub seam; Gate 10 by OVERRIDING its
# live-only docker/curl helpers IN THIS HARNESS (never in the shipped file), mirroring how
# the C5 logic test overrides _fire_observe_hooks / cycle_durability_barrier. A paraphrased
# copy of the gate would test nothing (R-01); sourcing the shipped bytes is the SoT (#5192).
#
# Fully local + deterministic: no Docker, no node, no network, no tag push.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE="${SCRIPT_DIR}/docker-http-posture-smoke.sh"
CLAIM_LIB="${SCRIPT_DIR}/claim-floor-lib.sh"
COMPOSE_YML="$(cd "$SCRIPT_DIR/../../../.." && pwd)/docker-compose.yml"

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# ---- source the SHIPPED smoke bytes (single source of truth) -----------------
# The sourced-guard stops before the Docker preflight / Gates 1–4, so sourcing defines
# client_works_gate() + compose_boot_gate() + the seam helpers WITHOUT running Docker.
set +e
# shellcheck source=docker-http-posture-smoke.sh
source "$SMOKE"
set +e; set +u 2>/dev/null; set -uo pipefail
if ! declare -F client_works_gate >/dev/null || ! declare -F compose_boot_gate >/dev/null; then
  echo "FATAL: client_works_gate/compose_boot_gate not found after sourcing $SMOKE" >&2
  exit 1
fi

# =============================================================================
# Part A — Gate 9 (client-works) truth table via the SMOKE_CLIENT_CALL_CMD stub seam.
# =============================================================================
# The stub emits the single-call driver envelope ({ok,tool,error}) client_works_gate
# greps. STUB_CALL_OK / STUB_CALL_RC / STUB_CALL_SLEEP drive the ok:false / rc / timeout
# rows. It is invoked exactly as the real driver is: under `timeout $CLIENT_CALL_DEADLINE_S`.
make_call_stub() {
  cat > "$1" <<'STUBEOF'
#!/usr/bin/env bash
# stub bridge single-call drive.
[ -n "${STUB_CALL_SLEEP:-}" ] && sleep "$STUB_CALL_SLEEP"
printf '{"ok": %s, "tool": "context_status", "error": null}\n' "${STUB_CALL_OK:-true}"
exit "${STUB_CALL_RC:-0}"
STUBEOF
  chmod +x "$1"
}

# run_gate9_case <name> <want_rc> <want_sub> -- <env assignments...>
run_gate9_case() {
  local name="$1" want_rc="$2" want_sub="$3"; shift 3
  [ "${1:-}" = "--" ] && shift
  local sb out got_rc stub
  sb="$(mktemp -d)"
  mkdir -p "$sb/home/.unimatrix/deadbeefcafe1234"
  printf '{"observe_url":"https://localhost:18443/v1/arch-research/observe","token":"t","fingerprint":"sha256:x"}\n' \
    > "$sb/home/.unimatrix/deadbeefcafe1234/remote.json"
  stub="$sb/call-stub.sh"; make_call_stub "$stub"
  out="$(
    env SANDBOX="$sb" SMOKE_CLIENT_CALL_CMD="bash $stub" \
      "$@" \
      bash -c '
        set -uo pipefail
        source "'"$SMOKE"'" >/dev/null 2>&1 || true
        compose_teardown() { :; }
        client_works_gate
      ' 2>&1
  )"
  got_rc=$?
  rm -rf "$sb"
  if [ "$got_rc" -ne "$want_rc" ]; then
    oops "$name (rc)" "expected rc=$want_rc got=$got_rc; out: $out"; return
  fi
  if [ -n "$want_sub" ] && ! printf '%s' "$out" | grep -qF "$want_sub"; then
    oops "$name (substr)" "expected '$want_sub' in: $out"; return
  fi
  pass "$name"
}

echo "== Part A: Gate 9 client-works truth table (SMOKE_CLIENT_CALL_CMD stub) =="

# Happy path: driver ok:true, rc 0 => the ONLY green.
run_gate9_case test_gate9_happy_path_green 0 "PASS gate 9 (client-works, #916/C15)" --

# Driver rc nonzero => bridge single-call failed (client cannot reach the server).
run_gate9_case test_gate9_drive_rc_nonzero_red 1 \
  "bridge single-call driver failed (rc=5)" -- STUB_CALL_RC=5

# rc 0 but ok:false => no real context_* op landed.
run_gate9_case test_gate9_not_ok_red 1 \
  "did not return ok — no real context_* op over the pinned-TLS bundle" -- STUB_CALL_OK=false

# Constraint 1: a hang is bounded by `timeout` (rc 124) => a distinct timeout failure,
# NEVER an unbounded wait that eats the blocking lane's job timeout.
run_gate9_case test_gate9_bounded_timeout_red 1 \
  "timed out after 1s (bounded" -- CLIENT_CALL_DEADLINE_S=1 STUB_CALL_SLEEP=3

# Missing credstore read-back => RED precondition (Gate-6 -> Gate-9 boundary).
test_gate9_missing_credstore_red() {
  local sb out rc stub
  sb="$(mktemp -d)"; mkdir -p "$sb/home/.unimatrix"   # NO hash dir => read-back fails
  stub="$sb/s.sh"; make_call_stub "$stub"
  out="$(
    env SANDBOX="$sb" SMOKE_CLIENT_CALL_CMD="bash $stub" \
      bash -c 'set -uo pipefail; source "'"$SMOKE"'" >/dev/null 2>&1 || true; compose_teardown(){ :; }; client_works_gate' 2>&1
  )"
  rc=$?
  rm -rf "$sb"
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qiE 'read-back ambiguous|credstore root .* absent'; then
    pass "test_gate9_missing_credstore_red (read-back precondition enforced)"
  else
    oops "test_gate9_missing_credstore_red" "expected rc=1 + read-back diagnostic; rc=$rc out=$out"
  fi
}
test_gate9_missing_credstore_red

# node-absent on the REAL path (SMOKE_CLIENT_CALL_CMD unset) => HARD fail exit 1, NOT a
# self-skip (mirrors the bundle-gate node-absence backstop).
test_gate9_node_absent_hard_fails() {
  local sb out rc
  sb="$(mktemp -d)"
  mkdir -p "$sb/home/.unimatrix/deadbeefcafe1234" "$sb/bin"
  printf '{"observe_url":"x","token":"t","fingerprint":"sha256:x"}\n' > "$sb/home/.unimatrix/deadbeefcafe1234/remote.json"
  out="$(
    env -i PATH="$sb/bin:/usr/bin:/bin" SANDBOX="$sb" \
      bash -c '
        set -uo pipefail
        if command -v node >/dev/null 2>&1; then echo "PRECOND-FAIL: node present"; exit 99; fi
        source "'"$SMOKE"'" >/dev/null 2>&1 || true
        compose_teardown() { :; }
        client_works_gate
      ' 2>&1
  )"
  rc=$?
  rm -rf "$sb"
  if [ "$rc" -eq 99 ]; then
    oops "test_gate9_node_absent_hard_fails" "could not construct node-absent PATH (node leaked in)"
  elif [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qF "node not available"; then
    pass "test_gate9_node_absent_hard_fails (exit 1, distinct message, no self-skip)"
  else
    oops "test_gate9_node_absent_hard_fails" "expected rc=1 + node-absent message; rc=$rc out=$out"
  fi
}
test_gate9_node_absent_hard_fails

# S4: on a Gate-9 timeout the bounded drive must leave NO orphaned process. The stub spawns
# a background child (its PID recorded OUTSIDE $SANDBOX so the smoke's cleanup() rm -rf can't
# race it), then hangs past the deadline. `setsid -w timeout -k` group-kills the stub AND its
# child; we assert rc=1 + the bounded-timeout message (rc=124->fail() intact) AND that the
# recorded child PID is gone (no orphan).
test_gate9_timeout_no_orphan() {
  local sb pidf stub out rc child
  sb="$(mktemp -d)"
  pidf="$(mktemp -u)"   # outside $sb: cleanup() rm -rf "$SANDBOX" must not delete it
  mkdir -p "$sb/home/.unimatrix/deadbeefcafe1234"
  printf '{"observe_url":"https://localhost:18443/v1/arch-research/observe","token":"t","fingerprint":"sha256:x"}\n' \
    > "$sb/home/.unimatrix/deadbeefcafe1234/remote.json"
  stub="$sb/orphan-stub.sh"
  cat > "$stub" <<'STUBEOF'
#!/usr/bin/env bash
# a would-be orphan: a background child that outlives the direct child on a naive kill.
sleep 30 &
echo $! > "$PIDF"
sleep 30
STUBEOF
  chmod +x "$stub"
  out="$(
    env SANDBOX="$sb" PIDF="$pidf" SMOKE_CLIENT_CALL_CMD="bash $stub" \
        CLIENT_CALL_DEADLINE_S=1 CLIENT_CALL_KILL_GRACE_S=2 \
      bash -c 'set -uo pipefail; source "'"$SMOKE"'" >/dev/null 2>&1 || true; compose_teardown(){ :; }; client_works_gate' 2>&1
  )"
  rc=$?
  sleep 1   # let the group TERM settle
  child="$(cat "$pidf" 2>/dev/null)"
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qF "timed out after 1s (bounded"; then
    if [ -n "$child" ] && kill -0 "$child" 2>/dev/null; then
      kill -9 "$child" 2>/dev/null || true
      oops "test_gate9_timeout_no_orphan" "background child $child SURVIVED the Gate-9 timeout (orphan)"
    else
      pass "test_gate9_timeout_no_orphan (process-group kill left no orphaned child; rc=124->fail intact)"
    fi
  else
    oops "test_gate9_timeout_no_orphan" "expected rc=1 + bounded-timeout message; rc=$rc out=$out"
  fi
  rm -rf "$sb"; rm -f "$pidf"
}
test_gate9_timeout_no_orphan

# =============================================================================
# Part B — Gate 10 (compose boot) control flow, live-only helpers OVERRIDDEN in-harness.
# =============================================================================
# The docker/curl helpers (compose_plugin_present/do_up/service_cid/expected_digest/
# container_digest/listener_active/extract_cert/health_code) are re-defined in the inner
# subshell AFTER sourcing (never in the shipped file) so the SHIPPED compose_boot_gate
# control flow — plugin-absence hard-fail, digest guard, listener wait, pinned /health —
# runs off-Docker. OV_* env vars drive each override.
#
# run_gate10_case <name> <want_rc> <want_sub> -- <env assignments...>
run_gate10_case() {
  local name="$1" want_rc="$2" want_sub="$3"; shift 3
  [ "${1:-}" = "--" ] && shift
  local sb out got_rc
  sb="$(mktemp -d)"
  # SANDBOX is set so the sourced smoke's EXIT trap (cleanup) ends on a 0-status command;
  # compose_boot_gate itself never reads SANDBOX (it uses only IMAGE/TMP).
  out="$(
    env IMAGE="unimatrix:claim-floor-test" TMP="$sb" SANDBOX="$sb" COMPOSE_LISTENER_DEADLINE_S=6 \
      "$@" \
      bash -c '
        set -uo pipefail
        source "'"$SMOKE"'" >/dev/null 2>&1 || true
        compose_plugin_present()   { return "${OV_PLUGIN_RC:-0}"; }
        compose_do_up()            { return "${OV_UP_RC:-0}"; }
        compose_service_cid()      { printf "%s" "${OV_CID-fake-cid}"; }
        compose_expected_digest()  { printf "%s" "${OV_WANT_DIGEST:-sha256:aaa}"; }
        compose_container_digest() { printf "%s" "${OV_GOT_DIGEST:-sha256:aaa}"; }
        compose_listener_active()  { return "${OV_LISTENER_RC:-0}"; }
        compose_extract_cert()     { if [ -n "${OV_CERT_EMPTY:-}" ]; then : > "$1"; else printf certdata > "$1"; fi; }
        compose_health_code()      { printf "%s" "${OV_HEALTH:-200}"; }
        compose_teardown()         { :; }
        compose_boot_gate
      ' 2>&1
  )"
  got_rc=$?
  rm -rf "$sb"
  if [ "$got_rc" -ne "$want_rc" ]; then
    oops "$name (rc)" "expected rc=$want_rc got=$got_rc; out: $out"; return
  fi
  if [ -n "$want_sub" ] && ! printf '%s' "$out" | grep -qF "$want_sub"; then
    oops "$name (substr)" "expected '$want_sub' in: $out"; return
  fi
  pass "$name"
}

echo "== Part B: Gate 10 compose-boot control flow (helpers overridden in-harness) =="

# Happy path: plugin present, up ok, digests match, listener active, /health 200.
run_gate10_case test_gate10_happy_path_green 0 "PASS gate 10 (compose boot, #915/C1)" --

# Constraint 2 (THE critical row): compose-plugin absent => HARD fail, NEVER self-skip.
run_gate10_case test_gate10_plugin_absent_hard_fails 1 \
  "docker compose plugin absent" -- OV_PLUGIN_RC=1
run_gate10_case test_gate10_plugin_absent_never_self_skip 1 \
  "never self-skip" -- OV_PLUGIN_RC=1

# IMAGE unset => the version-under-test cannot be pinned => RED (constraint 4 precondition).
run_gate10_case test_gate10_image_unset_red 1 \
  "IMAGE unset" -- OV_PLUGIN_RC=0 IMAGE=

# docker compose up fails => RED.
run_gate10_case test_gate10_up_fails_red 1 \
  "docker compose up failed" -- OV_UP_RC=1

# Empty service container id after up => RED.
run_gate10_case test_gate10_no_cid_red 1 \
  "could not resolve the unimatrix service container id" -- OV_CID=

# Constraint 4 guard: booted-image digest != version-under-test digest => RED
# (:latest regression guard — a future refactor can't silently re-introduce a :latest pull).
run_gate10_case test_gate10_digest_mismatch_red 1 \
  ":latest regression guard" -- OV_WANT_DIGEST=sha256:aaa OV_GOT_DIGEST=sha256:bbb

# Listener never active within deadline => RED (boot+serve proof).
run_gate10_case test_gate10_listener_timeout_red 1 \
  "HTTPS listener never became active" -- OV_LISTENER_RC=1 COMPOSE_LISTENER_DEADLINE_S=0

# Cert not extractable from the compose data volume => RED (can't pin TLS).
run_gate10_case test_gate10_cert_absent_red 1 \
  "could not extract the served TLS cert" -- OV_CERT_EMPTY=1

# Constraint 3: pinned TLS /health non-200 => RED.
run_gate10_case test_gate10_health_non200_red 1 \
  "pinned TLS /health on the shipped defaults" -- OV_HEALTH=503

# S1: teardown must FIRE on a partial/failed `up`. With COMPOSE_UP armed BEFORE compose_do_up,
# a nonzero up still leaves the trap flag set, so the smoke's cleanup() runs compose_teardown
# (`down -v`) — no orphaned containers/volumes. The teardown marker lives OUTSIDE $sb so the
# cleanup() rm -rf "$SANDBOX" can't race it. Fails on the pre-fix ordering (flag armed AFTER up
# => trap skips teardown on a failed up).
test_gate10_teardown_fires_on_failed_up() {
  local sb marker out rc
  sb="$(mktemp -d)"
  marker="$(mktemp -u)"   # outside $sb: survives cleanup() rm -rf "$SANDBOX"
  out="$(
    env IMAGE="unimatrix:claim-floor-test" TMP="$sb" SANDBOX="$sb" \
        OV_UP_RC=1 TEARDOWN_MARKER="$marker" \
      bash -c '
        set -uo pipefail
        source "'"$SMOKE"'" >/dev/null 2>&1 || true
        compose_plugin_present() { return 0; }
        compose_do_up()          { return "${OV_UP_RC:-1}"; }
        compose_teardown()       { printf fired > "$TEARDOWN_MARKER"; }
        compose_boot_gate
      ' 2>&1
  )"
  rc=$?
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qF "docker compose up failed" && [ -f "$marker" ]; then
    pass "test_gate10_teardown_fires_on_failed_up (COMPOSE_UP armed before up => trap tears down on a failed up)"
  else
    oops "test_gate10_teardown_fires_on_failed_up" \
      "rc=$rc marker=$( [ -f "$marker" ] && echo present || echo ABSENT ) out=$out"
  fi
  rm -rf "$sb"; rm -f "$marker"
}
test_gate10_teardown_fires_on_failed_up

# =============================================================================
# Part C — static (source/YAML grep) assertions binding the four blocking constraints.
# =============================================================================
echo "== Part C: four blocking constraints present in the shipped bytes (static) =="

test_c1_bridge_drive_bounded_by_timeout() {
  # Constraint 1 (intact after S4): the single-call bridge drive is still bounded by the
  # deadline `timeout ... "$CLIENT_CALL_DEADLINE_S"` — now hardened to a process-group form.
  if grep -q 'timeout -k "\$CLIENT_CALL_KILL_GRACE_S" "\$CLIENT_CALL_DEADLINE_S"' "$CLAIM_LIB"; then
    pass "test_c1_bridge_drive_bounded_by_timeout"
  else
    oops "test_c1_bridge_drive_bounded_by_timeout" "bridge drive not bounded by timeout \$CLIENT_CALL_DEADLINE_S"
  fi
}
test_c1_bridge_drive_bounded_by_timeout

test_s4_drive_process_group_kill() {
  # S4: the bounded drive runs in its OWN process group (`setsid -w timeout -k <grace>`) so
  # a Gate-9 timeout TERMs/KILLs the driver AND the spawned bridge child — no orphan. Both
  # drive branches (stub + real) must use the process-group form.
  local n
  n="$(grep -c 'setsid -w timeout -k "\$CLIENT_CALL_KILL_GRACE_S" "\$CLIENT_CALL_DEADLINE_S"' "$CLAIM_LIB")"
  if [ "$n" -eq 2 ]; then
    pass "test_s4_drive_process_group_kill (both drive branches use setsid process-group kill + kill-after)"
  else
    oops "test_s4_drive_process_group_kill" "expected 2 setsid -w timeout -k drive sites, found $n"
  fi
}
test_s4_drive_process_group_kill

test_s2_gate_publishes_loopback_only() {
  # S2: the GATE binds its published port to loopback (compose_do_up sets
  # UNIMATRIX_HOST_BIND=127.0.0.1:) AND the shipped compose exposes that as an interpolation
  # whose EMPTY default preserves the operator-facing 0.0.0.0 default (gate-scoped, no
  # operator-default change).
  if grep -q 'UNIMATRIX_HOST_BIND="127.0.0.1:" docker compose' "$CLAIM_LIB" \
     && grep -qF '${UNIMATRIX_HOST_BIND:-}8443:8443' "$COMPOSE_YML"; then
    pass "test_s2_gate_publishes_loopback_only"
  else
    oops "test_s2_gate_publishes_loopback_only" "gate does not loopback-bind via UNIMATRIX_HOST_BIND, or compose lost the empty-default interpolation"
  fi
}
test_s2_gate_publishes_loopback_only

test_c2_compose_plugin_absent_fails_never_skips() {
  # Constraint 2: plugin-absence calls fail() ("never self-skip") — NOT exit 3 / a skip.
  if grep -q 'compose_plugin_present \\' "$CLAIM_LIB" \
     && grep -q 'never self-skip' "$CLAIM_LIB" \
     && ! grep -q 'exit 3' "$CLAIM_LIB"; then
    pass "test_c2_compose_plugin_absent_fails_never_skips"
  else
    oops "test_c2_compose_plugin_absent_fails_never_skips" "plugin-absence does not hard-fail via fail() (or introduces a skip)"
  fi
}
test_c2_compose_plugin_absent_fails_never_skips

test_c3_health_pins_shipped_san_via_resolve() {
  # Constraint 3: pinned /health uses --resolve on the shipped SAN:port (cloud.example:8443).
  if grep -q -- '--resolve "\${COMPOSE_SAN}:\${COMPOSE_PORT}:127.0.0.1"' "$CLAIM_LIB" \
     && grep -q 'COMPOSE_SAN:-cloud.example' "$CLAIM_LIB" \
     && grep -q 'COMPOSE_PORT:-8443' "$CLAIM_LIB"; then
    pass "test_c3_health_pins_shipped_san_via_resolve"
  else
    oops "test_c3_health_pins_shipped_san_via_resolve" "pinned /health does not --resolve the shipped SAN cloud.example:8443"
  fi
}
test_c3_health_pins_shipped_san_via_resolve

test_c4_compose_binds_image_under_test_pull_never() {
  # Constraint 4: compose up binds UNIMATRIX_IMAGE=$IMAGE + --pull never, and asserts the
  # booted digest == $IMAGE's digest; the compose file keeps a back-compat interpolation.
  if grep -q 'UNIMATRIX_IMAGE="\$IMAGE" .*docker compose' "$CLAIM_LIB" \
     && grep -q -- '--pull never' "$CLAIM_LIB" \
     && grep -q 'want_digest="\$(compose_expected_digest' "$CLAIM_LIB" \
     && grep -q 'docker inspect "\$1" --format' "$CLAIM_LIB" \
     && grep -qF '${UNIMATRIX_IMAGE:-ghcr.io/dug-21/unimatrix:latest}' "$COMPOSE_YML"; then
    pass "test_c4_compose_binds_image_under_test_pull_never"
  else
    oops "test_c4_compose_binds_image_under_test_pull_never" "compose gate does not pin \$IMAGE with --pull never + digest assertion, or compose file lost the interpolation"
  fi
}
test_c4_compose_binds_image_under_test_pull_never

test_gates_wired_append_only_before_marker() {
  # Both new gates are called after bundle_attach_gates and before the single terminal marker.
  local ba c9 c10 marker
  ba="$(grep -n '^bundle_attach_gates$' "$SMOKE" | tail -1 | cut -d: -f1)"
  c9="$(grep -n '^client_works_gate' "$SMOKE" | tail -1 | cut -d: -f1)"
  c10="$(grep -n '^compose_boot_gate' "$SMOKE" | tail -1 | cut -d: -f1)"
  marker="$(grep -n 'log "ALL GATES PASSED' "$SMOKE" | tail -1 | cut -d: -f1)"
  if [ -n "$ba" ] && [ -n "$c9" ] && [ -n "$c10" ] && [ -n "$marker" ] \
     && [ "$ba" -lt "$c9" ] && [ "$c9" -lt "$marker" ] && [ "$c10" -lt "$marker" ]; then
    pass "test_gates_wired_append_only_before_marker (ba=$ba c9=$c9 c10=$c10 marker=$marker)"
  else
    oops "test_gates_wired_append_only_before_marker" "ba=$ba c9=$c9 c10=$c10 marker=$marker (not append-only before the marker)"
  fi
}
test_gates_wired_append_only_before_marker

test_compose_teardown_from_trap() {
  # Teardown is project-scoped `down -v` invoked from cleanup() (the trap), not inline.
  if grep -q 'compose_teardown' "$SMOKE" \
     && grep -q 'docker compose -p "\$COMPOSE_PROJECT" -f "\$COMPOSE_FILE" down -v' "$CLAIM_LIB"; then
    pass "test_compose_teardown_from_trap"
  else
    oops "test_compose_teardown_from_trap" "compose teardown is not a project-scoped down -v called from the trap"
  fi
}
test_compose_teardown_from_trap

test_cleanup_container_removal_reaps_anon_volumes() {
  # #915/#916 leak fix: the cleanup() container removal must carry -v so the runtime image's
  # unmounted VOLUME (/shared, anonymous each run) is reaped on removal — WITHOUT -v it orphans
  # one anonymous volume per run (unbounded disk growth). Static grep over the shipped smoke.
  if grep -qE 'docker rm -f -v "\$CNAME"' "$SMOKE"; then
    pass "test_cleanup_container_removal_reaps_anon_volumes (docker rm -f -v \$CNAME reaps the anon /shared volume)"
  else
    oops "test_cleanup_container_removal_reaps_anon_volumes" "cleanup() docker rm is missing -v => orphans the anonymous /shared volume each run (#915/#916 leak)"
  fi
}
test_cleanup_container_removal_reaps_anon_volumes

echo
echo "release-gate-claim-floor-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
