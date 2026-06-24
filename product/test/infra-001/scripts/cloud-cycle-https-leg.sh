#!/usr/bin/env bash
# cloud-cycle-https-leg.sh (nan-021 C5) — the UNIMATRIX_HTTPS_SMOKE target that
# wraps the C1+C2 HTTPS leg (docker-http-posture-smoke.sh, including Gate 8
# cloud_cycle_gates) in release-gate-lib.sh:run_smoke_gate so the verify-by-name
# exit-code discriminator + anchored terminal run-marker GATE the live HTTPS leg
# (ADR-005 / AC-05 / R-08). NO re-authored gate-runner logic: it sources the
# SHIPPED run_smoke_gate verbatim and the smoke does its own nan-019 image
# acquisition (docker pull || inspect || exit 4). A green leg PROVABLY means the
# smoke ran end-to-end and printed `[783-smoke] ALL GATES PASSED` — Docker-absent
# (exit 3) is a HARD failure, never green; an early-exit-0 without the marker is
# RED.
#
# WHY a wrapper (and not pytest calling run_smoke_gate directly): the pytest
# orchestrator (ADR-001, harness/parity_legs.py:run_https_leg) shells out to
# $UNIMATRIX_HTTPS_SMOKE with the LOCKED C2 contract env already exported
# (MANIFEST_PATH / RUN_TOKEN / HTTPS_VECTOR_OUT / SANDBOX). Pointing that env at
# THIS script inserts the run_smoke_gate discriminator between pytest and the
# smoke WITHOUT modifying C3's leg driver (AC-06/AC-07 — consume C1–C4, do not
# edit them). The smoke writes MetricVector(HTTPS) to $HTTPS_VECTOR_OUT; pytest
# then ingests it (token-guarded) and runs the C4 comparator.
#
# Usage (set by the release-gate lane, consumed by pytest):
#   IMAGE=<ghcr ref>  UNIMATRIX_HTTPS_SMOKE=.../cloud-cycle-https-leg.sh
# Inherited from pytest's run_https_leg env (NOT re-derived here):
#   MANIFEST_PATH RUN_TOKEN HTTPS_VECTOR_OUT SANDBOX
#
# Exit codes propagate the smoke's contract THROUGH run_smoke_gate's verdict:
#   0  -> smoke ran + ALL GATES PASSED marker present (the ONLY green)
#   1  -> run_smoke_gate RED: skip(3)/unacquirable(4)/broke(1)/no-marker/unexpected
# pytest's run_https_leg treats any non-zero as a HARD fail (never skip / never
# an empty compare — R-03).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE="${SCRIPT_DIR}/docker-http-posture-smoke.sh"

# shellcheck source=release-gate-lib.sh
source "${SCRIPT_DIR}/release-gate-lib.sh"
if ! declare -F run_smoke_gate >/dev/null; then
  echo "::error::cloud-cycle-https-leg: run_smoke_gate not found after sourcing release-gate-lib.sh" >&2
  exit 1
fi

# Required C2 contract env (set by pytest's run_https_leg). Absent => mis-wired:
# fail LOUD rather than run a contract-less smoke that can't emit the HTTPS vector.
: "${MANIFEST_PATH:?cloud-cycle-https-leg: MANIFEST_PATH unset (C2 contract — set by pytest run_https_leg)}"
: "${RUN_TOKEN:?cloud-cycle-https-leg: RUN_TOKEN unset (R-03 correlation token)}"
: "${HTTPS_VECTOR_OUT:?cloud-cycle-https-leg: HTTPS_VECTOR_OUT unset (out-file path)}"

# IMAGE selects the prebuilt GHCR tag for the smoke's nan-019 acquisition path
# (docker pull || inspect || exit 4). On a dev box without IMAGE the smoke builds
# locally; in the release lane the lane exports IMAGE via resolve_image.
IMAGE_REF="${IMAGE:-}"

# run_smoke_gate exports IMAGE for the child and invokes the smoke EXACTLY ONCE
# (no pipe — rc survives), discriminates the exit-code truth table, then asserts
# the anchored whole-line `[*-smoke] ALL GATES PASSED` marker. The smoke inherits
# MANIFEST_PATH/RUN_TOKEN/HTTPS_VECTOR_OUT/SANDBOX from THIS process's env, so its
# Gate 8 (cloud_cycle_gates) runs and writes the HTTPS MetricVector out-file.
run_smoke_gate "${IMAGE_REF}" bash "${SMOKE}"
