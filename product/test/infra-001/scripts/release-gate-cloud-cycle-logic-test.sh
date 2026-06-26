#!/usr/bin/env bash
# release-gate-cloud-cycle-logic-test.sh — pre-merge HARD gate for the nan-021 C5
# gate wiring (AC-05 / R-08 / R-12). Companion to release-gate-logic-test.sh
# (nan-019 spine) and release-gate-bundle-logic-test.sh (nan-020 Gates 5–7).
#
# R-12 (the first-green tax): the C5 gate spine — the false-green discriminator
# (run_smoke_gate exit-code truth table + anchored [*-smoke] ALL GATES PASSED
# marker), the cloud-cycle-https-leg.sh wrapper wiring, and C2's cloud_cycle_gates
# orchestration control flow — only ever runs LIVE on a release tag. This test
# EXERCISES THE EXACT SHIPPED BYTES off-Docker so the gate logic is proven BEFORE
# the first tag round (mirrors nan-019 #5258 / #5192). It `source`s the SAME
# release-gate-lib.sh release.yml sources and the SAME cloud-cycle-lib.sh the
# smoke sources, and drives them against stubs via the SMOKE_*_CMD seams. A
# paraphrased copy of the gate would test nothing (R-01).
#
# Fully local + deterministic: no Docker, no node-in-path requirement (the cycle
# drive is stubbed via SMOKE_CYCLE_CMD), no network, no tag push.
#
# Covers:
#   * the run_smoke_gate exit-code truth table THROUGH the C5 wrapper
#     (0=pass+marker · 3=Docker-absent HARD fail · 4=unacquirable · 1=broken ·
#      *=unexpected · 0-without-marker=RED) — the false-green discriminator.
#   * the anchored whole-line marker (substring forge rejected) — verbatim regex.
#   * acquisition reuse: the smoke (and the lane) reuse pull||inspect||exit-4
#     verbatim; no re-authored acquisition in C5 (fork-smell guard).
#   * the wrapper requires the C2 contract env (mis-wire => loud fail, not a
#     contract-less run).
#   * C2 cloud_cycle_gates control flow via SMOKE_CYCLE_CMD: read-back precondition,
#     gate 8a ok:true/ok:false, gate 8c review, and the gate-8 HTTPS MetricVector
#     out-file emission (non-empty-vector guard / R-06) — the live-only bits
#     (_fire_observe_hooks pinned curl + cycle_durability_barrier) are overridden
#     in THIS harness (not in the shipped file) since their curl path is live-only.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/release-gate-lib.sh"
CLOUD_LIB="${SCRIPT_DIR}/cloud-cycle-lib.sh"
WRAPPER="${SCRIPT_DIR}/cloud-cycle-https-leg.sh"
SMOKE="${SCRIPT_DIR}/docker-http-posture-smoke.sh"
STUB="${SCRIPT_DIR}/fixtures/stub-smoke.sh"
RELEASE_YML="$(cd "$SCRIPT_DIR/../../../.." && pwd)/.github/workflows/release.yml"

MARKER='[783-smoke] ALL GATES PASSED'

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# --- consume the SHIPPED gate bytes (single source of truth) ------------------
# shellcheck source=release-gate-lib.sh
source "$LIB"
if ! declare -F run_smoke_gate >/dev/null; then
  echo "FATAL: run_smoke_gate not found after sourcing $LIB" >&2
  exit 1
fi

# =============================================================================
# Part A — the false-green discriminator THROUGH the C5 wrapper / shipped lib.
# =============================================================================
# Drive the REAL run_smoke_gate (sourced) against the controllable stub-smoke,
# exactly as nan-019 does, but asserting the SAME truth table the C5 lane relies
# on. The wrapper (cloud-cycle-https-leg.sh) calls this same run_smoke_gate, so
# proving the spine here proves the wrapper's verdict path.
#
# run_case <name> <stub_rc> <stub_body> <stub_stream> <expect_gate_rc> <expect_diag>
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

echo "== Part A: false-green discriminator (run_smoke_gate truth table) =="

# (0, marker present) — the ONLY green cell (AC-05).
run_case test_c5_exit0_marker_present_green \
  0 "${MARKER} — clean image boots HTTP-on, cloud cycle drove parity over the bridge." stdout \
  0 ""

# (3, Docker-absent) — skip-is-failure: a HARD fail, NEVER reports passed (R-08).
run_case test_c5_docker_absent_exits_3_hard_fail \
  3 "[783-smoke] SKIP: Docker not available" stdout \
  1 "mis-provisioned"

# (4, image unacquirable) — DISTINCT from absent (3), distinct diagnostic (R-08/OQ3).
run_case test_c5_image_unacquirable_exits_4_distinct \
  4 "[783-smoke] FAIL: could not pull ghcr.io/x/unimatrix:latest-amd64" stdout \
  1 "could not pull prebuilt IMAGE"

# (1, shipped-path broken).
run_case test_c5_exit1_shipped_path_broken \
  1 "[783-smoke] FAIL: per-slug observe returned HTTP 404" stdout \
  1 "first-run path is broken"

# (2 / unexpected).
run_case test_c5_unexpected_exit2 \
  2 "boom" stdout \
  1 "exited unexpectedly (exit 2)"

# (0, NO marker) — early-exit-0 forge guard (R-08): exits 0 but no marker => RED.
run_case test_c5_exit0_no_marker_red \
  0 "[783-smoke] PASS gate 1" stdout \
  1 "exited 0 but never printed ALL GATES PASSED"

echo "== Part A: anchored whole-line marker (no substring forge — R-08) =="

# Marker as a SUBSTRING of a longer physical line: grep -qxE must NOT match => RED.
run_case test_c5_run_marker_anchored_substring_rejected \
  0 "xx ${MARKER} yy trailing junk on the same line" stdout \
  1 "exited 0 but never printed ALL GATES PASSED"

# Marker as its OWN whole line is the legitimate green (the smoke's real emission).
run_case test_c5_run_marker_whole_line_green \
  0 "${MARKER}
[783-smoke] (later harmless prose)" stdout \
  0 ""

# Assert the lib's grep pattern is the VERBATIM nan-019 anchored regex (no re-author).
echo "== Part A: marker regex + acquisition are nan-019 verbatim (no fork — R-10) =="
test_c5_marker_regex_verbatim() {
  if grep -qF "grep -qxE '\\[[a-z0-9-]+-smoke\\] ALL GATES PASSED.*'" "$LIB"; then
    pass "test_c5_marker_regex_verbatim (anchored [*-smoke] ALL GATES PASSED grep present in release-gate-lib.sh)"
  else
    oops "test_c5_marker_regex_verbatim" "anchored run-marker grep not the nan-019 verbatim regex in $LIB"
  fi
}
test_c5_marker_regex_verbatim

test_c5_acquisition_pull_then_inspect_verbatim() {
  # The smoke (the leg the lane gates) reuses nan-019's pull||inspect||exit-4
  # acquisition verbatim — inspect does NOT pull, so pull-first avoids the #5208
  # cross-runner cache-miss false-FAIL. C5 adds NO new acquisition path.
  if grep -q 'docker pull "\$IMAGE"' "$SMOKE" \
     && grep -q 'docker image inspect "\$IMAGE"' "$SMOKE" \
     && grep -q 'exit 4' "$SMOKE"; then
    pass "test_c5_acquisition_pull_then_inspect_verbatim (pull || inspect || exit 4 in the smoke)"
  else
    oops "test_c5_acquisition_pull_then_inspect_verbatim" "smoke acquisition is not the nan-019 pull||inspect||exit-4 chain"
  fi
}
test_c5_acquisition_pull_then_inspect_verbatim

test_c5_no_new_gate_runner_logic() {
  # C5 reuses run_smoke_gate AS-IS: the wrapper SOURCES release-gate-lib.sh and
  # calls run_smoke_gate; it does NOT re-implement the exit-code case or the grep.
  if grep -q 'source "${SCRIPT_DIR}/release-gate-lib.sh"' "$WRAPPER" \
     && grep -q 'run_smoke_gate' "$WRAPPER" \
     && ! grep -q '::error::smoke SKIPPED' "$WRAPPER"; then
    pass "test_c5_no_new_gate_runner_logic (wrapper sources + calls run_smoke_gate; no re-authored gate runner)"
  else
    oops "test_c5_no_new_gate_runner_logic" "wrapper re-authors gate-runner logic or does not source release-gate-lib.sh"
  fi
}
test_c5_no_new_gate_runner_logic

# =============================================================================
# Part B — the C5 wrapper requires the C2 contract env (mis-wire => loud fail).
# =============================================================================
echo "== Part B: cloud-cycle-https-leg.sh requires the C2 contract env =="
test_c5_wrapper_requires_contract_env() {
  local out rc
  # No MANIFEST_PATH/RUN_TOKEN/HTTPS_VECTOR_OUT => the `: \"${VAR:?}\"` guards fire
  # BEFORE any smoke run. We point IMAGE at nothing so even if it proceeded it
  # could not false-green. Must be non-zero with a contract diagnostic.
  out="$(env -u MANIFEST_PATH -u RUN_TOKEN -u HTTPS_VECTOR_OUT \
        bash "$WRAPPER" 2>&1)"
  rc=$?
  if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -qiE 'MANIFEST_PATH (unset|: unbound)|MANIFEST_PATH unset'; then
    pass "test_c5_wrapper_requires_contract_env (missing C2 env => non-zero + diagnostic)"
  else
    oops "test_c5_wrapper_requires_contract_env" "expected non-zero + MANIFEST_PATH diagnostic; rc=$rc out=$out"
  fi
}
test_c5_wrapper_requires_contract_env

# =============================================================================
# Part C — C2 cloud_cycle_gates orchestration control flow (SMOKE_CYCLE_CMD stub).
# =============================================================================
# Source the SHIPPED cloud-cycle-lib.sh (the smoke sources the same file) to get
# the REAL cloud_cycle_gates. It depends on log()/fail() and the live-only curl
# helpers (_fire_observe_hooks) + cycle_durability_barrier; we provide log()/fail()
# and OVERRIDE the two live-only helpers IN THIS HARNESS (never in the shipped
# file) so the stubbed drive exercises the read-back -> 8a -> 8c -> emit spine
# off-Docker. SMOKE_CYCLE_CMD drives the bridge step with a synthetic vector.
set +e
# shellcheck source=cloud-cycle-lib.sh
source "$CLOUD_LIB"
set -uo pipefail
if ! declare -F cloud_cycle_gates >/dev/null; then
  echo "FATAL: cloud_cycle_gates not found after sourcing $CLOUD_LIB" >&2
  exit 1
fi

# Parent-scope helpers the smoke normally provides (so cloud_cycle_gates can run).
log()  { printf '[c5-logic] %s\n' "$*"; }
fail() { printf '[c5-logic] FAIL: %s\n' "$*" >&2; exit 1; }
# Live-only helpers overridden in THIS harness (their pinned curl + busybox-vol
# du paths are Docker/network-only). Overriding here keeps the shipped C2 bytes
# untouched while letting the orchestration spine run off-Docker.
_fire_observe_hooks()        { log "stub: _fire_observe_hooks (live-only curl path overridden in C5 logic test)"; }
cycle_durability_barrier()   { log "stub: cycle_durability_barrier (live-only store-poll overridden in C5 logic test)"; }

# A SMOKE_CYCLE_CMD stub: emits the {ok:..,metric_vector:..} envelope the gate reads.
# Controlled by STUB_DRIVE_OK (true/false) so we can drive the ok:false RED row.
# nan-022: the gate-8a drive only greps ok:true here; the bridge-surface fragment
# (retrieval/proactive) is synthesized at the review step (SMOKE_REVIEW_VECTOR
# back-compat path) so the legacy {ok,metric_vector} drive stub still exercises
# the spine through the new bundle emit.
make_cycle_stub() {
  local f="$1" ok="$2"
  cat > "$f" <<STUBEOF
#!/usr/bin/env bash
# stub bridge cycle drive — writes the driver envelope cloud_cycle_gates greps.
printf '{"ok": ${ok}, "metric_vector": {"universal":{"total_tool_calls":3}}}\n'
STUBEOF
  chmod +x "$f"
}

# A contract-shaped shell-captures fragment (nan-022 SMOKE_SHELL_CAPTURES seam):
# {topic_signals, isolation, precompact} the bundle assembler composes with the
# bridge-surface fragment. Defaults to a VALID (non-empty) fragment; callers can
# point SMOKE_SHELL_CAPTURES at a malformed one to drive the never-empty RED rows.
make_shell_captures() {
  local f="$1"
  cat > "$f" <<'STUBEOF'
{"topic_signals":["nan-022"],
 "isolation":{"slug_a_writes_visible_to_b":false,"landed_only_in_a":true},
 "precompact":{"restored_payload":null,"measurable":false,"host_side_gap":"documented host-side gap (ADR-006/OQ-2)"}}
STUBEOF
}

# A growing store sampler stub (SMOKE_STORE_SIZE_CMD seam): the BEFORE sample
# reads small, the AFTER sample reads larger, so the gate's `store_after >
# store_before` durability check passes off-Docker. State via a counter file.
make_store_stub() {
  local f="$1" counter="$2"
  cat > "$f" <<STUBEOF
#!/usr/bin/env bash
# stub store_size — returns 1 on the first call (BEFORE), 9 thereafter (AFTER).
c="\$(cat '${counter}' 2>/dev/null || echo 0)"
c=\$((c+1)); printf '%s' "\$c" > '${counter}'
if [ "\$c" -le 1 ]; then echo 1; else echo 9; fi
STUBEOF
  chmod +x "$f"
}

# run_cloud_case — set up a sandbox + credstore + manifest, drive cloud_cycle_gates
# in a subshell with the live-only helpers overridden + SMOKE_CYCLE_CMD stubbed.
# <name> <expect_rc> <expect_substr> <drive_ok> [extra env KV...]
run_cloud_case() {
  local name="$1" want_rc="$2" want_sub="$3" drive_ok="$4"; shift 4
  local sb manifest out got_rc stub
  sb="$(mktemp -d)"
  mkdir -p "$sb/home/.unimatrix/deadbeefhash"
  printf '{"observe_url":"x","token":"t","fingerprint":"sha256:x"}\n' > "$sb/home/.unimatrix/deadbeefhash/remote.json"
  manifest="$sb/manifest.json"
  printf '{"session_id":"nan-021-run","feature_cycle":"nan-021","tool_calls":[]}\n' > "$manifest"
  stub="$sb/cycle-stub.sh"
  make_cycle_stub "$stub" "$drive_ok"
  local store_stub="$sb/store-stub.sh"
  make_store_stub "$store_stub" "$sb/store.counter"
  # nan-022: supply the shell-owned /observe-surface captures via the stub seam so
  # the bundle assembly + never-empty guard run off-Docker (no live container read).
  local shell_caps="$sb/shell-captures.json"
  make_shell_captures "$shell_caps"

  # Run cloud_cycle_gates with the overridden helpers in scope. We re-source the
  # cloud lib in the subshell (single source of truth) then re-apply the harness
  # overrides AFTER, so the shipped cloud_cycle_gates calls our stubs. The store
  # sampler is injected via the SMOKE_STORE_SIZE_CMD seam (grows AFTER>BEFORE so
  # the durability delta check passes off-Docker).
  out="$(
    SANDBOX="$sb" SLUG_DIR="$sb/slug" \
    MANIFEST_PATH="$manifest" RUN_TOKEN="nan-021-run" \
    HTTPS_VECTOR_OUT="$sb/https_vector.json" \
    SMOKE_CYCLE_CMD="bash $stub" \
    SMOKE_STORE_SIZE_CMD="bash $store_stub" \
    SMOKE_SHELL_CAPTURES="$shell_caps" \
    PORT=18443 SLUG=arch-research TOKEN=tkn TMP="$sb" \
    "$@" \
    bash -c '
      set -uo pipefail
      source "'"$CLOUD_LIB"'" >/dev/null 2>&1 || true
      log()  { printf "[c5-logic] %s\n" "$*"; }
      fail() { printf "[c5-logic] FAIL: %s\n" "$*" >&2; exit 1; }
      _fire_observe_hooks()      { log "stub fire"; }
      cycle_durability_barrier() { log "stub barrier"; }
      cloud_cycle_gates
    ' 2>&1
  )"
  got_rc=$?
  local out_file="$sb/https_vector.json"
  local emitted=0
  [ -s "$out_file" ] && emitted=1
  # Stash the emitted out-file CONTENT (before teardown) so callers can assert the
  # bundle shape (nan-022). Empty when no out-file was written (a RED row).
  LAST_OUT_CONTENT=""
  [ -s "$out_file" ] && LAST_OUT_CONTENT="$(cat "$out_file")"
  rm -rf "$sb"

  if [ "$got_rc" -ne "$want_rc" ]; then
    oops "$name (rc)" "expected rc=$want_rc got=$got_rc; out: $out"
    return
  fi
  if [ -n "$want_sub" ] && ! printf '%s' "$out" | grep -qF "$want_sub"; then
    oops "$name (substr)" "expected '$want_sub' in: $out"
    return
  fi
  # Stash whether the out-file was emitted for the caller via a global.
  LAST_EMITTED="$emitted"
  pass "$name"
}

echo "== Part C: C2 cloud_cycle_gates control flow (off-Docker, SMOKE_CYCLE_CMD) =="

# Happy path: read-back ok, drive ok:true, review ok:true, emit the dimension bundle.
LAST_EMITTED=0
LAST_OUT_CONTENT=""
run_cloud_case test_c5_cloud_cycle_happy_emits_bundle 0 "PASS gate 8 (cloud cycle)" true
if [ "${LAST_EMITTED:-0}" = "1" ]; then
  pass "test_c5_cloud_cycle_emits_https_out_file (out-file written non-empty)"
else
  oops "test_c5_cloud_cycle_emits_https_out_file" "dimension-bundle out-file not emitted on the happy path"
fi

# nan-022 R-09: the emitted out-file is {run_token, dimension_bundle:{...}} with ALL
# SIX capture_keys present (matching parity_bundle_contract.md), NOT the nan-021
# bare metric_vector. Asserted on the captured content via a small node shape check.
test_c5_cloud_cycle_emits_six_key_bundle() {
  if [ -z "$LAST_OUT_CONTENT" ]; then
    oops "test_c5_cloud_cycle_emits_six_key_bundle" "no out-file content captured from the happy path"
    return
  fi
  local rc
  printf '%s' "$LAST_OUT_CONTENT" | node -e '
    let b="";process.stdin.on("data",c=>b+=c).on("end",()=>{
      let o; try { o=JSON.parse(b) } catch(e){ console.error("not JSON: "+e.message); process.exit(1) }
      if (typeof o.run_token !== "string" || !o.run_token) { console.error("run_token missing/non-string"); process.exit(1) }
      const db=o.dimension_bundle;
      if (db===null || typeof db!=="object") { console.error("dimension_bundle missing"); process.exit(1) }
      const want=["retrieval","behavioral","analytics","proactive","precompact","isolation"];
      const have=Object.keys(db).sort();
      if (JSON.stringify(have)!==JSON.stringify([...want].sort())) { console.error("keys="+have.join(",")); process.exit(1) }
      // analytics carries the nan-021 metric_vector (consumed-verbatim slice).
      if (!db.analytics || typeof db.analytics.metric_vector!=="object") { console.error("analytics.metric_vector missing"); process.exit(1) }
      // precompact carries the documented host-side gap (never a vacuous pass).
      if (db.precompact.measurable!==false || !db.precompact.host_side_gap) { console.error("precompact gap not named"); process.exit(1) }
      // isolation booleans present (compared EXACTLY downstream — NFR-6).
      if (typeof db.isolation.slug_a_writes_visible_to_b!=="boolean" || typeof db.isolation.landed_only_in_a!=="boolean") { console.error("isolation booleans missing"); process.exit(1) }
    });
  '
  rc=$?
  if [ "$rc" -eq 0 ]; then
    pass "test_c5_cloud_cycle_emits_six_key_bundle (run_token + all 6 capture_keys, analytics.metric_vector, named precompact gap, isolation booleans)"
  else
    oops "test_c5_cloud_cycle_emits_six_key_bundle" "emitted out-file is not a contract-shaped six-key dimension bundle"
  fi
}
test_c5_cloud_cycle_emits_six_key_bundle

# Drive reports ok:false => gate 8a RED (the drive failed).
run_cloud_case test_c5_cloud_cycle_drive_not_ok_red 1 "bridge cycle reported ok:false" false

# Missing credstore read-back => RED (C1->C2 precondition, R-11 read-back).
test_c5_cloud_cycle_missing_credstore_red() {
  local sb out rc manifest stub
  sb="$(mktemp -d)"; mkdir -p "$sb/home/.unimatrix"   # NO hash dir => read-back fails
  manifest="$sb/manifest.json"
  printf '{"session_id":"x","feature_cycle":"nan-021","tool_calls":[]}\n' > "$manifest"
  stub="$sb/s.sh"; make_cycle_stub "$stub" true
  out="$(
    SANDBOX="$sb" SLUG_DIR="$sb/slug" MANIFEST_PATH="$manifest" RUN_TOKEN="x" \
    HTTPS_VECTOR_OUT="$sb/o.json" SMOKE_CYCLE_CMD="bash $stub" \
    PORT=1 SLUG=s TOKEN=t TMP="$sb" \
    bash -c '
      set -uo pipefail
      source "'"$CLOUD_LIB"'" >/dev/null 2>&1 || true
      log()  { printf "[c5-logic] %s\n" "$*"; }
      fail() { printf "[c5-logic] FAIL: %s\n" "$*" >&2; exit 1; }
      _fire_observe_hooks(){ :; }; cycle_durability_barrier(){ :; }; store_size(){ echo 10; }
      cloud_cycle_gates
    ' 2>&1
  )"
  rc=$?
  rm -rf "$sb"
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qiE 'read-back ambiguous|credstore .* absent'; then
    pass "test_c5_cloud_cycle_missing_credstore_red (read-back precondition enforced)"
  else
    oops "test_c5_cloud_cycle_missing_credstore_red" "expected rc=1 + read-back diagnostic; rc=$rc out=$out"
  fi
}
test_c5_cloud_cycle_missing_credstore_red

# Missing C2 contract env (MANIFEST_PATH) => RED precondition.
test_c5_cloud_cycle_requires_manifest_env() {
  local sb out rc
  sb="$(mktemp -d)"
  out="$(
    SANDBOX="$sb" SLUG_DIR="$sb/slug" RUN_TOKEN="x" HTTPS_VECTOR_OUT="$sb/o.json" \
    bash -c '
      set -uo pipefail
      source "'"$CLOUD_LIB"'" >/dev/null 2>&1 || true
      log()  { printf "[c5-logic] %s\n" "$*"; }
      fail() { printf "[c5-logic] FAIL: %s\n" "$*" >&2; exit 1; }
      cloud_cycle_gates
    ' 2>&1
  )"
  rc=$?
  rm -rf "$sb"
  if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qF "MANIFEST_PATH unset"; then
    pass "test_c5_cloud_cycle_requires_manifest_env (C2 contract precondition enforced)"
  else
    oops "test_c5_cloud_cycle_requires_manifest_env" "expected rc=1 + MANIFEST_PATH diagnostic; rc=$rc out=$out"
  fi
}
test_c5_cloud_cycle_requires_manifest_env

# nan-022 NOTE: the dimension-bundle ASSEMBLY scenarios (never-empty guard, barrier
# ordering, six-key shape, missing-capture->error) live in the sibling
# release-gate-bundle-assembly-logic-test.sh — split out so neither file exceeds the
# 500-line workspace rule (mirrors the nan-021 cloud-cycle-lib.sh lib factor-out).
# This file keeps the C5 false-green discriminator + cloud_cycle_gates control-flow
# spine (Parts A-D); the bundle-assembly file owns emit_dimension_bundle's R-09/R-04.

# =============================================================================
# Part D — the release-gate lane is workflow_dispatch/tag (D-3), NOT pull_request.
# =============================================================================
echo "== Part D: release-gate lane wiring (workflow_dispatch/tag, NOT pull_request) =="

# Extract a job block (2-space-indented job key to the next job key).
job_block() {
  awk -v job="$1" '
    $0 ~ "^  " job ":" { inblk=1; print; next }
    inblk && /^  [a-zA-Z0-9_-]+:/ { inblk=0 }
    inblk { print }
  ' "$RELEASE_YML"
}

test_c5_lane_in_release_workflow_not_ci() {
  # The parity job lives in release.yml (workflow_dispatch + tag-push triggers),
  # NOT in ci.yml. release.yml's top-level triggers are tags v* + workflow_dispatch.
  if grep -q '^  nan-021-https-uds-parity:' "$RELEASE_YML" \
     && grep -qE "tags: \['v\*'\]" "$RELEASE_YML" \
     && grep -q 'workflow_dispatch:' "$RELEASE_YML"; then
    pass "test_c5_lane_in_release_workflow_not_ci (parity job in release.yml under tag/dispatch)"
  else
    oops "test_c5_lane_in_release_workflow_not_ci" "nan-021-https-uds-parity not wired in release.yml under tag/dispatch"
  fi
}
test_c5_lane_in_release_workflow_not_ci

test_c5_lane_invokes_pytest_orchestrator() {
  local blk
  blk="$(job_block nan-021-https-uds-parity)"
  if printf '%s\n' "$blk" | grep -q 'test_https_uds_parity.py' \
     && printf '%s\n' "$blk" | grep -q '\-m parity' \
     && printf '%s\n' "$blk" | grep -q 'UNIMATRIX_HTTPS_SMOKE=' \
     && printf '%s\n' "$blk" | grep -q 'cloud-cycle-https-leg.sh'; then
    pass "test_c5_lane_invokes_pytest_orchestrator (pytest -m parity wired to the run_smoke_gate HTTPS leg)"
  else
    oops "test_c5_lane_invokes_pytest_orchestrator" "lane does not invoke the pytest orchestrator wired to UNIMATRIX_HTTPS_SMOKE -> cloud-cycle-https-leg.sh"
  fi
}
test_c5_lane_invokes_pytest_orchestrator

test_c5_lane_resolves_image_via_shipped_lib() {
  local blk
  blk="$(job_block nan-021-https-uds-parity)"
  # Acquisition: the lane sources release-gate-lib.sh and resolves IMAGE via the
  # SHIPPED resolve_image (no re-authored tag math); the smoke's pull||inspect||
  # exit-4 then acquires it.
  if printf '%s\n' "$blk" | grep -q 'source scripts/release-gate-lib.sh' \
     && printf '%s\n' "$blk" | grep -q 'resolve_image'; then
    pass "test_c5_lane_resolves_image_via_shipped_lib (resolve_image reused; no re-authored acquisition)"
  else
    oops "test_c5_lane_resolves_image_via_shipped_lib" "lane does not resolve IMAGE via the shipped release-gate-lib.sh"
  fi
}
test_c5_lane_resolves_image_via_shipped_lib

echo
echo "release-gate-cloud-cycle-logic-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
