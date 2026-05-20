## ADR-001: Module Split Strategy — Three Sibling Handler Modules

### Context

`graph_read.rs` is 387 lines post-vnc-019. Adding wire types, response envelopes,
validation expansion, and dispatch for three new modes projects the file to approximately
500 lines — at the workspace 500-line limit. All three mode handlers (inverse, filter,
path) would add significant logic:

- `handle_inverse`: SQL builder with N dynamic LEFT JOINs + result mapping (~80 lines).
- `handle_filter`: SQL builder with dynamic WHERE fragments + two correlated subqueries
  (~110 lines).
- `handle_path`: BFS loop with path-carrying frontier + endpoint resolution (~120 lines).

Placing any handler logic inline in `graph_read.rs` would breach the 500-line limit and
mix concerns (wire types + routing + business logic). The existing vnc-019 precedent
(`graph_read_subgraph.rs`) already established the sibling module pattern with `#[path]`
declaration.

SR-03 from the scope risk assessment explicitly requires the split decision to be made
at design time, not during delivery.

### Decision

Three sibling modules, each in a new file declared via `#[path]` in `graph_read.rs`:

- `graph_read_inverse.rs` — `handle_inverse` and SQL antijoin builder.
- `graph_read_filter.rs` — `handle_filter` and correlated subquery builder.
- `graph_read_path.rs` — `handle_path`, BFS loop, and endpoint resolution.

`graph_read.rs` retains all wire types (`GraphParams`, response envelopes), the
`handle_graph` entry point, and `validate_no_unsupported_params`. No handler logic
lives in `graph_read.rs`.

Each sibling module imports shared infrastructure from `graph_read_neighbors.rs` via
`super::graph_read_neighbors::{follow_to_current, all_non_supersedes_types}` —
identical to the pattern used by `graph_read_subgraph.rs`. This import path works
because all modules are `#[path]`-declared submodules of `graph_read.rs` and share
the same `super` scope.

`validate_no_unsupported_params` remains in `graph_read.rs` as the single authoritative
rejection point for all cross-mode parameter misuse. It is not split across modules.
This preserves the invariant from ADR-003 vnc-018: the unrecognized-mode error fires
before any field check, and each field's rejection message names the correct mode.

### Consequences

Easier: `graph_read.rs` stays within 500 lines. Each module has a single
responsibility. Adding a future fourth mode is a predictable pattern. Validation
remains centralized.

Harder: Three new files to create and test. Import paths for `follow_to_current` and
`all_non_supersedes_types` are slightly indirect (via `super::graph_read_neighbors`).
