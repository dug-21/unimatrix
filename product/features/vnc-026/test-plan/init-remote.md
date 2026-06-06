# Test Plan: init-remote (init.js + merge-settings.js changes)

Risks: R-11 (High), R-12, R-16, R-18; AC-11, AC-16. Extends existing `init.test.js` /
`init-integration.test.js` / `merge-settings.test.js` (cumulative infra — NFR-06).

## Ownership Pattern (R-11 — carries the ONLY open gate note)

**Gate obligation**: the spaced-path defect in `/(^|\s|\/)node\s+\S*\/hook-client\/index\.js\s/`
must be resolved in Stage 3a pseudocode BEFORE this table freezes; confirm `require.resolve`
output shapes on Windows/macOS first. The table below tests the FIXED pattern.

- `test_pattern_table` — positive/negative command-string table:
  - MATCH: `node /usr/lib/node_modules/@dug-21/unimatrix/lib/hook-client/index.js PreToolUse`; spaced paths `node "C:\Program Files\nodejs\…\hook-client\index.js" Stop` and `node /home/u/My Projects/…/hook-client/index.js Stop`; Windows backslash form per resolved pseudocode.
  - NO MATCH (foreign): `node /opt/other-tool/index.js X`; `node /x/hook-client-extra/index.js X`; `some-binary hook-client/index.js`; existing foreign hooks from the current `merge-settings.test.js` corpus.
  - Old-style `unimatrix hook` entries still matched by existing patterns (mode-switch replacement).

## Init Matrix (AC-11)

- `test_fresh_config` — empty `.claude/settings.json` → all 9 remote events written as `node /abs/path/lib/hook-client/index.js <EVENT>` (abs path via `require.resolve`), matchers correct (PreCompact `""`, PostToolUseFailure `"*"`).
- `test_rerun_idempotent` — second `init --remote` → recognized own entries replaced, not duplicated; **double-fire check: entries per event == 1 after two runs**.
- `test_foreign_hooks_preserved` — foreign hooks incl. a foreign `node` command survive byte-identically.
- `test_mode_switch_replaces_old_style` — config with `unimatrix hook` entries → replaced by node-command entries (SR-08).
- `test_settings_local_json` — `unimatrix.remote {url, token}` written merge-preserving (other keys untouched), mode 0600, gitignore warning emitted when `.gitignore` doesn't cover it; no warning when covered.
- `test_no_token_on_argv_or_settings_json` — hook command lines contain no token; `.claude/settings.json` content scan for token (R-16).
- `test_mcp_json_skipped_with_message` + binary/DB steps skipped (FR-20).
- `test_dry_run` — `--dry-run` writes nothing.

## Ping Validation (R-18 / FR-19 — the ONE loud path)

- `test_ping_happy` — stub returns Pong → init succeeds.
- `test_ping_wrong_token_loud_auth_failure` — stub 401 on bad Bearer → init exits non-zero with actionable `auth` message (proves Ping exercises Bearer auth, not mere reachability).
- `test_ping_non_pong_200` — 200 with non-Pong JSON → init fails (strict Pong parse).
- `test_ping_unreachable` — ECONNREFUSED → loud failure, nothing written? (per pseudocode ordering: assert documented behavior — config files not left half-written).

## mergeSettings Generalization (back-compat)

- `test_commandsource_backcompat_wrapper` — existing LOCAL init flow through the wrapper produces **byte-identical** `settings.json` output vs the pre-change snapshot (committed fixture from current behavior, captured before modification).
- `test_commandsource_remote` — remote `commandSource.commandForEvent(event)` + 9-event set produce expected entries.

## FR-21 / AC-16 — HOOK_EVENTS fix (R-12 blast radius)

- `test_local_9_events_fresh` — local init writes the full 9-event set (7 + PreCompact + PostToolUseFailure) with correct matchers.
- `test_local_9_events_rerun_recognized` — re-run over a PRE-EXISTING 7-event local config: new events added, existing recognized, no duplicates.
- `test_blast_radius_confined` — diff of local-mode output vs pre-change snapshot is EXACTLY the two new event entries + matchers — nothing else changes (SR-07 gate).
- Existing `init.test.js`/`merge-settings.test.js` suites must stay green unmodified except where the 9-event expectation legitimately changes (each such edit justified against FR-21).
