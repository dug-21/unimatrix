# vnc-015 Implementation Brief — Typed Edge Write Path + context_edge Tool

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-015/SCOPE.md |
| Architecture | product/features/vnc-015/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-015/specification/SPECIFICATION.md |
| Risk/Test Strategy | product/features/vnc-015/RISK-TEST-STRATEGY.md |
| Scope Risk Assessment | product/features/vnc-015/SCOPE-RISK-ASSESSMENT.md |

---

## Goal

Deliver the agent-facing write path for typed graph edges in Unimatrix. Add an `edges` parameter to `context_store` and `context_correct` for inline edge declaration at creation time, and a new `context_edge` MCP tool (13th tool) for standalone edge lifecycle management (add, remove, redirect) on existing entries. Extend the RelationType enum with 10 new SDLC/research variants, add bidirectional Contradicts semantics, expose stale-dependency observability in `context_status`, and register a `DependencyOnDeprecated` detection rule — all without any schema migration.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| EdgeInput / StoreParams / CorrectParams extension | pseudocode/edge-input-params.md | test-plan/edge-input-params.md |
| edge_write.rs helper module | pseudocode/edge-write.md | test-plan/edge-write.md |
| RelationType enum extension (graph.rs) | pseudocode/relation-type.md | test-plan/relation-type.md |
| PPR and graph_expand expansion | pseudocode/ppr-expand.md | test-plan/ppr-expand.md |
| stale_dependency_edges (read.rs) | pseudocode/stale-dependency.md | test-plan/stale-dependency.md |
| DependencyOnDeprecated detection rule | pseudocode/detection-rule.md | test-plan/detection-rule.md |
| query_contradicts_edges_for_entry fix | pseudocode/contradicts-fix.md | test-plan/contradicts-fix.md |
| context_edge handler (tools.rs) | pseudocode/context-edge-handler.md | test-plan/context-edge-handler.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Validation pipeline order | Edge type resolution runs pre-insert; self-ref check runs post-insert (source_id unknown pre-insert for context_store auto-increment); target validation runs pre-insert via DB SELECT | SCOPE.md, ADR-001 | architecture/ADR-001-validation-first-pipeline.md |
| Failure posture on validation error | Any validation failure (unknown type, self-ref, missing/quarantined target) fails the entire call — no entry written, no edges written. Confidence floor dropped entirely. | SCOPE.md, ADR-002 | architecture/ADR-002-edge-write-failure-posture.md |
| Partial-write blast radius | Entry insert + edge writes are NOT in a single DB transaction. Infrastructure edge-write failure after entry insert is logged, not rolled back, not surfaced to caller. `redirect_graph_edge` is the explicit exception — it IS transactional. | ADR-003 | architecture/ADR-003-partial-write-blast-radius.md |
| DependencyOnDeprecated Store access | Constructor injection: `context_cycle_review` pre-queries stale Prerequisite pairs and passes `Vec<(u64, u64)>` to `DependencyOnDeprecatedRule::new()`. `DetectionRule` trait unchanged. `default_rules()` gains second parameter `stale_edges: Vec<(u64, u64)>`. | ADR-004 | architecture/ADR-004-dependency-on-deprecated-constructor-injection.md |
| Edge-write logic extraction | New `crates/unimatrix-server/src/mcp/edge_write.rs` module (`pub(crate)`). `tools.rs` is 8209 lines; inlining would violate the 500-line new-module rule. | ADR-005 | architecture/ADR-005-edge-write-helper-module.md |
| RelatedTo PPR expansion only | `RelatedTo` added to `positive_out_degree_weight` and BFS set at equal weight to existing 4 positive types. `Advances` and `Motivates` are write-only in this feature — PPR expansion for directed semantic types deferred to Phase 2. Other 8 new variants also write-only. | ADR-006 | architecture/ADR-006-advances-motivates-ppr-weight.md |
| SR-01 mitigation — 10×4 checklist | Explicit 10×4 variant×site checklist table in spec (FR-10). Gate-3a must grep all 10 variants in enum body, as_str(), from_str(), and `RelatedTo` (only) in PPR/BFS. `Advances`/`Motivates` must NOT appear in PPR/BFS (negative check). Per-variant Pass 2b survival integration test required. | ADR-007 | architecture/ADR-007-from-str-guard-sr01-mitigation.md |
| EDGE_SOURCE_AGENT placement | `pub(crate) const EDGE_SOURCE_AGENT: &str = "agent"` in `edge_write.rs`. `write_graph_edge` dual-binds the `source` parameter to both `GRAPH_EDGES.source` and `GRAPH_EDGES.created_by` columns — both will be `"agent"`. AC-18 "edge attribution uses created_by" means attribution is via `ENTRIES.created_by` on the source entry (available via JOIN), not via a field on GRAPH_EDGES. No GRAPH_EDGES schema change in scope. | ADR-008 | architecture/ADR-008-edge-source-agent-constant.md |
| context_edge handler structure | Handler in `tools.rs` (~80–120 lines). Validation pipeline: capability → source fetch → ownership → source status → self-ref → edge type → target validation. remove is idempotent fire-and-forget; redirect uses `pool.begin().await?` RAII transaction (mandatory — lesson #2269 confirms manual BEGIN/COMMIT strings lose data with write_max_connections ≥ 2). If `write_pool_server()` does not expose `begin()`, delivery must expose it. | ADR-009 | architecture/ADR-009-context-edge-tool-design.md |
| Self-referential check timing | For `context_store`/`context_correct` edges param: check runs Phase B (post-insert, using assigned auto-increment ID, before edge writes). Entry is written if triggered; edges are not. For `context_edge`: check runs pre-operation (source_id is known). AC-08 "before entry insert" applies strictly to context_edge only. | ADR-001 | architecture/ADR-001-validation-first-pipeline.md |
| Target validation query | `store.get_entry_by_id(target_id)` for each edge target. Status 0=Active (allowed), 1=Deprecated (allowed), 2=Quarantined (rejected). Single-pass loop: resolve type + self-ref + target validation before any write. First-error abort. | ADR-010 | architecture/ADR-010-target-validation-query-pattern.md |

---

## Files to Create or Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/mcp/edge_write.rs` | **Create** | New `pub(crate)` module: `EdgeInput` struct, `EdgeValidationError` enum, `EDGE_SOURCE_AGENT` constant, `validate_and_write_edges()`, `delete_graph_edge()`, `redirect_graph_edge()` |
| `crates/unimatrix-server/src/mcp/tools.rs` | **Modify** | Extend `StoreParams` and `CorrectParams` with `edges: Option<Vec<EdgeInput>>`; add `context_edge` handler (13th tool); wire edge write step into `context_store` and `context_correct` pipelines |
| `crates/unimatrix-server/src/mcp/mod.rs` (or tools.rs mod declaration) | **Modify** | Declare `pub(crate) mod edge_write;` |
| `crates/unimatrix-engine/src/graph.rs` | **Modify** | Add 10 new RelationType variants with all 3 required sites per variant (enum body, as_str(), from_str()) |
| `crates/unimatrix-engine/src/graph_ppr.rs` | **Modify** | Add `RelatedTo` to `positive_out_degree_weight` and `personalized_pagerank` positive type sets (~3 lines). `Advances` and `Motivates` are NOT added. |
| `crates/unimatrix-engine/src/graph_expand.rs` | **Modify** | Add `RelatedTo` to positive BFS set (~2 lines). `Advances` and `Motivates` are NOT added. |
| `crates/unimatrix-store/src/read.rs` | **Modify** | Add `stale_dependency_edges: u64` field to `GraphCohesionMetrics`; add SQL query in `compute_graph_cohesion_metrics()`; fix `query_contradicts_edges_for_entry` to OR-clause bidirectional query |
| `crates/unimatrix-observe/src/detection/scope.rs` | **Modify** | Add `DependencyOnDeprecatedRule` struct with constructor injection `new(stale_edge_pairs: Vec<(u64, u64)>)` |
| `crates/unimatrix-observe/src/detection/mod.rs` | **Modify** | Register `DependencyOnDeprecated` as 23rd rule; update `default_rules()` signature to accept `stale_edges: Vec<(u64, u64)>`; update `test_default_rules_has_22_rules` to assert 23 |

---

## Data Structures

### EdgeInput
```rust
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeInput {
    pub edge_type: String,   // Must parse via RelationType::from_str(); case-sensitive
    pub target_id: u64,      // Target entry id; must not equal resolved source_id
}
```

### EdgeParams (context_edge wire struct)
```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeParams {
    pub mode:          String,        // "add" | "remove" | "redirect"
    pub source_id:     u64,
    pub edge_type:     String,        // Must parse via RelationType::from_str()
    pub target_id:     u64,
    pub new_target_id: Option<u64>,   // Required for redirect; rejected for add/remove
}
```

### EdgeValidationError
```rust
pub(crate) enum EdgeValidationError {
    UnknownType     { edge_type: String },
    SelfReferential { id: u64 },
    TargetNotFound  { target_id: u64 },
    TargetQuarantined { target_id: u64 },
}
```

### context_edge additional error variants (surface to MCP caller)
| Variant | Trigger |
|---------|---------|
| `SourceFrozen` | Source entry is quarantined or deprecated |

### GraphCohesionMetrics (modified)
Add field: `stale_dependency_edges: u64`

### DependencyOnDeprecatedRule
```rust
pub(crate) struct DependencyOnDeprecatedRule {
    stale_edge_pairs: Vec<(u64, u64)>,  // (source_id, target_id) of stale Prerequisite edges
}
```

### RelationType (post-feature — 16 total variants)
**Existing 6 (unchanged):** `Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`, `Informs`

**10 new variants:**
| Variant | as_str() | PPR/BFS positive |
|---------|----------|-----------------|
| `Advances` | `"Advances"` | **No (write-only — Phase 2)** |
| `Cites` | `"Cites"` | No (write-only) |
| `Asserts` | `"Asserts"` | No (write-only) |
| `Mentions` | `"Mentions"` | No (write-only) |
| `Refutes` | `"Refutes"` | No (write-only) |
| `Tests` | `"Tests"` | No (write-only) |
| `DerivedFrom` | `"DerivedFrom"` | No (write-only) |
| `Motivates` | `"Motivates"` | **No (write-only — Phase 2)** |
| `About` | `"About"` | No (write-only) |
| `RelatedTo` | `"RelatedTo"` | **Yes (added this feature)** |

---

## Function Signatures

```rust
// edge_write.rs — primary entry points

pub(crate) const EDGE_SOURCE_AGENT: &str = "agent";

pub(crate) async fn validate_and_write_edges(
    store: &Store,
    source_id: u64,
    edges: &[EdgeInput],
    created_at: u64,
) -> Result<(), EdgeValidationError>;

pub(crate) async fn delete_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
) -> Result<(), EdgeDeleteError>;

pub(crate) async fn redirect_graph_edge(
    store: &Store,
    source_id: u64,
    old_target_id: u64,
    new_target_id: u64,
    relation_type: &str,
    created_at: u64,
) -> Result<(), EdgeRedirectError>;

// Helper used by both edge_write and context_edge handler
async fn validate_target(
    store: &Store,
    target_id: u64,
) -> Result<(), EdgeValidationError>;

// detection/mod.rs — signature change
pub fn default_rules(
    history: Option<&[MetricVector]>,
    stale_edges: Vec<(u64, u64)>,
) -> Vec<Box<dyn DetectionRule>>;

// detection/scope.rs — new constructor
impl DependencyOnDeprecatedRule {
    pub fn new(stale_edge_pairs: Vec<(u64, u64)>) -> Self;
}
```

### Validation Pipeline — context_store / context_correct (edges param)

Two-phase:
1. **Pre-insert (Phase A)**: For each EdgeInput — (a) `RelationType::from_str()` type resolution, (b) target validation via `store.get_entry_by_id()`. First failure aborts loop; no writes.
2. **Post-insert self-ref check**: `source_id != target_id` after entry insert returns assigned ID.
3. **Duplicate guard**: if `insert_result.duplicate_of.is_some()`, skip all edge writes, return duplicate response.
4. **Write (Phase B)**: `write_graph_edge` per edge; double-write for Contradicts. Infrastructure failures logged, not rolled back.

### Validation Pipeline — context_edge

Ordered (6 steps, all pre-mutation): capability → source fetch → source status (not `Status::Quarantined`, not `Status::Deprecated`) → self-ref → edge type resolution → target validation.

No ownership check. `agent_id` is not a reliable ownership anchor in this RBAC model. The security gate is `Capability::Write` plus source entry status.

### write_graph_edge Three-Case Contract (pattern #4041)

| Return value | Meaning | Action |
|---|---|---|
| `true` | Row inserted | Continue |
| `false` (no Err) | INSERT OR IGNORE hit UNIQUE — already exists | Continue (idempotent) |
| `Err(_)` | Infrastructure error | Log once; do not roll back entry; do not surface to caller |

---

## Constraints

1. No schema migration. `GRAPH_EDGES.relation_type TEXT NOT NULL` accepts new string values immediately. Schema version unchanged.
2. `tools.rs` is 8209 lines. Edge-write logic MUST be extracted to `edge_write.rs` — not inlined.
3. `write_graph_edge` is `pub(crate)` in `nli_detection.rs` (same crate as `tools.rs`). No visibility change required.
4. `DetectionRule` trait is synchronous — `detect()` must not perform blocking I/O. Store data injected at construction time only.
5. `context_correct` passes `&None` for `session_id`. Edge attribution uses `created_by = EDGE_SOURCE_AGENT` (`"agent"`). This is correct per ADR-008.
6. Bidirectional Contradicts: both direction rows must be written before the handler returns (not deferred to a background tick). Sequential fire-and-forget, not transactional (ADR-003).
7. `redirect_graph_edge` MUST use `pool.begin().await?` RAII transaction (not raw `BEGIN`/`COMMIT` SQL strings — lesson #2269). All 4 SQL statements for Contradicts redirect must execute against `&mut *txn`.
8. `context_edge` is the 13th MCP tool. Any test asserting exact tool count must be updated from 12 to 13.
9. `default_rules()` signature change is a breaking API change. All callers in `tools.rs` and `detection/mod.rs` tests must be updated simultaneously.
10. No ownership check on `context_edge`. `agent_id` is not a reliable ownership anchor in this RBAC model. Security is `Capability::Write` + source entry status (not Quarantined, not Deprecated). `OwnershipViolation` error variant does not exist.

---

## Dependencies

### Crates (all within workspace — no external deps added)

| Crate | Change |
|-------|--------|
| `unimatrix-engine` | Modified — `graph.rs` (10 new RelationType variants), `graph_ppr.rs`, `graph_expand.rs` |
| `unimatrix-server` | Modified — `tools.rs` (params, handler), new `edge_write.rs` module |
| `unimatrix-store` | Modified — `read.rs` (`stale_dependency_edges`, `query_contradicts_edges_for_entry` fix) |
| `unimatrix-observe` | Modified — `detection/scope.rs` (new rule), `detection/mod.rs` (registration, signature) |

### Existing Functions Reused (no modification)

| Function | Location | Used by |
|----------|----------|---------|
| `write_graph_edge(...)` | `nli_detection.rs:78–118` | `edge_write.rs` write path |
| `store.get_entry_by_id(id)` | `read.rs` | `validate_target()` helper |
| `RelationType::from_str()` / `as_str()` | `graph.rs` | `edge_write.rs` type resolution |
| `StoreService.insert()` / `correct_entry()` | `tools.rs` | unchanged pipeline steps |
| `compute_graph_cohesion_metrics()` | `read.rs` | `context_status` (gains new field) |

---

## NOT in Scope

- `context_graph` traversal tool (Phase 2 — depends on graph population from this feature)
- Any schema migration
- `context_batch_write` tool (HNSW atomicity unsolved — OSC-6 in ASS-057)
- Auto-transfer of edges when a dependency target is superseded (`context_correct` does not cascade)
- `metadata` column on entries (separate migration decision)
- NLI contradiction scoped to Claims only (Phase 3 intelligence)
- PPR/graph_expand positive-type expansion beyond `RelatedTo` (all other 9 new variants including `Advances` and `Motivates` are write-only; deferred to Phase 2)
- Config extension for `ppr_positive_edge_types` or `symmetric_edge_types`
- Source ownership validation on `context_store`/`context_correct` (vacuously true)
- `context_edge` bulk/batch variant (single operation per call)
- `StoreConfig.edge_confidence_floor` field (confidence floor dropped entirely)
- Per-edge agent attribution in GRAPH_EDGES `created_by` column (deferred)
- `RelatedTo` bidirectionality (left to architect; not mandated)
- `resolve_supersessions` parameter, as-of timestamps, Thesis status lifecycle

---

## Alignment Status

No ALIGNMENT-REPORT.md was produced for this feature (design session did not include a vision guardian agent). The feature was scoped and designed through SCOPE.md, a risk assessment, and an architect session. Alignment observations from design artifacts:

- **On-vision**: Feature directly advances PPR graph density and Goal traceability, which are core to the Unimatrix self-learning vision. `RelatedTo` PPR inclusion enables the broad associative discovery the product vision describes. `Advances` and `Motivates` are write-only in this feature; their directed-edge PPR semantics are deferred to Phase 2.
- **Phase coherence**: vnc-015 is correctly placed as Phase 1 of the ASS-057 roadmap. Phase 2 (`context_graph`) is explicitly out of scope and deferred.
- **Scope tightness**: The scope has been appropriately bounded — 15 non-goals listed, confidence floor dropped after Phase 2a review, no scope creep observed.
- **Open architecture questions**: 2 OQs carry into Stage 3a. OQ-1 (default_rules caller audit — R-10 covers it) and OQ-4 (tool count test location). OQ-2, OQ-3, OQ-5 closed: edge attribution = EDGE_SOURCE_AGENT per ADR-008; Status enum importable from unimatrix_store::schema; write_pool.begin() RAII confirmed (4 callsites in write.rs).

---

## 10×4 Variant × Site Compliance Checklist (Gate-3a Verification Required)

Gate-3a must grep each cell. Intentionally absent entries are not errors.

| Variant | graph.rs enum | graph.rs as_str() | graph.rs from_str() | graph_ppr.rs positive | graph_expand.rs positive |
|---------|:---:|:---:|:---:|:---:|:---:|
| `Advances` | required | required | required | **intentionally absent (Phase 2)** | **intentionally absent (Phase 2)** |
| `Cites` | required | required | required | intentionally absent | intentionally absent |
| `Asserts` | required | required | required | intentionally absent | intentionally absent |
| `Mentions` | required | required | required | intentionally absent | intentionally absent |
| `Refutes` | required | required | required | intentionally absent | intentionally absent |
| `Tests` | required | required | required | intentionally absent | intentionally absent |
| `DerivedFrom` | required | required | required | intentionally absent | intentionally absent |
| `Motivates` | required | required | required | **intentionally absent (Phase 2)** | **intentionally absent (Phase 2)** |
| `About` | required | required | required | intentionally absent | intentionally absent |
| `RelatedTo` | required | required | required | **REQUIRED** | **REQUIRED** |

---

## Critical Risk Summary

| Risk | Severity | Mitigation |
|------|----------|-----------|
| R-01: from_str() arm missing — silent row-drop at Pass 2b (no compile error) | Critical | 10×4 checklist gate + per-variant Pass 2b integration test (AC-14) |
| R-02: redirect_graph_edge transaction via raw BEGIN/COMMIT — data loss (lesson #2269) | Critical | Use `pool.begin().await?` RAII — mandatory code review gate |
| R-03: write_graph_edge bool semantics misread — spurious errors or missing logs | Critical | Three-case contract table must precede loop body (pattern #4041) |
| R-04: Bidirectional Contradicts partial write — asymmetric graph, no rollback, no signal | Critical | Both directions tested per surface; both writes in same handler execution |
| R-05: redirect partial failure — old edge deleted, new edge not inserted (data loss) | Critical | RAII transaction on redirect_graph_edge; rollback-on-failure test required |
| R-10: default_rules() signature change breaks all callers | High | Atomic update across all callers; CI compile gate |
| R-07: query_contradicts_edges_for_entry behavior change breaks existing callers | High | Full call-site audit before implementation; transition-period compatibility tests |
