# Agent Report — C1 OpenCode Plugin Shim (vnc-049, #986)

- **Agent:** vnc-049-agent-3-c1-plugin-shim (role: uni-js-dev)
- **Wave:** 3 — Read + edge + plugin
- **Component:** C1 — OpenCode plugin shim (TypeScript/JS, net-new)
- **Status:** Complete — 38/38 unit tests green; committed on `feature/vnc-049`.

## Files created

All under `packages/unimatrix/opencode-plugin/` (net-new package tree):

| File | Responsibility |
|------|----------------|
| `package.json` | `@dug-21/unimatrix-opencode-plugin` v0.11.3, ESM, `main: unimatrix.js`, `@opencode-ai/plugin@1.18.31` as **peer** dep only |
| `unimatrix.js` | Plugin entry; exports `UnimatrixObservePlugin`; fail-open load (returns inert hooks on bad input) |
| `lib/context.js` | Context from PluginInput; PreCompact gate (OQ-7); content-free logger |
| `lib/frame.js` | Flat Claude-shaped HookInput builder |
| `lib/model.js` | `resolveModel` / `isValidModelId` (charset `^[a-z0-9._/-]{1,128}$`) |
| `lib/emit.js` | Single shell-out boundary (`unimatrix hook … --provider opencode [--model …]`), fully fail-open |
| `lib/session-state.js` | Stop over-count guard + per-session model cache |
| `lib/subagent.js` | Observe-channel agent + carried `parent_id` |
| `lib/events.js` | The 7-event map (event bus + typed hooks) |
| `test/helpers.js` + `entry`/`events`/`subagent`/`stop-guard`/`failopen`/`model`/`precompact` `.test.js` | Component unit tests |

No edits to `normalize.js` (C3), `init.js`/`opencode-install.js` (C9), or any Rust file. `package.json`/lockfile of the host package untouched.

## Tests

**38 pass / 0 fail** (`node --test` in the package dir). Coverage:
- 7-event mapping + bus-derived field synthesis (R-14): SessionStart/UserPromptSubmit/PreToolUse/PostToolUse/SubagentStart/PreCompact/Stop.
- Subagent alignment + spoof rejection (R-06/R-07/AC-04).
- Stop over-count guard, exactly-one-per-active-window (R-11).
- PreCompact present/gated/failsafe (R-12).
- Fail-open on reject/throw/missing-shell; content-free warnings (R-13/NFR-02).
- Model validation + omit-when-absent (R-15/AC-06).
- C9 package-identity contract.

## C9 package-identity confirmation (MANDATORY — test-asserted)

`test/entry.test.js` imports the constants from `packages/unimatrix/lib/opencode-install.js` and asserts equality:
- Package name: **`@dug-21/unimatrix-opencode-plugin`** = `PLUGIN_PACKAGE` ✓
- Export: **`UnimatrixObservePlugin`** = `PLUGIN_EXPORT` ✓
- Entry file: **`unimatrix.js`** = `PLUGIN_ENTRY_FILE` (and `package.json` `main`) ✓

The C9 re-export shim (`export { UnimatrixObservePlugin } from "@dug-21/unimatrix-opencode-plugin"`) resolves against this package cleanly.

## Delivery decisions

- **OQ-7 (PreCompact experimental flag default):** default **ENABLED** with an operator kill-switch (`disablePreCompact: true` plugin option, or env `UNIMATRIX_OPENCODE_PRECOMPACT=0|false`). Registering `experimental.session.compacting` is inert if OpenCode removes/renames the API (the handler simply never fires); the handler body is fully try/catch-guarded, so API drift can neither crash the session nor affect the other six legs. Documented degraded/experimental per ADR-009.
- **OQ-3 / R-06 (subagent ordering race):** the SubagentStart frame **carries `parent_id`** for server-side parent→feature/cycle correlation rather than resolving the parent cycle eagerly plugin-side. This resolves the child-before-parent race by construction (the link is never dropped). A delivery-time PoC may later add bounded plugin-side pre-resolution without changing this wire contract.
- **FLAT `extra` frame (serde flatten reconciliation):** the C1 pseudocode nested tool/agent fields under an `extra: {}` object; the Rust `HookInput.extra` is `#[serde(flatten)]`, so the emitted JSON must be **flat** (fields at top level land in `extra` on the Rust side, matching `hook.rs input.extra.get(...)`). A nested `extra` would deserialize to `extra.extra` and silently drop tool/agent attribution. `provider`/`model_id` ride the CLI flags, not the frame (per wire.rs).
- **Spoof-safety by construction (ADR-007/R-07):** `agent_type` is only ever set from the harness-validated `session.agent`; tool-execution handlers never lift `agent_type` from tool args, so a spoofed `agent` in args lands harmlessly inside `tool_input`.

## Constraints honored

- Fail-open (NFR-02/R-13): every emit and every handler wrapped; binary-absent / socket-down / API-drift → dropped event, never a throw or block.
- R-15: model charset-validated before shelling; untrusted values passed as discrete args / stdin bytes (Bun escapes each array element), never string-built.
- Modularity: largest file 237 lines (cap 500). Size gate (`check-hook-client-size.js`) unaffected — this dir is not gated; hook-client unchanged and within budget.
- SubagentStart injection deliberately NOT attempted (structurally unreachable in OpenCode — accepted parity gap, FR-12); observe frame emitted.

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_search` (category `pattern` and `decision`) + `context_get` #5744 (ADR-007 subagent observe channel) and #5746 (ADR-009 degraded legs). Surfaced ADR-007, ADR-009, #5737 (three-touchpoint harness onboarding), #5749 (domain-pack sentinel), #4800 (fail-open masks event loss). Applied ADR-007/009 directly to the subagent and degraded-leg design.
- **Stored:** entry **#5755** — "Claude-shaped HookInput frames must be FLAT — a nested extra:{} misfiles under serde(flatten)" via `/uni-store-pattern` (topic `opencode-plugin`). Captures the silent-misattribution trap, the `provider`/`model_id` CLI-only fact, and the spoof-safety-by-construction corollary.
