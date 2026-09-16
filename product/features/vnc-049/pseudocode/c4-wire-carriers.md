# C4 — Wire carriers (`model_id` on `HookInput` + `ImplantEvent`)

**Location:** `crates/unimatrix-engine/src/wire.rs` (237 code-lines, under cap)
**ADRs:** ADR-002 (model_id carrier). **Risks:** R-10 (blast radius), R-15 (charset), R-01 (must reach INSERT).
**Wave:** 1 (foundation — every downstream component carries this field).

## Purpose

Add a first-class `model_id` wire field so the OpenCode backend model (`ollama/qwen3-coder`) travels
plugin → `HookInput` → `ImplantEvent` → persistence, enabling AC-06 local-vs-cloud distinctness. Mirror
the existing `provider` field exactly so serde back-compat and the ts-rs drift gate hold.

## Modified structs

### `HookInput` (`wire.rs:57`)
Add after `provider` (`:84`), before `mcp_context`:
```
/// Backend model identity, format "<providerID>/<modelID>" (e.g. "ollama/qwen3-coder").
/// Populated by hook::run() from the --model CLI arg (like `provider`), NOT from stdin JSON.
/// #[serde(default)] so existing claude-code/gemini/codex hook JSON (which omits it) → None.
#[serde(default)]
pub model_id: Option<String>
```
- `#[serde(default)]` only (NOT skip_serializing_if — HookInput is deserialize-only, `#[derive(Deserialize)]`).
- ts-rs: struct already derives `TS` under `cfg(test)` (`:55`). New field auto-exports; regenerate bindings.

### `ImplantEvent` (`wire.rs:251`)
Add after `provider` (`:276`):
```
/// Backend model identity propagated from HookInput.model_id through normalization.
/// None for events predating vnc-049 or without a model (cloud default). Mirrors `provider`.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub model_id: Option<String>
```
- Same attrs as `provider` (`:275`): `#[serde(default, skip_serializing_if="Option::is_none")]` — a frame
  without `model_id` stays byte-identical on the wire (frozen-fixture safety, R-10.2).

## Data flow / transformations

- IN: `--model <providerID>/<modelID>` CLI arg (C1) → `hook::run()` sets `HookInput.model_id` (C2).
- THROUGH: every `ImplantEvent` construction site copies `hook_input.model_id → event.model_id`
  (mirror the `provider` copy). **Missed sites → NULL model_id, canaried like provider** (ADR-002 §5).
- OUT: `ImplantEvent.model_id` consumed by C5 write path.

## Validation (R-15) — flag & define

`model_id` value is `"<providerID>/<modelID>"` and **contains `/`**, so it does NOT satisfy the
`source_domain` contract `^[a-z0-9_-]{1,64}$`. Define a distinct carrier charset:
```
fn is_valid_model_id(s: &str) -> bool:
    1..=128 chars; each char in [a-z0-9._/-]; not empty; reject control chars
```
- Enforcement site: validate at the ingest boundary (C2 `hook::run` after reading the flag, and/or C5
  before bind). Invalid → drop to `None` + `tracing::warn!` (fail-open, never pass raw to SQL).
- **Open decision for delivery:** whether validation lives in C4 (helper next to the field) or C2/C5.
  Pseudocode places the helper here; call site is C2. Flagged, not silently assumed.

## Error handling

- Deserialization: absent field → `None` (serde default). No error path added.
- No panics; no `.unwrap()` in non-test code.

## Key test scenarios (hints for tester)

1. **ts-rs drift gate green** (R-10.1, vnc-024 #4726): regenerate bindings; `HookInput.ts` /
   `ImplantEvent.ts` include `model_id`; drift check passes.
2. **serde back-compat** (R-10.2): a `HookInput` / `ImplantEvent` JSON frame WITHOUT `model_id`
   deserializes to `model_id == None` (no error).
3. **round-trip**: `ImplantEvent { model_id: Some("ollama/qwen3-coder") }` serializes with the field;
   `{ model_id: None }` omits it (skip_serializing_if).
4. **charset**: `is_valid_model_id("ollama/qwen3-coder")` true; `"a;drop"` / `""` / 200-char string false.
5. **crossing** (deferred to C5/C8): field survives wire→ImplantEvent→INSERT (lesson #5670).
</content>
