# vnc-015 Pseudocode Overview — Typed Edge Write Path

## Components Involved

| Component | File | Crate | Action |
|-----------|------|-------|--------|
| 1. EdgeInput / StoreParams / CorrectParams | `edge-input-params.md` | unimatrix-server | Modify tools.rs |
| 2. edge_write.rs helper module | `edge-write.md` | unimatrix-server | Create new module |
| 3. RelationType enum extension | `relation-type.md` | unimatrix-engine | Modify graph.rs |
| 4. PPR and graph_expand expansion | `ppr-expand.md` | unimatrix-engine | Modify graph_ppr.rs, graph_expand.rs |
| 5. stale_dependency_edges | `stale-dependency.md` | unimatrix-store | Modify read.rs |
| 6. DependencyOnDeprecated detection rule | `detection-rule.md` | unimatrix-observe | Modify detection/scope.rs, detection/mod.rs |
| 7. query_contradicts_edges_for_entry fix | `contradicts-fix.md` | unimatrix-store | Modify read.rs |
| 8. context_edge handler | `context-edge-handler.md` | unimatrix-server | Modify tools.rs |

## Data Flow

```
MCP caller
    │
    ├─ context_store(edges: Option<Vec<EdgeInput>>)
    │       │
    │       ├─ Phase A: validate_and_write_edges(store, 0, edges, created_at)
    │       │     [type resolution + target validation — no source_id yet]
    │       │     returns Err(EdgeValidationError) → abort entire call
    │       │
    │       ├─ StoreService.insert() → assigned source_id
    │       │
    │       ├─ duplicate guard: if duplicate_of.is_some() → skip edges
    │       │
    │       └─ Phase B: validate_and_write_edges(store, source_id, edges, created_at)
    │             [self-ref check + write_graph_edge per edge]
    │             [Contradicts: write_graph_edge twice (A→B and B→A)]
    │             Err(write) → log, continue (ADR-003 partial-write posture)
    │
    ├─ context_correct(edges: Option<Vec<EdgeInput>>)
    │       │   (same two-phase flow; source_id = corrected entry's new id)
    │       └─ ...
    │
    ├─ context_edge(mode, source_id, edge_type, target_id, new_target_id?)
    │       │
    │       ├─ capability gate (Capability::Write)
    │       ├─ source fetch: store.get_entry_by_id(source_id)
    │       ├─ source status: not Quarantined, not Deprecated → SourceFrozen
    │       ├─ self-ref: source_id != target_id
    │       ├─ edge type: RelationType::from_str(edge_type) → UnknownType
    │       ├─ target validation: validate_target(store, target_id)
    │       │
    │       ├─ mode "add"      → write_graph_edge (idempotent; Contradicts: ×2)
    │       ├─ mode "remove"   → delete_graph_edge (idempotent; Contradicts: ×2)
    │       └─ mode "redirect" → redirect_graph_edge (RAII txn; Contradicts: 4 rows)
    │
    ├─ context_status()
    │       └─ compute_graph_cohesion_metrics()
    │               └─ SQL: COUNT Prerequisite edges with Deprecated source
    │                       → stale_dependency_edges: u64
    │
    └─ context_cycle_review(feature_cycle)
            │
            ├─ stale_dependency_edges query → Vec<(u64, u64)>
            ├─ default_rules(history, stale_edge_pairs)
            │       └─ DependencyOnDeprecatedRule::new(stale_edge_pairs)
            └─ detect_hotspots(attributed, &rules)
```

## Shared Types Introduced or Modified

### EdgeInput (new — tools.rs, pub(crate))
```
EdgeInput {
    edge_type: String,    // parsed via RelationType::from_str(); case-sensitive
    target_id: u64,       // must not equal resolved source_id
}
derives: Debug, Clone, Deserialize, JsonSchema
```

### EdgeParams (new — tools.rs, wire struct for context_edge)
```
EdgeParams {
    mode:          String,        // "add" | "remove" | "redirect"
    source_id:     u64,
    edge_type:     String,
    target_id:     u64,
    new_target_id: Option<u64>,   // required for redirect; rejected for add/remove
}
derives: Debug, Deserialize, JsonSchema
```

### EdgeValidationError (new — edge_write.rs, pub(crate))
```
EdgeValidationError {
    UnknownType     { edge_type: String },
    SelfReferential { id: u64 },
    TargetNotFound  { target_id: u64 },
    TargetQuarantined { target_id: u64 },
}
```

### EdgeDeleteError (new — edge_write.rs, pub(crate))
```
EdgeDeleteError {
    StoreError(StoreError),    // infrastructure only; idempotent delete is not an error
}
```

### EdgeRedirectError (new — edge_write.rs, pub(crate))
```
EdgeRedirectError {
    TargetNotFound   { target_id: u64 },
    TargetQuarantined { target_id: u64 },
    TransactionError(sqlx::Error),
}
```

### GraphCohesionMetrics (modified — read.rs)
Add field: `stale_dependency_edges: u64`

### DependencyOnDeprecatedRule (new — detection/scope.rs, pub(crate))
```
DependencyOnDeprecatedRule {
    stale_edge_pairs: Vec<(u64, u64)>,   // (source_id, target_id) of stale Prerequisite edges
}
```

### EDGE_SOURCE_AGENT (new constant — edge_write.rs)
```
pub(crate) const EDGE_SOURCE_AGENT: &str = "agent";
```

## Cross-Crate Dependencies

```
unimatrix-server/src/mcp/edge_write.rs
    depends on: unimatrix-store (Store, get_entry_by_id, write_pool_server)
    depends on: unimatrix-engine (RelationType, from_str, as_str)
    depends on: unimatrix-server/src/mcp/nli_detection.rs (write_graph_edge — same crate, pub(crate))

unimatrix-server/src/mcp/tools.rs
    depends on: edge_write.rs (validate_and_write_edges, delete_graph_edge, redirect_graph_edge)

unimatrix-engine/src/graph_ppr.rs
    depends on: unimatrix-engine/src/graph.rs (RelationType::RelatedTo)

unimatrix-engine/src/graph_expand.rs
    depends on: unimatrix-engine/src/graph.rs (RelationType::RelatedTo)

unimatrix-observe/src/detection/scope.rs
    no new dependencies (DetectionRule trait, ObservationRecord already imported)

unimatrix-observe/src/detection/mod.rs
    depends on: detection/scope.rs (DependencyOnDeprecatedRule)
    depends on: unimatrix-store (MetricVector — already imported for PhaseDurationOutlierRule)
```

## Sequencing Constraints

1. **RelationType enum first** — Components 3 and 4 must be implemented first. `edge_write.rs`
   calls `RelationType::from_str()` and `as_str()`. PPR expansion uses `RelationType::RelatedTo`.
   All compile errors will propagate from missing enum variants.

2. **edge_write.rs before tools.rs changes** — Component 2 must exist before Components 1 and 8
   can call it. tools.rs imports from edge_write.rs.

3. **stale_dependency_edges and contradicts-fix are independent** — Components 5 and 7 modify
   read.rs independently and can be implemented in either order.

4. **DependencyOnDeprecated after stale_dependency_edges** — The detection rule (Component 6)
   requires the stale edge query from Component 5 to be callable from context_cycle_review.

5. **context_edge handler last** — Component 8 depends on Components 1 and 2 being complete.

## write_graph_edge Three-Case Contract (Pattern #4041)

Every call site that invokes `write_graph_edge` MUST document this contract before the loop body:

| Return | Meaning | Required action |
|--------|---------|----------------|
| `true` | Row inserted (rows_affected = 1) | Continue |
| `false` (no Err) | INSERT OR IGNORE hit UNIQUE constraint — row already exists | Continue (idempotent) |
| `Err(_)` | Infrastructure error (logged inside write_graph_edge) | Log at call site if needed; do NOT roll back entry; do NOT surface to caller |

The return type of `write_graph_edge` is `bool` (not `Result<bool, _>`). Errors are handled
inside `write_graph_edge` and logged there. The caller receives `false` on any non-insert outcome.
This is a critical pattern — misreading `false` as an error causes spurious caller failures.
