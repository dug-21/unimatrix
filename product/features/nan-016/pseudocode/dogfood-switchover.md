# Component 2 — `scripts/dogfood-switchover.sh` (Switchover: promote / rollback / --dry-run)

## Purpose

Repoint a target `settings.json`'s hooks via the **installed** copy's `lib/merge-settings.js`:
- **promote** = node-client `commandSource` (object arm) → `node <client>/lib/hook-client/index.js <EVENT>`.
- **rollback** = Rust binary path **string** → `normalizeCommandSource` legacy arm →
  `LD_LIBRARY_PATH=<binDir> <binary> hook <EVENT>`.
- **stale-uni-hook prune** (Stage 3b amendment) = after `mergeSettings`, remove any uni-owned
  hook (per shipped `isUnimatrixHook`) whose command does NOT reference the mode's `targetToken`,
  so the legacy `"*"` PreToolUse Rust uni hook that `mergeSettings` leaves behind (#4930) does
  not survive. CLEAN soak requirement.
- `--dry-run` reports planned repoint actions AND planned prunes; writes nothing.

Delivered, NOT executed against this repo's live settings by nan-016. The harness always passes
explicit scratch `--settings` / `--client`. Covers FR-5, FR-6, FR-7, FR-8(emitted form), FR-9;
addresses SR-05, SR-06, SR-08 / R-05, R-06, R-09, R-10. ARCH-OQ-3: POSIX `sh` wrapping ONE
`node` one-liner that requires the installed merge API.

## Interface / invocation

```
scripts/dogfood-switchover.sh <mode> [--settings <path>] [--client <dir>] [--dry-run]
  <mode>            promote | rollback   (required, positional)
  --settings <path> target settings file. DEFAULT: <repo>/.claude/settings.json
  --client <dir>    installed client dir. DEFAULT: ${HOME}/.unimatrix/dogfood-client
                    (used by promote to build commands AND to require merge-settings.js;
                     rollback also requires merge-settings.js from --client)
  --dry-run         forward dryRun:true to mergeSettings (compute actions, do not write)
  exit 0   = merge applied (or dry-run computed); prints mergeSettings actions
  exit !=0 = loud failure (missing install, bad mode, require throw, unsafe args)
```

Note: rollback still needs `--client` to locate the installed `merge-settings.js` (the merge
engine), even though the *emitted* command is the Rust binary path. The Rust binary path itself
is derived from `<repo>/target/release/unimatrix` (resolved via `git rev-parse`).

## Initialization sequence

```
main(argv):
  set -eu
  MODE <- argv[0]   (must be "promote" or "rollback"; else die "usage" 2)
  SETTINGS <- default <repo>/.claude/settings.json
  CLIENT   <- default ${HOME}/.unimatrix/dogfood-client
  DRYRUN   <- false
  parse remaining argv:
    --settings <p> -> SETTINGS <- p
    --client <d>   -> CLIENT   <- d
    --dry-run      -> DRYRUN   <- true
    unknown        -> die 2
  REPO   <- `git rev-parse --show-toplevel`
  SETTINGS <- expand+absolutize(SETTINGS)
  CLIENT   <- expand+absolutize(CLIENT)

  # R-05: completeness BEFORE require — loud, actionable, not an opaque require stacktrace.
  MERGE_JS <- "$CLIENT/lib/merge-settings.js"
  [ -d "$CLIENT" ]    || die "client dir not found: $CLIENT — run dogfood-install.sh first" 5
  [ -f "$MERGE_JS" ]  || die "installed merge-settings.js missing: $MERGE_JS — re-run install" 5
  [ -f "$CLIENT/lib/hook-client/index.js" ] || die "installed entrypoint missing — re-run install" 5
  [ -f "$CLIENT/lib/hook-client/config.js" ] || die "installed config.js missing — re-run install" 5

  # We intentionally do NOT add a tmpdir guard here (that guard lives in the harness, R-08):
  # this script is general-purpose and the human runs it against live settings on the real flip.
  # The harness is responsible for only ever passing scratch --settings.
```

## How promote repoints (ADR-003 / FR-6)

The shell invokes node with `MERGE_JS`, `SETTINGS`, `CLIENT`, `DRYRUN`, `MODE` passed as
**argv/env** (never interpolated into a JS string literal — avoids injection; security note).

```
node-one-liner (mode=promote):
  const { mergeSettings, buildHookClientCommand, HOOK_EVENTS, isUnimatrixHook }
      = require(process.env.MERGE_JS);
  const path = require("path");
  const settingsPath = process.env.SETTINGS;
  const clientDir    = process.env.CLIENT;
  const dryRun       = process.env.DRYRUN === "true";
  const entry = path.join(clientDir, "lib/hook-client/index.js");
  const targetToken = entry;   // absolute installed entrypoint — the post-switchover target
  const result = mergeSettings(settingsPath, {
      events: HOOK_EVENTS,
      commandForEvent: (event) => buildHookClientCommand(entry, event)
  }, { dryRun });
  // mergeSettings filters SubagentStop unless settings.local.json (sibling of settingsPath)
  //   opts in -> on a fresh scratch root, 8 events registered (Component 3 asserts 8).
  // isUnimatrixHook recognizes the existing Rust commands UNDER PRETOOLUSE_CYCLE_MATCHER ->
  //   updated IN PLACE (no dupes); foreign hooks untouched. But mergeSettings keys every op on
  //   EVENT_MATCHERS[event]; a legacy uni hook under a DIFFERENT matcher (the live "*"
  //   PreToolUse Rust hook) is invisible to it and survives un-repointed (#4930). The prune
  //   below removes those stale uni hooks. PreToolUse narrowed "*" -> PRETOOLUSE_CYCLE_MATCHER.
  const prunes = pruneStaleUniHooks(result.content, targetToken, isUnimatrixHook); // see below
  emitActions(result.actions, prunes, dryRun);   // actions JSON + planned/applied prunes
  if (!dryRun) writeFile(settingsPath, result.content);   // see "Stale-uni-hook prune" note
```

Emitted command form (verified): `node <clientDir>/lib/hook-client/index.js <EVENT>`; path
quoted iff it contains whitespace. `~/.unimatrix/dogfood-client` has no whitespace → bare,
matched by the `UNIMATRIX_PATTERNS` node-client arm → idempotent re-point.

> **Write ownership (changed by this amendment).** Because the prune mutates `result.content`
> AFTER `mergeSettings` returns, the one-liner — not `mergeSettings` — now owns the final write
> (and must NOT also let `mergeSettings` write). Two equivalent shapes; pin ONE:
> (a) call `mergeSettings(..., { dryRun: true })` so it computes `content`+`actions` and writes
>     nothing, prune `content`, then `writeFile(settingsPath, JSON.stringify(content, null, 2))`
>     when the real (non-dry-run) mode is requested; or
> (b) if the shipped `mergeSettings` exposes no "compute-without-write" beyond `dryRun`, run it
>     with the real `dryRun` flag, then prune + re-write `content` unconditionally on real runs.
> **Pinned: shape (a)** — single source of file truth, no double-write, no read-back race. The
> script's own `dryRun` gates the final `writeFile`; `mergeSettings` is always invoked
> `{dryRun:true}` and treated as a pure compute of `{actions, content}`.

## How rollback repoints (ADR-003 / FR-7)

```
node-one-liner (mode=rollback):
  const { mergeSettings, isUnimatrixHook } = require(process.env.MERGE_JS);
  const settingsPath = process.env.SETTINGS;
  const dryRun       = process.env.DRYRUN === "true";
  const rustBinary   = process.env.RUST_BINARY;     // <repo>/target/release/unimatrix (a STRING)
  const targetToken  = rustBinary;   // the Rust binary path — the post-rollback target
  // STRING commandSource -> normalizeCommandSource legacy arm ->
  //   "LD_LIBRARY_PATH=<dirname(rustBinary)> <rustBinary> hook <EVENT>" over HOOK_EVENTS.
  const result = mergeSettings(settingsPath, rustBinary, { dryRun: true });  // pure compute
  // Mirror image of promote's hazard: a stale node-client uni hook (e.g. left under a matcher
  //   mergeSettings does not key on) would survive un-reverted. Prune drops any uni hook whose
  //   command does NOT contain the Rust binary path. The legacy command DOES contain rustBinary
  //   (both as LD_LIBRARY_PATH dir prefix and the binary arg) -> kept.
  const prunes = pruneStaleUniHooks(result.content, targetToken, isUnimatrixHook);
  emitActions(result.actions, prunes, dryRun);
  if (!dryRun) writeFile(settingsPath, result.content);
```

Shell sets `RUST_BINARY="$REPO/target/release/unimatrix"`. No bespoke revert string — the
shipped legacy arm owns the exact form, so promote↔rollback round-trips cannot drift (R-06).
Rollback is idempotent (re-running yields the same legacy commands; `isUnimatrixHook` re-owns;
the prune removes nothing on a second pass — every uni hook already references `rustBinary`).

## Stale-uni-hook prune (post-`mergeSettings`, both modes) — Stage 3b amendment

**Why.** `mergeSettings` keys every operation on `EVENT_MATCHERS[event]` (PreToolUse ⇒
`PRETOOLUSE_CYCLE_MATCHER`, never `"*"`). A uni hook living under a *different* matcher group —
this repo's REAL settings carry a legacy `"*"` PreToolUse Rust uni hook — is a foreign-matcher
group from `mergeSettings`' view and is left untouched. The harness empirically proved promote
then leaves that stale Rust hook un-pruned: only 8 of 9 uni hooks point at the installed client
(#4930). For a CLEAN dogfood soak the switchover must prune stale uni hooks itself. This reuses
the **shipped** `isUnimatrixHook` already required from the installed `lib/merge-settings.js`
(C-8; no new lib code, no new external dep). It follows the canonical opt-out prune shape from
#4826: scope by `isUnimatrixHook`, then drop emptied matcher groups and emptied event keys.

`targetToken` (the path the surviving uni hooks MUST reference):
- **promote**  → `path.join(clientDir, "lib/hook-client/index.js")` (absolute installed entrypoint)
- **rollback** → `RUST_BINARY` = `<repo>/target/release/unimatrix` (the Rust binary path)

```
pruneStaleUniHooks(content, targetToken, isUnimatrixHook) -> prunes[]   // prunes = report only
  prunes <- []                                    // {event, matcher, command} per removed entry
  if (!content || !content.hooks) return prunes   // nothing to walk
  for (const event of Object.keys(content.hooks)):      // EVERY event, not just HOOK_EVENTS
      const groups = content.hooks[event];              // array of matcher groups
      if (!Array.isArray(groups)) continue;
      for (const group of groups):                      // EVERY matcher group in this event
          const entries = group.hooks;                  // array of hook entries
          if (!Array.isArray(entries)) continue;
          group.hooks = entries.filter(entry => {
              const cmd = (entry && entry.command) || "";
              const isUni = isUnimatrixHook(entry);      // SHIPPED predicate, ownership of fact
              const pointsAtTarget = commandReferencesTarget(cmd, targetToken);
              if (isUni && !pointsAtTarget):             // STALE uni hook -> REMOVE
                  prunes.push({ event, matcher: group.matcher, command: cmd });
                  return false;
              return true;                               // KEEP: foreign, OR uni already on target
          });
      // drop matcher groups whose hook list became empty
      content.hooks[event] = groups.filter(g => Array.isArray(g.hooks) && g.hooks.length > 0);
      // drop the event key whose matcher list became empty
      if (content.hooks[event].length === 0) delete content.hooks[event];
  return prunes
```

**`commandReferencesTarget(cmd, targetToken)` — substring match, but anchored to avoid false
keep/prune.** A naive `cmd.includes(targetToken)` can both (a) *false-keep* a stale hook if its
old command happens to contain `targetToken` as a substring of a longer path
(`.../dogfood-client-OLD/lib/hook-client/index.js` contains nothing of the new dir, but
`.../dogfood-client/lib/hook-client/index.js.bak` *contains* the entrypoint as a prefix), and
(b) is fragile to quoting (`buildHookClientCommand` wraps the path in double quotes iff it has
whitespace). Pin the match as: **`targetToken` appears in `cmd` as a whole shell token** —
i.e. bounded on each side by a shell word boundary (start/end of string, ASCII whitespace, or a
surrounding `"`/`'` quote pair that `buildHookClientCommand` itself may have added). Concretely:
strip one optional matching pair of surrounding quotes from each whitespace-split token of `cmd`,
and test token-equality against `targetToken` (promote) or token-equality / `dirname(token)`
equality against `targetToken` (rollback, where `rustBinary` appears both as the bare arg and
inside `LD_LIBRARY_PATH=<dirname>`). This makes `.../index.js.bak` and `.../index.js2` NOT match
(different token), and `.../OLD/.../index.js` NOT match (different token) — so stale hooks under
any path variant are correctly PRUNED, and the genuine post-switchover command is correctly KEPT.
No `path.normalize` games and no regex over attacker-influenced strings beyond token splitting.

**Idempotence.** On a second promote (or rollback) every uni hook already references
`targetToken` as a whole token ⇒ `pointsAtTarget` is true for all of them ⇒ `prunes` is empty,
no group/event is dropped, `content` is byte-stable. The prune is purely subtractive of *stale*
uni hooks; it never touches a uni hook already on target and never touches a foreign hook.

**`--dry-run`.** `pruneStaleUniHooks` runs on the computed `content` exactly as in a real run
(it is a pure in-memory transform), so the returned `prunes[]` is the *planned* prune set.
`emitActions` prints it alongside the `mergeSettings` dry-run actions — count plus, per entry,
its `{event, matcher, command}` — and the one-liner writes nothing (`if (!dryRun) writeFile`).
A real run prints the same report (now describing applied prunes) and then writes.

```
emitActions(actions, prunes, dryRun):
  // single stdout payload so the harness/operator parses one JSON blob
  process.stdout.write(JSON.stringify({
      actions,                                  // mergeSettings repoint actions
      prunes,                                   // stale-uni-hook removals (planned iff dryRun)
      pruneCount: prunes.length,
      dryRun
  }));
```

**Net postconditions (stated explicitly).**
- *post-promote*: EVERY uni-owned hook (across all events/matchers) references the installed
  entrypoint `<clientDir>/lib/hook-client/index.js`; NO stale `"*"` PreToolUse Rust uni hook
  survives (it is pruned because it is a uni hook NOT referencing the entrypoint token); ALL
  foreign hooks preserved untouched; no duplicates (mergeSettings de-dupes the cycle-matcher
  group, the prune removes the orphaned legacy group).
- *post-rollback*: EVERY uni-owned hook is the Rust legacy form referencing
  `<repo>/target/release/unimatrix`; NO stale node-client uni hook survives; ALL foreign hooks
  preserved.

## No daemon lifecycle (ADR-004 / FR-9)

The script never starts/stops/probes the daemon. The emitted node-client command fail-opens if
the socket is absent (C-7). Switchover introduces no host-breaking hook path.

## State machine / main flow

```
main:
  init + parse + completeness-check        (above)
  if MODE == promote:  RUST_BINARY unset; run promote one-liner
  if MODE == rollback: RUST_BINARY <- "$REPO/target/release/unimatrix"; run rollback one-liner
  NODE_RC <- exit of node
  [ "$NODE_RC" -eq 0 ] || die "mergeSettings failed (mode=$MODE)" 7
  # On dry-run: actions printed, no file written (mergeSettings honors dryRun).
  exit 0
```

## Data flow

- IN: `--settings` path (scratch in tests; live on the human flip), `--client` installed dir,
  `--dry-run`, mode.
- OUT (promote): `SETTINGS` rewritten so every Unimatrix-owned hook =
  `node <client>/lib/hook-client/index.js <EVENT>`, PreToolUse matcher = `PRETOOLUSE_CYCLE_MATCHER`,
  stale uni hooks (incl. the legacy `"*"` Rust hook) pruned, foreign hooks preserved, no
  duplicates, 8 events (no opt-in) / 9 (opt-in). stdout = `{actions, prunes, pruneCount, dryRun}` JSON.
- OUT (rollback): `SETTINGS` rewritten to the Rust legacy command form over HOOK_EVENTS; stale
  node-client uni hooks pruned; foreign preserved. stdout = same JSON envelope.
- Side effects: writes only `SETTINGS` (the one-liner owns the write; skipped on `--dry-run`);
  requires JS only from `--client`. `mergeSettings` is always called `{dryRun:true}` (pure
  compute); the script's own `--dry-run` gates the final `writeFile`.

## Error handling

| Condition | Behavior |
|-----------|----------|
| bad/absent mode | usage die, exit 2 |
| `--client` dir / `merge-settings.js` / entrypoint / config.js missing | exit 5, actionable "run dogfood-install.sh first" (R-05) |
| node one-liner throws (e.g. malformed settings JSON) | non-zero from node → die exit 7 |
| `--dry-run` | actions computed + printed; no write |

Loud tooling errors (NOT fail-open) — this is operator tooling, distinct from the emitted hook
command which IS fail-open (C-7).

## Security

- `--settings`/`--client` pass to node as env/argv, never spliced into a JS string literal or an
  unquoted shell command → no path-injection into the merge logic.
- `buildHookClientCommand` does the path quoting for the emitted hook command (whitespace →
  quoted); the script does not hand-build the command string.
- No tmpdir guard here by design (the human uses live `--settings` on the real flip); the harness
  enforces scratch-only (R-08) on its side.

## Key test scenarios (hints)

1. promote on scratch seeded with the REAL-shape legacy `"*"` PreToolUse Rust uni hook →
   AFTER promote NO `"*"` Rust uni hook survives anywhere; EVERY uni-owned hook (all events) =
   `node <installed>/lib/hook-client/index.js <EVENT>`; PreToolUse === imported
   `PRETOOLUSE_CYCLE_MATCHER`; foreign survives; no dupes; `pruneCount` ≥ 1. (AC-02 a/b, R-09, R-10, #4930)
2. promote→rollback round-trip → every uni command = `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>`; no stale node-client uni hook survives; idempotent; foreign preserved. (R-06)
3. 8 events on no-opt-in scratch; 9 with `settings.local.json` opt-in. (R-10-1)
4. promote twice → no duplicate Unimatrix entries AND second run `pruneCount === 0` (prune idempotent). (R-10-2)
5. `--client` empty/nonexistent → loud exit 5 "run install first" (NOT opaque require trace). (R-05-1)
6. `--dry-run` on the `"*"`-seeded scratch → `{actions, prunes, pruneCount≥1, dryRun:true}` printed; `SETTINGS` byte-unchanged. (FR forward dryRun; prune is planned-only)
7. false-keep/false-prune guard: seed a uni hook whose command path is a SUBSTRING superset of the target (`.../index.js.bak`, `.../dogfood-client-OLD/.../index.js`) → it is PRUNED (whole-token mismatch), proving the match is token-anchored not naive `includes`.
8. foreign hook whose command merely CONTAINS the target token as a substring but is NOT a uni hook (`isUnimatrixHook` false) → KEPT untouched (prune is gated on `isUnimatrixHook` first).

## Gaps / flags

- None blocking. Rollback's `LD_LIBRARY_PATH` value and binary path are produced entirely by the
  shipped legacy arm from the single `RUST_BINARY` string; the script asserts nothing about the
  Rust binary existing on disk (rollback wires the *command*, not a run) — consistent with
  "delivered, not executed" and with hook fail-open if the binary is absent.
- **Edge: target token as a substring of a different path** (Stage 3b amendment). The prune's
  keep/prune decision uses `commandReferencesTarget` = whole-shell-token match (quote-stripped,
  whitespace-split), NOT `cmd.includes(targetToken)`. This avoids both false-keep (a stale hook
  at `.../index.js.bak` or `.../dogfood-client-OLD/.../index.js` is correctly PRUNED) and
  false-prune (the genuine post-switchover command is KEPT). Rollback additionally accepts a
  `dirname`-level token match so the `LD_LIBRARY_PATH=<dirname(rustBinary)>` prefix counts as
  on-target. Test scenarios 7–8 lock this in.
- **Edge: prune now owns the write** (Stage 3b amendment). Because the prune mutates
  `result.content` after `mergeSettings` returns, `mergeSettings` is always called `{dryRun:true}`
  (pure compute) and the one-liner performs the single `writeFile`, gated by the script's own
  `--dry-run`. This removes any double-write / write-then-mutate ordering hazard. Confirm at
  implementation that the installed `mergeSettings` returns `content` populated under
  `{dryRun:true}` (the OVERVIEW frozen contract states it returns `{actions, content}`); if a
  future shipped version only populated `content` on a real write, fall back to prune-then-rewrite
  on real runs (shape (b) in the promote note). No new external deps either way.
