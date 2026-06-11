#!/usr/bin/env sh
# dogfood-switchover.sh — Repoint a target settings.json's hooks via the
# INSTALLED client's lib/merge-settings.js (ADR-003).
#
#   promote  -> node-client commandSource (object arm) ->
#               `node <client>/lib/hook-client/index.js <EVENT>`
#   rollback -> Rust binary path STRING -> normalizeCommandSource legacy arm ->
#               `LD_LIBRARY_PATH=<binDir> <binary> hook <EVENT>`
#   --dry-run -> forwards dryRun:true to mergeSettings, writes nothing.
#
# Repoints through the shipped, frozen mergeSettings so the merge is idempotent
# and ownership-aware (isUnimatrixHook): re-running produces no duplicate/stale
# hooks, foreign hooks are preserved, and the PreToolUse matcher is narrowed
# from "*" to PRETOOLUSE_CYCLE_MATCHER (vnc-027 semantics).
#
# Stale-uni-hook prune (vnc-031 ADR-003). The cross-matcher migration now lives
# INSIDE the shipped mergeSettings (Step 3c): it holds the kept entry by object
# identity and prunes every other uni-owned hook — including this repo's legacy
# "*" PreToolUse Rust hook (#4930), .bak/old-client-dir hooks, and emptied matcher
# groups — for BOTH the object (promote) and string (rollback) arms. Foreign hooks
# are never pruned. promote/rollback therefore collapse to a plain
# mergeSettings(..., {dryRun}) that OWNS its own write (Step 4), matching
# initRemote's call shape — no bespoke prune, no quote-aware tokenizer, no
# targetToken reconstruction (the script-level workaround is retired, #4938).
#
# This is TOOLING: it MAY be loud and exit non-zero on failure. The hook
# COMMANDS it writes are themselves fail-open (C-7); the script never starts /
# stops / probes a daemon (ADR-004). It does NOT modify any packages/unimatrix
# lib/** surface or package.json (C-8). It writes ONLY the file passed via
# --settings (defaulting to <repo>/.claude/settings.json only when the human
# runs it explicitly); the harness always passes a scratch --settings.
#
# Usage:
#   scripts/dogfood-switchover.sh <mode> [--settings <path>] [--client <dir>] [--dry-run]
#     <mode>             promote | rollback   (required, positional)
#     --settings <path>  target settings file (DEFAULT <repo>/.claude/settings.json)
#     --client <dir>     installed client dir (DEFAULT ${HOME}/.unimatrix/dogfood-client)
#     --dry-run          forward dryRun:true to mergeSettings (no write)
#
# Exit codes:
#   0  merge applied (or dry-run computed); prints mergeSettings actions JSON
#   2  bad/absent mode or unknown argument
#   5  installed client incomplete (run dogfood-install.sh first) — R-05
#   7  node threw (e.g. malformed settings JSON) / mergeSettings failed
set -eu

die() {
  # die <message> <code>
  echo "dogfood-switchover: $1" >&2
  exit "${2:-1}"
}

# expand_home <path>: expand a leading ~ and ${HOME}/$HOME so a value can be
# passed to node as an absolute path. Mirrors dogfood-install.sh.
expand_home() {
  _p="$1"
  case "$_p" in
    "~") _p="$HOME" ;;
    "~/"*) _p="$HOME/${_p#~/}" ;;
  esac
  printf '%s' "$_p" | sed "s|\${HOME}|$HOME|g; s|\$HOME|$HOME|g"
}

main() {
  [ $# -ge 1 ] || die "usage: dogfood-switchover.sh <promote|rollback> [--settings <p>] [--client <d>] [--dry-run]" 2
  MODE="$1"
  shift
  case "$MODE" in
    promote | rollback) : ;;
    *) die "unknown mode: $MODE (expected promote|rollback)" 2 ;;
  esac

  REPO=$(git rev-parse --show-toplevel 2>/dev/null) \
    || die "not inside a git repository" 1

  SETTINGS="$REPO/.claude/settings.json"
  CLIENT="${HOME}/.unimatrix/dogfood-client"
  DRYRUN=false

  while [ $# -gt 0 ]; do
    case "$1" in
      --settings)
        [ $# -ge 2 ] || die "--settings requires a value" 2
        SETTINGS="$2"
        shift 2
        ;;
      --settings=*)
        SETTINGS="${1#--settings=}"
        shift
        ;;
      --client)
        [ $# -ge 2 ] || die "--client requires a value" 2
        CLIENT="$2"
        shift 2
        ;;
      --client=*)
        CLIENT="${1#--client=}"
        shift
        ;;
      --dry-run)
        DRYRUN=true
        shift
        ;;
      *)
        die "unknown arg: $1" 2
        ;;
    esac
  done

  SETTINGS=$(expand_home "$SETTINGS")
  CLIENT=$(expand_home "$CLIENT")

  # R-05: completeness BEFORE require — loud, actionable, never an opaque
  # require stacktrace. Check the merge engine, the entrypoint (promote emits
  # commands pointing at it), and config.js (run-time resolution).
  MERGE_JS="$CLIENT/lib/merge-settings.js"
  [ -d "$CLIENT" ] \
    || die "client dir not found: $CLIENT — run dogfood-install.sh first" 5
  [ -f "$MERGE_JS" ] \
    || die "installed merge-settings.js missing: $MERGE_JS — re-run dogfood-install.sh first" 5
  [ -f "$CLIENT/lib/hook-client/index.js" ] \
    || die "installed entrypoint missing: $CLIENT/lib/hook-client/index.js — re-run dogfood-install.sh first" 5
  [ -f "$CLIENT/lib/hook-client/config.js" ] \
    || die "installed config.js missing: $CLIENT/lib/hook-client/config.js — re-run dogfood-install.sh first" 5

  require_cmd node

  # Parameters reach node via env only — never interpolated into a JS string
  # literal or an unquoted shell command (no path-injection into merge logic).
  if [ "$MODE" = "promote" ]; then
    MERGE_JS="$MERGE_JS" SETTINGS="$SETTINGS" CLIENT="$CLIENT" DRYRUN="$DRYRUN" \
      run_promote || die "mergeSettings failed (mode=promote)" 7
  else
    RUST_BINARY="$REPO/target/release/unimatrix"
    MERGE_JS="$MERGE_JS" SETTINGS="$SETTINGS" DRYRUN="$DRYRUN" RUST_BINARY="$RUST_BINARY" \
      run_rollback || die "mergeSettings failed (mode=rollback)" 7
  fi

  exit 0
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1" 1
}

# run_promote: object commandSource arm. Emits `node <client>/lib/hook-client/
# index.js <EVENT>`; isUnimatrixHook updates existing Rust commands under the
# cycle matcher in place (no dupes), foreign hooks untouched, PreToolUse narrowed
# to PRETOOLUSE_CYCLE_MATCHER. mergeSettings Step 3c prunes the stale "*" Rust uni
# hook (and .bak/old-client-dir hooks) by object identity, then owns the write —
# this is a thin wrapper matching initRemote's call shape (no bespoke prune).
run_promote() {
  node - <<NODE
const path = require("path");
const { mergeSettings, buildHookClientCommand, HOOK_EVENTS } = require(process.env.MERGE_JS);
const settingsPath = process.env.SETTINGS;
const clientDir = process.env.CLIENT;
const dryRun = process.env.DRYRUN === "true";
const entry = path.join(clientDir, "lib/hook-client/index.js");
const result = mergeSettings(settingsPath, {
  events: HOOK_EVENTS,
  commandForEvent: (event) => buildHookClientCommand(entry, event),
}, { dryRun });
process.stdout.write(JSON.stringify({ actions: result.actions, dryRun }));
NODE
}

# run_rollback: STRING commandSource (legacy arm). The shipped
# normalizeCommandSource owns the exact `LD_LIBRARY_PATH=<binDir> <binary> hook
# <EVENT>` form over HOOK_EVENTS, so promote<->rollback round-trips cannot drift
# (no bespoke revert string). mergeSettings Step 3c keeps the just-written legacy
# command by object identity (no dirname heuristic) and prunes any stale
# node-client uni hook, then owns the write. Idempotent; foreign preserved.
run_rollback() {
  node - <<NODE
const { mergeSettings } = require(process.env.MERGE_JS);
const settingsPath = process.env.SETTINGS;
const dryRun = process.env.DRYRUN === "true";
const rustBinary = process.env.RUST_BINARY;
const result = mergeSettings(settingsPath, rustBinary, { dryRun });
process.stdout.write(JSON.stringify({ actions: result.actions, dryRun }));
NODE
}

main "$@"
