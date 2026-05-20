# Pseudocode: graph_read.rs (Wave 1)
# Modified file: crates/unimatrix-server/src/mcp/graph_read.rs

## Purpose

`graph_read.rs` is the entry point module for `context_graph`. In vnc-020 it gains:
1. Eight new `Option<T>` fields on `GraphParams`.
2. Four new response structs: `InverseResponse`, `FilterResponse`, `PathHop`, `PathResponse`.
3. Three `#[path]` module declarations for the new sibling handler files.
4. Three dispatch arms in `handle_graph` for "inverse", "filter", "path".
5. Expanded `validate_no_unsupported_params` with three new mode arms and rejection
   clauses for the 8 new fields on the 4 existing mode arms.

Post-vnc-020, `graph_read.rs` is projected at approximately 500 lines. Handler logic
MUST NOT be added here — it stays entirely in the sibling modules (ADR-001, C5).

---

## 1. New Module Declarations

Add immediately after the existing `#[path = "graph_read_subgraph.rs"]` declaration:

```
#[path = "graph_read_inverse.rs"]
mod graph_read_inverse;

#[path = "graph_read_filter.rs"]
mod graph_read_filter;

#[path = "graph_read_path.rs"]
mod graph_read_path;
```

These follow the exact same pattern as the existing `graph_read_supersession`,
`graph_read_neighbors`, and `graph_read_subgraph` declarations already present
(lines 32-38 of the current file).

IMPORTANT (pattern #4509): Wave 2 agents must create their `.rs` files immediately
on spawn — even if initially empty — so that Wave 1 compilation succeeds. If a file
is missing the compiler will fail on the `mod` declaration.

---

## 2. GraphParams Field Additions

Add to the existing `GraphParams` struct after the `max_depth` field. All fields are
`Option<T>` additions — no existing field is renamed, retyped, or removed (ADR-002, C4).

```
// -- vnc-020 fields: inverse and filter modes --
/// inverse and filter: entry category filter (required for both modes).
pub category: Option<String>,

/// inverse only: edge type(s) whose absence is tested (required, non-empty).
/// REJECTED on all other modes. Do not confuse with edge_types.
pub missing_edge_types: Option<Vec<String>>,

/// inverse and filter: max entries to return (default 100, range [1, 500]).
pub limit: Option<u32>,

/// filter only: entries where created_at <= (NOW - N days).
pub min_age_days: Option<u32>,

/// filter only: entries where confidence >= N.
pub min_confidence: Option<f64>,

/// filter only: entries where confidence <= N.
pub max_confidence: Option<f64>,

/// filter only: entries with at least this many outgoing edges of edge_types.
/// Requires edge_types to be present and non-empty.
pub min_edge_count: Option<u32>,

/// filter only: entries with at most this many outgoing edges of edge_types.
/// max_edge_count=0 is valid: returns entries with zero matching edges.
/// Requires edge_types to be present and non-empty.
pub max_edge_count: Option<u32>,
```

---

## 3. New Response Types

Add after the existing `SubgraphResponse` struct definition. All four structs need
`#[derive(Debug, Clone, Serialize)]` at minimum. Add `Deserialize` only if needed for
tests. `JsonSchema` is NOT needed (these are outputs, not inputs).

```
/// Response envelope for inverse mode (vnc-020).
#[derive(Debug, Clone, Serialize)]
pub struct InverseResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

/// Response envelope for filter mode (vnc-020).
#[derive(Debug, Clone, Serialize)]
pub struct FilterResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

/// A single hop in a path traversal result (vnc-020, ADR-005).
///
/// relation_type is never null — always one of the 16 RelationType variant name strings.
/// from_id is NOT a PathHop — it is a top-level PathResponse field.
#[derive(Debug, Clone, Serialize)]
pub struct PathHop {
    pub entry_id: u64,
    pub relation_type: String,
}

/// Response envelope for path mode (vnc-020, ADR-005).
///
/// found=false when: (a) no path within depth hops, or (b) from_id/to_id absent from
/// the current graph snapshot. Both are valid results, NOT errors (FR-13, AC-14, AC-15).
/// length always equals hops.len().
/// from_id and to_id are resolved IDs when resolve_supersessions=true (ADR-006).
#[derive(Debug, Clone, Serialize)]
pub struct PathResponse {
    pub found: bool,
    pub from_id: u64,
    pub to_id: u64,
    pub hops: Vec<PathHop>,
    pub length: u8,
}
```

---

## 4. handle_graph Dispatch Additions

The existing dispatch is:
```
match params.mode.as_str() {
    "chain" | "current" | "neighbors" => { ... }
    "subgraph" => { ... }
    _ => unreachable!(...)
}
```

Replace the trailing `_ => unreachable!(...)` arm with three new arms BEFORE it:

```
"inverse" => {
    // Pure SQL antijoin — no graph state needed.
    let result = graph_read_inverse::handle_inverse(store, &params).await?;
    let json = serde_json::to_string(&result).map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
    })?;
    Ok(CallToolResult::success(vec![rmcp::model::Content::text(json)]))
}

"filter" => {
    // Pure SQL correlated subquery — no graph state needed.
    let result = graph_read_filter::handle_filter(store, &params).await?;
    let json = serde_json::to_string(&result).map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
    })?;
    Ok(CallToolResult::success(vec![rmcp::model::Content::text(json)]))
}

"path" => {
    // In-memory BFS — requires typed_graph_state.
    let result =
        graph_read_path::handle_path(store, typed_graph_state, &params).await?;
    let json = serde_json::to_string(&result).map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
    })?;
    Ok(CallToolResult::success(vec![rmcp::model::Content::text(json)]))
}

// validate_no_unsupported_params already caught unrecognized modes above;
// this arm is unreachable under normal flow but required for exhaustiveness.
_ => unreachable!(
    "validate_no_unsupported_params must catch unrecognized modes before reaching dispatch"
),
```

Note: `typed_graph_state` is already available in `handle_graph`'s parameter list.
It is passed through to `handle_path` only (inverse and filter do not use it).

---

## 5. validate_no_unsupported_params Expansion

### 5a. Unrecognized-mode error update

The existing fallthrough `_ =>` arm produces:
```
"unrecognized mode '{}' — supported modes: chain, current, neighbors, subgraph"
```

Update to list all seven modes (FR-16, AC-26):
```
"unrecognized mode '{}' — supported modes: chain, current, neighbors, subgraph, inverse, filter, path"
```

This `_ =>` arm MUST remain the first pattern matched (before any field check) so
the unrecognized-mode error fires before any field check (R-04, existing invariant).

### 5b. depth rejection on chain, current, subgraph (behavior change — FR-17, AC-25)

In each of the `"chain"`, `"current"`, and `"subgraph"` arms, add before `Ok(())`:

```
// depth is accepted only by neighbors and path modes (ADR-004 vnc-020, FR-17).
// Previously silently ignored — now a validation error (AC-25).
if params.depth.is_some() {
    return Err(
        "depth is not supported in {MODE} mode — use neighbors or path mode".to_string()
    );
}
```

Replace `{MODE}` with the literal mode string for each arm.

### 5c. 8 new field rejections on existing arms

Add the following rejection blocks to each of the four existing mode arms. The exact
placement is: after the existing rejections, before `Ok(())`.

**chain arm** — add after `max_depth` check:
```
if params.category.is_some() {
    return Err("category is not supported in chain mode — use inverse or filter mode".to_string());
}
if params.missing_edge_types.is_some() {
    return Err("missing_edge_types is not supported in chain mode — use inverse mode".to_string());
}
if params.limit.is_some() {
    return Err("limit is not supported in chain mode — use inverse or filter mode".to_string());
}
if params.min_age_days.is_some() {
    return Err("min_age_days is not supported in chain mode — use filter mode".to_string());
}
if params.min_confidence.is_some() {
    return Err("min_confidence is not supported in chain mode — use filter mode".to_string());
}
if params.max_confidence.is_some() {
    return Err("max_confidence is not supported in chain mode — use filter mode".to_string());
}
if params.min_edge_count.is_some() {
    return Err("min_edge_count is not supported in chain mode — use filter mode".to_string());
}
if params.max_edge_count.is_some() {
    return Err("max_edge_count is not supported in chain mode — use filter mode".to_string());
}
```

**current arm** — identical set of 8 rejections with "current mode" in the message.

**neighbors arm** — identical set of 8 rejections with "neighbors mode" in the message.

**subgraph arm** — identical set of 8 rejections with "subgraph mode" in the message.

### 5d. Three new mode arms

Add three new arms to the match inside `validate_no_unsupported_params`.
These must appear before the `_ =>` fallthrough arm.

**"inverse" arm:**
```
"inverse" => {
    // edge_types is the wrong parameter for inverse — inverse uses missing_edge_types (AC-03a).
    if params.edge_types.is_some() {
        return Err(
            "edge_types is not supported in inverse mode — use missing_edge_types instead".to_string()
        );
    }
    // depth rejected: only neighbors and path (ADR-004, FR-17).
    if params.depth.is_some() {
        return Err(
            "depth is not supported in inverse mode — use neighbors or path mode".to_string()
        );
    }
    // from_id / to_id are path-mode-only.
    if params.from_id.is_some() {
        return Err("from_id is not supported in inverse mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in inverse mode — use path mode".to_string());
    }
    // subgraph-only params.
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in inverse mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in inverse mode — use subgraph mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in inverse mode — use subgraph mode".to_string());
    }
    // filter-only params.
    if params.min_age_days.is_some() {
        return Err("min_age_days is not supported in inverse mode — use filter mode".to_string());
    }
    if params.min_confidence.is_some() {
        return Err("min_confidence is not supported in inverse mode — use filter mode".to_string());
    }
    if params.max_confidence.is_some() {
        return Err("max_confidence is not supported in inverse mode — use filter mode".to_string());
    }
    if params.min_edge_count.is_some() {
        return Err("min_edge_count is not supported in inverse mode — use filter mode".to_string());
    }
    if params.max_edge_count.is_some() {
        return Err("max_edge_count is not supported in inverse mode — use filter mode".to_string());
    }
    // category, missing_edge_types, limit: accepted (range validation inside handle_inverse).
    Ok(())
}
```

**"filter" arm:**
```
"filter" => {
    // depth rejected: only neighbors and path (ADR-004, FR-17).
    if params.depth.is_some() {
        return Err(
            "depth is not supported in filter mode — use neighbors or path mode".to_string()
        );
    }
    // from_id / to_id are path-mode-only.
    if params.from_id.is_some() {
        return Err("from_id is not supported in filter mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in filter mode — use path mode".to_string());
    }
    // inverse-only param.
    if params.missing_edge_types.is_some() {
        return Err(
            "missing_edge_types is not supported in filter mode — use inverse mode".to_string()
        );
    }
    // subgraph-only params.
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in filter mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in filter mode — use subgraph mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in filter mode — use subgraph mode".to_string());
    }
    // category, edge_types, limit, min_age_days, min_confidence, max_confidence,
    // min_edge_count, max_edge_count: accepted (range/required validation inside handle_filter).
    Ok(())
}
```

**"path" arm:**
```
"path" => {
    // subgraph-only params.
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in path mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in path mode — use subgraph mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in path mode — use subgraph mode".to_string());
    }
    // chain/current/neighbors anchor param.
    if params.id.is_some() {
        return Err(
            "id is not supported in path mode — use from_id and to_id".to_string()
        );
    }
    // inverse/filter-only params.
    if params.category.is_some() {
        return Err(
            "category is not supported in path mode — use inverse or filter mode".to_string()
        );
    }
    if params.missing_edge_types.is_some() {
        return Err(
            "missing_edge_types is not supported in path mode — use inverse mode".to_string()
        );
    }
    if params.limit.is_some() {
        return Err(
            "limit is not supported in path mode — use inverse or filter mode".to_string()
        );
    }
    if params.min_age_days.is_some() {
        return Err("min_age_days is not supported in path mode — use filter mode".to_string());
    }
    if params.min_confidence.is_some() {
        return Err(
            "min_confidence is not supported in path mode — use filter mode".to_string()
        );
    }
    if params.max_confidence.is_some() {
        return Err(
            "max_confidence is not supported in path mode — use filter mode".to_string()
        );
    }
    if params.min_edge_count.is_some() {
        return Err(
            "min_edge_count is not supported in path mode — use filter mode".to_string()
        );
    }
    if params.max_edge_count.is_some() {
        return Err(
            "max_edge_count is not supported in path mode — use filter mode".to_string()
        );
    }
    // from_id, to_id, depth, edge_types, resolve_supersessions: accepted.
    // Range/required validation happens inside handle_path.
    Ok(())
}
```

---

## Error Handling

- `validate_no_unsupported_params` returns `Result<(), String>`. The caller in
  `handle_graph` converts the `String` to `ErrorData::new(ERROR_INVALID_PARAMS, msg, None)`.
  This is unchanged from the current implementation.
- The three new dispatch arms propagate `ErrorData` from handlers using `?`.
- Serialization errors use `ERROR_INTERNAL` (same pattern as existing arms).

---

## Key Test Scenarios

- AC-25: Pass `depth=Some(3)` to each of chain, current, subgraph, inverse, filter — assert
  validation error with "use neighbors or path mode" in message.
- AC-26: Call with `mode="unknown"` — assert error lists all 7 modes.
- AC-22: Pass `from_id=Some(1)` to each of chain, current, neighbors, subgraph, filter —
  assert error names "path mode".
- AC-23: Pass `missing_edge_types=Some(["Cites"])` to filter mode — assert error names
  "inverse mode".
- AC-24: Pass `min_edge_count=Some(1)` to inverse mode — assert error names "filter mode".
- AC-03a: Pass `edge_types=Some(["Cites"])` to inverse mode — assert validation error
  naming `missing_edge_types` as the correct parameter.
- R-04: For each of the 8 new fields, test at least one wrong-mode rejection.

---

## Line Budget

Current: 387 lines.
Additions: ~16 (GraphParams fields) + ~25 (response structs) + ~30 (module decls + dispatch arms) + ~120 (validation expansion) = ~191 lines.
Projected total: ~578 lines — OVER BUDGET.

Resolution: The validation expansion is the only risk. Compress the repeated 8-field
rejection blocks by extracting the rejection logic into a helper or by placing the new
field checks in a dense style (no blank lines between `if` blocks within each arm).
Implementation agent must check line count before committing; if over 500, either
extract a `validate_new_fields_rejected(params, mode: &str) -> Result<(), String>`
helper or move the 8-field blocks to compact form. The 500-line limit is enforced at
the code review gate (IR-03, C5).

Note: The estimate above is conservative. With compact formatting (no blank lines between
rejection guards) the expansion is closer to ~80 lines, yielding ~467 total — within budget.
