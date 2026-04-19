# Agent Report: vnc-013-agent-3-wire-protocol

## Task

Implement the wire-protocol component for vnc-013 (Canonical Event Normalization for Multi-LLM Hook Providers).

## Files Modified

- `crates/unimatrix-engine/src/wire.rs` — primary implementation
- `crates/unimatrix-server/src/main.rs` — `--provider` CLI argument
- `crates/unimatrix-server/src/uds/hook.rs` — `run()` signature extension + struct literal fixes
- `crates/unimatrix-server/src/uds/listener.rs` — struct literal fixes only (no behavioral changes)

## Changes Made

### wire.rs

Added to `HookInput` (before the `extra` flatten, as specified):
- `provider: Option<String>` with `#[serde(default)]`
- `mcp_context: Option<serde_json::Value>` with `#[serde(default)]`

Added to `ImplantEvent` (after `topic_signal`):
- `provider: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`

`HookInput` already had `#[derive(Deserialize, Debug, Clone)]` — no change needed.

Added 11 new unit tests per the wire-protocol test plan (all passing):
- `test_hook_input_deserializes_without_new_fields`
- `test_hook_input_deserializes_with_provider_field`
- `test_hook_input_deserializes_gemini_payload_with_mcp_context`
- `test_hook_input_mcp_context_non_object_deserializes`
- `test_mcp_context_not_duplicated_in_extra`
- `test_implant_event_deserializes_without_provider`
- `test_implant_event_provider_present_serializes`
- `test_implant_event_provider_none_not_serialized`
- `test_hook_input_provider_none_when_absent`
- `test_hook_input_clone_includes_new_fields`

### main.rs

Added `provider: Option<String>` with `#[arg(long)]` to the `Hook` command variant. Updated dispatch to pass `provider` to `hook::run()`.

### hook.rs

Extended `run()` signature to `run(event: String, provider: Option<String>, project_dir: Option<PathBuf>)`. Provider is accepted but stubbed with `let _ = &provider;` — normalization wiring is Wave 2 (normalization component).

Fixed `HookInput` and `ImplantEvent` struct literals throughout tests to include new fields. The `parse_hook_input` error fallback path also updated.

### listener.rs

Added `provider: None` to all `ImplantEvent` struct literals in test code. No behavioral changes.

## Test Results

- `cargo test -p unimatrix-engine`: all tests pass (11 new vnc-013 tests + all pre-existing)
- `cargo test -p unimatrix-server --lib`: 2885 passed, 0 failed
- `cargo build --workspace`: zero errors

## Design Decisions / Deviations

None. Implementation follows pseudocode exactly. The `provider` parameter in `run()` is accepted and stubbed for Wave 2 rather than being silently dropped — this allows main.rs to pass it through without a dead_code warning while the normalization component wires it up.

The `let _ = &provider` suppression is intentional and documented in a comment. It will be removed when the normalization agent implements Wave 2.

## Commit

`99dec56d` — `impl(wire-protocol): add provider and mcp_context fields to HookInput/ImplantEvent; extend Hook CLI with --provider (#567)`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 19 entries; entry #3255 (serde(default) alone does not omit None on serialization) and #1898 (UDS hook protocol) were relevant confirmations. No surprises from briefing.
- Stored: entry #4313 "Adding Option<T> fields to ImplantEvent or HookInput requires touching every struct literal in hook.rs, listener.rs, and wire.rs — grep first to enumerate all sites" via /uni-store-pattern. The blast radius of struct literal updates across ~25 sites in two files was the non-obvious part that future agents should know before starting.
