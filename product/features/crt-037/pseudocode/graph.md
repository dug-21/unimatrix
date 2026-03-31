# Component: graph.rs (unimatrix-engine)

## Purpose

Extend `RelationType` with a sixth variant `Informs`. Update `as_str()` and `from_str()`
to include the new variant. Update the module doc comment. No other logic changes —
`graph_penalty` and `find_terminal_active` remain untouched and do not traverse `Informs`.

Wave 1. No I/O. Pure changes only.

## Files Modified

`crates/unimatrix-engine/src/graph.rs`

## New/Modified Sections

### RelationType enum — add Informs variant

Current enum has five variants. Add sixth:

```
enum RelationType {
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,
    Informs,        // NEW (crt-037): empirical→normative cross-feature bridge; positive PPR
}
```

`Informs` is a positive edge type. Penalty traversal functions (`graph_penalty`,
`find_terminal_active`) are unchanged and continue to filter exclusively to `Supersedes`
via `edges_of_type`. `Informs` is invisible to penalty logic (SR-01, C-06, FR-13).

### as_str() — add Informs arm

Current implementation has five match arms. Add one:

```
fn as_str(&self) -> &'static str:
    match self:
        Supersedes  => "Supersedes"
        Contradicts => "Contradicts"
        Supports    => "Supports"
        CoAccess    => "CoAccess"
        Prerequisite => "Prerequisite"
        Informs     => "Informs"     // NEW
```

The string `"Informs"` is the canonical relation_type string stored in GRAPH_EDGES.
This string must match the literal passed to `write_nli_edge` in `nli_detection_tick.rs`.
Case-sensitive. No truncation or aliasing.

### from_str() — add Informs arm

Current implementation has five match arms and a wildcard `_ => None`. Add one arm:

```
fn from_str(s: &str) -> Option<Self>:
    match s:
        "Supersedes"  => Some(Supersedes)
        "Contradicts" => Some(Contradicts)
        "Supports"    => Some(Supports)
        "CoAccess"    => Some(CoAccess)
        "Prerequisite" => Some(Prerequisite)
        "Informs"     => Some(Informs)     // NEW
        _             => None
```

With this arm present, `build_typed_relation_graph`'s R-10 guard no longer fires for
`"Informs"` rows — the row is accepted and added to the graph (FR-02, AC-03, AC-04).

### Module doc comment — update line 16

The doc comment at the top of `graph.rs` lists `Supports`, `CoAccess`, `Prerequisite` as
examples of non-Supersedes edge types (line 16). Add `Informs` to that list:

```
// Before:
// `graph_penalty` and `find_terminal_active` filter exclusively to Supersedes edges
// via `edges_of_type`. Non-Supersedes edges (CoAccess, Contradicts, Supports, Prerequisite)
// are present in the graph but invisible to all penalty logic (SR-01 mitigation).

// After:
// `graph_penalty` and `find_terminal_active` filter exclusively to Supersedes edges
// via `edges_of_type`. Non-Supersedes edges (CoAccess, Contradicts, Supports, Prerequisite,
// Informs) are present in the graph but invisible to all penalty logic (SR-01 mitigation).
```

The doc comment on the `RelationType` enum itself (line 73) also says "Five edge types".
Update to "Six edge types" and add `Informs` to the explanatory note:
```
// Before:
/// Five edge types covering the full relationship taxonomy.
///
/// `Prerequisite` is reserved for W3-1; no write path exists in crt-021.

// After:
/// Six edge types covering the full relationship taxonomy.
///
/// `Prerequisite` is reserved for W3-1; no write path exists in crt-021.
/// `Informs` bridges empirical knowledge (lesson-learned, pattern) from earlier feature
/// cycles to normative knowledge (decision, convention) in later cycles (crt-037).
```

## State Machines

None. `RelationType` is a pure enum with no lifecycle state.

## Data Flow

Input: `&str` via `from_str`. Output: `Option<RelationType>`.
Input: `&RelationType` via `as_str`. Output: `&'static str`.

No I/O. No side effects.

## Error Handling

`from_str` returns `None` for unrecognized strings. Callers in `build_typed_relation_graph`
check `is_none()` and emit `tracing::warn!` then skip — existing behavior unchanged. The
`Informs` arm makes `"Informs"` recognized, so the warn guard does not fire for it (AC-04).

`as_str` is infallible — all variants are covered.

## Key Test Scenarios

AC-01: `RelationType::from_str("Informs")` returns `Some(RelationType::Informs)`.

AC-02: `RelationType::Informs.as_str()` returns the string `"Informs"` exactly.

AC-03: `build_typed_relation_graph` called with a `GraphEdgeRow` where
`relation_type = "Informs"` includes that edge in the output graph — the edge is present
in `graph.inner` with `relation_type == "Informs"`.

AC-04: `build_typed_relation_graph` with `relation_type = "Informs"` does NOT emit a
`tracing::warn` message. Capture log output and assert no WARN mentioning "Informs".

AC-24: `graph_penalty` called on a graph with only an `Informs` edge returns
`FALLBACK_PENALTY` (no penalty contribution from `Informs`). `find_terminal_active`
returns `None` for a graph with only an `Informs` edge.

Negative — from_str case sensitivity: `from_str("informs")`, `from_str("INFORMS")`,
`from_str("Inform")` all return `None`.

## Constraints

- C-06: `graph_penalty` and `find_terminal_active` must not be modified. No new
  `edges_of_type` calls for `Informs` in those functions.
- C-07: `edges_of_type` is the sole filter boundary in traversal — never call
  `.edges_directed()` directly in penalty or supersession logic.
- No new exported functions. No new public types. The change is purely additive to the enum.
