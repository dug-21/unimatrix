# Agent Report — nan-016-agent-1-pseudocode (rework: stale-hook prune)

## Task
Stage 3b amendment: add a post-`mergeSettings` stale-uni-hook prune to the switchover pseudocode
so the dogfood soak is CLEAN (no surviving legacy `"*"` PreToolUse Rust uni hook).

## Files amended
- `product/features/nan-016/pseudocode/dogfood-switchover.md`
- `product/features/nan-016/pseudocode/OVERVIEW.md` (cross-boundary data flow + frozen-contract consumer note)

## Design encoded (exact)
- `targetToken`: promote → `path.join(clientDir, "lib/hook-client/index.js")`; rollback → `RUST_BINARY` = `<repo>/target/release/unimatrix`.
- `pruneStaleUniHooks(content, targetToken, isUnimatrixHook)`: walk EVERY `content.hooks[event]`, EVERY matcher group, EVERY entry; remove entry iff `isUnimatrixHook(entry) && !commandReferencesTarget(cmd, targetToken)`; keep foreign + already-on-target; drop emptied matcher groups, then drop emptied event keys. Returns a `prunes[]` report.
- `commandReferencesTarget`: whole-shell-token match (quote-stripped, whitespace-split token equality), NOT naive `cmd.includes`. Rollback also accepts a `dirname`-level match for the `LD_LIBRARY_PATH=<dirname>` prefix.
- Reuses shipped `isUnimatrixHook` required from the INSTALLED `lib/merge-settings.js` (C-8, no new lib code, no new external dep).
- Idempotent (second run prunes nothing). `--dry-run` reports planned prunes (count + per-entry {event, matcher, command}) and writes nothing.
- Write ownership shifted: `mergeSettings` now always called `{dryRun:true}` (pure compute of `{actions, content}`); the one-liner owns the single `writeFile`, gated by the script's own `--dry-run`.

## Postconditions stated in pseudocode
- post-promote: every uni hook → installed entrypoint; no stale `"*"` Rust uni hook survives; foreign preserved; no dupes.
- post-rollback: every uni hook → Rust legacy form; no stale node-client uni hook survives; foreign preserved.

## Edge cases flagged
- Substring-superset paths (`.../index.js.bak`, `.../dogfood-client-OLD/.../index.js`): token-anchored match correctly PRUNES them (avoids false-keep) and KEEPS the genuine command (avoids false-prune). Test scenarios 7–8 added.
- Foreign hook containing target token as substring but `isUnimatrixHook===false`: KEPT (prune gated on ownership first).
- Write-ordering hazard removed by always-`{dryRun:true}` mergeSettings + one-liner-owned write. Implementation must confirm installed `mergeSettings` populates `content` under `{dryRun:true}`; fallback (prune-then-rewrite on real runs) noted if a future shipped version differs.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #4930 (the exact empirical finding: mergeSettings keys on EVENT_MATCHERS[event], stale "*" uni hook not auto-pruned) and #4826 (canonical opt-out prune shape: scope by isUnimatrixHook, drop emptied matcher groups + event key). Both directly applied.
- Deviations from established patterns: none. The prune follows #4826's canonical shape exactly and reuses the shipped predicate per C-8.
