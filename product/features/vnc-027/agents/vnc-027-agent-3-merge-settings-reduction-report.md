# Agent Report — vnc-027-agent-3-merge-settings-reduction

Component 7 (ADR-004 §1,§2,§5 / FR-28, FR-29 / AC-08 / R-11, R-12). Merge step 4.

## 1. Files Modified
- `/workspaces/unimatrix/packages/unimatrix/lib/merge-settings.js` (357 lines — under 500 limit)
- `/workspaces/unimatrix/packages/unimatrix/test/merge-settings.test.js` (component tests, cumulative)
- `/workspaces/unimatrix/packages/unimatrix/test/init-remote.test.js` (consumer test fixtures: opt-in)
- `/workspaces/unimatrix/packages/unimatrix/test/init-integration.test.js` (consumer test fixture: opt-in)

Commit: `7de9f8b8 impl(merge-settings-reduction): PreToolUse cycle matcher + SubagentStop opt-in with opt-out prune (#680)` on branch `feature/vnc-027`.

## 2. Tests
- Component file `merge-settings.test.js`: all pass (15 new tests added).
- 4 affected files together: 104 pass / 1 fail (the 1 fail, `test_creates_mcp_json_on_clean_project`, is PRE-EXISTING — identical on clean tree).
- Full package suite: 676 tests, 667 pass, 8 fail — all 8 fail identically on the clean baseline (writeMcpJson env + 7 Layer-1 request-parity goldens owned by build-request-sentinel/index-dispatch Wave 4). **Zero new failures introduced.**
- Did NOT run Stage 3c integration tests, per instructions.

## 3. Issues / Blockers
None. Note: deliberate default-set reduction (SubagentStop now opt-in) rippled to consumer init tests that asserted the fixed 9-event set. These were updated to opt in via a `settings.local.json` fixture (preserving their full-set intent) rather than weakening assertions — except `test_dry_run_writes_nothing_and_skips_network`, which asserts no settings file is written, so the opt-in was kept out of the shared `makeTempProject()` and added only to the three tests that need the full set.

## 4. Confirmations
- **PreToolUse matcher exact**: `EVENT_MATCHERS.PreToolUse = "context_cycle|mcp__unimatrix__context_cycle"` (constant `PRETOOLUSE_CYCLE_MATCHER`). PreToolUse stays in `HOOK_EVENTS`. Asserted by `test_pretooluse_matcher_exactly_cycle_tools` and `test_pretooluse_stays_in_hook_events`.
- **SubagentStop opt-in on/off/non-boolean**: registered only when `unimatrix.hooks.subagent_stop === true` in `{root}/.claude/settings.local.json`; absent/false/non-boolean (`"true"`, `1`, `0`, `null`, `{}`, `[]`, `"yes"`) treated as unset; fail-open on missing/malformed/unreadable. Asserted by the opt-in matrix suite + `subagentStopEnabled` unit matrix.
- **Prune-on-opt-out**: a previously-registered Unimatrix-owned SubagentStop entry is stripped on re-init when the key is absent/false (scoped via `isUnimatrixHook`; emptied groups + event key removed; foreign hooks preserved; idempotent; opt-in↔opt-out round-trip). Asserted by the opt-out pruning suite.
- **Other events unchanged**: SessionStart/Stop/UserPromptSubmit/PreCompact `""`, PostToolUse/PostToolUseFailure/SubagentStart `"*"` — byte-identical snapshot in `test_all_other_matchers_unchanged`. `UNIMATRIX_PATTERNS`, `buildHookClientCommand`, `normalizeCommandSource`, `isUnimatrixHook`, malformed-JSON throw, dry-run all untouched.
- **Shared install surface (ADR-004 §5)**: both node-client (object commandSource) and legacy Rust-hook (string commandSource) shapes get the reduced set — `test_reduced_set_applies_to_node_client_reinit` / `test_reduced_set_applies_to_rust_hook_reinit`.
- **Size gate**: PASSES (exit 0). merge-settings.js does NOT count against the client budget — the gate's `ROOT = lib/hook-client`, and merge-settings.js is in `lib/`, so it is absent from the per-file table. Totals: stripped 68907/100000, raw 112773/160000. merge-settings.js byte count: 12693 B (informational only — not budgeted).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (pattern/decision) — surfaced ADR-004 (#4811), prefix-match settings merge (#1195), and the install-surface-registration pattern (#4809, applied: confirmed PreToolUse/SubagentStop remain in HOOK_EVENTS).
- Stored: entry #4826 "Making a hook event opt-in / narrowing a matcher in merge-settings.js needs an active opt-out prune + ripples to consumer init tests" via /uni-store-pattern.
