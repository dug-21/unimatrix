## ADR-005: Extract Edge-Write Logic into edge_write.rs Module

### Context

`tools.rs` is 8209 lines. The 500-line per-file rule in `rust-workspace.md` applies to new
modules and to code added to existing files. Both `context_store` and `context_correct` handlers
would need edge validation and write logic added to them. Inlining in both handlers would:
1. Add ~80-120 lines of edge logic to each handler (duplicated or via closure), further growing
   an already very large file.
2. Make the edge write logic hard to test in isolation.
3. Violate the principle of single-responsibility extraction.

The SCOPE.md Constraints section explicitly states: "the edge-write logic should be extracted to
a helper function, not inlined in each handler."

SR-04 from the scope risk assessment identifies this as a medium risk: "the 500-line file limit
may be breached by inlining edge-write logic in both context_store and context_correct handlers."
The target module location was flagged as unspecified.

### Decision

Create `crates/unimatrix-server/src/mcp/edge_write.rs` as a `pub(crate)` module within the
`unimatrix-server` crate. This module:
- Defines `EdgeInput` (deserialization struct)
- Defines `EdgeValidationError` (error enum)
- Defines `EDGE_SOURCE_AGENT` constant
- Implements `validate_and_write_edges()` (the primary entry point for both handlers)

`edge_write.rs` is declared as `pub(crate) mod edge_write;` in `mcp/mod.rs` (or `tools.rs`
top-level module declaration, per the existing module layout in `unimatrix-server`).

`EdgeInput` is also referenced in `tools.rs` for the `StoreParams` and `CorrectParams` struct
fields. The import is `use crate::mcp::edge_write::EdgeInput;` in `tools.rs`.

The module does NOT import or reference `context_store` or `context_correct` — dependency is
one-way (handlers → edge_write). `write_graph_edge` remains in `nli_detection.rs` as the
low-level DB write function; `edge_write.rs` calls it.

`EDGE_SOURCE_AGENT` follows the naming convention established by `EDGE_SOURCE_NLI` (col-029,
entry #3591) and `EDGE_SOURCE_CO_ACCESS` (crt-034). The constant is defined in `edge_write.rs`
and re-exported from `unimatrix-store/src/lib.rs` for SQL filter use by graph cohesion metrics
if needed in future (following the EDGE_SOURCE_NLI re-export pattern).

### Consequences

Easier: edge-write logic is testable in isolation. Both handlers call the same function.
The 500-line rule is respected for the new module. Future edge write extensions (batching,
additional edge types) are confined to `edge_write.rs`.

Harder: one additional file to track and review. `EdgeInput` must be imported in `tools.rs`
rather than defined inline, which adds a small indirection. The module boundary means
`edge_write.rs` cannot access any private helpers in `tools.rs` — all its dependencies must be
passed as parameters.

Supersedes: none.
Related: ADR-001 (validation order), ADR-002 (confidence posture), ADR-003 (partial-write).
