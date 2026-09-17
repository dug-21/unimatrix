# Test Plan — C14 Verifier (`test/`)

> Component: the manifest-executing verifier (ADR-006). **The load-bearing component** — the whole design's parity value rests here.
> Runner: `node --test`, driving real/stub MCP servers (`test/helpers/real-server.js`, `mcp-stub-server.js`, `stub-server.js`) and spawning the JS hook client (`spawn(process.execPath, [ENTRY, EVENT, "--provider", "codex-cli"], {env})` + synthetic stdin — the pattern already in `test/hook-client/index-decoration.test.js`).
> Primary risks: **R-01** (ceremonial wiring), **R-02** (tautological verifier), **R-03** (cloud never-green-on-tag), **R-06** (trust), **R-15** (event-set). Behavioral ACs: **AC-05** (opencode return), **AC-09** (codex fire), **AC-10** (return, all harnesses).

## Non-negotiable design rule (R-02, SR-09)

The verifier **EXECUTES `manifest[].command`** and **CONNECTS via `manifest[].entry`** — it MUST NOT
string-match the command, re-assert a literal, or reconstruct paths of its own. Config-presence checks
are **forbidden** as the discharge. Every behavioral AC carries a **mutation/negative control** that
fails when the wired artifact is broken but its manifest string is intact — this is what proves the
verifier runs the command rather than inspecting it (#4177).

## Retrieval-returns (AC-10, AC-05, R-01)

For each C14 leg, connect the MCP target from `manifest[].entry` (the actual wired command/url) and
issue a `context_*` call; assert a **non-empty RETURN**:

- `test_verify_claude_retrieval_returns_local` — claude-code `.mcp.json` `mcpServers.unimatrix` → `context_*` returns (local, Rust binary/UDS). **required**.
- `test_verify_claude_retrieval_returns_cloud` — claude-code via the token-free bridge → returns (cloud). **required** (proven bridge path).
- `test_verify_opencode_retrieval_returns` — connect the wired `mcp.unimatrix` slug → `context_*` returns; the vnc-049 sentinel is byte-preserved (AC-05, C-05). **required** local + cloud (via `mcp.unimatrix` bridge).
- `test_verify_codex_retrieval_returns_local` — codex `[mcp_servers.unimatrix]` `command="<binaryPath>"` → returns (local). **required** (fixture seeds local trust — see Gate-0).
- `test_verify_codex_retrieval_returns_cloud` — codex cloud entry `command="node", args=[bridge, hash]` (never `url=`, Q1) → returns via the bridge. **required** — the bridge is installed/tested and codex-independent for the *return* path.

### Mutation controls — return (R-02)

- `test_verify_retrieval_mutation_broken_entry_fails` — point `manifest[].entry` at a non-existent/blanked MCP target → the verifier **FAILS**. If it still passes, it is a presence proxy → reject. One per behavioral leg (claude, opencode, codex).

## Hook-fires (AC-09, R-01, R-05)

Execute the exact `manifest[].command` (`node <clientPath> <EVENT> --provider codex-cli`) with a
synthetic event on stdin; assert the event **reaches the JS hook client stamped `provider=codex-cli`**
(breadcrumb / queue / server record). Assert on **`provider`**, NOT `source_domain` (ingress forces
`claude-code`, #5748 — asserting source_domain will false-fail).

- `test_verify_codex_hook_fires_local` — drive the wired command → ingress observed, `provider="codex-cli"`. **required (local)**. Codex-independent: the command IS the JS-client invocation (Gate-0 §3).
- `test_verify_codex_hook_fire_mutation_broken_client_fails` — point `manifest[].command` at a non-existent client path → verifier **FAILS**, proving it runs the command rather than string-matching (R-02 mutation control for AC-09).
- `test_verify_claude_hook_fires` — claude hook command fires → ingress observed (regression baseline for the shared client runtime).

## AC-09 per-event firing evidence (Q4 — 7-event table, R-15)

Mirroring Claude's event names does NOT prove Codex fires them. For **each** of the 7 emitted events,
drive the wired command with a synthetic event and RECORD `fires` vs `wired-inactive`:

- `test_verify_codex_per_event_firing_records[SessionStart|UserPromptSubmit|PreToolUse|PostToolUse|PreCompact|SubagentStart|Stop]` — parametrized over the 7 events; each RECORDS the result into the AC-09 per-event table in RISK-COVERAGE-REPORT. Any `wired-inactive` is a **named documented gap**, never a silent drop.

> Distinction (per Gate-0): executing the wired **command** for an event proves the JS-client ingress path fires. Whether **codex itself raises** that event in a real trusted `.codex/` is a separate claim requiring codex installed — see Gate-0 outcome in OVERVIEW; the "codex-raises-it" column is documented-conditional in cloud.

## Trust precondition (R-06)

- `test_verify_untrusted_codex_surfaces_precondition` — in an untrusted `.codex/` env, assert the leg surfaces a loud precondition warning / fail-loud, never an inert no-op read as success (NFR-09). The firing assertion runs in a **confirmed-trusted** fixture (local seed) or the arm is documented-conditional per Gate-0.

## Backward-compat golden (SR-07, R-07)

- `test_golden_claude_mcp_json_unchanged` — capture byte-for-byte golden of `.mcp.json` from a fixture **before** the per-harness refactor; assert byte-identical after `writeMcpJson` routes through `wire.js`.
- `test_golden_claude_settings_json_unchanged` — same for `.claude/settings.json` hooks common path. (Goldens land in `test/fixtures/` before refactor — capture is a Stage 3b/early-3c precondition.)

## Pre-tag real-server exercise (SR-03, R-03)

- `test_pretag_real_server_exercise` — run codex hook-fire + retrieval-return + opencode/claude return against a **real** local+cloud server (`real-server.js`) **before** the release chain — not a release-only gate. Surfaces layered failures off-tag, cutting the multi-round tag tax (#5267). This is the primary R-03 mitigation given the cloud arms.

## Per-AC feasibility matrix recording (Q3)

- `test_record_feasibility_matrix` — the verifier records which of AC-05/AC-08/AC-09/AC-10 ran green in cloud vs local, so "where feasible" is a **stated, asserted fact** (RISK-COVERAGE-REPORT matrix), not a silent gap. Cells: required / conditional-on-trust / infeasible(reason) / n/a(reason).

## Coverage note

This component discharges the Critical risks. No leg's acceptance rests on config presence; every behavioral AC has a mutation/negative control that fails on a broken artifact with an intact manifest string. Cloud-codex firing feasibility is governed by the Gate-0 outcome (OVERVIEW).
