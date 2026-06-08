#!/usr/bin/env bash
#
# infra-002 regression guard for bug #122 (cargo-test orphan cleanup).
#
# Bug recap: the canonical convention `cargo test --workspace 2>&1 | tail -30`
# runs cargo as the head of a bare pipe with no process group and no timeout.
# When the harness interrupts the Bash tool call, only the pipeline leader is
# signaled; cargo/rustc/test-binary children reparent to PID 1 and survive,
# holding target/.cargo-lock and test .db handles -> later runs hang/false-fail.
#
# The hardened form runs in its own session/process group with a hard ceiling and
# writes to a file instead of a live pipe. `setsid -w` is mandatory:
#
#   setsid -w timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace \
#     > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; \
#     rm -f /tmp/uni-test.$$.log; exit $rc
#
# Second defect (GH#709): bare `setsid` (no -w) forks and returns the *fork's*
# status (always 0), so `rc=$?` reads 0 even when cargo test FAILS or is killed at
# the ceiling -> silent false-green gate, inverting the #122 guarantee. `setsid -w`
# waits for the child and propagates its real exit code.
#
# This guard FAILS (exit 1) if any agent / protocol / rule file under .claude/:
#   (a) invokes `cargo test` as the head of a bare pipe without `setsid`, OR
#   (b) invokes `cargo test` via `setsid` WITHOUT the `-w` flag.
# It is a standalone shell check (NOT a `cargo test --workspace` target) so it
# cannot be defeated by the very bug it guards against.
#
# Run:
#   bash product/test/infra-002/check-cargo-test-convention.sh
#   bash product/test/infra-002/check-cargo-test-convention.sh --self-test
#
# Exit codes: 0 = clean, 1 = violation(s) found, 2 = usage/self-test failure.

set -uo pipefail

# Resolve the repo root from this script's location (works from any cwd).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SCAN_DIR="${REPO_ROOT}/.claude"

# Two violation classes are detected (a real `cargo test` invocation, never the
# documentation placeholder `cargo test ...` with a literal ellipsis):
#
#   CLASS 1 — bare pipe (#122): `cargo test` is the head of a pipe
#     (`cargo test ... |`) and the line does NOT contain `setsid`. No process
#     group -> orphaned cargo subtree.
#
#   CLASS 2 — setsid without -w (GH#709): the line uses `setsid` to launch
#     `cargo test` but does NOT pass `-w`. Bare `setsid` returns its fork status
#     (0), swallowing the real exit code -> false-green gate.
#
# `setsid -w` (with the flag) is the only accepted hardened form.

# CLASS 1: cargo test as head of a pipe (`|`, not `||`) on the same line,
# without setsid.
scan_bare_pipe() {
  local target_dir="$1"
  grep -rnE 'cargo test([^|]|\|\|)*\|([^|]|$)' "${target_dir}" 2>/dev/null \
    | grep -v 'setsid' \
    | grep -v 'cargo test \.\.\.'
}

# CLASS 2: a `setsid` that launches `cargo test` on the same line but lacks the
# `-w` flag. Match `setsid` followed by a token that is neither `-w` nor the
# start of another option leading into `cargo test`. The placeholder
# `cargo test ...` is excluded as in CLASS 1.
scan_setsid_no_w() {
  local target_dir="$1"
  grep -rnE 'setsid[[:space:]]+([^[:space:]-]|-[^w])[^|]*cargo test' "${target_dir}" 2>/dev/null \
    | grep -v 'cargo test \.\.\.'
}

# Combined scan used by --self-test against fixtures: emits any violating line.
scan() {
  local target_dir="$1"
  { scan_bare_pipe "${target_dir}"; scan_setsid_no_w "${target_dir}"; } | sort -u
}

check() {
  local bare_pipe setsid_no_w rc=0
  bare_pipe="$(scan_bare_pipe "${SCAN_DIR}")"
  setsid_no_w="$(scan_setsid_no_w "${SCAN_DIR}")"

  if [ -n "${bare_pipe}" ]; then
    echo "FAIL: bare 'cargo test' pipe(s) found without setsid (bug #122 regression):" >&2
    echo "${bare_pipe}" >&2
    echo "" >&2
    rc=1
  fi
  if [ -n "${setsid_no_w}" ]; then
    echo "FAIL: 'setsid' launching cargo test WITHOUT -w (GH#709 false-green regression):" >&2
    echo "${setsid_no_w}" >&2
    echo "" >&2
    rc=1
  fi
  if [ "${rc}" -ne 0 ]; then
    echo "Use the hardened convention from .claude/rules/rust-workspace.md:" >&2
    echo "  setsid -w timeout \"\${CARGO_TEST_TIMEOUT_SECS:-600}\" cargo test --workspace > /tmp/uni-test.\$\$.log 2>&1; rc=\$?; tail -30 /tmp/uni-test.\$\$.log; rm -f /tmp/uni-test.\$\$.log; exit \$rc" >&2
    return 1
  fi
  echo "OK: no bare 'cargo test' pipes and no setsid-without-w under ${SCAN_DIR} (#122 + GH#709 convention intact)."
  return 0
}

# --self-test proves the guard catches BOTH defective forms and passes the
# hardened one, so the check itself cannot silently rot. It uses temp fixtures,
# not the repo. Cases:
#   (a) bare-pipe form (no setsid)            -> MUST be flagged (CLASS 1, #122)
#   (b) setsid WITHOUT -w                     -> MUST be flagged (CLASS 2, GH#709)
#   (c) setsid -w hardened form               -> MUST pass
self_test() {
  local tmp bad nowait good
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN
  bad="${tmp}/bad.md"
  nowait="${tmp}/nowait.md"
  good="${tmp}/good.md"
  printf 'cargo test --workspace 2>&1 | tail -30\n' > "${bad}"
  printf 'setsid timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; rm -f /tmp/uni-test.$$.log; exit $rc\n' > "${nowait}"
  printf 'setsid -w timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; rm -f /tmp/uni-test.$$.log; exit $rc\n' > "${good}"

  # (a) bare-pipe form must be flagged.
  if ! scan "${tmp}" | grep -q 'bad.md'; then
    echo "SELF-TEST FAIL: guard did NOT flag the old bare-pipe form (CLASS 1)." >&2
    return 2
  fi
  # (b) setsid WITHOUT -w must be flagged.
  if ! scan "${tmp}" | grep -q 'nowait.md'; then
    echo "SELF-TEST FAIL: guard did NOT flag the setsid-without-w form (CLASS 2, GH#709)." >&2
    return 2
  fi
  # (c) setsid -w hardened form must pass.
  if scan "${tmp}" | grep -q 'good.md'; then
    echo "SELF-TEST FAIL: guard wrongly flagged the hardened 'setsid -w' form." >&2
    return 2
  fi
  echo "SELF-TEST OK: flags bare-pipe + setsid-without-w forms, passes 'setsid -w' hardened form."
  return 0
}

case "${1:-}" in
  --self-test) self_test ;;
  "")          check ;;
  *)           echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac
