# Component 1: EdgeInput / StoreParams / CorrectParams Extension

## Purpose

Define the `EdgeInput` wire struct and extend `StoreParams` and `CorrectParams` in
`crates/unimatrix-server/src/mcp/tools.rs` to accept an `edges: Option<Vec<EdgeInput>>`
parameter. This makes edge declaration at entry creation/correction time possible without
changing any existing call that omits the field (backward-compatible default of `None`).

`EdgeParams` is also defined here — the wire struct for the new `context_edge` tool handler.

## File

`crates/unimatrix-server/src/mcp/tools.rs` (modified)

## New/Modified Structs

### EdgeInput (new)

```
STRUCT EdgeInput
    derives: Debug, Clone, Deserialize, JsonSchema
    FIELD edge_type: String
        // Must parse via RelationType::from_str(); case-sensitive string
        // Validated by validate_and_write_edges before any write
    FIELD target_id: u64
        // Target entry id; must not equal the resolved source_id
        // Validated by validate_and_write_edges before any write
END STRUCT
```

Location: defined in `tools.rs`, above `StoreParams`. Inline — not in a separate types module
(consistent with how existing param structs are defined in tools.rs).

### StoreParams (modified — add edges field)

```
STRUCT StoreParams
    // ... existing fields unchanged ...
    FIELD edges: Option<Vec<EdgeInput>>
        // serde(default) or Option — both produce None when field is absent from JSON
        // AC-01: omitting the field produces identical behavior to current implementation
        // An empty vec Some([]) is treated as no edges — no validation, no writes
END STRUCT
```

### CorrectParams (modified — add edges field)

```
STRUCT CorrectParams
    // ... existing fields unchanged ...
    FIELD edges: Option<Vec<EdgeInput>>
        // Same semantics as StoreParams.edges
        // AC-02: edges attach to the new (corrected) entry's id, not the deprecated original's id
        // No edge transfer from the deprecated entry occurs
END STRUCT
```

### EdgeParams (new — wire struct for context_edge)

```
STRUCT EdgeParams
    derives: Debug, Deserialize, JsonSchema
    FIELD mode: String
        // "add" | "remove" | "redirect"
        // Handler validates this is one of the three accepted strings; any other value → error
    FIELD source_id: u64
        // Existing entry id; fetched from store to validate existence and status
    FIELD edge_type: String
        // Must parse via RelationType::from_str(); case-sensitive
    FIELD target_id: u64
        // Current target for add/remove/redirect (the "from" side of the move in redirect)
    FIELD new_target_id: Option<u64>
        // Required for redirect; rejected (error) if present for add or remove
        // serde(default) = None
END STRUCT
```

## Integration Points

- `EdgeInput` is imported by `edge_write.rs` in the `edges: &[EdgeInput]` parameter of
  `validate_and_write_edges`. The module declaration `pub(crate) mod edge_write;` must be added
  to `tools.rs` (or `mod.rs`), and `edge_write.rs` must import `EdgeInput` from the parent module
  or `tools.rs` must re-export it.
- `StoreParams` and `CorrectParams` are deserialized from MCP call arguments by the rmcp framework;
  adding an optional field with `serde(default)` is backward-compatible.
- `EdgeParams` is deserialized from MCP call arguments for the `context_edge` tool handler.

## Handler Pipeline Changes (where edges param is consumed)

### context_store pipeline (modified — steps shown relative to existing flow)

```
FUNCTION context_store_handler(params: StoreParams, agent_context) -> MCP response
    // Step 1-2: identity + capability (unchanged)
    check_capability(Capability::Write)

    // Step 3: extract edges or default to empty
    LET edges: &[EdgeInput] = params.edges.as_deref().unwrap_or(&[])

    // Step 3a [NEW] Phase A — pre-insert type resolution + target validation
    // Self-ref check cannot run here; source_id is not known until after insert
    IF edges is not empty THEN
        FOR EACH edge IN edges
            // Resolve edge type (pure, no DB)
            LET rel_type = RelationType::from_str(&edge.edge_type)
            IF rel_type is None THEN
                RETURN error UnknownEdgeType(edge.edge_type)
            END IF
            // Target validation (1 DB read per edge via read_pool)
            LET target_entry = store.get_entry_by_id(edge.target_id).await
            MATCH target_entry
                None    → RETURN error TargetNotFound(edge.target_id)
                Some(e) where e.status == Status::Quarantined
                        → RETURN error TargetQuarantined(edge.target_id)
                Some(_) → continue  // Active or Deprecated — allowed
            END MATCH
            // Collect resolved pair (self-ref check deferred to Phase B)
            PUSH (rel_type, edge.target_id) to resolved_edges
        END FOR
    END IF

    // Step 4: StoreService.insert (entry written — source_id now known)
    LET insert_result = store_service.insert(params...)

    // Step 5: duplicate guard (existing)
    IF insert_result.duplicate_of.is_some() THEN
        // Do NOT write any edges for duplicate entries
        RETURN duplicate_response(insert_result)
    END IF

    LET source_id = insert_result.entry.id

    // Step 6 [NEW] Phase B — self-ref check + edge writes
    IF resolved_edges is not empty THEN
        FOR EACH (rel_type, target_id) IN resolved_edges
            IF source_id == target_id THEN
                // Self-referential: entry was written, edges are NOT written
                // Log the anomaly (caller should not have submitted this)
                LOG warn("self-referential edge rejected post-insert: source={} target={}", source_id, target_id)
                RETURN error SelfReferentialEdge(source_id)
            END IF
        END FOR
        // Write edges (Phase B)
        CALL validate_and_write_edges(store, source_id, edges, created_at)
        // Infrastructure failures inside are logged; entry is NOT rolled back (ADR-003)
    END IF

    // Steps 7-8: confidence recompute + usage recording (unchanged)
    ...
END FUNCTION
```

Note: The "Phase A / Phase B" split above is an implementation split within `validate_and_write_edges`
and its callers. See `edge-write.md` for the exact function signature and internal structure.

### context_correct pipeline (modified)

```
FUNCTION context_correct_handler(params: CorrectParams, agent_context) -> MCP response
    // Existing steps: capability, content validation, StoreService.correct → new entry
    ...
    LET correct_result = store_service.correct_entry(params...)
    LET source_id = correct_result.corrected_entry.id  // New entry id — NOT deprecated original

    // [NEW] Edge writes attach to corrected entry's new id
    LET edges: &[EdgeInput] = params.edges.as_deref().unwrap_or(&[])
    IF edges is not empty THEN
        // Phase A: type resolution + target validation (same as context_store)
        // Phase B: self-ref check + writes with source_id = corrected entry's new id
        CALL validate_and_write_edges(store, source_id, edges, created_at)
        // Failures logged, not rolled back (ADR-003)
    END IF
    ...
END FUNCTION
```

## Error Handling

| Error | Trigger | Behavior |
|-------|---------|----------|
| `UnknownEdgeType` | `RelationType::from_str` returns None in Phase A | Entire call fails; no entry written |
| `TargetNotFound` | `get_entry_by_id` returns None in Phase A | Entire call fails; no entry written |
| `TargetQuarantined` | Entry status is Quarantined in Phase A | Entire call fails; no entry written |
| `SelfReferentialEdge` | source_id == target_id after insert (Phase B) | Entry written, no edges written; error returned |
| `InfrastructureEdgeFailure` | write_graph_edge returns false (Err path) | Logged; entry not rolled back; success returned to caller |

## Key Test Scenarios

1. `context_store` without `edges` field → identical response to pre-feature call (AC-01)
2. `context_store` with `edges: Some([])` → no GRAPH_EDGES rows, no error (empty vec case)
3. `context_store` with valid edges → GRAPH_EDGES rows written with correct source_id (AC-05)
4. `context_correct` with edges → edges reference new entry id, not deprecated original (AC-02)
5. `context_store` with unknown edge_type → entire call rejected, no entry in DB
6. `context_store` with quarantined target_id → entire call rejected, no entry in DB
7. Duplicate insert with edges → no edge rows written, duplicate response returned (AC-09)
