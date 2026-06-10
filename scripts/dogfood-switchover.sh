#!/usr/bin/env sh
# dogfood-switchover.sh — Repoint a target settings.json's hooks via the
# INSTALLED client's lib/merge-settings.js (ADR-003).
#
#   promote  -> node-client commandSource (object arm) ->
#               `node <client>/lib/hook-client/index.js <EVENT>`
#   rollback -> Rust binary path STRING -> normalizeCommandSource legacy arm ->
#               `LD_LIBRARY_PATH=<binDir> <binary> hook <EVENT>`
#   --dry-run -> computes actions + planned prunes, writes nothing.
#
# Repoints through the shipped, frozen mergeSettings so the merge is idempotent
# and ownership-aware (isUnimatrixHook): re-running produces no duplicate/stale
# hooks, foreign hooks are preserved, and the PreToolUse matcher is narrowed
# from "*" to PRETOOLUSE_CYCLE_MATCHER (vnc-027 semantics).
#
# Stale-uni-hook prune (Stage 3b). mergeSettings keys every op on
# EVENT_MATCHERS[event] (PreToolUse => PRETOOLUSE_CYCLE_MATCHER, never "*"), so a
# uni-owned hook living under a DIFFERENT matcher group — e.g. this repo's legacy
# "*" PreToolUse Rust hook (#4930) — is invisible to it and survives un-repointed.
# After mergeSettings (always called {dryRun:true} = pure compute of {actions,
# content}), the one-liner prunes every uni-owned hook (per shipped
# isUnimatrixHook) whose command does NOT reference the mode's targetToken as a
# whole shell token, drops emptied matcher groups + event keys, then owns the
# SINGLE writeFile — gated by this script's own --dry-run. Foreign hooks are
# never pruned. targetToken: promote => <client>/lib/hook-client/index.js;
# rollback => <repo>/target/release/unimatrix.
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
#   7  node one-liner threw (e.g. malformed settings JSON) / mergeSettings failed
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

# Shared node prune + write fragment, sourced into both one-liners. mergeSettings
# is ALWAYS called {dryRun:true} (pure compute of {actions, content}); the prune
# mutates `content`; this fragment owns the single writeFile, gated by the
# script's own DRYRUN. commandReferencesTarget is a whole-shell-token match
# (quote-stripped, whitespace-split token-equality), NOT naive includes — so a
# stale hook at .../index.js.bak or .../dogfood-client-OLD/.../index.js is a
# DIFFERENT token and is correctly pruned, while the genuine post-switchover
# command is kept. Rollback (MODE=rollback) also accepts a dirname-level token
# match for the LD_LIBRARY_PATH=<dirname(rustBinary)> prefix.
PRUNE_FRAGMENT='
const fs = require("fs");
const path = require("path");
// Split a command into whole shell tokens, honoring "..." / '"'"'...'"'"' quoting so
// a quoted path containing whitespace (buildHookClientCommand quotes iff the path
// has whitespace) is ONE token with the surrounding quotes stripped — not split at
// the internal space. Unquoted runs split on ASCII whitespace.
function shellTokens(cmd) {
  const out = [];
  const re = /"([^"]*)"|'"'"'([^'"'"']*)'"'"'|(\S+)/g;
  let mm;
  while ((mm = re.exec(String(cmd))) !== null) {
    if (mm[1] !== undefined) out.push(mm[1]);
    else if (mm[2] !== undefined) out.push(mm[2]);
    else out.push(mm[3]);
  }
  return out;
}
function commandReferencesTarget(cmd, targetToken, allowDirname) {
  const targetDir = allowDirname ? path.dirname(targetToken) : null;
  for (const tok of shellTokens(cmd)) {
    const eq = tok.indexOf("=");
    if (eq >= 0) {
      // env-assignment token (e.g. LD_LIBRARY_PATH=<dir>): test the value side too.
      const val = tok.slice(eq + 1);
      if (val === targetToken) return true;
      if (targetDir !== null && val === targetDir) return true;
    }
    if (tok === targetToken) return true;
    if (targetDir !== null && tok === targetDir) return true;
  }
  return false;
}
function pruneStaleUniHooks(content, targetToken, isUnimatrixHook, allowDirname) {
  const prunes = [];
  if (!content || !content.hooks) return prunes;
  for (const event of Object.keys(content.hooks)) {
    const groups = content.hooks[event];
    if (!Array.isArray(groups)) continue;
    for (const group of groups) {
      const entries = group && group.hooks;
      if (!Array.isArray(entries)) continue;
      group.hooks = entries.filter((entry) => {
        const cmd = (entry && entry.command) || "";
        if (isUnimatrixHook(entry) && !commandReferencesTarget(cmd, targetToken, allowDirname)) {
          prunes.push({ event: event, matcher: group.matcher, command: cmd });
          return false;
        }
        return true;
      });
    }
    content.hooks[event] = groups.filter(
      (g) => g && Array.isArray(g.hooks) && g.hooks.length > 0
    );
    if (content.hooks[event].length === 0) delete content.hooks[event];
  }
  return prunes;
}
function emitAndWrite(settingsPath, result, prunes, dryRun) {
  process.stdout.write(JSON.stringify({
    actions: result.actions,
    prunes: prunes,
    pruneCount: prunes.length,
    dryRun: dryRun,
  }));
  if (!dryRun) {
    fs.mkdirSync(path.dirname(settingsPath), { recursive: true });
    fs.writeFileSync(settingsPath, JSON.stringify(result.content, null, 2) + "\n", "utf8");
  }
}
'

# run_promote: object commandSource arm. Emits `node <client>/lib/hook-client/
# index.js <EVENT>`; isUnimatrixHook updates existing Rust commands under the
# cycle matcher in place (no dupes), foreign hooks untouched, PreToolUse narrowed
# to PRETOOLUSE_CYCLE_MATCHER. The stale "*" Rust uni hook left by mergeSettings
# is pruned (it is a uni hook NOT referencing the installed entrypoint token).
run_promote() {
  node - <<NODE
$PRUNE_FRAGMENT
const { mergeSettings, buildHookClientCommand, HOOK_EVENTS, isUnimatrixHook } = require(process.env.MERGE_JS);
const settingsPath = process.env.SETTINGS;
const clientDir = process.env.CLIENT;
const dryRun = process.env.DRYRUN === "true";
const entry = path.join(clientDir, "lib/hook-client/index.js");
const targetToken = entry;
const result = mergeSettings(settingsPath, {
  events: HOOK_EVENTS,
  commandForEvent: (event) => buildHookClientCommand(entry, event),
}, { dryRun: true });
const prunes = pruneStaleUniHooks(result.content, targetToken, isUnimatrixHook, false);
emitAndWrite(settingsPath, result, prunes, dryRun);
NODE
}

# run_rollback: STRING commandSource (legacy arm). The shipped
# normalizeCommandSource owns the exact `LD_LIBRARY_PATH=<binDir> <binary> hook
# <EVENT>` form over HOOK_EVENTS, so promote<->rollback round-trips cannot drift
# (no bespoke revert string). A stale node-client uni hook is pruned (it does NOT
# reference the Rust binary token); the legacy command DOES reference rustBinary
# (bare arg AND LD_LIBRARY_PATH dir prefix) -> kept. Idempotent; foreign preserved.
run_rollback() {
  node - <<NODE
$PRUNE_FRAGMENT
const { mergeSettings, isUnimatrixHook } = require(process.env.MERGE_JS);
const settingsPath = process.env.SETTINGS;
const dryRun = process.env.DRYRUN === "true";
const rustBinary = process.env.RUST_BINARY;
const targetToken = rustBinary;
const result = mergeSettings(settingsPath, rustBinary, { dryRun: true });
const prunes = pruneStaleUniHooks(result.content, targetToken, isUnimatrixHook, true);
emitAndWrite(settingsPath, result, prunes, dryRun);
NODE
}

main "$@"
