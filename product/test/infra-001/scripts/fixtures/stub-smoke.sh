#!/usr/bin/env bash
# stub-smoke.sh — a controllable stand-in for docker-http-posture-smoke.sh used by
# release-gate-logic-test.sh. It does NOT touch Docker, the network, or any real image.
#
# It mirrors the only two things the gate (run_smoke_gate in release-gate-lib.sh) keys on:
#   1. the process exit code  (the 0/1/3/other discrimination — ADR-003)
#   2. the captured stdout/stderr buffer (where the anchored terminal run-marker may live)
#
# Behaviour is driven entirely by env so the truth table can be swept without editing files:
#   STUB_RC      : integer exit code the stub exits with        (default 0)
#   STUB_BODY    : literal text printed before exiting          (default empty)
#   STUB_STREAM  : "stdout" | "stderr"  — where STUB_BODY goes  (default stdout)
#
# The gate captures `"$(... 2>&1)"`, so a stub that writes to stderr must still reach the
# marker grep (R-02 `test_gate_captures_stderr`). STUB_STREAM=stderr exercises that path.

rc="${STUB_RC:-0}"
body="${STUB_BODY:-}"
stream="${STUB_STREAM:-stdout}"

if [ -n "$body" ]; then
  if [ "$stream" = "stderr" ]; then
    printf '%s\n' "$body" >&2
  else
    printf '%s\n' "$body"
  fi
fi

exit "$rc"
