## ADR-002: Edge Write Failure Posture — Any Validation Failure Fails the Entire Call

### Context

The original ADR-002 defined a confidence floor check (`source_entry.confidence >=
edge_confidence_floor`) as the primary quality gate on edge writes, with a configurable
`StoreConfig.edge_confidence_floor` field (default 0.1). After Phase 2a scope review, the
confidence floor was dropped entirely from vnc-015. The rationale:

- For `context_store`, the source entry does not yet exist before insert. A new entry's
  initial confidence (Bayesian prior, ~0.5) is always above any meaningful floor, making
  the check vacuously true.
- For `context_correct`, the corrected entry's confidence was the meaningful check site,
  but the anti-spoofing threat model is better addressed by target validation (ownership
  on context_edge, existence/quarantine checks on all edge targets) than by a source
  confidence threshold.
- The floor introduced an anomalous partial-write failure mode: the entry would be written
  but no edges would follow. This inconsistency was identified as a design defect.

With the floor removed, the complete failure posture for all edge writes must be
re-documented. Three validation failures remain:

1. Unknown edge type — `RelationType::from_str()` returns `None`
2. Self-referential edge — `source_id == target_id`
3. Missing or quarantined target — `target_id` does not reference an existing entry, or
   it references a quarantined entry

All three are hard errors. The question is: what happens to the in-progress call when any of
them fires?

Supersedes: original ADR-002 (Confidence Floor Failure Posture — Fail Entire Call), stored as
Unimatrix entry #4419.

### Decision

Any validation failure (unknown edge type, self-referential edge, missing target, quarantined
target) fails the **entire call** — no entry is written, no edges are written.

This is consistent across all three validation categories and both call surfaces
(`edges` param on context_store/context_correct, and context_edge tool):

**For `edges` param on context_store / context_correct:**
- Validation runs before `StoreService.insert()` where possible (type resolution, self-ref).
- Target validation (DB lookup) runs as part of the pre-insert phase — one SELECT per
  target_id. If any target is missing or quarantined, the entire call fails immediately.
  No entry is written. No edges are written.
- Deprecated targets are allowed. The `DependencyOnDeprecated` rule surfaces them.

**For `context_edge` tool:**
- Validation order: capability → source fetch → ownership → source status → self-ref →
  target fetch → target status.
- Any failure at any step returns an error immediately.
- There is no entry write in context_edge (pure graph operation). Failure simply means
  no graph mutation occurs.

**Confidence floor is removed entirely.** No `StoreConfig.edge_confidence_floor` field.
No `BelowConfidenceFloor` variant in `EdgeValidationError`. The removed component:
- Component 8 (confidence_floor Config in config.rs) is dropped from the architecture.
- The `BelowConfidenceFloor` variant is removed from `EdgeValidationError`.
- The `validate_and_write_edges` function signature no longer accepts a `confidence_floor`
  parameter.

**Updated `validate_and_write_edges` signature:**
```rust
pub(crate) async fn validate_and_write_edges(
    store: &Store,
    source_id: u64,
    edges: &[EdgeInput],
    created_at: u64,
) -> Result<(), EdgeValidationError>;
```

**Updated `EdgeValidationError`:**
```rust
pub(crate) enum EdgeValidationError {
    UnknownType { edge_type: String },
    SelfReferential { id: u64 },
    TargetNotFound { target_id: u64 },
    TargetQuarantined { target_id: u64 },
}
```

### Consequences

Easier: the failure posture is consistent and predictable — any validation failure is a clean
pre-write rejection. No partial-write anomaly from confidence checks failing after entry insert.
Callers receive a clear error with the specific failing condition. The `StoreConfig` struct is
simpler (no `edge_confidence_floor` field, no range validation).

Harder: target validation requires one DB SELECT per target_id before any write, adding latency
proportional to the number of edges declared. For typical call sizes (1–5 edges) this is
acceptable and is dominated by existing write latency. The `validate_and_write_edges` function
must hold a reference to `Store` and await async DB reads (already required by the target
validation design).

Related: ADR-001 (validation-first pipeline), ADR-003 (partial-write posture), ADR-010
(target validation query pattern).
