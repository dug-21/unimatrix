# Test Plan — get-params (`GetParams.include_edges`)

`GetParams` gains `#[serde(default)] include_edges: Option<bool>`. Additive, backward-compatible.
Owns **R-14 (param resolution)** and **NFR-4 (backward compat)**. The opt-out *behavior* (queries
skipped, no `edges` key) is asserted in get-edge-assembly; this component owns the **field
contract**. Server unit + integration tests.

## Unit Test Expectations

### FR-2 / AC-11 — Additive, backward-compatible field

**`test_get_params_deserializes_absent_field`** (NFR-4)
A `GetParams` payload with **no** `include_edges` key deserializes with `include_edges == None`
(`#[serde(default)]`) → resolves default-on. A pre-vnc-037 caller behaves unchanged.

**`test_get_params_three_values`**
`include_edges` deserializes correctly for absent (`None`), `true` (`Some(true)`), `false`
(`Some(false)`).

**`test_get_params_no_existing_field_removed_or_retyped`** (NFR-4)
Assert existing fields (`id`, `agent_id`, `format`, `feature`, `helpful`, `session_id`) are
unchanged in name and type — the field is purely additive.

### Resolution semantics (D-01)

**`test_include_edges_resolution`**
`None` ⇒ surface; `Some(true)` ⇒ surface; `Some(false)` ⇒ suppress. (The surface/suppress
**effect** — query skip + key absence — is in get-edge-assembly; here assert the resolve function
maps the three input values to the correct surface/skip decision.)

## Integration Expectations (through MCP)
- `test_get_default_on_no_param` (AC-01) — omitting `include_edges` surfaces edges.
- `test_get_include_edges_opt_out` (AC-11) — `include_edges:false` suppresses.
- `test_get_include_edges_true` — explicit `true` surfaces.
- Backward-compat: existing `context_get` tests (no `include_edges`) still pass (default-on does
  not break them beyond the additive `edges` key, which they do not assert against).

## Edge Cases
- Absent field → default-on (the backward-compat invariant).
- Explicit `false` → suppress.
- Malformed value (non-bool) → serde rejects per existing param-validation behavior.

## Security
- `include_edges` is a bounded `Option<bool>` — no injection surface.
