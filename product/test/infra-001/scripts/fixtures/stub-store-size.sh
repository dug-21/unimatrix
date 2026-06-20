#!/usr/bin/env bash
# stub-store-size.sh — controllable stand-in for the Gate 7 per-slug store
# sampler (gate7_store_size -> store_size -> busybox `du -s`) used by
# release-gate-logic-test.sh. Touches no Docker volume.
#
# Invoked by gate7_store_size() as:  $SMOKE_STORE_SIZE_CMD "<store-dir>"
# (the store dir arg is accepted but ignored — the stub measures a controllable
# file so the BEFORE/AFTER delta is driven by stub-hook-fire's append.)
#
# Env:
#   STUB_HOOK_STORE_FILE : the file whose byte size is the "store size". The
#                          hook stub appends to it on a fresh attach, so the
#                          Gate 7 AFTER sample exceeds BEFORE => delta>0 (PASS).
#                          A broken attach never appends => delta 0 => Gate 7 RED.

store_file="${STUB_HOOK_STORE_FILE:-}"

if [ -n "$store_file" ] && [ -f "$store_file" ]; then
  # byte count; portable (no busybox du needed in the stub harness).
  wc -c < "$store_file" | tr -d ' '
else
  printf '0'
fi
