# Agent Report — vnc-026-agent-17-rework-index-parse

## Task
REWORK (Stage 3b, pre-Gate-3b): fix the Layer 1 parity divergence
`stdin-lone-surrogate-escape` in `index.js::parseHookInput`.

## What changed
`parseHookInput` accepted Node JSON.parse's lone-surrogate `\uD800` escape and
kept the real `session_id`. The Rust oracle (`serde_json::from_str::<HookInput>`)
rejects the whole document — every Rust String, including the `#[serde(flatten)]`
`extra` Value strings and object keys, must be valid UTF-8. So the oracle falls
back to empty input + ppid-fallback session id (golden: `session_id: "ppid-X"`,
empty payload).

The fixture's surrogate lives in an UNKNOWN field (`note`) that flattens into
`extra`, so checking only named fields would not suffice. Fix deep-scans every
string (keys + values, any depth, arrays included) after JSON.parse via a UTF-8
round-trip check (`Buffer.from(s,"utf8").toString("utf8") !== s`) and routes lone
surrogates to the same defensive empty-input fallback serde takes. Well-formed
astral surrogate pairs (emoji) round-trip losslessly and are unaffected.

## Files modified
- `packages/unimatrix/lib/hook-client/index.js` — added `hasLoneSurrogate` /
  `containsLoneSurrogate` helpers; lone-surrogate guard in `parseHookInput`.
- `packages/unimatrix/test/hook-client/index.test.js` — 4 new unit tests
  (unknown-field, named-field, object-key surrogates; astral-pair non-regression).
- `packages/unimatrix/test/hook-client/parity-layer1.test.js` — removed the
  `stdin-lone-surrogate-escape` entry from `REQUEST_TODO` (now `{}`); todo case
  is now a passing assertion.

## Tests
Full hook-client suite (`node --test "test/hook-client/**/*.test.js"`):
- 427 pass, 0 fail, 1 skip (pre-existing), 1 todo.
- The 1 remaining todo is `stdout-subagent-non-entries-fallback`, owned by the
  concurrent transform agent — left untouched as instructed.
- `test_request_parity_stdin-lone-surrogate-escape_matches_golden`: PASS.

## Commit
`37aefb99` on `feature/vnc-026` — committed ONLY my three files via
`git commit -o -m ... -- <paths>` (shared checkout, siblings active).

## Issues / blockers
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-001 (#4751, Rust hook
  is the parity oracle), #247 (graceful parse failure → empty input, exit 0),
  ADR-002/#4775 (stdout layer). Confirmed the empty-input fallback is the correct
  parity target.
- Stored: entry #4777 "JS JSON.parse vs serde_json lone-surrogate divergence:
  deep-scan after parse for UTF-8 well-formedness" via context_store (pattern).
