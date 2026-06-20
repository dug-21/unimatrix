#!/usr/bin/env bash
# stub-hook-fire.sh — controllable stand-in for the Gate 7 hook fire
# (node … lib/hook-client/index.js <EVENT>) used by release-gate-logic-test.sh.
# Touches no node, no network. Reads the hook event JSON on stdin (drained).
#
# Invoked by fire_hook() as:  HOME="$SANDBOX/home" $SMOKE_HOOK_CMD   (event on stdin)
#
# CRITICAL hermeticity model: the real hook client is fail-open and reads its
# credstore from the HOME-keyed store. This stub mirrors that — it reads a
# credential ONLY from the isolated $HOME/.unimatrix (written by stub-init-bundle
# when the attach was fresh+valid). If no fresh credential exists under the
# isolated HOME, the stub emits NO observe code and does NOT grow the store —
# exactly what happens when isolation works and the attach was broken: the
# poisoned stale cred at the REAL ~/.unimatrix is unreachable (wrong HOME).
#
# Env-driven (best-effort observe-code surface; the store delta is load-bearing):
#   STUB_HOOK_OBSERVE_CODE : HTTP code echoed on stdout when a cred is present
#                            (default "204"). Set to 500/404/etc. to drive the
#                            observe-non-204 row.
#   STUB_HOOK_STORE_FILE   : path the harness samples for the store-grow delta.
#                            When a fresh cred is present (a real attach this run)
#                            the stub appends a byte to this file => delta>0.
#
# The store growth is gated on a FRESH credential under the ISOLATED HOME, so a
# residue-fed (non-isolated) run is what the negative control flips to RED.

cat >/dev/null 2>&1 || true   # drain stdin (the hook event), like the real client

observe_code="${STUB_HOOK_OBSERVE_CODE:-204}"
store_file="${STUB_HOOK_STORE_FILE:-}"

cred="${HOME}/.unimatrix/stub-hash/remote.json"
if [ -f "$cred" ]; then
  # A fresh attach landed under the ISOLATED HOME this run => observe succeeds
  # and the per-slug store grows by the NEW write.
  [ -n "$store_file" ] && printf 'x' >> "$store_file" 2>/dev/null
  printf '%s' "$observe_code"
fi
# No fresh isolated credential => no observe, no growth (fail-open: exit 0).
exit 0
