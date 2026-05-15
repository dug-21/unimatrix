## ADR-010: Target Validation — Query Pattern and Failure Posture

### Context

All edge writes — both via the `edges` param on `context_store`/`context_correct` and via the
`context_edge` tool — must validate that the target entry exists and is not quarantined before
any mutation occurs. This is Goal 4 in the updated SCOPE.md:

> Validate target entries on all edge writes: target must exist and must not be quarantined.
> Deprecated targets are allowed. A declared edge to a non-existent or quarantined target is
> an unambiguous caller error; fail the call.

The existing `StoreService` provides entry fetching. The question is what query pattern to use
and how to integrate it into the validation pipeline without introducing new abstractions.

Two surfaces with distinct needs:

**`edges` param (context_store / context_correct):**
The source entry does not yet exist. Multiple edges may be declared (1–5 typical, up to N).
Target validation must run before the entry insert (pre-write fail-fast). All targets are
validated in a loop before any write.

**`context_edge` tool:**
Single edge operation (by design — no bulk variant). Target validation runs after source fetch
and source status check (steps 2–3), still before any mutation. No ownership check.

For `redirect` mode, two targets must be validated: `target_id` (the old edge — should exist
in GRAPH_EDGES; if not, the remove is a no-op, which is acceptable) and `new_target_id` (the
new destination — must exist and not be quarantined, same as for add).

Existing query pattern: `StoreService.get_entry(id: u64) -> Result<Option<EntryRecord>, ...>`
or the equivalent raw Store read function. The SCOPE.md background states: "The existing
`StoreService` likely has a method to fetch an entry by ID — identify it and use it."

Confirmed: `store.get_entry_by_id(id: u64)` exists in `unimatrix-store/src/read.rs` (used
by `context_get` handler in tools.rs). This returns `Result<Option<EntryRecord>, StoreError>`.
`EntryRecord.status` is an integer: 0 = Active, 1 = Deprecated, 2 = Quarantined.

### Decision

**Query function:** Use `store.get_entry_by_id(target_id)` directly in `validate_and_write_edges`
and in the `context_edge` handler. No new query function needed — the existing function is
sufficient.

**Validation logic per target_id:**
```rust
match store.get_entry_by_id(target_id).await? {
    None => return Err(EdgeValidationError::TargetNotFound { target_id }),
    Some(entry) if entry.status == 2 => {
        return Err(EdgeValidationError::TargetQuarantined { target_id })
    }
    Some(_) => { /* Active or Deprecated — allowed */ }
}
```

Deprecated targets (status = 1) are explicitly allowed. The `DependencyOnDeprecated` detection
rule surfaces them during `context_cycle_review`. The write path does not prevent edges to
deprecated targets — a declared dependency on a deprecated decision is meaningful signal, not
an error.

**Loop structure for `validate_and_write_edges` (multiple edges):**
All targets are validated in a sequential loop before any write. The loop resolves edge types
to `RelationType` in the same pass. First error encountered aborts the loop and returns
immediately — no entry is written, no edges are written.

```rust
// Validate all edges before writing any
let resolved: Vec<(RelationType, u64)> = vec![];
for edge in edges {
    let rel_type = RelationType::from_str(&edge.edge_type)
        .ok_or(EdgeValidationError::UnknownType { edge_type: edge.edge_type.clone() })?;
    if source_id == edge.target_id {
        return Err(EdgeValidationError::SelfReferential { id: source_id });
    }
    validate_target(store, edge.target_id).await?;  // checks existence + quarantine
    resolved.push((rel_type, edge.target_id));
}
// All validation passed — now write
for (rel_type, target_id) in resolved { ... }
```

**For `redirect` mode — `new_target_id` validation:**
Only `new_target_id` requires existence + quarantine validation. The `old target_id` in the
GRAPH_EDGES row may or may not exist (the DELETE is idempotent either way). Validate
`new_target_id` using the same `validate_target()` helper before executing the transaction.

**Status integer constants:**
Define named constants or use the existing `EntryStatus` enum from `unimatrix-engine` rather
than magic integers. The spec must confirm whether `EntryStatus` is accessible at the
`edge_write.rs` call site, or whether the integer comparison is the accepted pattern (as used
in existing `read.rs` queries).

**Performance:**
One SELECT per target_id. For typical call sizes (1–5 edges), this adds 1–5 async DB reads
before the entry insert. Each read hits `read_pool()` (non-blocking reads). The latency is
proportional to edge count but dominated by insert + confidence recompute latency. No caching
or batching is needed for this feature.

**Error codes surfaced to caller:**
- `TargetNotFound` → MCP error with clear message: "target entry {target_id} does not exist"
- `TargetQuarantined` → MCP error: "target entry {target_id} is quarantined and cannot be referenced"

These use the existing `ServerError` variant dispatch pattern from `tools.rs`. The spec must
assign error codes consistent with existing conventions (pattern #ADR-007 in vnc-002, existing
`ServerError` variants).

### Consequences

Easier: target validation reuses an existing DB query function — no new store layer code.
The validation loop is self-contained in `edge_write.rs` and keeps the handler lean. Callers
get actionable errors identifying the specific failing target_id.

Harder: the `validate_and_write_edges` function signature requires an async context and a
`&Store` reference (already required post-confidence-floor-removal). The function is no longer
pure-synchronous-validation + async-write; it is fully async end-to-end. This was already the
direction with confidence floor removal.

The sequential loop validates targets one-by-one. If edge[0] is valid but edge[1] has an
unknown quarantined target, only edge[1]'s error is returned. There is no "validate all, collect
all errors" behavior — first-error abort is consistent with all other validation in the pipeline.

Supersedes: none.
Related: ADR-001 (validation-first), ADR-002 (failure posture), ADR-009 (context_edge tool),
ADR-003 (partial-write posture).
