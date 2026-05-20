## ADR-001: Add `max_depth: Option<u8>` to `GraphParams`

### Context

`GraphParams` is locked by ADR-003 vnc-018: the struct layout is a wire contract and
field removal or retyping is a breaking change. That ADR explicitly locks the fields
present at vnc-018 delivery and lists the forward-compat stubs (`seed_ids`, `max_nodes`,
`from_id`, `to_id`) for future modes.

`max_depth` is not in that locked set — it was identified as missing in SCOPE.md OQ-01.
The subgraph mode needs a BFS depth parameter: default 3, range [1, 10]. Two options:

(A) Add `max_depth: Option<u8>` to `GraphParams` — consistent with the forward-compat
    pattern, one field in the shared struct, validated in `validate_no_unsupported_params`.

(B) Accept `max_depth` as a nested structure or a separate query parameter not in
    `GraphParams` — adds complexity with no benefit; inconsistent with existing depth
    parameter on neighbors mode.

ADR-003 vnc-018 Consequences section explicitly states: "Struct layout is a wire
contract; field removal is a breaking change requiring ADR update." It does not prohibit
adding new `Option<T>` fields — which are backward-compatible (callers omitting the
field receive the `None` default, which maps to default behavior). This is the
forward-compat pattern the struct was designed for.

`validate_no_unsupported_params` must be updated to reject `max_depth` on `chain`,
`current`, and `neighbors` modes with an error message naming the supporting mode,
exactly as `seed_ids` and `max_nodes` are rejected today. SCOPE.md AC-06 resolves the
validation behavior.

### Decision

Add `max_depth: Option<u8>` to `GraphParams` in `graph_read.rs`.

Field doc comment: "subgraph mode only: BFS max depth 1..=10 (default 3 when absent).
Error if passed to chain, current, or neighbors modes."

Update `validate_no_unsupported_params`:
- `"chain"`, `"current"`, `"neighbors"` arms: reject `max_depth` with message
  `"max_depth is not supported in {mode} mode — use subgraph mode (#597)"`.
- `"subgraph"` arm (new): permits `seed_ids`, `max_nodes`, and `max_depth`; rejects
  `from_id` and `to_id` (path mode only).

Validation in `handle_subgraph`:
- `max_depth.unwrap_or(3)` — default 3.
- Range check: `if depth == 0 || depth > 10` → validation error:
  `"max_depth must be in range 1..=10, got {depth}"`.

Update the unrecognized-mode error in `validate_no_unsupported_params` to include
`subgraph` in the supported-modes list:
`"unrecognized mode '{x}' — supported modes: chain, current, neighbors, subgraph"`.

### Consequences

Easier: subgraph BFS depth is caller-controlled within a documented range. Validation
is centralized in one function. Unrecognized-mode error is self-documenting.

Harder: `GraphParams` grows by one field. Any test that serializes a `GraphParams` to
JSON and checks field exhaustiveness must be updated. The wire struct is now one field
wider for all callers regardless of mode used.
