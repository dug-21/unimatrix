## ADR-003: GraphParams Struct Layout and Forward-Compat Validation

### Context

`context_graph` is designed in three delivery increments: vnc-018 (chain, current,
neighbors), #597 (subgraph), and #598 (path, inverse, filter). Fields needed by
future modes are present in `GraphParams` from day one to avoid a breaking wire
contract change when those increments ship.

SR-03 (scope risk) flags this as a high risk: if these fields are defined but
accepted silently when passed to unsupported modes, agents will not know they are
being ignored. If the struct layout is later found inadequate for #597/#598, a type
change is a breaking change.

Two validation strategies were considered:

**Option A — Per-mode guards in each handler**
Each of `handle_chain`, `handle_current`, `handle_neighbors` checks for unsupported
fields and returns an error. Adding a new forward-compat field in the future requires
updating each handler.

**Option B — Centralized validation function**
A single `validate_no_unsupported_params(&GraphParams)` called at the top of
`handle_graph`, before mode dispatch. The function's `match` on `params.mode` allows
the `"subgraph"` arm (when added) to permit `seed_ids` without changing the
`"neighbors"` arm. Adding a new field touches exactly one function.

### Decision

The `GraphParams` struct layout is locked as follows:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphParams {
    /// Traversal mode: "chain" | "current" | "neighbors"
    /// (future: "subgraph" | "path" | "inverse" | "filter")
    pub mode: String,

    /// Agent making the request.
    pub agent_id: Option<String>,

    /// Response format: "summary" | "markdown" | "json".
    pub format: Option<String>,

    // ── chain, current, neighbors ──────────────────────────────────────────
    /// Entry ID to traverse from. Required for chain, current, neighbors.
    pub id: Option<u64>,

    // ── chain ──────────────────────────────────────────────────────────────
    /// Traversal direction: "forward" (descendants) | "backward" (ancestors) | "both" (default).
    pub direction: Option<String>,

    // ── neighbors ──────────────────────────────────────────────────────────
    /// Edge types to traverse. Empty or absent = all types except Supersedes.
    pub edge_types: Option<Vec<String>>,

    /// Traversal depth 1..=10, default 1.
    pub depth: Option<u8>,

    /// If true, substitute deprecated endpoints with their terminal active successor.
    pub resolve_supersessions: Option<bool>,

    // ── Forward-compat: subgraph mode (#597) ──────────────────────────────
    /// Multi-seed BFS starting points (subgraph mode only).
    /// Error if passed to chain, current, or neighbors.
    pub seed_ids: Option<Vec<u64>>,

    /// Node cap for subgraph BFS (default 200 when subgraph mode ships).
    /// Error if passed to chain, current, or neighbors.
    pub max_nodes: Option<u32>,

    // ── Forward-compat: path mode (#598) ──────────────────────────────────
    /// Path source entry (path mode only).
    /// Error if passed to chain, current, or neighbors.
    pub from_id: Option<u64>,

    /// Path target entry (path mode only).
    /// Error if passed to chain, current, or neighbors.
    pub to_id: Option<u64>,
}
```

Option B (centralized validation) is used. `validate_no_unsupported_params` is
called as the first action in `handle_graph`, before capability check and before
mode dispatch. Error messages are explicit about which mode supports the field:

- `seed_ids`: `"seed_ids is not supported in {mode} mode — use subgraph mode (#597)"`
- `max_nodes`: `"max_nodes is not supported in {mode} mode — use subgraph mode (#597)"`
- `from_id`: `"from_id is not supported in {mode} mode — use path mode (#598)"`
- `to_id`: `"to_id is not supported in {mode} mode — use path mode (#598)"`

An unrecognized `mode` value falls through to an error listing supported modes:
`"unrecognized mode '{x}' — supported modes: chain, current, neighbors"`.

**Forward-compat testing requirement** (SR-03): vnc-018 delivery must include a unit
test that passes `seed_ids` to `mode="neighbors"` and asserts the error response.
This ensures the validation path is exercised before #597 ships, not discovered
missing at that point.

### Consequences

Easier: #597 delivery adds the `"subgraph"` arm to `validate_no_unsupported_params`
and implements the handler. No struct changes, no wire protocol changes. Agents
integrating in vnc-018 discover unsupported-field errors immediately rather than
building on silent-ignore behavior that breaks when the field becomes meaningful.

Harder: centralized validation means unrecognized future modes produce "unrecognized
mode" errors rather than "unsupported field" errors — the ordering matters. The
`validate_no_unsupported_params` function's `match` arm must include `_` →
`unrecognized mode` as the fallthrough, ensuring unknown modes are caught before any
field checks run. Delivery agent must preserve this ordering.

The struct layout is a wire contract. Any field added after vnc-018 ships is
automatically optional (via `Option<T>`) for backward compatibility. Any field
removed is a breaking change requiring an ADR update.
