# Component 8: context_edge Handler (tools.rs)

## Purpose

The 13th MCP tool. Provides standalone edge lifecycle management (add, remove, redirect) on
existing entries. Primary use case: retargeting an edge after a supersession without creating
a new version of the source entry.

Handler lives in `tools.rs` (~80–120 lines). All edge logic delegates to `edge_write.rs`.
Pure graph operation: no embedding recompute, no confidence update, no duplicate detection,
no usage recording.

## File

`crates/unimatrix-server/src/mcp/tools.rs` (modified)

## Wire Struct

`EdgeParams` is defined in Component 1 (edge-input-params.md). Repeated here for clarity:

```
pub struct EdgeParams {
    pub mode:          String,        // "add" | "remove" | "redirect"
    pub source_id:     u64,
    pub edge_type:     String,
    pub target_id:     u64,
    pub new_target_id: Option<u64>,   // required for redirect; rejected for add/remove
}
```

## Tool Registration

```
// In tools.rs MCP tool list (wherever tools are registered):
// Add context_edge as the 13th tool.
// Any test asserting tool count must be updated from 12 to 13 (AC-19).
// The exact registration pattern follows the existing 12 tools.
```

## Validation Pipeline (6 steps, all before any mutation — ADR-009)

```
ASYNC FUNCTION context_edge_handler(params: EdgeParams, agent_context) -> MCP response

    // ── Step 1: Capability gate ───────────────────────────────────────────────
    // Use the same gate invocation as context_store and context_correct.
    // Do NOT write a new capability check — reuse the existing pattern.
    check_capability(agent_context, Capability::Write)?
    // On failure: return existing permission error (same error code as other Write-gated tools)

    // ── Step 2: Source fetch ──────────────────────────────────────────────────
    LET source_entry = store.get_entry_by_id(params.source_id).await?
    IF source_entry is None THEN
        RETURN error SourceNotFound(params.source_id)
        // MCP error: "source entry {source_id} does not exist"
    END IF

    // ── Step 3: Source status — SourceFrozen gate ─────────────────────────────
    // Use Status enum from unimatrix_store::schema — NOT integer literals (R-06)
    IF source_entry.status == Status::Quarantined
       OR source_entry.status == Status::Deprecated
    THEN
        RETURN error SourceFrozen(params.source_id)
        // Error message: "source entry {source_id} is frozen (quarantined or deprecated)"
        // Applies to both quarantined AND deprecated sources (AC-23)
    END IF

    // ── Step 4: Self-referential check ────────────────────────────────────────
    IF params.source_id == params.target_id THEN
        RETURN error SelfReferentialEdge(params.source_id)
        // "self-referential edge rejected: source_id equals target_id ({id})"
    END IF
    // For redirect mode: also check source_id != new_target_id
    IF params.mode == "redirect" THEN
        IF LET Some(new_id) = params.new_target_id THEN
            IF params.source_id == new_id THEN
                RETURN error SelfReferentialEdge(params.source_id)
            END IF
        END IF
    END IF

    // ── Step 5: new_target_id presence check (R-13) ──────────────────────────
    // Reject before edge type and target validation to give callers the most actionable error.
    IF (params.mode == "add" OR params.mode == "remove") AND params.new_target_id.is_some() THEN
        RETURN error UnexpectedNewTargetId
        // "new_target_id is not valid for mode '{mode}'"
    END IF

    // ── Step 6: Edge type resolution ─────────────────────────────────────────
    LET rel_type = RelationType::from_str(&params.edge_type)
    IF rel_type is None THEN
        RETURN error UnknownEdgeType(params.edge_type)
        // "unknown edge type '{type}'"
    END IF

    // ── Step 7: Target validation ─────────────────────────────────────────────
    // For add and remove: validate target_id
    // For redirect: validate new_target_id (old target_id need not exist — idempotent delete)
    IF params.mode == "add" OR params.mode == "remove" THEN
        validate_target(store, params.target_id).await?
        // EdgeValidationError → converted to MCP TargetNotFound or TargetQuarantined error
    ELSE IF params.mode == "redirect" THEN
        LET new_target = params.new_target_id
            .ok_or_else(|| error MissingNewTargetId)?
            // "new_target_id is required for redirect mode"
        validate_target(store, new_target).await?
        // target_id (old edge) need not be validated — DELETE is idempotent on missing rows
    END IF

    // ── Mode dispatch ─────────────────────────────────────────────────────────
    LET created_at = current_unix_timestamp_millis()

    MATCH params.mode.as_str()

        "add" →
            // Idempotent INSERT OR IGNORE (write_graph_edge)
            // Contradicts: both directions written before handler returns
            // write_graph_edge is called via validate_and_write_edges with a single-element slice
            // OR directly via a write-only helper that avoids the Phase A validation re-run
            // (target already validated in Step 6 above)

            LET edge_input = EdgeInput {
                edge_type: params.edge_type.clone(),
                target_id: params.target_id,
            }
            // Call the write path with source_id known (no Phase A loop needed; validation done)
            // Three-case contract (Pattern #4041):
            //   write_graph_edge returns bool; false on UNIQUE conflict is not an error

            LET inserted = write_graph_edge(
                store,
                params.source_id,
                params.target_id,
                rel_type.as_str(),
                1.0,
                created_at,
                EDGE_SOURCE_AGENT,
                ""
            ).await

            // Bidirectional Contradicts
            IF rel_type == RelationType::Contradicts THEN
                LET _rev = write_graph_edge(
                    store,
                    params.target_id,
                    params.source_id,
                    "Contradicts",
                    1.0,
                    created_at,
                    EDGE_SOURCE_AGENT,
                    ""
                ).await
            END IF

            RETURN success_response("edge added")

        "remove" →
            delete_graph_edge(
                store,
                params.source_id,
                params.target_id,
                rel_type.as_str(),
            ).await
            .map_err(|e| convert_to_mcp_error(e))?

            // Idempotent: 0-row DELETE is success (AC-25)
            // Contradicts: delete_graph_edge handles both directions internally

            RETURN success_response("edge removed")

        "redirect" →
            LET new_target = params.new_target_id.unwrap()
            // unwrap is safe: validated as Some in Step 6

            redirect_graph_edge(
                store,
                params.source_id,
                params.target_id,      // old_target_id
                new_target,            // new_target_id
                rel_type.as_str(),
                created_at,
            ).await
            .map_err(|e| convert_to_mcp_error(e))?
            // Contradicts: redirect_graph_edge handles all 4 rows atomically (ADR-009)
            // RAII transaction: on failure, old edge is restored (R-05 mitigation)

            RETURN success_response("edge redirected")

        _ →
            RETURN error InvalidMode(params.mode)
            // "mode must be one of: add, remove, redirect"

    END MATCH

END FUNCTION
```

## No Ownership Check

There is no ownership check in `context_edge`. Any agent with `Capability::Write` may operate
on any non-frozen source entry regardless of who created it. The security gate is:
- `Capability::Write` (Step 1)
- Source entry not Quarantined and not Deprecated (Step 3)

This is the accepted RBAC model for collaborative graph maintenance. The `OwnershipViolation`
error variant does not exist (ADR-009, Constraint #10 in IMPLEMENTATION-BRIEF.md, AC-22).

## No Side Effects

`context_edge` triggers NONE of the following:
- Embedding recompute
- Confidence update
- Duplicate detection
- Usage recording

Pure graph mutation only (NFR-09, AC-20). The handler returns after the mode dispatch without
calling any analytics, scoring, or observation pipelines.

## Error Code Mapping

| Condition | Error code | Message guidance |
|-----------|-----------|------------------|
| Missing `Capability::Write` | Existing permission error | (same as context_store rejection) |
| Source entry does not exist | SourceNotFound | "source entry {id} does not exist" |
| Source is Quarantined or Deprecated | SourceFrozen | "source entry {id} is frozen (quarantined or deprecated)" |
| source_id == target_id | SelfReferentialEdge | "self-referential edge rejected: source_id equals target_id ({id})" |
| Unknown edge_type | UnknownEdgeType | "unknown edge type '{type}'" |
| target_id not found | TargetNotFound | "target entry {id} does not exist" |
| target_id quarantined | TargetQuarantined | "target entry {id} is quarantined and cannot be referenced" |
| new_target_id missing for redirect | MissingNewTargetId | "new_target_id is required for redirect mode" |
| new_target_id present for add/remove | UnexpectedNewTargetId | "new_target_id is not valid for mode '{mode}'" |
| Invalid mode string | InvalidMode | "mode must be one of: add, remove, redirect" |

Error codes follow the existing `ServerError` variant dispatch pattern in `tools.rs`.

## State Machine: Validation → Dispatch

```
[start]
   │
   ▼
[capability check] → fail → permission error
   │ ok
   ▼
[source fetch] → not found → SourceNotFound error
   │ found
   ▼
[source status] → quarantined or deprecated → SourceFrozen error
   │ active
   ▼
[self-ref check] → source_id == target_id → SelfReferentialEdge error
   │ ok
   ▼
[new_target_id presence check] → unexpected on add/remove → UnexpectedNewTargetId error
   │ ok
   ▼
[edge type resolution] → unknown → UnknownEdgeType error
   │ resolved
   ▼
[target validation] → not found → TargetNotFound error
   │               → quarantined → TargetQuarantined error
   │ ok
   ▼
[mode dispatch: add | remove | redirect]
   │
   ├─ add    → write_graph_edge (+ reverse if Contradicts) → success
   ├─ remove → delete_graph_edge (handles Contradicts internally) → success
   └─ redirect → redirect_graph_edge (RAII txn) → success or transaction error
```

## Key Test Scenarios

1. Unenrolled agent → permission error (AC-15, AC-21)
2. Non-existent source_id → SourceNotFound error; no mutation (source fetch)
3. Quarantined source → SourceFrozen error; no mutation (AC-23)
4. Deprecated source → SourceFrozen error; no mutation (AC-23)
5. Active source with two different agents → both succeed (AC-22 — no ownership check)
6. source_id == target_id → SelfReferentialEdge; no mutation (AC-08)
7. Unknown edge_type → UnknownEdgeType; no mutation (Step 5)
8. Non-existent target_id for add → TargetNotFound; no mutation (AC-24)
9. Quarantined target for add → TargetQuarantined; no mutation (AC-24)
10. Deprecated target for add → success; edge written (AC-24)
11. add with valid inputs → GRAPH_EDGES row written; source+created_by = EDGE_SOURCE_AGENT (AC-24)
12. add idempotent: same triplet twice → no error, 1 row (AC-10)
13. add Contradicts → both (A,B) and (B,A) rows written (AC-06)
14. remove existing edge → row deleted; success (AC-25)
15. remove non-existent edge → success (idempotent, AC-25)
16. remove Contradicts → both direction rows deleted (AC-25)
17. redirect success → old row absent, new row present (AC-26)
18. redirect Contradicts → all 4 rows updated atomically (AC-26, R-02)
19. redirect to non-existent new_target → TargetNotFound; original rows unchanged (R-05)
20. redirect fails mid-transaction → ROLLBACK; original edge preserved (R-05)
21. new_target_id present for add mode → UnexpectedNewTargetId error (R-13)
22. new_target_id present for remove mode → UnexpectedNewTargetId error (R-13)
23. new_target_id absent for redirect mode → MissingNewTargetId error
24. No embedding/confidence side effects after add → confirm no embedding job queued (AC-20)
25. Tool count: 13 tools registered (AC-19)
