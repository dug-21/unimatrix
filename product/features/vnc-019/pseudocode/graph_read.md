# Pseudocode: `graph_read.rs` Changes

## Purpose

`graph_read.rs` owns all wire types for `context_graph` and the `handle_graph` entry
point. For vnc-019 it gains:

1. One new field on `GraphParams`: `max_depth: Option<u8>`.
2. One new response envelope: `SubgraphResponse`.
3. One new `#[path]` submodule declaration for `graph_read_subgraph`.
4. Extensions to `validate_no_unsupported_params`: `"subgraph"` arm + `max_depth`
   rejection on existing modes + updated unrecognized-mode error.
5. `"subgraph"` dispatch arm in `handle_graph`.

No existing functions are renamed or reordered. File limit: stays under 500 lines
(ADR-002 ensures BFS logic lives in `graph_read_subgraph.rs`).

---

## New/Modified Items

### 1. Submodule Declaration (addition)

After the existing `#[path = "graph_read_neighbors.rs"]` declaration:

```
#[path = "graph_read_subgraph.rs"]
mod graph_read_subgraph;
```

This makes `graph_read_subgraph::handle_subgraph` visible to `handle_graph`.

---

### 2. `GraphParams` — add `max_depth` field

Existing struct definition extended with one field (ADR-001). Position: append after
the existing `to_id` forward-compat stub to keep grouped intent.

```
pub struct GraphParams {
    // ---- existing fields (unchanged — ADR-003 lock) ----
    pub mode: String,
    pub agent_id: Option<String>,
    pub format: Option<String>,
    #[schemars(with = "Option<u64>")]
    pub id: Option<u64>,
    pub direction: Option<String>,
    pub edge_types: Option<Vec<String>>,
    pub depth: Option<u8>,
    pub resolve_supersessions: Option<bool>,
    pub seed_ids: Option<Vec<u64>>,
    pub max_nodes: Option<u32>,
    pub from_id: Option<u64>,
    pub to_id: Option<u64>,

    // ---- new field (ADR-001 vnc-019) ----
    /// subgraph mode only: BFS max depth 1..=10 (default 3 when absent).
    /// Error if passed to chain, current, or neighbors modes.
    pub max_depth: Option<u8>,
}
```

Derives remain unchanged: `Debug, Clone, Deserialize, JsonSchema, Default`.
The `Default` derive continues to work — `Option<u8>` defaults to `None`.

---

### 3. `SubgraphResponse` — new struct

Inserted adjacent to `ChainResult`, `CurrentResponse`, `NeighborsResponse`
(the other response envelopes in the wire types section).

```
/// Response envelope for subgraph mode (vnc-019).
///
/// `direction` on every EdgeRecord is always "outgoing" — canonical stored direction
/// (source_id → target_id). See FR-12, ADR-004 vnc-018.
///
/// `truncated: true` means the max_nodes cap was hit before BFS completed.
/// `depth_reached`: actual max BFS depth traversed (0 when no edges discovered).
#[derive(Debug, Clone, Serialize)]   // Serialize only — not deserialized from wire
pub struct SubgraphResponse {
    pub nodes: Vec<EntryRecord>,
    pub edges: Vec<EdgeRecord>,
    pub truncated: bool,
    pub seed_ids: Vec<u64>,
    pub depth_reached: u8,
}
```

Note: `Deserialize` is not needed — this is an outbound response type only.

---

### 4. `validate_no_unsupported_params` — extended

Existing function signature unchanged. Changes:

a. Each of the three existing mode arms (`"chain"`, `"current"`, `"neighbors"`) gains
   a `max_depth` rejection check.

b. A new `"subgraph"` arm is added.

c. The default `_` arm (unrecognized-mode error) is updated to include `subgraph` in
   the supported-modes list.

```
pub(crate) fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String> {
    match params.mode.as_str() {

        "chain" => {
            // existing rejections (unchanged):
            if params.seed_ids.is_some() {
                return Err("seed_ids is not supported in chain mode — use subgraph mode");
            }
            if params.max_nodes.is_some() {
                return Err("max_nodes is not supported in chain mode — use subgraph mode");
            }
            if params.from_id.is_some() {
                return Err("from_id is not supported in chain mode — use path mode (#598)");
            }
            if params.to_id.is_some() {
                return Err("to_id is not supported in chain mode — use path mode (#598)");
            }
            if params.resolve_supersessions == Some(true) {
                return Err("resolve_supersessions is not applicable to chain mode — ...");
            }
            // NEW rejection (ADR-001):
            if params.max_depth.is_some() {
                return Err(
                    "max_depth is not supported in chain mode — use subgraph mode"
                );
            }
            Ok(())
        }

        "current" => {
            // existing rejections (unchanged):
            if params.seed_ids.is_some() { ... }
            if params.max_nodes.is_some() { ... }
            if params.from_id.is_some() { ... }
            if params.to_id.is_some() { ... }
            // NEW rejection (ADR-001):
            if params.max_depth.is_some() {
                return Err(
                    "max_depth is not supported in current mode — use subgraph mode"
                );
            }
            Ok(())
        }

        "neighbors" => {
            // existing rejections (unchanged):
            if params.seed_ids.is_some() { ... }
            if params.max_nodes.is_some() { ... }
            if params.from_id.is_some() { ... }
            if params.to_id.is_some() { ... }
            // NEW rejection (ADR-001):
            if params.max_depth.is_some() {
                return Err(
                    "max_depth is not supported in neighbors mode — use subgraph mode"
                );
            }
            Ok(())
        }

        // NEW arm (FR-20, ADR-001):
        // subgraph permits seed_ids, max_nodes, max_depth.
        // Rejects from_id, to_id (path mode only — preserved forward-compat guard).
        "subgraph" => {
            if params.from_id.is_some() {
                return Err(
                    "from_id is not supported in subgraph mode — use path mode (#598)"
                );
            }
            if params.to_id.is_some() {
                return Err(
                    "to_id is not supported in subgraph mode — use path mode (#598)"
                );
            }
            // seed_ids, max_nodes, max_depth: permitted. Range validation happens
            // inside handle_subgraph after dispatch.
            Ok(())
        }

        // UPDATED unrecognized-mode error — now lists "subgraph" (FR-20, R-05):
        _ => Err(format!(
            "unrecognized mode '{}' — supported modes: chain, current, neighbors, subgraph",
            params.mode
        )),
    }
}
```

Error message exact strings (from IMPLEMENTATION-BRIEF.md validation table):
- `max_depth` on wrong mode: `"max_depth is not supported in {mode} mode — use subgraph mode"`
- `from_id` on subgraph: existing pattern, `"from_id is not supported in subgraph mode — use path mode (#598)"`
- Unrecognized mode: `"unrecognized mode '{x}' — supported modes: chain, current, neighbors, subgraph"`

---

### 5. `handle_graph` — subgraph dispatch arm

The existing `_` / unreachable arm must be modified. The change:

- Remove the `_ => unreachable!(...)` catch-all.
- Add a `"subgraph"` dispatch arm before a new, still-unreachable catch-all.

```
pub(crate) async fn handle_graph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: GraphParams,
    _ctx: &ToolContext,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // Step 1: centralized validation (unchanged)
    if let Err(msg) = validate_no_unsupported_params(&params) {
        return Err(ErrorData::new(ERROR_INVALID_PARAMS, msg, None));
    }

    // Step 2: anchor ID required for chain/current/neighbors.
    // NOTE: subgraph does NOT use `id` — it uses `seed_ids`. The id-required check
    // must move INSIDE each non-subgraph arm, OR the subgraph arm must be listed
    // before the id check.
    //
    // DESIGN NOTE: The current code extracts `id` unconditionally before the match.
    // For subgraph mode this fails because `id` is not required.
    // Solution: move the id-required guard inside each non-subgraph arm,
    // OR match on mode first and fall through to id extraction only for chain/current/neighbors.
    //
    // Recommended implementation pattern:
    match params.mode.as_str() {
        "chain" | "current" | "neighbors" => {
            // existing: require id
            let id = params.id.ok_or_else(|| ErrorData::new(
                ERROR_INVALID_PARAMS,
                "id is required for chain, current, and neighbors modes",
                None,
            ))?;
            match params.mode.as_str() {
                "chain" => {
                    let result = graph_read_supersession::handle_chain(store, &params, id).await?;
                    serialize_and_succeed(result)
                }
                "current" => {
                    match graph_read_supersession::handle_current(store, id).await {
                        Ok(resp) => serialize_and_succeed(resp),
                        Err(msg) => Err(ErrorData::new(ERROR_INVALID_PARAMS, msg, None)),
                    }
                }
                "neighbors" => {
                    let result = graph_read_neighbors::handle_neighbors(
                        store, typed_graph_state, &params, id
                    ).await?;
                    serialize_and_succeed(result)
                }
                _ => unreachable!("validate_no_unsupported_params already caught this")
            }
        }
        "subgraph" => {
            // seed_ids replaces id for this mode. No `id` field required.
            let result = graph_read_subgraph::handle_subgraph(
                store,
                typed_graph_state,
                &params,
            ).await?;
            let json = serde_json::to_string(&result).map_err(|e| {
                ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
            })?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(json)]))
        }
        _ => unreachable!(
            "validate_no_unsupported_params must catch unrecognized modes before reaching dispatch"
        ),
    }
}
```

Implementation note on id-extraction restructuring: The current code extracts `id`
unconditionally before the mode match. This works for vnc-018 because all three modes
require `id`. Adding `subgraph` breaks this — subgraph uses `seed_ids`, not `id`, and
returning an error for a missing `id` when the caller correctly passed `seed_ids` is
wrong. The implementor must restructure accordingly. The simplest approach is a
two-level match: outer match on `mode`, inner extraction of `id` only for the three
modes that need it.

---

## Error Handling

`validate_no_unsupported_params` returns `Err(String)` on any validation failure.
`handle_graph` converts this to `ErrorData::new(ERROR_INVALID_PARAMS, msg, None)`.

Serialization errors in the dispatch arms return `ErrorData::new(ERROR_INTERNAL, ...)`.

The `handle_subgraph` call propagates `ErrorData` via `?`.

---

## Key Test Scenarios

These are regression scenarios the implementation must cover (R-05, FR-20):

1. `mode="subgraph"` no longer returns `"unrecognized mode"` — verified by unit test
   updating the existing vnc-018 `test_validate_unrecognized_mode_fires_before_field_check`.
   The subgraph case must be removed from that test and added to a recognized-mode test.

2. `mode="chain", max_depth=Some(2)` returns validation error containing
   `"max_depth is not supported in chain mode"`.

3. `mode="current", max_depth=Some(1)` returns validation error.

4. `mode="neighbors", max_depth=Some(3)` returns validation error.

5. `mode="subgraph", seed_ids=Some([1]), max_depth=Some(3)` passes validation
   (no error from `validate_no_unsupported_params`).

6. `mode="subgraph", from_id=Some(1)` returns validation error (from_id rejected).

7. `mode="subgraph", to_id=Some(1)` returns validation error (to_id rejected).

8. `GraphParams` with `max_depth: None` deserializes without error (backward compat).

9. `GraphParams` with `max_depth: Some(5)` deserializes correctly.
