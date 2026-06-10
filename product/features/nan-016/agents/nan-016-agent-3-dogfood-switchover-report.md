# Agent Report — nan-016 Agent 3 (dogfood-switchover, rework: stale-hook prune)

## Scope
Extend `scripts/dogfood-switchover.sh` with a Stage-3b stale-uni-hook PRUNE so the dogfood
switchover produces a CLEAN switch (no stale `"*"` PreToolUse Rust uni hook surviving, #4930).

## Files modified
- `scripts/dogfood-switchover.sh`

(No `lib/**`, no `package.json`/lockfile — C-8 honored, zero new deps.)

## Write-ownership branch implemented: shape (a) — dryRun-compute
Verified the installed/in-repo `lib/merge-settings.js`: `mergeSettings` returns `{ actions, content }`
and populates `content` regardless of `dryRun` (it builds `content` first, writes only if `!dryRun`).
So shape (a) applies: both one-liners call `mergeSettings(..., { dryRun: true })` as a PURE compute,
`pruneStaleUniHooks` mutates `content`, then the one-liner owns the SINGLE `fs.writeFileSync`, gated
by the script's own `--dry-run`. No double-write, no read-back race. (Note: `{dryRun:true}` prefixes
every action string with `[dry-run] ` — cosmetic; the JSON envelope carries the script's real
`dryRun` flag separately.)

## Implementation notes
- `pruneStaleUniHooks(content, targetToken, isUnimatrixHook, allowDirname)` — walks EVERY event /
  matcher group / entry; removes iff `isUnimatrixHook(entry) && !commandReferencesTarget(...)`;
  drops emptied matcher groups, then emptied event keys. Returns a `prunes[]` report.
- `targetToken`: promote → `path.join(clientDir, "lib/hook-client/index.js")`; rollback →
  `RUST_BINARY` = `<repo>/target/release/unimatrix`.
- `commandReferencesTarget` uses a QUOTE-AWARE tokenizer (not naive whitespace-split — see below),
  whole-token equality, plus env-assignment value match (`LD_LIBRARY_PATH=<dir>`), and on rollback a
  `dirname(targetToken)` match for the `LD_LIBRARY_PATH` prefix.
- `emitAndWrite` writes the single `{ actions, prunes, pruneCount, dryRun }` JSON envelope to stdout
  and performs the gated `writeFile`.
- Reuses the SHIPPED `isUnimatrixHook` / `buildHookClientCommand` / `HOOK_EVENTS` /
  `PRETOOLUSE_CYCLE_MATCHER` from the installed `lib/merge-settings.js` (C-8; no new lib code).

## Deviation from the amended pseudocode (corrected, with rationale)
The pseudocode specified the token match as "strip one quote pair from each WHITESPACE-SPLIT token."
Implemented literally, `cmd.split(/\s+/)` breaks `buildHookClientCommand`'s own output for any client
path with whitespace: the entrypoint is double-quoted (`node "<dir with space>/.../index.js" EVENT`),
a whitespace split shatters the quoted path, NO token equals `targetToken`, and the prune deletes the
hooks `mergeSettings` just wrote (promote nukes all 8, non-idempotent). I implemented a quote-aware
tokenizer (`/"([^"]*)"|'([^']*)'|(\S+)/g`) that treats a quoted segment as one token with quotes
stripped — faithful to the "whole shell token" INTENT and to the `buildHookClientCommand` quoting
contract. The `.bak` / `-OLD/` false-keep guards still hold (different whole token → pruned).

## Tests run (dev-time, scratch only — never live settings)
Installed via `scripts/dogfood-install.sh --target <tmp>`; seeded scratch settings with a legacy
`"*"` PreToolUse Rust uni hook + a foreign Bash hook + an OLD-dir node-client uni hook under a
foreign matcher. Node assertion harness (execFileSync the script against scratch):

- **23 / 23 assertions PASS, 0 fail.** Covered:
  1. promote: every uni hook → installed entrypoint; stale `"*"` Rust uni hook count == 0
     (`pruneCount>=1`); OLD-dir uni hook pruned (token-anchored); PreToolUse matcher === imported
     `PRETOOLUSE_CYCLE_MATCHER`; no `"*"` group survives; foreign preserved; 8 events (no opt-in).
  2. promote idempotent: 2nd run `pruneCount===0`, byte-identical settings.
  3. promote→rollback: every uni hook == exact Rust legacy form; zero stale node-client uni hooks;
     foreign preserved.
  4. rollback idempotent: `pruneCount===0`, byte-identical.
  5. `--dry-run`: settings byte-unchanged, `dryRun:true`, `pruneCount>=1`, the planned `"*"`
     PreToolUse prune is named in `prunes[]`.
  6. NEGATIVE CONTROL: the no-prune (mergeSettings-alone) post-state STILL has the stale `"*"` Rust
     uni hook → the count-zero assertion would FAIL → prune is load-bearing.
  7. non-uni foreign hook whose command merely mentions the target token → KEPT (prune gated on
     `isUnimatrixHook` first).
- Whitespace-path security/idempotence: spaced `--client` → emitted command quoted exactly as
  `buildHookClientCommand`, `pruneCount 0` on first AND second promote.
- Loud error paths: missing `--client` → exit 5 actionable message; bad mode → exit 2.

## Gates
- `node --test test/merge-settings.test.js` (frozen API): **48 / 48 pass** — API untouched.
- `node test/check-hook-client-size.js`: **PASS** — stripped 76597/100000 (headroom 23403),
  raw 129550/160000 (headroom 30450). (No `lib/**` touched → gate unaffected.)
- `lib/`, `package.json`, `package-lock.json`: unchanged — zero new deps.

## Issues / blockers
None blocking. One pseudocode correction (quote-aware tokenizer) documented above and stored as a
pattern.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #4930 (the exact stale-`"*"`-hook trap this
  rework fixes) and ADR-003 (switchover repoints through shipped mergeSettings). Applied both.
- Stored: entry #4931 "dogfood-switchover prune: commandReferencesTarget needs a QUOTE-AWARE
  tokenizer, not naive whitespace-split" via `/uni-store-pattern` (edge Supports #4930).
