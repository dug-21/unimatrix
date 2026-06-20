#!/usr/bin/env bash
# stub-init-bundle.sh — controllable stand-in for the Gate 6 host consume
# (node … init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj") used by
# release-gate-logic-test.sh. Touches no node, no network, no real attach.
#
# Invoked by consume_bundle() as:  $SMOKE_INIT_CMD "<blob>" "<project-dir>"
# with HOME="$SANDBOX/home" set on the CHILD by the harness (process-boundary
# isolation — the stub reads $HOME to prove the credstore lands under the
# isolated sandbox, never the runner's real ~/.unimatrix).
#
# Env-driven:
#   STUB_INIT_RC        : integer exit code                       (default 0)
#   STUB_INIT_WRITE_CRED: 1 => write a fresh credstore under $HOME (default 1).
#                         0 => no-op (broken/no-fresh-credential attach — the
#                         negative-control "break" condition).
#
# A fresh credential is written to $HOME/.unimatrix/stub-hash/remote.json so the
# positive twin can prove the FRESH attach (under the isolated HOME), and the
# negative control (STUB_INIT_WRITE_CRED=0 + poisoned REAL ~/.unimatrix) proves
# the stale cred is unreachable because HOME is isolated.

rc="${STUB_INIT_RC:-0}"
write_cred="${STUB_INIT_WRITE_CRED:-1}"

blob="${1:-}"
projdir="${2:-}"

# Validate the blob shape the way the real init would reject a malformed bundle.
case "$blob" in
  unimatrix-bundle:*) : ;;
  *) printf 'stub-init: invalid bundle blob\n' >&2; exit 1 ;;
esac

[ -n "$projdir" ] && mkdir -p "$projdir" 2>/dev/null

if [ "$write_cred" = "1" ] && [ "$rc" = "0" ]; then
  cred_dir="${HOME}/.unimatrix/stub-hash"
  mkdir -p "$cred_dir" 2>/dev/null
  printf '{"observe_url":"https://localhost:18443/v1/arch-research/observe","token":"fresh-stub-token","fingerprint":"sha256:fresh"}\n' \
    > "$cred_dir/remote.json"
fi

exit "$rc"
