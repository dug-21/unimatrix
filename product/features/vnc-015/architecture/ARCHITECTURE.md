# vnc-015: Typed Edge Write Path — Architecture

## System Overview

vnc-015 delivers the write path for typed edges in the Unimatrix knowledge graph. The existing
GRAPH_EDGES table and RelationType enum provide typed edge infrastructure, but no agent-facing
write surface exists. Agents can only observe auto-generated edges (NLI Supports/Contradicts,
CoAccess, S1/S2/S8 structural Informs). The graph stays sparse, making PPR traversal and
Goal traceability thin.

This feature adds:
1. An `edges: Option<Vec<EdgeInput>>` parameter on `context_store` and `context_correct`
2. 10 new RelationType variants covering the full SDLC + research taxonomy
3. Bidirectional Contradicts insert (CoAccess precedent)
4. Target validation on all edge writes (target must exist and not be quarantined)
5. A `context_edge` MCP tool (13th tool) for standalone edge lifecycle management
6. `stale_dependency_edges` in `context_status`
7. A `DependencyOnDeprecated` detection rule in `context_cycle_review`
8. `RelatedTo` added to PPR and graph_expand positive types (`Advances` and `Motivates` are write-only in this feature — Phase 2)

This is Phase 1 of the ASS-057 roadmap. Phase 2 (`context_graph` traversal tool) depends on
this feature having populated the graph. vnc-015 does not introduce any schema migration.

## Component Breakdown

### Component 1: EdgeInput Deserialization (unimatrix-server/src/mcp/tools.rs)

Responsibility: define the `EdgeInput` wire struct, extend `StoreParams` and `CorrectParams`.

```rust
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeInput {
    pub edge_type: String,
    pub target_id: u64,
}
```

`EdgeInput` is inline in `tools.rs`. No new file required; the struct is small and local to the
MCP tool handler parameter structs.

`StoreParams` gains `edges: Option<Vec<EdgeInput>>`.
`CorrectParams` gains `edges: Option<Vec<EdgeInput>>`.

Both fields default to `None` (backward compatible with all existing callers — AC-01, AC-02).

### Component 2: Edge Validation and Write Helper (unimatrix-server/src/mcp/edge_write.rs)

Responsibility: validate edges, perform target validation DB lookups, call `write_graph_edge`
per edge, implement delete and redirect operations, emit the `EDGE_SOURCE_AGENT` constant.

Extracted into a dedicated module `edge_write.rs` to prevent `tools.rs` from growing further
(tools.rs is already 8209 lines; the 500-line rule applies to new modules). This module is
`pub(crate)` and called from `context_store`, `context_correct`, and `context_edge` handlers.

Public surface:

```rust
// Constants
pub(crate) const EDGE_SOURCE_AGENT: &str = "agent";

// Validation error — returned before any write
pub(crate) enum EdgeValidationError {
    UnknownType { edge_type: String },
    SelfReferential { id: u64 },
    TargetNotFound { target_id: u64 },
    TargetQuarantined { target_id: u64 },
}

// Write path entry point (context_store and context_correct)
pub(crate) async fn validate_and_write_edges(
    store: &Store,
    source_id: u64,
    edges: &[EdgeInput],
    created_at: u64,
) -> Result<(), EdgeValidationError>;

// Delete a single edge (context_edge remove mode)
pub(crate) async fn delete_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
) -> Result<(), EdgeDeleteError>;

// Atomic remove+add (context_edge redirect mode)
pub(crate) async fn redirect_graph_edge(
    store: &Store,
    source_id: u64,
    old_target_id: u64,
    new_target_id: u64,
    relation_type: &str,
    created_at: u64,
) -> Result<(), EdgeRedirectError>;
```

Validation contract for `validate_and_write_edges` (all checked before any write):
1. For each edge: `edge_type` maps to a known `RelationType` via `RelationType::from_str()` — unknown → fail
2. For each edge: `source_id != target_id` — self-referential → fail
3. For each edge: `target_id` references an existing, non-quarantined entry via
   `store.get_entry_by_id(target_id)` — missing → TargetNotFound, quarantined → TargetQuarantined,
   deprecated → allowed (see ADR-010)

All three checks run in a single pass over the edges vec, resolving types and validating targets
before any write. First failure aborts and returns immediately. No partial writes.

After validation passes, write loop:
- For each edge, call `write_graph_edge(store, source_id, target_id, relation_type.as_str(), 1.0, created_at, EDGE_SOURCE_AGENT, "")`.
- For `Contradicts` edges, call `write_graph_edge` twice: `(source_id, target_id, ...)` and `(target_id, source_id, ...)` — both in the same synchronous call sequence before handler returns (AC-06).
- `write_graph_edge` returns `bool`. `false` on UNIQUE conflict (INSERT OR IGNORE, idempotent) is not an error. `false` on SQL error is logged inside `write_graph_edge` — do not double-log at call site (pattern #4041).
- If an edge write returns `false` due to infrastructure error after entry insert: log once, continue loop. The entry is not rolled back. This is the accepted partial-write posture (see ADR-003).

`delete_graph_edge` executes a DELETE by `(source_id, target_id, relation_type)` triplet.
For `Contradicts`, both directions are deleted in sequence. Idempotent: 0 rows affected is success.

`redirect_graph_edge` executes an atomic SQLite transaction: DELETE old + INSERT new. For
`Contradicts`, all four rows (two deletes + two inserts) are managed in one transaction.
This is the only transactional operation in `edge_write.rs` — non-transactional writes follow
the ADR-003 partial-write posture; redirect is an explicit exception (see ADR-009).

### Component 3: RelationType Enum Extension (unimatrix-engine/src/graph.rs)

Responsibility: define 10 new variants with all 4 required update sites.

10 new variants: `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`,
`Motivates`, `About`, `RelatedTo`.

All 4 sites must be updated for every variant (pattern #3950):
1. Enum body
2. `as_str()` match arm
3. `from_str()` match arm
4. PPR positive type inclusion (only for `RelatedTo` — all 9 remaining new variants including `Advances` and `Motivates` are write-only in this feature)

A missing `from_str()` arm causes the R-10 guard in `build_typed_relation_graph` Pass 2b to
silently drop rows. Verification gate: grep all 10 variant strings in each of the 4 sites.

### Component 4: PPR and graph_expand Expansion (unimatrix-engine/src/graph_ppr.rs, graph_expand.rs)

Responsibility: add `RelatedTo` to the positive edge type set so PPR mass flows through
broad-associative edges. See ADR-006 for the decision to include `RelatedTo` only.

`graph_ppr.rs`: add `RelatedTo` to `positive_out_degree_weight` and to both `edges_of_type`
calls in `personalized_pagerank`. Same weight factor as the existing four positive types
(`Supports`, `CoAccess`, `Prerequisite`, `Informs`). No algorithm changes.

`graph_expand.rs`: add `RelatedTo` to the positive BFS set in the outgoing traversal loop.

The other 9 new variants (`Advances`, `Motivates`, `Cites`, `Asserts`, `Mentions`, `Refutes`,
`Tests`, `DerivedFrom`, `About`) are NOT added to PPR or graph_expand in this feature — they
are write-only until Phase 2 (`context_graph`). `Advances` and `Motivates` are deferred because
their directed-edge PPR semantics require careful Phase 2 design (see ADR-006).

### Component 5: stale_dependency_edges (unimatrix-store/src/read.rs)

Responsibility: compute the count of GRAPH_EDGES rows where `relation_type='Prerequisite'` and
the source entry `status=1` (Deprecated). Added as a field on `GraphCohesionMetrics` and
computed by `compute_graph_cohesion_metrics()` via one additional SQL query against `read_pool()`.

SQL pattern:
```sql
SELECT COUNT(*) AS stale_count
FROM graph_edges ge
JOIN entries e ON e.id = ge.source_id
WHERE ge.relation_type = 'Prerequisite'
  AND e.status = 1
```

This follows the existing JOIN-to-entries style in `compute_graph_cohesion_metrics()`. The new
field type is `u64` (non-nullable; zero when no stale edges exist).

The SQL filter uses an inclusive relation_type string match, not an exclusive NOT IN. Stale
dependency count is intentionally Prerequisite-only — `Advances`, `Motivates`, and others are
not dependency-assertion edges and stale detection for them is deferred to Phase 2.

### Component 6: DependencyOnDeprecated Detection Rule (unimatrix-observe/src/detection/)

Responsibility: fire a `Warning`-severity `HotspotFinding` when any Prerequisite edge in the
current cycle's entries points to a source entry with `status=Deprecated`.

New file: `unimatrix-observe/src/detection/scope.rs` (extends existing file — `DependencyOnDeprecated`
joins `scope` category alongside `PhaseDurationOutlierRule`).

Struct:
```rust
pub(crate) struct DependencyOnDeprecatedRule {
    stale_edge_pairs: Vec<(u64, u64)>,  // (source_id, target_id) of stale Prerequisite edges
}

impl DependencyOnDeprecatedRule {
    pub fn new(stale_edge_pairs: Vec<(u64, u64)>) -> Self {
        DependencyOnDeprecatedRule { stale_edge_pairs }
    }
}
```

The rule's `detect()` is synchronous. Store access happens in `context_cycle_review` before
`default_rules()` is called — the handler pre-queries stale Prerequisite edges for the current
cycle's feature_cycle entries and passes the pairs at construction time. This matches the
`PhaseDurationOutlierRule` constructor injection pattern (ADR-001 of that feature).

`default_rules()` signature changes from `(history: Option<&[MetricVector]>)` to
`(history: Option<&[MetricVector]>, stale_edges: Vec<(u64, u64)>)`.

`test_default_rules_has_22_rules` must be updated to assert 23.

### Component 7: query_contradicts_edges_for_entry Fix (unimatrix-store/src/read.rs)

Responsibility: correct the asymmetric query to use bidirectional lookup.

Current: `WHERE target_id = ?1 AND relation_type = 'Contradicts'`
Fixed: bidirectional — `WHERE (source_id = ?1 OR target_id = ?1) AND relation_type = 'Contradicts'`

Once Contradicts edges are stored bidirectionally (both A→B and B→A), the `WHERE source_id = ?1`
direction alone is sufficient and avoids the OR clause cost. However, existing unidirectional
Contradicts edges (NLI-written, pre-vnc-015) in production are stored with direction determined
by detection order. The safest fix uses OR to handle the transition period. A follow-up can
simplify once the corpus is confirmed fully bidirectional.

Existing call sites that invoke `query_contradicts_edges_for_entry` must be audited for
behavior change (SR-06). Pattern #3650 confirms the bidirectional Contradicts traversal requires
both Outgoing and Incoming calls — the OR fix satisfies this.

### Component 9: context_edge Handler (unimatrix-server/src/mcp/tools.rs)

Responsibility: the 13th MCP tool. Standalone edge lifecycle management on existing entries.
Supports three modes: add, remove, redirect.

Parameters:
```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeParams {
    pub mode: String,         // "add" | "remove" | "redirect"
    pub source_id: u64,
    pub edge_type: String,
    pub target_id: u64,
    pub new_target_id: Option<u64>,  // required for redirect, rejected for add/remove
}
```

Validation pipeline (ordered, all before any mutation):
1. Capability gate: `Capability::Write` — existing agent permission check
2. Source fetch: `store.get_entry_by_id(source_id)` — not found → error
3. Source status: not Quarantined, not Deprecated — either → error
4. Self-ref check: `source_id != target_id` — equal → error
5. Edge type resolution: `RelationType::from_str(&edge_type)` — None → error
6. Target validation: `target_id` exists and not quarantined (via `validate_target()` in edge_write.rs)
   For redirect: also validate `new_target_id`

Mode dispatch after validation:
- `add`: call `write_graph_edge` (idempotent INSERT OR IGNORE). For Contradicts, both directions.
- `remove`: call `delete_graph_edge`. For Contradicts, both directions. Idempotent.
- `redirect`: call `redirect_graph_edge` (atomic transaction). For Contradicts, all 4 rows.

Pure graph operation: no embedding recompute, no confidence update, no duplicate detection,
no usage recording.

`new_target_id` is only meaningful for `redirect`. If present for `add` or `remove`, the handler
returns an error. If absent for `redirect`, the handler returns an error.

Tool count: this is the 13th MCP tool. Any test asserting exact MCP tool count must be
updated from 12 to 13.

## Component Interactions

```
context_store handler (tools.rs)
  │
  ├─ 1. identity → capability (Capability::Write)
  ├─ 2. validation (title, content, category)
  ├─ 3. [NEW] pre-insert edge validation in validate_and_write_edges:
  │      ├─ unknown edge_type → fail entire call (no entry written)
  │      ├─ source_id == target_id → fail entire call
  │      └─ target_id not found or quarantined → fail entire call
  ├─ 4. StoreService.insert (entry written)
  ├─ 5. duplicate guard → if duplicate: skip edges, return duplicate response
  ├─ 6. [NEW] write edges via edge_write:
  │      ├─ write_graph_edge per non-Contradicts edge
  │      └─ write_graph_edge ×2 for Contradicts edges (bidirectional)
  ├─ 7. confidence recompute
  └─ 8. usage recording

context_correct handler (tools.rs)
  │  (same validation flow; edges attach to corrected NEW entry, not deprecated original)
  └─ edges step runs after correct_result.corrected_entry is available

context_edge handler (tools.rs)  [NEW — 13th MCP tool]
  │
  ├─ 1. capability (Capability::Write)
  ├─ 2. source fetch → get_entry_by_id(source_id)
  ├─ 3. source status → not Quarantined, not Deprecated
  ├─ 4. self-ref check → source_id != target_id
  ├─ 5. edge type resolution → RelationType::from_str
  ├─ 6. target validation → validate_target(target_id) [+ new_target_id for redirect]
  └─ 7. mode dispatch:
         add      → write_graph_edge (idempotent)
         remove   → delete_graph_edge (idempotent, bidirectional for Contradicts)
         redirect → redirect_graph_edge (atomic transaction, bidirectional for Contradicts)

context_cycle_review handler (tools.rs)
  │
  ├─ (existing) load historical MetricVectors
  ├─ [NEW] query stale Prerequisite edges for current cycle's entries
  ├─ default_rules(history_slice, stale_edge_pairs)  ← signature change
  └─ detect_hotspots(attributed, &rules)

context_status handler (tools.rs)
  └─ compute_graph_cohesion_metrics()
       └─ [NEW] stale_dependency_edges: SQL JOIN query (read_pool)
```

## Validation Pipeline — Order and Failure Posture (SR-01, SR-02, SR-03)

The validation pipeline for the `edges` param (context_store / context_correct) runs in two
phases:

**Phase A — Pre-insert validation (type, self-ref, target):**
For each EdgeInput in the `edges` vec, in a single pass:
1. Resolve `edge_type` string → `RelationType` via `from_str()` — unknown type → fail immediately
2. Check `source_id != target_id` — self-referential → fail immediately
3. Fetch `target_id` via `store.get_entry_by_id(target_id)`:
   - Not found → TargetNotFound, fail immediately
   - Status = Quarantined → TargetQuarantined, fail immediately
   - Status = Deprecated → allowed (DependencyOnDeprecated rule surfaces these)
   - Status = Active → allowed

All edges must pass Phase A before any write occurs. First failure aborts the loop and returns
an error to the caller with no state written.

**Phase B — Write:**
4. Insert entry (StoreService.insert)
5. Duplicate guard — if duplicate, skip all edge writes, return duplicate response
6. Write edges via the resolved (RelationType, target_id) pairs from Phase A

**Fail posture (ADR-002)**: Any Phase A failure returns an error to the caller. No entry is
written, no edges are written. Phase B failures (infrastructure errors on edge write after entry
insert) follow the partial-write posture (ADR-003): entry stays, edge write failure is logged.

**Confidence floor is removed.** There is no Phase B confidence check. The `StoreConfig` struct
gains no `edge_confidence_floor` field. The `BelowConfidenceFloor` error variant does not exist.

**context_edge validation pipeline** (separate surface — see Component 9 and ADR-009):
The pipeline is: capability → source fetch → source status → self-ref → edge type → target
validation. All six checks run before any graph mutation. No entry is written (pure graph
operation). Any failure → error returned, no mutation. No ownership check — `agent_id` is not
a reliable ownership anchor in this RBAC model; security gate is `Capability::Write` plus
source entry status.

## Technology Decisions

See individual ADR files:
- ADR-001: Validation-First Pipeline Order (type + self-ref before entry insert)
- ADR-002: Edge Write Failure Posture — Any Validation Failure Fails the Entire Call
  (supersedes original ADR-002: Confidence Floor Failure Posture)
- ADR-003: Partial-Write Blast Radius (infrastructure error, not rolled back)
- ADR-004: DependencyOnDeprecated Constructor Injection
- ADR-005: edge_write Helper Module Extraction
- ADR-006: PPR Positive-Type Expansion — RelatedTo Only (Advances/Motivates deferred to Phase 2)
- ADR-007: from_str() Guard and SR-01 Mitigation (10×4 checklist gate)
- ADR-008: EDGE_SOURCE_AGENT Constant Placement
- ADR-009: context_edge Tool Design (handler structure, validation pipeline, atomic operations)
- ADR-010: Target Validation Query Pattern (DB lookup, failure posture)

## Integration Points

### Existing interfaces consumed:

| Interface | Location | Consumed by |
|-----------|----------|-------------|
| `write_graph_edge(store, source_id, target_id, relation_type, weight, created_at, source, metadata) -> bool` | `nli_detection.rs:78` | edge_write.rs |
| `RelationType::from_str(s: &str) -> Option<Self>` | `graph.rs` | edge_write.rs |
| `RelationType::as_str(&self) -> &'static str` | `graph.rs` | edge_write.rs |
| `StoreService.insert()` | tools.rs (existing) | context_store handler |
| `store.correct_entry()` | tools.rs (existing) | context_correct handler |
| `compute_graph_cohesion_metrics()` | read.rs | context_status handler |
| `default_rules(history)` | detection/mod.rs | context_cycle_review handler |
| `DetectionRule` trait | detection/mod.rs | DependencyOnDeprecatedRule |
| `PhaseDurationOutlierRule::new(history)` | detection/scope.rs | default_rules() |
| `query_contradicts_edges_for_entry` | read.rs | suppress_contradicts (existing) |

### New interfaces introduced:

| Interface | Location | Used by |
|-----------|----------|---------|
| `EdgeInput { edge_type: String, target_id: u64 }` | tools.rs | StoreParams, CorrectParams, EdgeParams |
| `EdgeParams { mode, source_id, edge_type, target_id, new_target_id }` | tools.rs | context_edge handler |
| `EDGE_SOURCE_AGENT: &str` | edge_write.rs (re-export from lib.rs) | edge_write.rs write loop |
| `validate_and_write_edges(store, source_id, edges, created_at) -> Result<(), EdgeValidationError>` | edge_write.rs | context_store handler, context_correct handler |
| `delete_graph_edge(store, source_id, target_id, relation_type) -> Result<(), EdgeDeleteError>` | edge_write.rs | context_edge remove mode |
| `redirect_graph_edge(store, source_id, old_target_id, new_target_id, relation_type, created_at) -> Result<(), EdgeRedirectError>` | edge_write.rs | context_edge redirect mode |
| `EdgeValidationError` enum | edge_write.rs | validate_and_write_edges callers |
| `DependencyOnDeprecatedRule::new(stale_edge_pairs: Vec<(u64, u64)>) -> Self` | detection/scope.rs | default_rules() |
| `default_rules(history: Option<&[MetricVector]>, stale_edges: Vec<(u64, u64)>)` | detection/mod.rs | context_cycle_review handler |
| `GraphCohesionMetrics.stale_dependency_edges: u64` | read.rs | context_status output |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EdgeInput` struct | `{ edge_type: String, target_id: u64 }` — Deserialize + JsonSchema | tools.rs (new) |
| `StoreParams.edges` | `Option<Vec<EdgeInput>>` | tools.rs (new field) |
| `CorrectParams.edges` | `Option<Vec<EdgeInput>>` | tools.rs (new field) |
| `EdgeParams` struct | `{ mode: String, source_id: u64, edge_type: String, target_id: u64, new_target_id: Option<u64> }` — Deserialize + JsonSchema | tools.rs (new) |
| `EDGE_SOURCE_AGENT` | `pub(crate) const &str = "agent"` | edge_write.rs; re-export from unimatrix-server |
| `validate_and_write_edges` | `async fn(store: &Store, source_id: u64, edges: &[EdgeInput], created_at: u64) -> Result<(), EdgeValidationError>` | edge_write.rs (new) |
| `delete_graph_edge` | `async fn(store: &Store, source_id: u64, target_id: u64, relation_type: &str) -> Result<(), EdgeDeleteError>` | edge_write.rs (new) |
| `redirect_graph_edge` | `async fn(store: &Store, source_id: u64, old_target_id: u64, new_target_id: u64, relation_type: &str, created_at: u64) -> Result<(), EdgeRedirectError>` | edge_write.rs (new) |
| `EdgeValidationError` | enum: `UnknownType { edge_type }`, `SelfReferential { id }`, `TargetNotFound { target_id }`, `TargetQuarantined { target_id }` | edge_write.rs (new) |
| `DependencyOnDeprecatedRule` | struct with `pub fn new(stale_edge_pairs: Vec<(u64, u64)>) -> Self` | detection/scope.rs (new) |
| `default_rules` | `fn(history: Option<&[MetricVector]>, stale_edges: Vec<(u64, u64)>) -> Vec<Box<dyn DetectionRule>>` | detection/mod.rs (signature change) |
| `GraphCohesionMetrics.stale_dependency_edges` | `u64` | read.rs (new field) |
| RelationType new variants | `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `Motivates`, `About`, `RelatedTo` | graph.rs |

## Risk Mitigations

**SR-01 — 40-site RelationType update**: The spec must include an explicit 10×4 checklist table
(variant × site). Gate-3a grepping must verify all 10 strings appear in each of the 4 match
arms in `graph.rs`, and `RelatedTo` in both PPR functions. `Advances` and `Motivates` are NOT
in PPR (write-only in this feature). See ADR-006, ADR-007.

**SR-02 — write_graph_edge bool semantics**: The edge write loop in `validate_and_write_edges`
must lead with the three-case contract table before the implementation body. `false` from UNIQUE
conflict is not an error. No budget counters exist for agent-declared edges; the bool result is
used only for warn-and-continue on SQL error. See pattern #4041.

**SR-03 — Partial-write blast radius**: Entry insert and edge writes are not in a single DB
transaction. If edge writes fail after entry insert, the entry exists with no edges. This is
accepted as an infrastructure error (logged, not rolled back). See ADR-003.

**SR-06 — query_contradicts_edges_for_entry behavior change**: The bidirectional fix changes
behavior for existing callers. The spec must audit all callers of this function before writing
new tests. The OR-clause form handles both pre-vnc-015 unidirectional edges and post-vnc-015
bidirectional edges during the transition period.

**SR-05 — Injection interface generality**: The `DependencyOnDeprecated` rule is the first
`DetectionRule` that requires injected store data. The typed `Vec<(u64, u64)>` injection
interface is purpose-designed for this rule. A future rule with different injection needs will
define its own typed parameter. This avoids premature abstraction while establishing the
constructor injection pattern as canonical. See ADR-004.

## Open Questions

The following OQs are unresolved and carry into Stage 3a pseudocode:

1. **`default_rules` signature change impact** (OQ-1): Adding `stale_edges: Vec<(u64, u64)>` to
   `default_rules()` is a breaking change to any caller outside `context_cycle_review`. Stage 3a
   must audit all callers of `default_rules()` and update them. R-10 in RISK-TEST-STRATEGY.md
   covers this. Test `test_default_rules_has_22_rules` must be updated to 23.

2. **Tool count test location** (OQ-4): Stage 3a must identify the specific test(s) asserting
   exact MCP tool count and update from 12 to 13. Likely in `unimatrix-server/src/mcp/tests/`
   or server integration tests.

---

## Closed Questions

These were resolved before Stage 3a and must not be re-opened:

- **OQ-2 (Edge attribution)**: CLOSED. ADR-008 is authoritative. `GRAPH_EDGES.source` and
  `created_by` both store `EDGE_SOURCE_AGENT = "agent"`. Agent identity traceable via
  `ENTRIES.created_by` on the source entry (JOIN). No GRAPH_EDGES schema change.

- **OQ-3 (EntryStatus enum accessibility)**: CLOSED. `Status` enum is `pub` in
  `unimatrix_store::schema` (`schema.rs:10`). `edge_write.rs` in `unimatrix-server` depends on
  `unimatrix-store`. Use `Status::Quarantined` and `Status::Deprecated` directly — no integer
  literals needed.

- **OQ-5 (redirect transaction API)**: CLOSED. `write_pool.begin()` is already the established
  pattern in `write.rs` (4 callsites: lines 22, 101, 198, 233). `redirect_graph_edge` follows
  the same RAII pattern: `let mut txn = store.write_pool_server().begin().await?;` — confirmed
  safe with `write_max_connections >= 2` (lesson #2269).

- **Ownership check on context_edge**: CLOSED — DROPPED. `agent_id` is not a reliable ownership
  anchor in this RBAC model. Validation pipeline is 6 steps: capability → source fetch →
  source status → self-ref → edge type → target validation. No `OwnershipViolation` error.
