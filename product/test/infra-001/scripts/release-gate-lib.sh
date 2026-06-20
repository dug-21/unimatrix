#!/usr/bin/env bash
# release-gate-lib.sh — single source of truth for the nan-019 verify-by-name release gate.
#
# This library is SOURCED by .github/workflows/release.yml (the smoke-amd64 / smoke-arm64
# jobs) AND by the pre-merge gate-logic stub-smoke test (product/test/infra-001/...).
# Both consume the SAME bytes, so the tested logic cannot silently diverge from the
# shipped logic (test-gate-logic-stub-smoke.md, OQ — extraction mechanism (a), R-01).
#
# Contracts (VERBATIM — IMPLEMENTATION-BRIEF Data Structures / ADR-003 / ADR-002 / ADR-004):
#   smoke exit contract : 0 = ran+passed · 1 = ran+failed (fail()) · 3 = self-skipped (Docker/net absent)
#                         · 4 = IMAGE= prebuilt tag could not be pulled / not present locally (#795)
#   positive run-marker : terminal line  [<name>-smoke] ALL GATES PASSED  (printed only after all gates).
#                         <name> is the per-smoke tag, e.g. 783 (http-posture) or 767 (embed-readiness);
#                         the gate matches any [*-smoke] ALL GATES PASSED line so one spine drives all smokes.
#   tag resolution      : push    -> TAG="${GITHUB_REF_NAME}"  (UN-stripped, keeps the v) => :v<version>-<arch>
#                         dispatch -> TAG="latest"                                          => :latest-<arch>
#                         NEVER ${GITHUB_REF_NAME#v}
#   image owner         : ghcr.io/<owner>/unimatrix
#
# No retry, no continue-on-error: the smoke is invoked exactly once (OQ-6 / ADR-003).

# resolve_image OWNER EVENT_NAME REF_NAME ARCH
#   Echoes the per-arch GHCR image reference the smoke must pull.
#   push    (REF_NAME=v1.2.3)        -> ghcr.io/<owner>/unimatrix:v1.2.3-<arch>   (UN-stripped)
#   dispatch                          -> ghcr.io/<owner>/unimatrix:latest-<arch>
resolve_image() {
  local owner="$1" event_name="$2" ref_name="$3" arch="$4"
  local tag
  if [ "${event_name}" = "workflow_dispatch" ]; then
    tag="latest"                       # branch ref: only :latest-<arch> was pushed
  else
    tag="${ref_name}"                  # v* push: KEEP the v. NEVER ${ref_name#v}
  fi
  printf 'ghcr.io/%s/unimatrix:%s-%s' "${owner}" "${tag}" "${arch}"
}

# run_smoke_gate IMAGE SMOKE_CMD...
#   The load-bearing verify-by-name gate (ADR-003, pinned shape).
#   Invokes the smoke exactly once with IMAGE exported, captures the exit code so it
#   survives set -e / pipefail (NO pipe between the smoke and $? — the #4873 class, R-02),
#   discriminates on the exit code, then asserts the anchored terminal run-marker.
#   Returns 0 iff RC==0 AND the marker line was captured; otherwise emits a cause-specific
#   ::error:: and returns 1. Caller must `set -e` / `exit` on the returned status.
run_smoke_gate() {
  local image="$1"; shift
  local out rc
  set +e
  out="$(IMAGE="${image}" "$@" 2>&1)"
  rc=$?
  set -e
  echo "${out}"                        # surface full smoke log into the job log
  case "${rc}" in
    0) : ;;
    3) echo "::error::smoke SKIPPED (exit 3): Docker-capable lane mis-provisioned — HARD failure (SR-01)."; return 1 ;;
    4) echo "::error::smoke FAILED (exit 4): could not pull prebuilt IMAGE — confirm the tag was pushed / network healthy (#795)."; return 1 ;;
    1) echo "::error::smoke FAILED (exit 1): shipped image first-run path is broken."; return 1 ;;
    *) echo "::error::smoke exited unexpectedly (exit ${rc})."; return 1 ;;
  esac
  echo "${out}" | grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*' \
    || { echo "::error::smoke exited 0 but never printed ALL GATES PASSED — early-exit-0 (SR-01)."; return 1; }
  return 0
}
