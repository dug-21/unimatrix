#!/usr/bin/env bash
# stub-client-bundle.sh — controllable stand-in for the Gate 5 `client-bundle`
# emit (docker run … --project-dir /data client-bundle <slug>) used by
# release-gate-logic-test.sh. Touches no Docker, no network, no real image.
#
# Mirrors the only two things Gate 5 keys on:
#   1. the process exit code (emit rc≠0 => Gate 5 hard-fails)
#   2. the captured STDOUT blob (prefix/empty validation)
#
# Env-driven so the truth table sweeps without editing files:
#   STUB_EMIT_RC      : integer exit code               (default 0)
#   STUB_EMIT_STDOUT  : blob printed on stdout          (default a valid bundle)
#   STUB_EMIT_STDERR  : token-redacted echo on stderr   (default a redacted line)
#
# The real emitter drops stderr (2>/dev/null in emit_bundle); the stderr line
# here proves the harness never folds the token-redacted echo into the blob
# (R-05 test_capture_stdout_only_not_stderr).

rc="${STUB_EMIT_RC:-0}"
out="${STUB_EMIT_STDOUT-unimatrix-bundle:v2.stub.blob}"
err="${STUB_EMIT_STDERR-observe_url=https://localhost:18443/v1/arch-research/observe token=***REDACTED*** fp=sha256:deadbeef}"

[ -n "$err" ] && printf '%s\n' "$err" >&2
[ -n "$out" ] && printf '%s\n' "$out"
exit "$rc"
