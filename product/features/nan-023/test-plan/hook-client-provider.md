# Test Plan — hook-client `--provider` argv hint (`lib/hook-client/index.js`, `normalize.js`, `merge-settings.js`)

> Components: `parseHookArgs(argv) -> {event, providerHint}` (new, `index.js`); `buildHookClientCommand(clientPath, event, providerHint?)` (3rd arg, `merge-settings.js`); the `normalize.js` hint path (existing, reused). ADR-003, Q5.
> Runner: `node --test` + the hook-client parity runner (`npm run test:hook-client`). Test files: extend `test/merge-settings.test.js` (buildHookClientCommand), a new/extended hook-client unit test for `parseHookArgs`, and `test/check-hook-client-size.js` for the gate.
> Primary risks: **R-09** (size gate), **R-05** (provider mislabel / split-brain). Secondary: R-10 (injection into hook command).
> ACs: AC-07 (`--provider codex-cli` on every command), AC-08 (targets JS client). Behavioral firing → `c14-verifier.md`.

The **only** JS-client behavior change is teaching `index.js` to parse `--provider`; the `codex-cli`
arm already exists in both `normalize.js:23` and Rust `hook.rs:35` (confirmed Q5 — **no normalizer arm
to add**). Backward compat: when no `--provider`, behavior is byte-identical to today (claude-code
inference, SR-07).

## `parseHookArgs` (ADR-003 §1)

- `test_parseHookArgs_event_only_no_hint` — `["node","index.js","PreToolUse"]` → `{event:"PreToolUse", providerHint:null}` (backward-compat path).
- `test_parseHookArgs_space_form` — `["PostToolUse","--provider","codex-cli"]` → `{event:"PostToolUse", providerHint:"codex-cli"}`.
- `test_parseHookArgs_equals_form` — `--provider=codex-cli` → `providerHint:"codex-cli"`.
- `test_parseHookArgs_unknown_hint_falls_back_to_inference` — hint not in `KNOWN_PROVIDERS` → `providerHint` ignored / inference used; **never throws** (fail-open, exit-0 contract).
- `test_parseHookArgs_hint_stamps_provider_not_inference` — with a valid hint, the client uses the `normalize.js` **hint path** (as opencode does) → provider stamped `= hint`, NOT inferred `claude-code` (R-05 root defect).

## `buildHookClientCommand` 3rd arg (ADR-003 §2, AC-07/AC-08)

- `test_buildHookClientCommand_no_hint_backward_compatible` — 2-arg call → `"node <path> <EVENT>"` byte-identical to today (guards claude-code golden, SR-07).
- `test_buildHookClientCommand_appends_provider_hint` — 3-arg `"codex-cli"` → `"node <path> <EVENT> --provider codex-cli"`.
- `test_buildHookClientCommand_targets_hook_client_not_binary` — command invokes `node …/hook-client/index.js`, does **not** invoke the `unimatrix` binary (AC-08, C-03).
- `test_buildHookClientCommand_matches_ownership_regex` — output matches `UNIMATRIX_PATTERNS` pattern 5 so re-runs are idempotent/non-clobbering (AC-04).

## Provider attribution — static (R-05, AC-07)

- `test_every_codex_hook_command_carries_provider_flag` — for the codex writer's emitted command set, string-assert `--provider codex-cli` on **every** command; a missing flag is a **fail-loud** defect, not a warning (NFR-07). (Emission driven from `writeCodexHooks` — cross-ref `codex-install.md`.)

## Split-brain parity (R-05, #5737)

- `test_normalize_codex_arm_matches_rust_oracle` — via the parity corpus #4751 runner (`test:hook-client:layer2`): `normalize.js` `codex-cli` hint handling round-trips consistently with the Rust `hook.rs` twin. nan-023 does **not** add a normalizer arm, so this asserts the existing arm still round-trips the hint (guard against accidental drift). Assert on `provider`, NOT `source_domain`.

## Injection into hook command (R-10)

- `test_buildHookClientCommand_path_with_space_quoted` — `clientPath` containing a space/quote → emitted command is correctly quoted/escaped and re-parses to the intended argv (not naive concat).

## Size gate (R-09 — MANDATORY, never raise)

- `test_hook_client_size_gate` — after the `--provider` addition, `node test/check-hook-client-size.js`: assert **stripped ≤ 110 KB (PRIMARY)** and **raw ≤ 200 KB (BACKSTOP)**. Budget against the stripped total, not the ~10 KB raw headroom.
- `test_size_gate_meta_assertion_lockstep` — if cap constants change, the `size-gate` meta-assertion moved in lockstep (#5378). **Never minify or raise the gate** (#5372, #4780). Addition is lean code + trimmed comment prose only.

## Coverage note

Provider attribution covered statically (flag on command) here; the **behavioral** half (ingested event carries `provider="codex-cli"`) is in `c14-verifier.md`. `source_domain` is explicitly excluded from every assertion (ingress forces `claude-code`, #5748).
