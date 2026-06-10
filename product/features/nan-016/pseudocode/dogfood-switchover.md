# Component 2 — `scripts/dogfood-switchover.sh` (Switchover: promote / rollback / --dry-run)

## Purpose

Repoint a target `settings.json`'s hooks via the **installed** copy's `lib/merge-settings.js`:
- **promote** = node-client `commandSource` (object arm) → `node <client>/lib/hook-client/index.js <EVENT>`.
- **rollback** = Rust binary path **string** → `normalizeCommandSource` legacy arm →
  `LD_LIBRARY_PATH=<binDir> <binary> hook <EVENT>`.
- `--dry-run` forwarded to `mergeSettings`'s `dryRun` (no write).

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
  const { mergeSettings, buildHookClientCommand, HOOK_EVENTS } = require(process.env.MERGE_JS);
  const path = require("path");
  const settingsPath = process.env.SETTINGS;
  const clientDir    = process.env.CLIENT;
  const dryRun       = process.env.DRYRUN === "true";
  const entry = path.join(clientDir, "lib/hook-client/index.js");
  const result = mergeSettings(settingsPath, {
      events: HOOK_EVENTS,
      commandForEvent: (event) => buildHookClientCommand(entry, event)
  }, { dryRun });
  // mergeSettings filters SubagentStop unless settings.local.json (sibling of settingsPath)
  //   opts in -> on a fresh scratch root, 8 events registered (Component 3 asserts 8).
  // isUnimatrixHook recognizes the existing Rust commands -> updated IN PLACE (no dupes);
  //   foreign hooks untouched. PreToolUse narrowed "*" -> PRETOOLUSE_CYCLE_MATCHER (SR-05).
  process.stdout.write(JSON.stringify(result.actions));   // loud, parseable actions
  // mergeSettings itself writes settingsPath unless dryRun.
```

Emitted command form (verified): `node <clientDir>/lib/hook-client/index.js <EVENT>`; path
quoted iff it contains whitespace. `~/.unimatrix/dogfood-client` has no whitespace → bare,
matched by the `UNIMATRIX_PATTERNS` node-client arm → idempotent re-point.

## How rollback repoints (ADR-003 / FR-7)

```
node-one-liner (mode=rollback):
  const { mergeSettings } = require(process.env.MERGE_JS);
  const settingsPath = process.env.SETTINGS;
  const dryRun       = process.env.DRYRUN === "true";
  const rustBinary   = process.env.RUST_BINARY;     // <repo>/target/release/unimatrix (a STRING)
  // STRING commandSource -> normalizeCommandSource legacy arm ->
  //   "LD_LIBRARY_PATH=<dirname(rustBinary)> <rustBinary> hook <EVENT>" over HOOK_EVENTS.
  const result = mergeSettings(settingsPath, rustBinary, { dryRun });
  process.stdout.write(JSON.stringify(result.actions));
```

Shell sets `RUST_BINARY="$REPO/target/release/unimatrix"`. No bespoke revert string — the
shipped legacy arm owns the exact form, so promote↔rollback round-trips cannot drift (R-06).
Rollback is idempotent (re-running yields the same legacy commands; `isUnimatrixHook` re-owns).

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
  foreign hooks preserved, no duplicates, 8 events (no opt-in) / 9 (opt-in). stdout = actions JSON.
- OUT (rollback): `SETTINGS` rewritten to the Rust legacy command form over HOOK_EVENTS.
- Side effects: writes only `SETTINGS` (skipped on `--dry-run`); requires JS only from `--client`.

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

1. promote on scratch seeded with Rust `"*"` shape → every command = `node <installed>/lib/hook-client/index.js <EVENT>`; PreToolUse === imported `PRETOOLUSE_CYCLE_MATCHER`; foreign survives; no dupes. (AC-02 a/b, R-09, R-10)
2. promote→rollback round-trip → every command = `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>`; idempotent; foreign preserved. (R-06)
3. 8 events on no-opt-in scratch; 9 with `settings.local.json` opt-in. (R-10-1)
4. promote twice → no duplicate Unimatrix entries (idempotent re-point). (R-10-2)
5. `--client` empty/nonexistent → loud exit 5 "run install first" (NOT opaque require trace). (R-05-1)
6. `--dry-run` → actions printed, `SETTINGS` byte-unchanged. (FR forward dryRun)

## Gaps / flags

- None blocking. Rollback's `LD_LIBRARY_PATH` value and binary path are produced entirely by the
  shipped legacy arm from the single `RUST_BINARY` string; the script asserts nothing about the
  Rust binary existing on disk (rollback wires the *command*, not a run) — consistent with
  "delivered, not executed" and with hook fail-open if the binary is absent.
