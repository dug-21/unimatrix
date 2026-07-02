#!/usr/bin/env bash
#
# infra-002 full-workspace LINK smoke — durable guard for bug #878 (link-step OOM).
#
# Bug recap: `cargo test --workspace` OOM-killed (SIGKILL) at the LINK step — the BFD
# `ld` process was killed because cumulative N-parallel-link RSS summed past the memory
# ceiling (swap exhausted). The failure was salvaged THREE times without ever exercising
# the real thing: `-j1` (#750), then `--lib` (#873/#877, which SKIPS the integration-test
# links entirely), then this fix (#878). Nothing exercised the full-workspace link, so a
# re-regression stayed invisible until a human ran the full suite locally.
#
# This guard runs `cargo test --workspace --no-run` — link only, no test execution — at
# the repo's CONFIGURED build parallelism (the `jobs` cap in .cargo/config.toml) and FAILS
# if the link does not complete.
#
# Why the configured cap and NOT default `-j nproc`: MEASURED on the #878 box, after the
# [profile.dev] debug-info reduction, peak `ld` RSS is ~1112 MB per heavy server link and
# default `-j nproc(10)` STILL OOMs (10 x 1112 ~= 11 GB > ~9.4 GB avail, swap exhausted).
# The fix delivers link safety through the jobs cap, not by making 10-way links fit — so a
# default-`-j` smoke is guaranteed-RED post-fix and could never be a green gate check.
# It is also NOT `-j1`: a single link always fits (~1.1-1.8 GB) and would neuter the guard.
# Running at the configured cap sums the real operational load (e.g. 6 x 1112 ~= 6.7 GB of
# concurrent link RSS) — a genuine memory stress that still TRIPS on:
#   - cumulative growth: when per-link RSS grows enough that cap x per-link crosses the
#     ceiling (the exact recurrence this cycle-breaks), AND
#   - profile-stanza removal: without [profile.dev]'s levers per-link reverts to ~1842 MB,
#     so cap x 1842 (e.g. 6 x 1842 ~= 11 GB) OOMs -> RED. The guard therefore also covers
#     the fix's *presence* without a separate profile-presence assertion.
# If the cap is later raised in config, this guard runs at the raised value and goes RED if
# that raise is unsafe — correctly gating cap changes too.
#
# Why a standalone shell guard (not a `cargo test` target): the invariant it protects is
# "the workspace still LINKS under its real build parallelism". This trips on the actual
# outcome, BEFORE a `--lib` salvage (which skips the integration-test links entirely) can
# hide a re-regression — the salvage that masked #878 three times (#750, #873/#877).
#
# Run:
#   bash product/test/infra-002/check-workspace-link-smoke.sh
#   bash product/test/infra-002/check-workspace-link-smoke.sh --self-test
#
# Exit codes:
#   0 = full-workspace link completed (invariant holds)
#   1 = link failed — OOM (SIGKILL/signal 9) or any other link/compile error
#   2 = usage / self-test failure
#   3 = self-skipped (cargo unavailable) — non-blocking in environments without a toolchain
#
# Tunable: LINK_SMOKE_TIMEOUT_SECS (default 1200) — hard ceiling for the link-only build.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

# Signatures of a linker OOM (the #878 failure mode). Any of these in the build output
# means the linker was killed by the OOM killer rather than exiting on a normal error.
OOM_REGEX='signal:? 9|SIGKILL|terminated with signal 9|\[Killed\]|Killed\b|out of memory|Cannot allocate memory'

# classify_link_output RC LOGFILE
#   Emits a human verdict and returns: 0 = passed, 1 = failed. On failure, distinguishes
#   an OOM (the bug this guard exists for) from an ordinary compile/link error.
classify_link_output() {
  local rc="$1" log="$2"
  if [ "${rc}" -eq 0 ]; then
    echo "PASS: full-workspace --no-run link completed at configured parallelism (#878 invariant holds)."
    return 0
  fi
  if grep -qiE "${OOM_REGEX}" "${log}" 2>/dev/null; then
    echo "FAIL: linker OOM at the workspace link step (rc=${rc}) — the #878 regression is BACK."
    echo "      Cumulative parallel-link RSS crossed the memory ceiling. Do NOT salvage with"
    echo "      --lib or -j1; re-derive the [profile.dev] debug settings / .cargo jobs cap."
    grep -iE "${OOM_REGEX}" "${log}" 2>/dev/null | head -5 | sed 's/^/      > /'
    return 1
  fi
  echo "FAIL: workspace --no-run link did not complete (rc=${rc}), no OOM signature found —"
  echo "      likely an ordinary compile/link error unrelated to #878. Tail:"
  tail -8 "${log}" 2>/dev/null | sed 's/^/      > /'
  return 1
}

run_link_smoke() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "SKIP: cargo not found — link smoke self-skipped (exit 3)."
    return 3
  fi
  local log rc
  log="$(mktemp -t uni-linksmoke.XXXXXX.log)"
  # NO --jobs flag: inherit .cargo/config.toml's `jobs` cap (the repo's real parallelism).
  echo "[878-link-smoke] cargo test --workspace --no-run at configured parallelism (ceiling ${LINK_SMOKE_TIMEOUT_SECS:-1200}s)"
  # Own session/process group + hard ceiling + file-not-pipe — same orphan-safety contract
  # as the hardened cargo-test convention (infra-002 / #122 / GH#709). setsid -w propagates
  # the child's real exit code (bare setsid returns the fork's 0 -> false green).
  setsid -w timeout "${LINK_SMOKE_TIMEOUT_SECS:-1200}" \
    cargo test --workspace --no-run \
    > "${log}" 2>&1
  rc=$?
  if [ "${rc}" -eq 124 ]; then
    echo "FAIL: link smoke killed at the ${LINK_SMOKE_TIMEOUT_SECS:-1200}s ceiling (rc=124) — investigate a hang or a swap-thrash stall (a near-OOM box thrashes before it kills)."
    rm -f "${log}"
    return 1
  fi
  classify_link_output "${rc}" "${log}"
  local verdict=$?
  rm -f "${log}"
  return "${verdict}"
}

# --self-test proves the OOM-detection logic without provoking a real OOM: feed a captured
# linker-OOM log and a clean log through classify_link_output and assert the verdicts.
# capture VERDICT_OUT (stdout) and VERDICT_RC (return code) of classify_link_output
# without a pipe — `set -o pipefail` would otherwise surface the function's non-zero
# return through a `classify | grep` pipeline and mask grep's real match result.
_classify_capture() {
  VERDICT_OUT="$(classify_link_output "$1" "$2")"; VERDICT_RC=$?
}

self_test() {
  local tmp fails=0
  echo "[self-test] classify_link_output OOM detection"

  tmp="$(mktemp)"
  printf '%s\n' \
    'error: linking with `cc` failed: exit status: 1' \
    'collect2: fatal error: ld terminated with signal 9 [Killed]' > "${tmp}"
  _classify_capture 1 "${tmp}"
  if [ "${VERDICT_RC}" -eq 0 ]; then
    echo "  FAIL: an OOM log (signal 9 [Killed]) was NOT flagged as failure"; fails=1
  elif ! printf '%s' "${VERDICT_OUT}" | grep -q 'linker OOM'; then
    echo "  FAIL: OOM log flagged, but not classified as an OOM"; fails=1
  else
    echo "  ok: OOM log -> FAIL + OOM classification"
  fi
  rm -f "${tmp}"

  tmp="$(mktemp)"
  printf '%s\n' '    Finished `test` profile [unoptimized + debuginfo] target(s) in 42s' > "${tmp}"
  _classify_capture 0 "${tmp}"
  if [ "${VERDICT_RC}" -eq 0 ]; then
    echo "  ok: clean 'Finished' log (rc=0) -> PASS"
  else
    echo "  FAIL: a clean rc=0 link was flagged as failure"; fails=1
  fi
  rm -f "${tmp}"

  tmp="$(mktemp)"
  printf '%s\n' 'error[E0432]: unresolved import `foo::bar`' > "${tmp}"
  _classify_capture 1 "${tmp}"
  if [ "${VERDICT_RC}" -ne 0 ] && printf '%s' "${VERDICT_OUT}" | grep -q 'ordinary compile/link error'; then
    echo "  ok: non-OOM error (rc=1) -> FAIL, classified as ordinary error (not #878)"
  else
    echo "  FAIL: a non-OOM error was misclassified"; fails=1
  fi
  rm -f "${tmp}"

  if [ "${fails}" -eq 0 ]; then echo "[self-test] PASS"; return 0; fi
  echo "[self-test] FAIL"; return 2
}

main() {
  case "${1:-}" in
    --self-test) self_test; exit $? ;;
    "") : ;;
    *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
  esac
  cd "${REPO_ROOT}" || { echo "cannot cd to repo root ${REPO_ROOT}" >&2; exit 2; }
  run_link_smoke
  exit $?
}

main "$@"
