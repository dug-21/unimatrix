# Test Plan — Codex Installer (`lib/codex-install.js`)

> Component: `maybeWireCodex`, `writeCodexMcpToml`, `writeCodexHooks` (+ internal `upsertTomlTable`/`readTomlTable` — see `toml-surgical.md`). ADR-002/003.
> Runner: `node --test` · Test file: new `test/codex-install.test.js`, temp `.git` fixtures.
> Primary risks: **R-04** (TOML preservation), **R-05** (provider mislabel), **R-10** (injection). Secondary: R-06 (trust surface), R-11 (malformed), R-15 (event set).
> ACs: AC-06 (TOML foreign-preserved), AC-07 (hooks shape + `--provider codex-cli`), AC-08 (targets JS client). Behavioral fire/return (AC-09/AC-10) → `c14-verifier.md`.

Component-level tests for the codex writers: they produce the `.codex/config.toml` `[mcp_servers.unimatrix]`
table and `.codex/hooks.json`, and return `WireLeg[]`. The **byte-preservation**, **command-string**,
and **WireLeg** assertions live here; the **execution** (does the hook fire / does retrieval return)
lives in the C14 verifier.

## `writeCodexMcpToml` — MCP TOML (AC-06, R-04)

- `test_writeCodexMcpToml_fresh_creates_owned_table` — no `.codex/config.toml` → creates it with `[mcp_servers.unimatrix]`; WireLeg `{surface:"mcp", action:"created", path, entry}`.
- `test_writeCodexMcpToml_preserves_foreign_tables_comments_order` — fixture with foreign `[mcp_servers.other]`, interleaved comments, non-alpha key order → owned table present AND foreign regions byte-preserved incl. **adjacency** (delegates to `upsertTomlTable`; asserts at the file level here) (AC-06, NFR-08).
- `test_writeCodexMcpToml_update_in_place_stale_value` — existing `[mcp_servers.unimatrix]` with stale `command` → upserted; surrounding bytes untouched.
- `test_writeCodexMcpToml_idempotent` — run twice → byte-identical; second `action:"unchanged"` (AC-04).
- `test_writeCodexMcpToml_local_command_shape` — `binaryPath` → `command = "<abs binaryPath>"`.
- `test_writeCodexMcpToml_cloud_bridge_shape` — cloud → `command="node", args=["<mcp-bridge.js>","<hash>"]`; **never** `url=` with a token (Principle 8, Q1). Assert no `url =` line and no secret bytes written.

## `writeCodexHooks` — hooks JSON (AC-07, AC-08, R-05)

- `test_writeCodexHooks_writes_seven_event_set` — emitted `.codex/hooks.json` contains exactly the 7 events (SessionStart, UserPromptSubmit, PreToolUse w/ matcher `^context_cycle$|^mcp__unimatrix__context_cycle$`, PostToolUse `*`, PreCompact, SubagentStart `*`, Stop); excludes PostToolUseFailure, SubagentStop (ADR-003 §3).
- `test_writeCodexHooks_every_command_targets_hook_client` — each command invokes `node <…/hook-client/index.js> <EVENT> …`, does **NOT** invoke the `unimatrix` binary (AC-08, C-03) — string assertion on the written command.
- `test_writeCodexHooks_every_command_carries_provider_flag` — `--provider codex-cli` on **every** command; a missing flag is a **fail-loud defect** (NFR-07, C-04). Iterate all 7 — none exempt.
- `test_writeCodexHooks_preserves_foreign_hooks` — foreign hook entries in an existing `.codex/hooks.json` preserved via `isUnimatrixHook` scoping; owned entries idempotent (AC-04).
- `test_writeCodexHooks_command_is_exact_wireleg_command` — the `WireLeg.command` equals the byte-exact string written to disk (manifest fidelity — the verifier executes this).

## Injection into written config (R-10)

- `test_codex_writers_quote_metacharacter_paths` — project root / clientPath / binaryPath containing a space and a `"` → emitted TOML `command`/`args` and hooks `command` are correctly quoted/escaped and re-parse (TOML read-back + argv split) to the intended values; not naive concat. One fixture per written surface (TOML command, hooks command).

## Trust precondition surface (R-06, NFR-09)

- `test_maybeWireCodex_untrusted_surfaces_precondition` — in an untrusted `.codex/` context, the leg surfaces the trust precondition (warn text / `reason` on the WireLeg), never a silent inert pass presented as success. (Behavioral firing in an untrusted env is the C14-verifier / Gate-0 concern — see OVERVIEW Gate-0 outcome.)

## Malformed / fail-safe (R-11, AC-15)

- `test_maybeWireCodex_malformed_toml_skips_preserves` — invalid TOML → `skipped-malformed`, input byte-preserved, no throw, no partial write; distinct from `skipped-undetected`.
- `test_maybeWireCodex_malformed_hooks_json_skips_preserves` — invalid `.codex/hooks.json` → same fail-safe posture.

## Coverage note

R-04/R-05/R-10 discharged at the written-artifact level here. The load-bearing R-01/R-02/R-06/R-15 assertions (hook actually fires; retrieval actually returns; per-event Q4 evidence; trust in a real env) are executed from this component's `WireLeg` manifest in `c14-verifier.md`, per the Gate-0 outcome in OVERVIEW.
