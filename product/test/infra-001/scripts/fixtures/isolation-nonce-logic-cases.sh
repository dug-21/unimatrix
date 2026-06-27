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

# marker_is_feature_id_shaped <marker> : 0 (true) iff it satisfies the server's
# looks_like_feature_id (uds/listener.rs:289-303) — >=1 all-digit hyphen-segment AND
# >=1 alpha segment. Independent off-Docker oracle (mirrors assert_marker_feature_id_shaped);
# the OTHER server filter the #859 marker must thread (observe topic_signal else NULL).
marker_is_feature_id_shaped() {
  local m="$1" seg has_digit=0 has_alpha=0 IFS=-
  for seg in $m; do
    [ -z "$seg" ] && continue
    case "$seg" in
      *[!0-9]*) case "$seg" in *[a-z]*) has_alpha=1;; esac ;;
      *)        has_digit=1 ;;
    esac
  done
  [ "$has_digit" -ne 0 ] && [ "$has_alpha" -ne 0 ]
}

# derive_run_markers <pid> <epoch> : echo the 5 derived markers for a (pid,epoch)
# pair via the REAL default path (RUN UNSET) through the shipped _default_nonce seam.
derive_run_markers() {
  local pid="$1" epoch="$2"
  env -u RUN PID_OVERRIDE="$pid" EPOCH_OVERRIDE="$epoch" \
    bash -c 'set -uo pipefail; source "'"$NONCE_GATE"'" >/dev/null 2>&1 || true
      derive_markers
      printf "%s\n%s\n%s\n%s\n%s\n" "$M_OBS_A" "$M_OBS_B" "$M_MCP_A" "$M_MCP_B" "infra003-warmup-${MARKER_FID_TOKEN}-$RUN"'
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
      # #859 two-filter: every marker must ALSO be feature-id-shaped (observe topic_signal
      # else NULL). The two filters pull opposite directions on digit runs; assert BOTH.
      if ! marker_is_feature_id_shaped "$m"; then
        ok=0; oops "test_c_nonce_battery_shape_safe" "marker '$m' is NOT feature-id-shaped (pid=$pid epoch=$epoch) — observe would drop it to NULL"
      fi
    done < <(derive_run_markers "$pid" "$epoch")
  done
  [ "$ok" -eq 1 ] && pass "test_c_nonce_battery_shape_safe (adversarial epoch/pid battery: PII-safe AND feature-id-shaped; R-12 held)"
}

test_c_default_path_self_check_passes() {
  # The in-gate self-check (assert_markers_distinct -> assert_marker_pii_safe) and the
  # warmup self-check must PASS on the REAL default path for an adversarial pair —
  # exit 0, no canary trip (N3 false-positive guard). The seam RUN=t1 never exercised.
  local out rc
  out="$(env -u RUN PID_OVERRIDE=18530 EPOCH_OVERRIDE=1782573915 \
    bash -c 'set -uo pipefail; source "'"$NONCE_GATE"'" >/dev/null 2>&1 || true
      derive_markers; assert_markers_distinct
      wm="infra003-warmup-${MARKER_FID_TOKEN}-$RUN"
      assert_marker_pii_safe "$wm"; assert_marker_feature_id_shaped "$wm"; echo "__SELF_CHECK_OK__"' 2>&1)"
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
    "infra003-obs-a-1-eaqxthaqu3" "infra003-obs-b-1-eaqxthaqu3"
    "infra003-mcp-a-1-eaqxthaqu3" "infra003-mcp-b-1-eaqxthaqu3"
    "infra003-warmup-1-eaqxthaqu3"
    "infra003-obs-a-1-1x0" "infra003-mcp-b-1-1x0" "infra003-warmup-1-1x0"
    "infra003-mcp-a-1-lflrx4ldqpdr"
    "infra003-obs-b-1-2xth95sw" "infra003-mcp-a-1-eaqx2fqkdqp"
    "infra003-warmup-1-2hwcgxgjdgxs"
  )
  for m in "${golden[@]}"; do
    case "$m" in *[!a-z0-9-]*) ok=0; oops "test_c_golden_markers_match_rust_anchor" "golden marker '$m' broke R-12";; esac
    marker_is_pii_shaped "$m" && { ok=0; oops "test_c_golden_markers_match_rust_anchor" "golden marker '$m' is PII-shaped"; }
    marker_is_feature_id_shaped "$m" || { ok=0; oops "test_c_golden_markers_match_rust_anchor" "golden marker '$m' is NOT feature-id-shaped"; }
  done
  [ "$ok" -eq 1 ] && pass "test_c_golden_markers_match_rust_anchor (shared golden set is PII-safe AND feature-id-shaped; mirrors the Rust scanner anchor)"
}

test_c_feature_id_check_trips_on_non_feature_id() {
  # Negative control SYMMETRIC to test_c_canary_trips_on_regression: a marker with NO
  # all-digit hyphen-segment (letter-only nonce) would have its observe topic_signal
  # dropped to NULL (looks_like_feature_id=false, uds/listener.rs:289-303) — the #859
  # regression. assert_marker_feature_id_shaped MUST trip INFRA (exit 2) and name the
  # CATEGORY, never the value. Proves the symmetric self-check has teeth (not a no-op).
  local out rc bad="infra003-obs-a-thaxt2"
  out="$(bash -c 'set -uo pipefail; source "'"$NONCE_GATE"'" >/dev/null 2>&1 || true; assert_marker_feature_id_shaped "'"$bad"'"' 2>&1)"; rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -qiF "feature-id shape" \
     && ! printf '%s' "$out" | grep -qF "$bad"; then
    pass "test_c_feature_id_check_trips_on_non_feature_id (INFRA exit 2; category named, value withheld)"
  else
    oops "test_c_feature_id_check_trips_on_non_feature_id" "rc=$rc out=$out (want INFRA exit 2, no value echo)"
  fi
}

# run_nonce_safety_cases — invoke the five (c) cases (parent supplies pass/oops).
run_nonce_safety_cases() {
  test_c_nonce_battery_shape_safe
  test_c_default_path_self_check_passes
  test_c_canary_trips_on_regression
  test_c_feature_id_check_trips_on_non_feature_id
  test_c_golden_markers_match_rust_anchor
}
