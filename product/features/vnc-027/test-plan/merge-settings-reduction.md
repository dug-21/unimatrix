# Test Plan — merge-settings-reduction (`lib/merge-settings.js`)

Component 7 / ADR-004 §1-§3, §5 / FR-28, FR-29 / **AC-08** / Risks R-11 (High), R-12 (Low).
PreToolUse matcher narrowed; SubagentStop registered only when opt-in key set; all other HOOK_EVENTS/matchers
unchanged. `node --test` on `merge-settings.test.js`. Snapshot-style output assertions.

## Matcher narrowing — R-11 s1 (install-level reduction)

- `test_pretooluse_matcher_exactly_cycle_tools` — generated `EVENT_MATCHERS.PreToolUse` is EXACTLY `context_cycle|mcp__unimatrix__context_cycle` (no longer `*`). The hook process no longer spawns for ordinary tool calls.
- `test_pretooluse_stays_in_hook_events` — PreToolUse remains a registered HOOK_EVENT (the matcher narrows, the event is not dropped).
- `test_all_other_matchers_unchanged` — output snapshot: SessionStart, UserPromptSubmit, SubagentStart, PreCompact, PostToolUse, PostToolUseFailure, Stop matchers byte-identical to F3.

## SubagentStop opt-in — AC-08 / R-12 (on/off/non-boolean matrix)

- `test_subagentstop_absent_by_default` — no opt-in key → SubagentStop NOT in generated settings.
- `test_subagentstop_registered_when_key_true` — `unimatrix.hooks.subagent_stop: true` in `{root}/.claude/settings.local.json` → SubagentStop registered with matcher `*`.
- `test_subagentstop_non_boolean_treated_as_unset` — key set to `"true"` (string), `1`, `null`, `{}` → treated as unset, SubagentStop absent (type-confusion guard, security surface).
- `test_subagentstop_key_false_absent` — key explicitly `false` → absent.

## Shared install surface (ADR-004 §5)

- `test_reduced_set_applies_to_rust_hook_reinit` — the reduced set applies to legacy Rust-hook re-init command shapes too (the install surface is shared; ass-069 Q3 is a product decision, not a TS-only detail). Snapshot covers both command shapes.

## SubagentStop server-side independence — R-12 (one explicit lifecycle proof)

The asserted contract that no server lifecycle awaits SubagentStop is a node:test Layer 2 integration
(see parity-corpus-uds.md `test_no_subagentstop_full_lifecycle`) plus the Rust unit in listener-preformatted.md
(`test_subagentstop_all_none_fallthrough`). This component owns only the install-set membership matrix.

## Edge cases
- Missing `settings.local.json` entirely → SubagentStop absent, no throw.
- Pre-existing install with PreToolUse `*` (stale settings, pre-reduction) → re-init narrows it; client sentinel makes any stale `*` spawns no-ops until re-init (graceful, R-11 s5).
