# crt-013: Retrieval Calibration — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-013/SCOPE.md |
| Risk Assessment | product/features/crt-013/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/crt-013/specification/SPECIFICATION.md |
| Architecture | product/features/crt-013/architecture/ARCHITECTURE.md |
| Risk-Test Strategy | product/features/crt-013/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-013/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| coaccess-consolidation | pseudocode/coaccess-consolidation.md | test-plan/coaccess-consolidation.md |
| status-penalty-validation | pseudocode/status-penalty-validation.md | test-plan/status-penalty-validation.md |
| briefing-config | pseudocode/briefing-config.md | test-plan/briefing-config.md |
| status-scan-optimization | pseudocode/status-scan-optimization.md | test-plan/status-scan-optimization.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Resolve co-access signal double-counting by removing two dead code paths (episodic augmentation stub, `co_access_affinity()`), validate crt-010 status penalties with behavior-based integration tests, make briefing semantic neighbor count configurable (default 3), and replace the status scan full-table iteration with SQL aggregation queries. Four components across `unimatrix-engine`, `unimatrix-adapt`, `unimatrix-server`, `unimatrix-store`.

## Resolved Decisions

| Decision | Resolution | Source | ADR |
|----------|-----------|--------|-----|
| W_COAC weight disposition | Option A: delete constant, keep stored weights at 0.92. Zero behavioral change. | SCOPE.md, ARCHITECTURE.md | ADR-001 |
| Co-access mechanism architecture | Keep MicroLoRA (pre-HNSW) + scalar boost (post-rerank). Remove episodic stub and `co_access_affinity()`. Transitional pending Graph Enablement. | ARCHITECTURE.md | ADR-002 |
| Status penalty test assertions | Assert ranking outcomes (deprecated < active), not score constants. Tests survive Graph Enablement constant replacement. | RISK-TEST-STRATEGY.md | ADR-003 |
| Status aggregation Store method | Single `compute_status_aggregates()` returning `StatusAggregates` struct. Separate `load_active_entries_with_tags()` for active entries. | ARCHITECTURE.md, ALIGNMENT-REPORT.md | ADR-004 |

## Files to Create/Modify

### Component 1: Co-Access Signal Consolidation

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/confidence.rs` | Modify | Remove `W_COAC` constant (~line 28), `co_access_affinity()` function (~lines 239-250), 9 associated tests |
| `crates/unimatrix-adapt/src/episodic.rs` | Delete | Remove entire no-op stub module |
| `crates/unimatrix-adapt/src/lib.rs` | Modify | Remove `pub mod episodic` declaration and docstring reference |
| `crates/unimatrix-adapt/src/service.rs` | Modify | Remove `episodic` field, import, constructor init, `episodic_adjustments()` method |

### Component 2: Status Penalty Validation

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/tests/` | Create | Integration tests T-SP-01 through T-SP-06 for penalty ranking validation |

### Component 3: Configurable Briefing Neighbor Count

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/services/briefing.rs` | Modify | Add `semantic_k: usize` field, read `UNIMATRIX_BRIEFING_K` env var, clamp [1, 20], default 3 |
| Server construction site | Modify | Pass `semantic_k` when constructing `BriefingService` |

### Component 4: Status Scan Optimization

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/` | Modify | Add `StatusAggregates` struct, `compute_status_aggregates()`, `load_active_entries_with_tags()` methods |
| `crates/unimatrix-server/src/services/status.rs` | Modify | Replace full-table scan (lines 136-144) with calls to new Store methods |

## Data Structures

```rust
/// Aggregate metrics computed via SQL, avoiding full entries deserialization.
/// Returned by Store::compute_status_aggregates().
pub struct StatusAggregates {
    pub entries_with_supersedes: u64,
    pub entries_with_superseded_by: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: Vec<(String, u64)>,
    pub entries_without_attribution: u64,
}
```

Note: Architecture separates `StatusAggregates` (scalar aggregates via SQL) from `load_active_entries_with_tags()` (entry loading with deserialization + tag joins). This is the correct design per ALIGNMENT-REPORT.md — do not combine them into one struct.

## Function Signatures

```rust
// unimatrix-store — new methods
impl Store {
    pub fn compute_status_aggregates(&self) -> Result<StatusAggregates, StoreError>;
    pub fn load_active_entries_with_tags(&self) -> Result<Vec<EntryRecord>, StoreError>;
}

// unimatrix-server — modified BriefingService
impl BriefingService {
    pub fn new(/* existing params */, semantic_k: usize) -> Self;
    // semantic_k wired to ServiceSearchParams::k at briefing.rs:228
}
```

### Removed Signatures

```rust
// unimatrix-engine/confidence.rs — REMOVE
pub const W_COAC: f64 = 0.08;
pub fn co_access_affinity(partner_count: usize, avg_partner_confidence: f64) -> f64;

// unimatrix-adapt/episodic.rs — DELETE ENTIRE MODULE
pub struct EpisodicAugmenter;

// unimatrix-adapt/service.rs — REMOVE
pub fn episodic_adjustments(&self, result_ids: &[u64], result_scores: &[f64]) -> Vec<f64>;
```

## Constraints

1. **crt-011 dependency**: Must be merged and CI green before implementation begins. Component 2 tests inject deterministic confidence values — isolated from live confidence computation.
2. **No schema migrations**: All tables and columns exist. No new indexes.
3. **No confidence formula changes**: Stored weights remain 0.92 (6 factors). W_COAC removal is dead-code cleanup only.
4. **Backward compatible**: Default briefing k=3 preserves current behavior. Status scan optimization is internal.
5. **Extend existing test infrastructure**: Use `TestServiceContext` and existing integration test patterns. No isolated scaffolding.
6. **SQL equivalence contract**: Comparison test (AC-10) runs both old and new paths on same dataset, asserts field-by-field equality.
7. **Briefing config minimalism**: Field + env var. Clamp [1, 20]. No config framework.
8. **Behavior-based test assertions**: Assert ranking outcomes, not score constants. Tests must survive Graph Enablement's replacement of hardcoded penalties.

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| crt-011 (Session Count Dedup) | Hard | Must be merged before implementation. Confidence data integrity. |
| crt-010 (Status-Aware Retrieval) | Landed | Penalties being validated. Component 2 tests expose any bugs. |
| `unimatrix-engine` | Modify | Remove `co_access_affinity()`, `W_COAC`, associated tests |
| `unimatrix-adapt` | Modify | Remove `episodic.rs` module and references |
| `unimatrix-server` | Modify | Briefing k config, status scan optimization, penalty integration tests |
| `unimatrix-store` | Modify | New `StatusAggregates`, `compute_status_aggregates()`, `load_active_entries_with_tags()` |

## NOT in Scope

- No changes to the 6-factor stored confidence formula (W_BASE through W_TRUST)
- No changes to HNSW vector index structure (handled by crt-010 compaction)
- No new MCP tools or parameters (briefing k is internal config)
- No schema migrations
- No changes to CO_ACCESS pair recording (storage/UsageDedup unmodified)
- No changes to MicroLoRA adaptation logic (crt-006, kept as-is)
- No marker injection fix (#17 item 3)
- No empirical evaluation of MicroLoRA vs scalar boost overlap (deferred to col-015)
- No redistribution of W_COAC weight across other factors (Option A: delete only)

## Alignment Status

**5 PASS, 1 WARN.** No variances requiring approval.

- **WARN: StatusAggregates struct inconsistency** — Architecture defines separate methods (`compute_status_aggregates()` for scalars, `load_active_entries_with_tags()` for entries). Specification's domain model combines them into one struct with `active_entries` field. **Resolution: follow Architecture's two-method design** — it correctly separates lightweight SQL aggregation from heavier deserialization + tag-join. No action needed; implementation naturally resolves this.

All vision checks pass: Trust (penalty validation), Integrity (dead code removal), Learning (pipeline clarity), Invisible Delivery (configurable briefing k), zero cloud dependency, proper milestone discipline (Wave 3, no future capabilities pulled forward).
