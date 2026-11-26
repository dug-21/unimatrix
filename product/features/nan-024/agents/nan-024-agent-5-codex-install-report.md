# nan-024-agent-5-codex-install Report

Unimatrix entries consulted: #5897, #5896, #5894, #5770, #5774, #5765, #1195

## Report
- Files: `packages/unimatrix/lib/codex-install.js` (rewritten, 389 lines), `lib/codex-toml.js` (new, pure TOML helpers, 359 lines), `test/codex-install.test.js` (rewritten for new API), `test/codex-toml-surgical.test.js` (extended).
- New API: `codexHome`, `detectsCodex`, `writeCodexHooks(o)`, `removeOwnedCodexHooks(o)`, `writeCodexMcpToml(o)`, `removeOwnedCodexProjectMcp(root, dryRun, userCfg?)`, `removeOwnedHooksToml`, `removeTomlTable`, `tomlTableToEntry`, `maybeWireCodex(opts)` (object arg), `codexApprovalStep`.
- Removed exports: `resolveCodexTransport`, `codexTrustNote`, `CONFIG_FILE`, `HOOKS_FILE`. wire.js (Wave 4) still calls the old `maybeWireCodex(dir, opts)` and must move to the object signature `{home, env, platform, node, projectRoot, harnessSel, dryRun}`.
- Legs: new actions `removed`, `skipped-foreign`, `skipped-not-detected` (surface "codex"); wire-init must accept them.
- Deviations: guard skipping project removal when project `.codex` path equals the user-level path (root == home); one-owned-per-event enforced across matcher groups in the user hooks file.
- Tests: 81 pass / 0 fail (codex-install + codex-toml-surgical); hook-ownership + merge-settings 106 pass.
- Size gate: `check-hook-client-size.js` FAILS (stripped 115047/113639, raw 203093/201399), caused by uncommitted changes under `lib/hook-client/` (remote-resolve.js, sibling agent); none of my files are in that dir. Not self-raised.
- No package.json changes. Not committed.

## Knowledge Stewardship
- Queried: context_briefing, context_search (pattern), context_lookup (nan-024 decisions) - ADR-001/003/006 and codex-install pattern #5774 applied.
- Stored: pattern "codex-install: one-owned-per-event needs cross-group strip; project root == home guard".
