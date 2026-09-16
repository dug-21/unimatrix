# Agent Report — C3 Provider Normalization Arm (JS mirror)

**Agent:** vnc-049-agent-3-c3-provider-arm-js (role: uni-js-dev)
**Component:** C3 — opencode provider arm, JS mirror of C2 (`normalize.js`)
**Wave:** 3 | **Issue:** #986 | **Commit:** `261ce634`

## Files modified
- `packages/unimatrix/lib/hook-client/normalize.js`
- `packages/unimatrix/test/hook-client/normalize.test.js`

FILE BOUNDARY honored: no plugin dir (C1), no Rust files (C2/C6), no `init.js`/`opencode-install.js` (C9).

## Tests
- `node --test test/hook-client/normalize.test.js`: **22 pass / 0 fail**.
- Coverage: 7-event canonicalization mirror (each asserts full `(name, provider)` contract), all-seven identity, unknown→`__unknown__` sentinel, alias-guard-must-not-hijack shared canonical names (#5751), existing-provider non-regression (claude-code/gemini-cli), `KNOWN_PROVIDERS` membership, `isValidModelId` accept/reject incl. trailing-`\n` and non-string inputs.
- Size gate (`node test/check-hook-client-size.js`): **PASS** — stripped 104500/110000 (~5.5 KB headroom), raw 189946/200000 (~10 KB headroom).
- `package.json` / lockfile unchanged; zero new dependencies.

## Parity decisions flagged (for Gate / C8)
1. **Mirrored the shipped C2 code, not pseudocode §4.** Committed `opencode.rs` implements the delegation as an `is_opencode_event_alias(event)` guard at the top of `normalize_event_name` (inert — returns `false` for the 7 shared canonical names, #5751) plus `canonicalize`/`normalize`. Pseudocode §4 described a `provider === "opencode"` param path that does not exist in the JS client (F3/hook-client has no `--provider` flag; the OpenCode plugin shells to the Rust binary). The oracle is the committed Rust. JS mirror: `isOpencodeEventAlias` guard at top of `normalizeEventName` → `normalizeOpencode` → `[canonicalizeOpencode(event), "opencode"]`. Exported `canonicalizeOpencode`/`normalizeOpencode`/`isOpencodeEventAlias`/`isValidModelId`/`KNOWN_PROVIDERS` for the C8 corpus to assert against.
2. **Added `KNOWN_PROVIDERS` to the JS client.** It did not previously exist anywhere in the hook-client (only Rust `hook.rs:158`), because the JS client only ran the inference path. Added as the single-source mirror anchor: `["claude-code","gemini-cli","codex-cli","opencode"]`.
3. **`isValidModelId` byte-parity note.** Rust `is_valid_model_id` = `!empty && len<=128 && chars in [a-z0-9._/-]`. The regex `/^[a-z0-9._/-]+$/` is byte-parity safe in JS because JS `$` without the `m` flag matches only the true end of input — no Python/Ruby trailing-`\n` leniency — so `"ollama/qwen3-coder\n"` correctly rejects (explicitly tested). No redundant `\n` guard added.
4. Repo size-gate budgets are 110000/200000 (authoritative), not the 100/160 KB in the role brief; both pass.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern) → #5737 (three-touchpoints harness onboarding), #4298 (normalize-at-boundary); (decision, vnc-049) → #5741 ADR-004, #5746 ADR-009, #5742 ADR-005; `mcp__unimatrix__context_get` #5751 (Rust alias-guard must return false for shared canonical names). All applied.
- Stored: entry **#5754** "JS provider-arm mirror: mirror the shipped alias-guard, and JS regex $ is byte-parity safe vs Rust" via `context_store` (pattern; topic `hook-client`, feature_cycle `vnc-049`).
