# nan-024-agent-4-merge-settings Report

## Report
Files: packages/unimatrix/lib/merge-settings.js (edited), lib/claude-json.js (new), lib/mcp-entry.js (new), test/merge-settings.test.js, test/dogfood-effect.test.js, test/init-remote.test.js, scripts/dogfood-switchover.sh (rollback built a string commandSource; now an object).
- UNIMATRIX_PATTERNS and the normalizeCommandSource string arm deleted (string now throws TypeError). buildShimCommand(event, provider, platform, home) exported; hook-ownership round-trip test now runs (9 pass, 0 skip).
- mergeSettings targets settings.local.json (opt-in key read from the same file), write-if-changed via atomicWriteText (tmp+rename), crossScopeFiles removal via exported removeOwnedHooks, result.commands.
- buildHookClientCommand kept exported (init.js, wire.js, codex-install.js still import it; owned by sibling/other agents). UnsafeShimPathError lazily required from install-store.js with local fallback.
- Tests: merge-settings 81 -> 97 pass; dogfood-effect 8 -> 8 pass; hook-ownership 9 pass; init-remote 56 -> 57 total, 49 pass / 8 fail. The 8 failures are initRemote/legacy-init paths driven by lib/init.js, which still writes .claude/settings.json; they pass once init.js targets settings.local.json (readSettings and seeds already point there). Not run: integration tests. Size gate not applicable (lib/hook-client untouched).
- Behavior change flagged: end-anchored ownership (ADR-003) makes `unimatrix-server hook X | tee ...` foreign; test_handles_tee_pipeline updated to assert preserved + managed entry appended.
- dogfood-effect had one transient failure on first run, green on 3 reruns.
- Unimatrix entries consulted: #5897, #5896, #1195, #4938, #4926, #4936, #4939, #4940, #5770, #5774, #5765 (briefing list)

## Knowledge Stewardship
- Queried: context_briefing, context_lookup (nan-024 decisions) -- ADR-003/001 applied.
- Stored: nothing novel to store -- ownership end-anchor change recorded above.
