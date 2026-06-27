#!/usr/bin/env bash
# release-gate-isolation-lane-static-test.sh — pre-merge static (YAML grep/parse) gate
# for the infra-004 C-LN standing isolation lane in .github/workflows/release.yml.
# Sibling of release-gate-bundle-static-test.sh; same job_block scoping discipline.
#
# Verify-by-name discipline (#5180): assert release.yml carries the required C-LN
# lane shape — a `multi-tenant-isolation-amd64` job that needs build-container-x64,
# inherits tags+dispatch (no if: guard excluding dispatch), provisions node AND a
# self-contained sqlite3 step, sources release-gate-lib.sh, resolves the image via
# resolve_image (amd64), exports IMAGE, invokes the TRI-STATE runner
# (run_smoke_gate_tristate, not run_smoke_gate), carries NO ${GITHUB_REF_NAME#v} tag
# swallow (R-09 / C-4) and NO docker build (AC-07, pushed bytes). Negative control:
# the lane is NOT YET in create-container-manifest.needs (Wave-2 state; C-FLIP adds
# the blocking edge in Wave 3). No Docker, no node, no network — pre-merge-provable.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_YML="$(cd "$SCRIPT_DIR/../../../.." && pwd)/.github/workflows/release.yml"
LANE="multi-tenant-isolation-amd64"

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# Extract the line range of a job block (from 'job-name:' to the next top-level job
# key at the same 2-space indent), so assertions are scoped to the lane only.
job_block() {
  awk -v job="$1" '
    $0 ~ "^  " job ":" { inblk=1; print; next }
    inblk && /^  [a-zA-Z0-9_-]+:/ { inblk=0 }
    inblk { print }
  ' "$RELEASE_YML"
}

echo "== AC-06 lane present with independent status + needs build-container-x64 =="
test_lane_job_exists() {
  if grep -Eq "^  ${LANE}:" "$RELEASE_YML"; then
    pass "test_lane_job_exists"
  else
    oops "test_lane_job_exists" "job '${LANE}:' not found in release.yml"
  fi
}
test_lane_job_exists

test_lane_needs_build_container_x64() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq '^[[:space:]]*needs:[[:space:]]*\[[[:space:]]*build-container-x64[[:space:]]*\]'; then
    pass "test_lane_needs_build_container_x64"
  else
    oops "test_lane_needs_build_container_x64" "lane needs: is not exactly [build-container-x64]"
  fi
}
test_lane_needs_build_container_x64

echo "== AC-06 lane inherits BOTH triggers (tags + dispatch); no if: excluding dispatch =="
test_workflow_triggers_tags_and_dispatch() {
  # The lane has no own `on:`; it inherits the workflow-level triggers. Assert both
  # are declared at the workflow level so the lane runs on tag-push AND dispatch.
  if grep -Eq "^[[:space:]]+tags:[[:space:]]*\['v\*'\]" "$RELEASE_YML" \
     && grep -Eq '^[[:space:]]+workflow_dispatch:' "$RELEASE_YML"; then
    pass "test_workflow_triggers_tags_and_dispatch"
  else
    oops "test_workflow_triggers_tags_and_dispatch" "workflow on: must declare tags:['v*'] AND workflow_dispatch"
  fi
}
test_workflow_triggers_tags_and_dispatch

test_lane_no_if_guard() {
  # No `if:` in the lane — an `if: github.event_name != 'workflow_dispatch'` (as on
  # build-linux-* / create-container-manifest) would exclude the AC-11 dispatch path.
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq '^[[:space:]]*if:'; then
    oops "test_lane_no_if_guard" "lane has an if: guard — would exclude the dispatch path (AC-11)"
  else
    pass "test_lane_no_if_guard"
  fi
}
test_lane_no_if_guard

echo "== AC-10 lane provisions node AND a self-contained sqlite3 step =="
test_lane_provisions_node() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -q 'uses: actions/setup-node@v4'; then
    pass "test_lane_provisions_node"
  else
    oops "test_lane_provisions_node" "lane missing actions/setup-node@v4"
  fi
}
test_lane_provisions_node

test_lane_provisions_sqlite3() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq 'apt-get install -y sqlite3'; then
    pass "test_lane_provisions_sqlite3"
  else
    oops "test_lane_provisions_sqlite3" "lane missing self-contained 'apt-get install -y sqlite3' step"
  fi
}
test_lane_provisions_sqlite3

echo "== AC-07 lane sources lib, resolves image, exports IMAGE; no rebuild =="
test_lane_calls_resolve_image() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -q 'source product/test/infra-001/scripts/release-gate-lib.sh' \
     && printf '%s\n' "$blk" | grep -Eq 'resolve_image "\$\{GITHUB_REPOSITORY_OWNER\}" "\$\{GITHUB_EVENT_NAME\}" "\$\{GITHUB_REF_NAME\}" amd64'; then
    pass "test_lane_calls_resolve_image"
  else
    oops "test_lane_calls_resolve_image" "lane does not source the lib and call resolve_image(... amd64)"
  fi
}
test_lane_calls_resolve_image

test_lane_exports_image() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq '^[[:space:]]*export IMAGE[[:space:]]*$'; then
    pass "test_lane_exports_image"
  else
    oops "test_lane_exports_image" "lane does not export IMAGE for the smoke acquisition path"
  fi
}
test_lane_exports_image

test_lane_no_docker_build() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq 'docker build|build-push-action|setup-buildx'; then
    oops "test_lane_no_docker_build" "lane contains a build step — it must smoke the PUSHED bytes (AC-07), not rebuild"
  else
    pass "test_lane_no_docker_build"
  fi
}
test_lane_no_docker_build

echo "== R-09 / C-4 no \${GITHUB_REF_NAME#v} tag swallow anywhere in the lane =="
test_lane_no_ref_strip() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq 'GITHUB_REF_NAME#v|#v'; then
    oops "test_lane_no_ref_strip" "forbidden \${GITHUB_REF_NAME#v} / #v strip in the lane (nan-019 swallow class)"
  else
    pass "test_lane_no_ref_strip"
  fi
}
test_lane_no_ref_strip

echo "== AC-07 lane invokes the TRI-STATE runner (not run_smoke_gate) =="
test_lane_invokes_tristate() {
  local blk
  blk="$(job_block "$LANE")"
  if printf '%s\n' "$blk" | grep -Eq 'run_smoke_gate_tristate "\$\{IMAGE\}" bash product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh'; then
    pass "test_lane_invokes_tristate"
  else
    oops "test_lane_invokes_tristate" "lane does not invoke run_smoke_gate_tristate against multi-tenant-isolation-smoke.sh"
  fi
}
test_lane_invokes_tristate

test_lane_not_plain_run_smoke_gate() {
  # The lane must call ONLY the tri-state variant — a bare run_smoke_gate (no exit-2
  # branch) would make INFRA blocking. Strip the tri-state token, then assert no
  # remaining run_smoke_gate occurrence survives.
  local blk residual
  blk="$(job_block "$LANE")"
  residual="$(printf '%s\n' "$blk" | sed 's/run_smoke_gate_tristate//g' | grep -c 'run_smoke_gate')"
  if [ "$residual" -eq 0 ]; then
    pass "test_lane_not_plain_run_smoke_gate"
  else
    oops "test_lane_not_plain_run_smoke_gate" "lane calls bare run_smoke_gate ($residual) — INFRA would block; use the tri-state runner"
  fi
}
test_lane_not_plain_run_smoke_gate

echo "== Wave-2 state (negative): lane NOT YET in create-container-manifest.needs (C-FLIP) =="
test_lane_not_in_manifest_needs() {
  local blk
  blk="$(job_block create-container-manifest)"
  if printf '%s\n' "$blk" | grep -Eq '^[[:space:]]*needs:.*'"${LANE}"; then
    oops "test_lane_not_in_manifest_needs" "lane is already in create-container-manifest.needs — that is C-FLIP (Wave 3), not Wave 2"
  else
    pass "test_lane_not_in_manifest_needs"
  fi
}
test_lane_not_in_manifest_needs

echo
echo "release-gate-isolation-lane-static-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
