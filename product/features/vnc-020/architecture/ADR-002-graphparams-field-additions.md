## ADR-002: GraphParams Field Additions and Backward Compatibility

### Context

`GraphParams` is a wire-contract struct locked by ADR-003 vnc-018: field removal and
retyping are prohibited breaking changes. The lock explicitly permits `Option<T>`
additions because absent fields deserialize as `None` — callers omitting them receive
default behavior unchanged.

vnc-020 requires 8 new fields not present in the post-vnc-019 struct:
- `category: Option<String>` — entry category filter (inverse, filter)
- `missing_edge_types: Option<Vec<String>>` — antijoin target types (inverse)
- `limit: Option<u32>` — result count cap (inverse, filter; default 100, max 500)
- `min_age_days: Option<u32>` — property filter on created_at (filter)
- `min_confidence: Option<f64>` — lower confidence bound (filter)
- `max_confidence: Option<f64>` — upper confidence bound (filter)
- `min_edge_count: Option<u32>` — minimum outgoing edge count (filter)
- `max_edge_count: Option<u32>` — maximum outgoing edge count (filter)

`from_id: Option<u64>` and `to_id: Option<u64>` are already present as forward-compat
stubs from vnc-018; no new field is needed for path mode endpoints.

`depth: Option<u8>` is already present and reused by path mode (same hop-limit semantics
as neighbors mode — see ADR-004).

`edge_types: Option<Vec<String>>` is already present and reused by path and filter modes.

The `validate_no_unsupported_params` function must be updated to reject each new field
on every non-owning mode with a message naming the correct mode. SR-08 flags the
combinatorial rejection surface as a medium risk: a missed rejection entry results in
a wrong-mode handler silently receiving a param it was not designed to handle.

### Decision

Add all 8 new fields to `GraphParams` as `Option<T>` with doc comments naming the owning
mode(s). No fields are removed, renamed, or retyped.

`validate_no_unsupported_params` is updated with a full rejection matrix (documented in
ARCHITECTURE.md §Param/Mode Rejection Matrix). Every new field is rejected on every
non-owning mode arm. The `chain`, `current`, `neighbors`, and `subgraph` arms each gain
8 new rejection clauses. The three new arms (`inverse`, `filter`, `path`) each accept
only their own fields and reject everything else.

`limit` defaults to 100 when absent (not 0, not unlimited). Valid range [1, 500];
out-of-range values produce a validation error stating the allowed range.

`min_edge_count` and `max_edge_count` each require at least one `edge_types` entry when
present; this is validated in `handle_filter`, not in `validate_no_unsupported_params`
(it is a semantic constraint, not a mode-routing constraint).

### Consequences

Easier: All callers of prior modes are unaffected — new fields absent = `None` = no
change in behavior. Wire contract remains additive. Spec writer has a complete rejection
matrix to derive ACs from.

Harder: `GraphParams` grows by 8 fields (was 13 fields post-vnc-019, now 21). Each
existing arm in `validate_no_unsupported_params` gains up to 8 new rejection clauses,
increasing the function's line count. Any test checking `GraphParams` exhaustively
must be updated.
