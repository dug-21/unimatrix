#!/usr/bin/env bash
# release-gate-bundle-assembly-logic-test.sh — pre-merge HARD gate for the nan-022
# C5' dimension-bundle ASSEMBLY (cloud-bundle-lib.sh::emit_dimension_bundle +
# assemble_shell_captures). Companion to release-gate-cloud-cycle-logic-test.sh
# (nan-021/C5 spine) — split out so neither file exceeds the 500-line workspace
# rule (mirrors the nan-021 cloud-cycle-lib.sh lib factor-out).
#
# R-09 (the highest-value cross-language seam): JS/shell EMIT the on-disk bundle;
# Python INGESTS it. A missing/empty capture must ERROR (exit 1), never write an
# empty-key bundle that reads as empty-equals-empty PARITY-PASS. This test drives
# the SHIPPED emit_dimension_bundle against synthesized fragments OFF-Docker (no
# Docker, no live container read) so the never-empty guard + barrier-ordering +
# bundle shape are proven BEFORE the first tag round (#5258 / R-10). It sources the
# SAME cloud-cycle-lib.sh the smoke sources (which transitively sources
# cloud-bundle-lib.sh), so it exercises the EXACT shipped bytes (a paraphrased copy
# would test nothing — R-01).
#
# Fully local + deterministic: no Docker, no network, no tag push. node required
# (the assembly + shape check use it); the shell captures are supplied as fixtures,
# so sqlite3 is NOT required here (the live D2 read is a Docker/Stage-3c concern).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLOUD_LIB="${SCRIPT_DIR}/cloud-cycle-lib.sh"

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

command -v node >/dev/null 2>&1 || { echo "FATAL: node required for the bundle-assembly logic test" >&2; exit 1; }

# Source the SHIPPED libs so emit_dimension_bundle / assemble_shell_captures are the
# REAL functions under test (single source of truth). They need log()/fail() from
# the parent; the emit subshells re-source + re-provide them.
set +e
# shellcheck source=cloud-cycle-lib.sh
source "$CLOUD_LIB" >/dev/null 2>&1
set -uo pipefail
if ! declare -F emit_dimension_bundle >/dev/null; then
  echo "FATAL: emit_dimension_bundle not found after sourcing $CLOUD_LIB (cloud-bundle-lib.sh not sourced?)" >&2
  exit 1
fi

# A VALID bridge-surface driver fragment (retrieval/proactive/metric_vector + edges).
write_valid_fragment() {
  cat > "$1" <<'JSONEOF'
{"ok":true,
 "metric_vector":{"universal":{"total_tool_calls":3}},
 "retrieval":{"queries":[{"tool":"context_search","args":{},"result_ids":["1","2"],"scores":[0.9,0.8]}],
              "capture_2":[{"tool":"context_search","args":{},"result_ids":["1","2"],"scores":[0.9,0.8]}]},
 "proactive":{"briefing_ids":["1"],"briefing_scores":[0.9],"injection_set":["1"],
              "capture_2":{"briefing_ids":["1"],"briefing_scores":[0.9],"injection_set":["1"]}},
 "informs_edges":[],"phase_signal":{}}
JSONEOF
}

# A VALID shell-captures fragment {topic_signals, isolation, precompact}.
write_valid_shell_captures() {
  cat > "$1" <<'JSONEOF'
{"topic_signals":["nan-022"],
 "isolation":{"slug_a_writes_visible_to_b":false,"landed_only_in_a":true},
 "precompact":{"restored_payload":null,"measurable":false,"host_side_gap":"documented host-side gap (ADR-006/OQ-2)"}}
JSONEOF
}

# emit_case <name> <fragment_json> <shell_json> <want_rc> <want_substr_on_red>
# Sources the SHIPPED libs in a subshell, provides log()/fail(), and calls the REAL
# emit_dimension_bundle against the synthesized inputs. On a green row it asserts the
# out-file is a contract-shaped six-key bundle; on a RED row it asserts NO bundle
# leaked (an empty-key bundle would be an R-09 empty-pass).
emit_case() {
  local name="$1" frag="$2" shellcap="$3" want_rc="$4" want_sub="$5"
  local sb out rc out_file
  sb="$(mktemp -d)"
  out_file="$sb/out.json"
  out="$(
    SANDBOX="$sb" \
    bash -c '
      set -uo pipefail
      source "'"$CLOUD_LIB"'" >/dev/null 2>&1 || true
      log()  { printf "[bundle-logic] %s\n" "$*"; }
      fail() { printf "[bundle-logic] FAIL: %s\n" "$*" >&2; exit 1; }
      _dump_bridge_err() { :; }
      emit_dimension_bundle "'"$frag"'" "'"$shellcap"'" "run-tok-E" "'"$out_file"'"
    ' 2>&1
  )"
  rc=$?
  local six_ok=0
  if [ -s "$out_file" ]; then
    node -e '
      const fs=require("fs"); const o=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));
      const want=["analytics","behavioral","isolation","precompact","proactive","retrieval"];
      const have=Object.keys(o.dimension_bundle||{}).sort();
      process.exit(o.run_token==="run-tok-E" && JSON.stringify(have)===JSON.stringify(want) ? 0 : 1);
    ' "$out_file" >/dev/null 2>&1 && six_ok=1
  fi
  rm -rf "$sb"
  if [ "$rc" -ne "$want_rc" ]; then
    oops "$name (rc)" "expected rc=$want_rc got=$rc; out: $out"
    return
  fi
  if [ -n "$want_sub" ] && ! printf '%s' "$out" | grep -qF "$want_sub"; then
    oops "$name (substr)" "expected '$want_sub' in: $out"
    return
  fi
  if [ "$want_rc" -eq 0 ] && [ "$six_ok" -ne 1 ]; then
    oops "$name (bundle shape)" "green row did not write a contract-shaped six-key bundle"
    return
  fi
  if [ "$want_rc" -ne 0 ] && [ "$six_ok" -eq 1 ]; then
    oops "$name (empty-pass leak)" "RED row STILL wrote a bundle — an empty-key bundle leaked (R-09 violation)"
    return
  fi
  pass "$name"
}

echo "== dimension-bundle assembly: never-empty guard + barrier order (nan-022 R-09/R-04) =="

EFRAG="$(mktemp)"; write_valid_fragment "$EFRAG"
ESHELL="$(mktemp)"; write_valid_shell_captures "$ESHELL"

# Green: a valid fragment + valid shell captures => six-key bundle written.
emit_case test_bundle_emit_valid_writes_six_keys "$EFRAG" "$ESHELL" 0 ""

# RED: retrieval capture empty (queries:[]) => exit 1, NO bundle written (R-09).
ERET_EMPTY="$(mktemp)"
cat > "$ERET_EMPTY" <<'JSONEOF'
{"ok":true,"metric_vector":{"universal":{"total_tool_calls":3}},
 "retrieval":{"queries":[],"capture_2":[]},
 "proactive":{"briefing_ids":["1"],"briefing_scores":[0.9],"injection_set":["1"],"capture_2":{"briefing_ids":["1"]}},
 "informs_edges":[],"phase_signal":{}}
JSONEOF
emit_case test_bundle_emit_empty_retrieval_errors "$ERET_EMPTY" "$ESHELL" 1 "retrieval capture missing/empty"

# RED: missing proactive capture_2 (intra double-capture absent) => exit 1, no bundle.
EPRO_NOCAP2="$(mktemp)"
cat > "$EPRO_NOCAP2" <<'JSONEOF'
{"ok":true,"metric_vector":{"universal":{"total_tool_calls":3}},
 "retrieval":{"queries":[{"tool":"context_search","args":{},"result_ids":["1"],"scores":[0.9]}],"capture_2":[{"tool":"context_search","args":{},"result_ids":["1"],"scores":[0.9]}]},
 "proactive":{"briefing_ids":["1"],"briefing_scores":[0.9],"injection_set":["1"]},
 "informs_edges":[],"phase_signal":{}}
JSONEOF
emit_case test_bundle_emit_missing_proactive_capture2_errors "$EPRO_NOCAP2" "$ESHELL" 1 "proactive capture missing/empty"

# RED: empty MetricVector (analytics) => exit 1 (R-06 barrier-released-early class).
EMV_EMPTY="$(mktemp)"
cat > "$EMV_EMPTY" <<'JSONEOF'
{"ok":true,"metric_vector":{},
 "retrieval":{"queries":[{"tool":"context_search","args":{},"result_ids":["1"],"scores":[0.9]}],"capture_2":[{"tool":"context_search","args":{},"result_ids":["1"],"scores":[0.9]}]},
 "proactive":{"briefing_ids":["1"],"briefing_scores":[0.9],"injection_set":["1"],"capture_2":{"briefing_ids":["1"]}},
 "informs_edges":[],"phase_signal":{}}
JSONEOF
emit_case test_bundle_emit_empty_metric_vector_errors "$EMV_EMPTY" "$ESHELL" 1 "empty/short MetricVector"

# RED: behavioral topic_signals empty => exit 1, never an empty-pass (R-09).
ESHELL_NOBEH="$(mktemp)"
cat > "$ESHELL_NOBEH" <<'JSONEOF'
{"topic_signals":[],
 "isolation":{"slug_a_writes_visible_to_b":false,"landed_only_in_a":true},
 "precompact":{"restored_payload":null,"measurable":false,"host_side_gap":"gap"}}
JSONEOF
emit_case test_bundle_emit_empty_behavioral_errors "$EFRAG" "$ESHELL_NOBEH" 1 "behavioral topic_signals missing/empty"

# RED: precompact measurable=false but host_side_gap NOT named => vacuous-pass guard (R-08).
ESHELL_VACUOUS="$(mktemp)"
cat > "$ESHELL_VACUOUS" <<'JSONEOF'
{"topic_signals":["nan-022"],
 "isolation":{"slug_a_writes_visible_to_b":false,"landed_only_in_a":true},
 "precompact":{"restored_payload":null,"measurable":false,"host_side_gap":""}}
JSONEOF
emit_case test_bundle_emit_unnamed_precompact_gap_errors "$EFRAG" "$ESHELL_VACUOUS" 1 "host_side_gap not named"

# RED: precompact restored_payload null but measurable=true (illegal null) => exit 1 (R-08).
ESHELL_BADNULL="$(mktemp)"
cat > "$ESHELL_BADNULL" <<'JSONEOF'
{"topic_signals":["nan-022"],
 "isolation":{"slug_a_writes_visible_to_b":false,"landed_only_in_a":true},
 "precompact":{"restored_payload":null,"measurable":true,"host_side_gap":null}}
JSONEOF
emit_case test_bundle_emit_illegal_null_payload_errors "$EFRAG" "$ESHELL_BADNULL" 1 "illegal null"

# RED: isolation booleans missing => exit 1 (NFR-6 exact-compare floor needs them).
ESHELL_NOISO="$(mktemp)"
cat > "$ESHELL_NOISO" <<'JSONEOF'
{"topic_signals":["nan-022"],
 "isolation":{"slug_a_writes_visible_to_b":false},
 "precompact":{"restored_payload":null,"measurable":false,"host_side_gap":"gap"}}
JSONEOF
emit_case test_bundle_emit_missing_isolation_booleans_errors "$EFRAG" "$ESHELL_NOISO" 1 "isolation booleans missing"

echo "== barrier ordering + single-source (R-04 / C-5) =="

# Barrier ordering (R-04): the shell-owned captures (assemble_shell_captures) run
# ONLY AFTER cycle_durability_barrier in cloud_cycle_gates' control flow. A pre-
# barrier DB read is an INFRA condition, never a parity verdict. Verbatim-source
# check (the live ordering is asserted in the Docker matrix; this pins the bytes).
test_bundle_shell_captures_after_barrier() {
  local bar_line cap_line
  bar_line="$(grep -n 'cycle_durability_barrier "\$SLUG_DIR"' "$CLOUD_LIB" | head -1 | cut -d: -f1)"
  cap_line="$(grep -n 'assemble_shell_captures "\$SLUG_DIR"' "$CLOUD_LIB" | head -1 | cut -d: -f1)"
  if [ -n "$bar_line" ] && [ -n "$cap_line" ] && [ "$cap_line" -gt "$bar_line" ]; then
    pass "test_bundle_shell_captures_after_barrier (DB-read captures gated AFTER the durability barrier — R-04)"
  else
    oops "test_bundle_shell_captures_after_barrier" "shell captures (line $cap_line) not ordered after the barrier (line $bar_line) — R-04 pre-barrier read hazard"
  fi
}
test_bundle_shell_captures_after_barrier

# Single-source the contract: cloud-cycle-lib.sh sources cloud-bundle-lib.sh (the
# split mirrors nan-021's lib factor-out; no second copy of the assembly — C-5).
test_bundle_lib_sourced_single_source() {
  if grep -q 'cloud-bundle-lib.sh' "$CLOUD_LIB" \
     && declare -F emit_dimension_bundle >/dev/null \
     && declare -F assemble_shell_captures >/dev/null; then
    pass "test_bundle_lib_sourced_single_source (cloud-bundle-lib.sh sourced; emit + assemble defined once)"
  else
    oops "test_bundle_lib_sourced_single_source" "cloud-bundle-lib.sh not sourced or bundle fns missing (C-5 single-source)"
  fi
}
test_bundle_lib_sourced_single_source

# SMOKE_SHELL_CAPTURES stub seam: assemble_shell_captures honours the env override
# (proves the off-Docker drive path that supplies synthesized captures w/o sqlite3).
test_bundle_assemble_honours_stub_seam() {
  local sb caps out_file rc
  sb="$(mktemp -d)"; caps="$sb/caps.json"; out_file="$sb/shell.json"
  write_valid_shell_captures "$caps"
  (
    set -uo pipefail
    source "$CLOUD_LIB" >/dev/null 2>&1 || true
    log()  { :; }
    fail() { printf "FAIL: %s\n" "$*" >&2; exit 1; }
    SANDBOX="$sb" SMOKE_SHELL_CAPTURES="$caps" assemble_shell_captures "$sb/slug" "$sb/manifest.json" "$out_file"
  ) >/dev/null 2>&1
  rc=$?
  local ok=0
  [ "$rc" -eq 0 ] && [ -s "$out_file" ] \
    && node -e 'const o=require(process.argv[1]);process.exit(Array.isArray(o.topic_signals)&&o.isolation&&o.precompact?0:1)' "$out_file" >/dev/null 2>&1 && ok=1
  rm -rf "$sb"
  if [ "$ok" -eq 1 ]; then
    pass "test_bundle_assemble_honours_stub_seam (SMOKE_SHELL_CAPTURES short-circuits the live captures off-Docker)"
  else
    oops "test_bundle_assemble_honours_stub_seam" "assemble_shell_captures did not honour the SMOKE_SHELL_CAPTURES stub seam (rc=$rc)"
  fi
}
test_bundle_assemble_honours_stub_seam

rm -f "$EFRAG" "$ESHELL" "$ERET_EMPTY" "$EPRO_NOCAP2" "$EMV_EMPTY" \
      "$ESHELL_NOBEH" "$ESHELL_VACUOUS" "$ESHELL_BADNULL" "$ESHELL_NOISO"

echo
echo "release-gate-bundle-assembly-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
