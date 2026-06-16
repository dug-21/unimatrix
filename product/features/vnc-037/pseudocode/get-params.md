# Component: get-params

## Purpose

Add the additive `include_edges: Option<bool>` field to `GetParams` and set the resolution
semantics (`None`/`Some(true)` ⇒ surface; `Some(false)` ⇒ suppress). Default-on at the type level
keeps the agent-facing MCP tool surfacing edges; internal/programmatic by-ID callers pass
`Some(false)` so they pay zero edge-query cost (FR-2/D-01, AC-11; OQ-03 internal-caller opt-out).

## Location

- `crates/unimatrix-server/src/mcp/tools.rs` — `GetParams` struct (`:243`); resolution consumed by
  the `context_get` handler (`:935`, see get-edge-assembly).
- Internal by-ID call sites (enumerated below) — each passes `include_edges: Some(false)`.

## Modified Type

```
struct GetParams {
    id: i64                          // existing
    agent_id: Option<String>         // existing
    format: Option<String>           // existing
    feature: Option<String>          // existing
    helpful: Option<bool>            // existing
    session_id: Option<String>       // existing  (#[serde(default)])
    include_edges: Option<bool>      // NEW — #[serde(default)] so a pre-vnc-037 caller (no field) ⇒ None ⇒ default-on
}
```

> `#[serde(default)]` makes the field backward-compatible: an omitted field deserializes to `None`.
> No existing field is removed or retyped (NFR-4). Add a `#[schemars(...)]` doc comment describing
> the three-state semantics for the MCP tool schema.

## Resolution Semantics (consumed in get-edge-assembly)

```
match params.include_edges {
    None       => surface,     // DEFAULT-ON — the agent-facing point of the feature
    Some(true) => surface,     // explicit opt-in
    Some(false)=> suppress,    // skip ranked select + count + title join; edges = None (FR-3)
}
```

## Internal-caller opt-out (OQ-03 — each asserted as a named test)

Programmatic by-ID fetches that never present the next-hop affordance to a reading agent SHALL pass
`include_edges: Some(false)` so they behave exactly as pre-vnc-037 (zero added query cost). The
agent-facing `context_get` MCP tool stays default-on (`None`). Enumerated sites to set `Some(false)`:

- the **hook / write-back path** by-ID fetch,
- the **briefing pipeline's by-ID fetches**,
- any **by-ID loop fetch** (bulk machine reads).

> These are the sites named in SCOPE OQ-03 / ARCHITECTURE OQ-03. The implementer locates the exact
> call sites; each gets an asserted test that it passes `Some(false)` (R-14.3). **No agent-facing
> path is flipped default-off** (WARN-1 in the alignment report — that would weaken the
> proactive-delivery loop). If a call site cannot be located or is ambiguous, flag it rather than
> guessing (see Open Questions in the return summary).

## Constraints honored

- **NFR-4**: additive `Option<T>`; backward-compatible; no existing field changed.
- **FR-3**: `Some(false)` resolution drives the handler to skip all edge queries.
- **WARN-1 (alignment)**: tool boundary stays default-on; only internal callers opt out.

## Data Flow

- **Inputs**: deserialized MCP params (or internal struct construction).
- **Outputs**: `include_edges` consumed by the handler's resolution match (get-edge-assembly).

## Error Handling

- None — a bounded `Option<bool>`, no injection surface, no fallible parse beyond serde default.

## Key Test Scenarios

- **all three resolutions (AC-11, R-14.2)** — `None` and `Some(true)` surface edges; `Some(false)`
  suppresses (no `edges` key) and skips the queries (query-count/instrumentation).
- **backward-compat (FR-2, AC-11)** — a params payload with no `include_edges` field deserializes
  to `None` ⇒ default-on (pre-vnc-037 caller behaves as default-on).
- **internal-caller opt-out (OQ-03, R-14.3)** — each enumerated internal call site passes
  `Some(false)`; asserted per site by name, not assumed.
- **no agent-facing default-off (WARN-1)** — assert the MCP `context_get` tool path leaves
  `include_edges` at `None` (default-on).
