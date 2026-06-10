# Component: `dogfood-switchover.sh` — Retire the Bespoke Prune

File: `scripts/dogfood-switchover.sh`
Change: collapse `run_promote` / `run_rollback` to a plain
`mergeSettings(..., {dryRun})` that owns its own write; delete the
`PRUNE_FRAGMENT` machinery. Matches `initRemote`'s call shape (nan-016 ADR-003 —
one battle-tested ownership-aware path).

> ⛔ GATE C (binding). This describes the POST-RETIRE shape. The actual deletion
> of `PRUNE_FRAGMENT` is blocked until P1–P8 parity is proven GREEN on REAL
> legacy-shaped input (a genuine `"*"` Rust `PreToolUse` uni hook plus `.bak` /
> old-client-dir uni hooks), NOT a pre-narrowed seed (#4938). The
> fragment-deletion commit MUST NOT precede the parity proof. A green
> `merge-settings.test.js` does not satisfy this gate.

## Purpose

`PRUNE_FRAGMENT` (`shellTokens`, `commandReferencesTarget`, `pruneStaleUniHooks`,
`emitAndWrite`) existed only because `lib/merge-settings.js` was frozen (nan-016
C-8) and the script ran AFTER `mergeSettings` as a separate process, so it had to
reconstruct the keep-target from a `targetToken` via a quote-aware tokenizer
(#4931) and a rollback dirname special-case. vnc-031 Step 3c moves the prune
inside `mergeSettings`, holding the kept object by reference (ADR-001). Every
script case is now subsumed by Step 3c (parity table P1–P8). The script collapses
to a thin wrapper that calls `mergeSettings` and relies on its Step 4 write.

## What is DELETED

- `PRUNE_FRAGMENT` heredoc block (the entire `const fs ... }` shared fragment).
- `shellTokens(cmd)`
- `commandReferencesTarget(cmd, targetToken, allowDirname)`
- `pruneStaleUniHooks(content, targetToken, isUnimatrixHook, allowDirname)`
- `emitAndWrite(settingsPath, result, prunes, dryRun)`
- The `{ dryRun: true }`-always-then-bespoke-write pattern, including
  `targetToken` derivation and the `prunes` / `pruneCount` JSON shape.

## What is RETAINED (unchanged)

- `main`, arg parsing, `--settings` / `--client` / `--dry-run`, `--settings=` /
  `--client=` forms.
- `expand_home`, `die`, `require_cmd`.
- Exit codes: 0 (applied/dry-run), 2 (bad mode / unknown arg), 5 (incomplete
  client — the R-05 completeness checks on `merge-settings.js`, entrypoint,
  `config.js`), 7 (node threw / mergeSettings failed).
- Env-only parameter passing into node (`MERGE_JS`, `SETTINGS`, `CLIENT`,
  `DRYRUN`, `RUST_BINARY`) — never interpolated into the JS string; this SHRINKS
  the surface (no more shell-token parsing of stale commands).
- `set -eu`, the `MODE` dispatch, the git-root resolution, defaults.

## Post-Retire `run_promote` (object arm)

```
run_promote:
    node - <<NODE
        const { mergeSettings, buildHookClientCommand, HOOK_EVENTS } = require(process.env.MERGE_JS);
        const settingsPath = process.env.SETTINGS;
        const clientDir    = process.env.CLIENT;
        const dryRun       = process.env.DRYRUN === "true";
        const entry        = path.join(clientDir, "lib/hook-client/index.js");   // path via require("path")
        const result = mergeSettings(
            settingsPath,
            { events: HOOK_EVENTS, commandForEvent: (event) => buildHookClientCommand(entry, event) },
            { dryRun }                       // mergeSettings OWNS the write now (was {dryRun:true} + bespoke write)
        );
        process.stdout.write(JSON.stringify({ actions: result.actions, dryRun }));
    NODE
```

Notes:
- `require("path")` is still needed for `path.join(clientDir, ...)`; keep a single
  `const path = require("path");` at the top of the heredoc (previously supplied
  by `PRUNE_FRAGMENT`). `fs` is no longer needed (mergeSettings owns the write).
- `isUnimatrixHook` is no longer imported here — the prune lives in mergeSettings.
- `targetToken` derivation is gone — Step 3c keeps by identity, no token needed.
- Output JSON drops `prunes` / `pruneCount`. Cross-matcher removals now surface in
  `result.actions` as `Removed stale unimatrix hook: <event> (cross-matcher
  migration)` (and `[dry-run] `-prefixed under `--dry-run`). See harness component
  for the consumer impact (the harness no longer reads `pruneCount`).

## Post-Retire `run_rollback` (string / legacy arm)

```
run_rollback:
    node - <<NODE
        const { mergeSettings } = require(process.env.MERGE_JS);
        const settingsPath = process.env.SETTINGS;
        const dryRun       = process.env.DRYRUN === "true";
        const rustBinary   = process.env.RUST_BINARY;
        const result = mergeSettings(settingsPath, rustBinary, { dryRun });   // legacy string arm; mergeSettings owns write
        process.stdout.write(JSON.stringify({ actions: result.actions, dryRun }));
    NODE
```

Notes:
- The legacy string arm flows through `normalizeCommandSource` →
  `LD_LIBRARY_PATH=<binDir> <binary> hook <event>` over `HOOK_EVENTS`, unchanged.
- The rollback dirname special-case (`allowDirname`) is GONE: the genuine legacy
  command's managed-group entry IS the keep-target by identity (P5), so no
  dirname heuristic is needed (ADR-003 parity row).
- `path` import: rollback's post-retire body no longer uses `path`; omit it (or
  keep a harmless single import). Do not reintroduce `fs`.

## Parity Argument (the GATE-C proof obligation, P1–P8)

Each row must be GREEN on REAL legacy input for the correct arm before deletion:

| # | Legacy input | Step 3c outcome | Arm |
|---|---|---|---|
| P1 | Stale `"*"` PreToolUse Rust uni hook | Pruned; fresh under `PRETOOLUSE_CYCLE_MATCHER` kept | promote |
| P2 | Stale node-client uni hook (prior install) | Pruned | rollback |
| P3 | `.../index.js.bak` uni hook | Pruned (uni-owned, not kept object — unconditional) | both |
| P4 | Old-client-dir uni hook (`dogfood-client-OLD/...`) | Pruned | promote |
| P5 | Rollback genuine `LD_LIBRARY_PATH=<dir>` command | Kept by identity (no dirname heuristic) | rollback |
| P6 | Quoted spaced-path target (#4931 tokenizer bug) | Kept by identity; quoting irrelevant | both |
| P7 | Foreign hook alongside stale uni hooks | Preserved byte-for-byte | both |
| P8 | Group emptied solely of uni hooks | Group dropped, event key retained | both |

The whole-shell-token cases (P3, P4) collapse to FR-04's unconditional
prune-outside-managed; the dirname keep (P5) and quoted-path keep (P6) collapse
to FR-02's identity keep — strictly safer than the tokenizer.

## Error Handling

- Node failure (malformed settings JSON, mergeSettings throw) → non-zero node
  exit → `die "mergeSettings failed (mode=...)" 7` (unchanged dispatch in `main`).
- Incomplete client → exit 5 before `require` (unchanged R-05 checks).
- Bad mode / unknown arg → exit 2 (unchanged).
- Write failures now propagate from `mergeSettings` Step 4 (`fs.writeFileSync`)
  rather than the deleted `emitAndWrite` — same non-zero-exit → exit-7 path.

## Key Test Scenarios (hints)

- **AC-09a (static)** the script contains no `PRUNE_FRAGMENT` /
  `pruneStaleUniHooks` / `commandReferencesTarget` / `shellTokens` /
  `emitAndWrite`; both arms call `mergeSettings(..., {dryRun})` owning the write.
- **P1–P8 parity (R-04, binding ordering gate)** promote on a real legacy seed
  carrying P1+P3+P4+P7 → zero stale uni off the entrypoint, fresh node-client
  present, foreign preserved. Rollback mirror carrying P2+P3+P7 plus the genuine
  legacy command (P5/P6) → every uni hook is the exact Rust form, node-client uni
  count 0, genuine command kept. ON REAL LEGACY INPUT — not a pre-narrowed seed.
- **Dry-run** `--dry-run` writes nothing and emits `[dry-run]`-prefixed actions
  (exit 0); completeness/exit-code behavior unchanged.
- **Idempotency** promote then promote (and rollback then rollback) byte-stable
  (exercised via the effect harness round-trip, T2).
