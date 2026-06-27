#!/usr/bin/env bash
# isolation-nonce-logic-cases.sh — (c) #859 construction-safe-nonce gate-logic cases,
# factored out of release-gate-isolation-logic-test.sh to keep that file <=500 lines
# (workspace rule), mirroring the isolation-probe-lib.sh lib-split precedent.
#
# SOURCED by release-gate-isolation-logic-test.sh AFTER it has defined the pass()/
# oops() counters; run_nonce_safety_cases() drives the four (c) cases against the
# shipped multi-tenant-isolation-smoke.sh (located relative to THIS file so no global
# is required). It relies only on the parent harness providing pass()/oops().
#
# Coverage (the historical blind spot: the suite pinned RUN=t1 and NEVER exercised
# the real numeric-epoch default — the exact path that tripped the MCP context_store
# content scanner). These cases drive the REAL default path (RUN UNSET) through the
# injectable PID_OVERRIDE/EPOCH_OVERRIDE seam and assert: (i) derived markers match
# NEITHER charset-reduced PII shape, (ii) the in-gate self-check PASSES on the default
# path (N3 false-positive guard), (iii) the canary DOES trip on a shaped regression,
# and (iv) the shared golden set (mirrored in the Rust scanner anchor) is shape-safe.

# Locate the shipped gate relative to this fixture (scripts/fixtures/.. -> scripts/).
NONCE_GATE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/multi-tenant-isolation-smoke.sh"

# The same reduced projections the gate's assert_marker_pii_safe canary uses
# (scanning.rs:300-309), duplicated here as the independent off-Docker oracle.
NONCE_PHONE_SHAPE='1?[2-9][0-9]{2}-?[0-9]{3}-?[0-9]{4}'
NONCE_SSN_SHAPE='[0-9]{3}-[0-9]{2}-[0-9]{4}'

# marker_is_pii_shaped <marker> : 0 (true) iff it matches either reduced PII shape.
marker_is_pii_shaped() {
  local m="$1"
  [[ "$m" =~ $NONCE_PHONE_SHAPE ]] && return 0
  [[ "$m" =~ $NONCE_SSN_SHAPE ]] && return 0
  return 1
}

# derive_run_markers <pid> <epoch> : echo the 5 derived markers for a (pid,epoch)
# pair via the REAL default path (RUN UNSET) through the shipped _default_nonce seam.
derive_run_markers() {
  local pid="$1" epoch="$2"
  env -u RUN PID_OVERRIDE="$pid" EPOCH_OVERRIDE="$epoch" \
    bash -c 'set -uo pipefail; source "'"$NONCE_GATE"'" >/dev/null 2>&1 || true
      derive_markers
      printf "%s\n%s\n%s\n%s\n%s\n" "$M_OBS_A" "$M_OBS_B" "$M_MCP_A" "$M_MCP_B" "infra003-warmup-$RUN"'
}

test_c_nonce_battery_shape_safe() {
  # Adversarial battery: the captured failing epoch, all-digit/boundary encodings,
  # values that WOULD have formed phone/SSN shapes under the OLD numeric nonce, and
  # the live current epoch. Each must produce markers free of both PII shapes.
  local ok=1 pair pid epoch m
  local battery=(
    "18530:1782573915"      # the captured failing pid/epoch (old nonce: 18530-1782573915)
    "1:0" "2:1" "1:9999999999" "999999:9999999999"   # boundary / all-digit / max
    "5305178:25305178"      # old-nonce would be 5305178-25305178 (phone-shaped run)
    "123:456789012"         # old-nonce 123-456789012 -> 23-456-7890 phone shape
    "123:4500006789"        # old-nonce 123-4500006789: digit-dense epoch
    "$$:$(date +%s)"        # the live default pid/epoch
  )
  for pair in "${battery[@]}"; do
    pid="${pair%%:*}"; epoch="${pair##*:}"
    while IFS= read -r m; do
      [ -n "$m" ] || continue
      case "$m" in *[!a-z0-9-]*) ok=0; oops "test_c_nonce_battery_shape_safe" "marker '$m' broke R-12 charset (pid=$pid epoch=$epoch)";; esac
      if marker_is_pii_shaped "$m"; then
        ok=0; oops "test_c_nonce_battery_shape_safe" "marker matched a PII shape for pid=$pid epoch=$epoch (digits withheld)"
      fi
    done < <(derive_run_markers "$pid" "$epoch")
  done
  [ "$ok" -eq 1 ] && pass "test_c_nonce_battery_shape_safe (adversarial epoch/pid battery: no phone/SSN shape; R-12 held)"
}

test_c_default_path_self_check_passes() {
  # The in-gate self-check (assert_markers_distinct -> assert_marker_pii_safe) and the
  # warmup self-check must PASS on the REAL default path for an adversarial pair —
  # exit 0, no canary trip (N3 false-positive guard). The seam RUN=t1 never exercised.
  local out rc
  out="$(env -u RUN PID_OVERRIDE=18530 EPOCH_OVERRIDE=1782573915 \
    bash -c 'set -uo pipefail; source "'"$NONCE_GATE"'" >/dev/null 2>&1 || true
      derive_markers; assert_markers_distinct
      assert_marker_pii_safe "infra003-warmup-$RUN"; echo "__SELF_CHECK_OK__"' 2>&1)"
  rc=$?
  if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -qF "__SELF_CHECK_OK__" \
     && printf '%s' "$out" | grep -qF "pairwise non-substring"; then
    pass "test_c_default_path_self_check_passes (default-path markers pass the canary; no false trip)"
  else
    oops "test_c_default_path_self_check_passes" "rc=$rc out=$out (want self-check pass on default path)"
  fi
}

test_c_canary_trips_on_regression() {
  # Negative control: a hand-built phone/SSN-shaped marker MUST trip assert_marker_pii_safe
  # (INFRA exit 2) — proves the canary has teeth and is not a no-op, and that it does
  # NOT echo the offending digits (N4). (digits are a synthetic shape, never a real marker.)
  local out rc shaped offending
  for shaped in "infra003-mcp-a-2025551234" "infra003-mcp-a-123-45-6789"; do
    offending="${shaped##infra003-mcp-a-}"   # the shape-bearing digit suffix
    out="$(bash -c 'set -uo pipefail; source "'"$NONCE_GATE"'" >/dev/null 2>&1 || true; assert_marker_pii_safe "'"$shaped"'"' 2>&1)"; rc=$?
    if [ "$rc" -ne 2 ] || ! printf '%s' "$out" | grep -qiE 'phone-shape|SSN-shape'; then
      oops "test_c_canary_trips_on_regression" "shaped marker did not trip the canary (rc=$rc out=$out)"; return
    fi
    if printf '%s' "$out" | grep -qF "$offending"; then
      oops "test_c_canary_trips_on_regression" "canary message echoed the offending marker digits (N4 violation)"; return
    fi
  done
  pass "test_c_canary_trips_on_regression (phone/SSN shapes trip INFRA; no digits echoed)"
}

test_c_golden_markers_match_rust_anchor() {
  # SHARED GOLDEN SET (ISOLATION_GATE_GOLDEN_MARKERS): identical literals to the Rust
  # anchor test_scan_isolation_gate_golden_markers_pass (scanning.rs). The Rust test
  # asserts the REAL ContentScanner accepts them; here the off-Docker oracle agrees
  # they are shape-safe and charset-clean — the two teeth share one set so they cannot
  # drift. If you edit one list, edit the other.
  local ok=1 m
  local golden=(
    "infra003-obs-a-eaqxthaqu3" "infra003-obs-b-eaqxthaqu3"
    "infra003-mcp-a-eaqxthaqu3" "infra003-mcp-b-eaqxthaqu3"
    "infra003-warmup-eaqxthaqu3"
    "infra003-obs-a-1x0" "infra003-mcp-b-1x0" "infra003-warmup-1x0"
    "infra003-mcp-a-lflrx4ldqpdr"
    "infra003-obs-b-2xth95sw" "infra003-mcp-a-eaqx2fqkdqp"
    "infra003-warmup-2hwcgxgjdgxs"
  )
  for m in "${golden[@]}"; do
    case "$m" in *[!a-z0-9-]*) ok=0; oops "test_c_golden_markers_match_rust_anchor" "golden marker '$m' broke R-12";; esac
    marker_is_pii_shaped "$m" && { ok=0; oops "test_c_golden_markers_match_rust_anchor" "golden marker '$m' is PII-shaped"; }
  done
  [ "$ok" -eq 1 ] && pass "test_c_golden_markers_match_rust_anchor (shared golden set is shape-safe; mirrors the Rust scanner anchor)"
}

# run_nonce_safety_cases — invoke the four (c) cases (parent supplies pass/oops).
run_nonce_safety_cases() {
  test_c_nonce_battery_shape_safe
  test_c_default_path_self_check_passes
  test_c_canary_trips_on_regression
  test_c_golden_markers_match_rust_anchor
}
