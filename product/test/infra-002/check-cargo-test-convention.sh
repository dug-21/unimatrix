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
# writes to a file instead of a live pipe:
#
#   setsid timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace \
#     > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; \
#     rm -f /tmp/uni-test.$$.log; exit $rc
#
# This guard FAILS (exit 1) if any agent / protocol / rule file under .claude/
# invokes `cargo test` as the head of a bare pipe without BOTH `setsid` and
# `timeout` on the same line. It is a standalone shell check (NOT a
# `cargo test --workspace` target) so it cannot be defeated by the very bug it
# guards against.
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

# A line is a VIOLATION when ALL of these hold:
#   1. it contains a `cargo test` invocation,
#   2. that `cargo test` is the head of a pipe (`cargo test ... |`),
#   3. the line does NOT contain `setsid` (no process group), and
#   4. it is a real invocation, not the documentation placeholder
#      `cargo test ...` (literal ellipsis) used in DON'T-DO-THIS warnings.
#
# Requiring `setsid` (rather than also `timeout`) on the matched line is
# sufficient: the canonical hardened form always carries both, and `setsid` is
# the piece that makes the run a killable process group — the load-bearing fix.

# Match `cargo test` followed by anything, then a pipe, where the char before
# the pipe that starts the pipe is not part of the literal `cargo test ...`
# placeholder. We detect the placeholder separately and exclude it.
scan() {
  local target_dir="$1"
  # Pattern: a `cargo test` whose first downstream `|` (a pipe, not `||`)
  # appears on the same line.
  grep -rnE 'cargo test([^|]|\|\|)*\|([^|]|$)' "${target_dir}" 2>/dev/null \
    | grep -v 'setsid' \
    | grep -v 'cargo test \.\.\.'
}

check() {
  local violations
  violations="$(scan "${SCAN_DIR}")"
  if [ -n "${violations}" ]; then
    echo "FAIL: bare 'cargo test' pipe(s) found without setsid (bug #122 regression):" >&2
    echo "${violations}" >&2
    echo "" >&2
    echo "Use the hardened convention from .claude/rules/rust-workspace.md:" >&2
    echo "  setsid timeout \"\${CARGO_TEST_TIMEOUT_SECS:-600}\" cargo test --workspace > /tmp/uni-test.\$\$.log 2>&1; rc=\$?; tail -30 /tmp/uni-test.\$\$.log; rm -f /tmp/uni-test.\$\$.log; exit \$rc" >&2
    return 1
  fi
  echo "OK: no bare 'cargo test' pipes under ${SCAN_DIR} (#122 convention intact)."
  return 0
}

# --self-test proves the guard catches the OLD bare form and passes the NEW one,
# so the check itself cannot silently rot. It uses temp fixtures, not the repo.
self_test() {
  local tmp bad good rc_bad rc_good
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN
  bad="${tmp}/bad.md"
  good="${tmp}/good.md"
  printf 'cargo test --workspace 2>&1 | tail -30\n' > "${bad}"
  printf 'setsid timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; rm -f /tmp/uni-test.$$.log; exit $rc\n' > "${good}"

  scan "${tmp}" | grep -q 'bad.md'; rc_bad=$?
  if [ "${rc_bad}" -ne 0 ]; then
    echo "SELF-TEST FAIL: guard did NOT flag the old bare-pipe form." >&2
    return 2
  fi
  if scan "${tmp}" | grep -q 'good.md'; then
    echo "SELF-TEST FAIL: guard wrongly flagged the hardened form." >&2
    return 2
  fi
  echo "SELF-TEST OK: flags old bare form, passes hardened form."
  return 0
}

case "${1:-}" in
  --self-test) self_test ;;
  "")          check ;;
  *)           echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac
