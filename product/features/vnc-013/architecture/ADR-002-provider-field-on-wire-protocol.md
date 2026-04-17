## ADR-002: Explicit `provider` Field on Wire Protocol (HookInput + ImplantEvent)

### Context

Provider identity must be threaded from the hook binary into `listener.rs` so that
`source_domain` can be derived correctly at the write path. Two approaches were
considered:

**Option A — Inference-only**: After normalization, infer provider from which mapping
arm translated the event name. `"BeforeTool"` → infer `"gemini-cli"`. Shared names
(`PreToolUse`, `PostToolUse`) → infer `"claude-code"` by default. Problems:
(1) Codex shares all Claude Code event names — inference produces wrong provider for
Codex without a `--provider` flag. (2) Inference logic must be duplicated at any site
that needs provider identity. (3) Future providers with shared event names cannot be
distinguished at all without a flag, but the flag has nowhere to write its value.

**Option B — Explicit `provider` field on `HookInput` and `ImplantEvent`**: Add
`provider: Option<String>` to both structs with `#[serde(default)]`. The hook binary
populates `HookInput.provider` from either the `--provider` CLI flag or inference from
the event name (for Gemini-unique names). `ImplantEvent.provider` is set from
`HookInput.provider` when each event is constructed in `build_request()`. `listener.rs`
reads `event.provider` directly — no inference at the listener. This is the recommended
approach from the SCOPE.md (Layer 1 / Layer 3 analysis).

Option B is preferred by the existing SCOPE.md analysis and by the design principle
that provider identity should be an explicit wire-protocol field rather than an
inference rule that may silently mislabel events.

### Decision

Add `provider: Option<String>` to `HookInput` and `ImplantEvent` in
`unimatrix-engine/src/wire.rs`. Both fields use `#[serde(default)]` for backward
compatibility. `ImplantEvent.provider` additionally uses
`#[serde(skip_serializing_if = "Option::is_none")]` to avoid expanding existing wire
frames for Claude Code events that omit the flag.

The `run()` function in `hook.rs` is extended with a `provider: Option<String>`
parameter sourced from the new `--provider` CLI argument on the `Hook` subcommand
in `main.rs`. After `normalize_event_name()` returns `(canonical, inferred_provider)`:
- If `--provider` was supplied, its value is used as the provider string.
- Otherwise, `inferred_provider` from `normalize_event_name()` is used.
This value is written to `HookInput.provider` before `build_request()` and propagated
into each `ImplantEvent.provider` constructed within `build_request()`.

`listener.rs` Site A derives `source_domain` as:
```rust
event.provider.clone().unwrap_or_else(|| "claude-code".to_string())
```
No inference logic at the listener — provider identity travels on the wire.

### Consequences

Easier: `source_domain` derivation on the live write path is O(1) field access;
future providers add a new `--provider` value and a normalization arm in
`normalize_event_name()` — no listener changes; Codex events are correctly attributed
when the config includes `--provider codex-cli`; the rework detection gate can use
`provider == "claude-code"` as a reliable predicate.

Harder: `ImplantEvent` acquires a new optional field that must be populated by every
site that constructs it. Sites inside `build_request()` and `build_cycle_event_or_fallthrough()`
must each thread `provider` through. If any construction site is missed, `provider` is
`None` and falls back to `"claude-code"` — a silent mislabel rather than a loud error.
The `debug_assert!(event.provider.is_some(), "ImplantEvent.provider must be set at normalization boundary")` in `listener.rs` serves as a canary.
