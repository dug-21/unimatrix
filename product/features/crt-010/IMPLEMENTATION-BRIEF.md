# crt-010: Status-Aware Retrieval — Implementation Brief

**Tracking:** [GH #119](https://github.com/anthropics/unimatrix/issues/119)

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-010/SCOPE.md |
| Scope Risk Assessment | product/features/crt-010/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-010/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-010/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/crt-010/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-010/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1: RetrievalMode + SearchService Status Logic | pseudocode/c1-retrieval-mode.md | test-plan/c1-retrieval-mode.md |
| C2: Supersession Candidate Injection | pseudocode/c2-supersession-injection.md | test-plan/c2-supersession-injection.md |
| C3: Co-Access Deprecated Exclusion | pseudocode/c3-coaccess-exclusion.md | test-plan/c3-coaccess-exclusion.md |
| C4: UDS Path Hardening | pseudocode/c4-uds-hardening.md | test-plan/c4-uds-hardening.md |
| C5: MCP Filter Asymmetry Fix | pseudocode/c5-mcp-asymmetry.md | test-plan/c5-mcp-asymmetry.md |
| C6: Vector Index Pruning During Compaction (ALREADY SATISFIED — verification test only) | pseudocode/c6-compaction-pruning.md | test-plan/c6-compaction-pruning.md |
| C7: Penalty Constants | pseudocode/c7-penalty-constants.md | test-plan/c7-penalty-constants.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Harden Unimatrix's retrieval pipeline so that entry lifecycle signals (deprecation, supersession, quarantine) directly influence search ranking and filtering. UDS (silent injection) delivers only Active, non-superseded entries; MCP search aggressively de-ranks deprecated entries while keeping them accessible; supersession chains inject successor candidates; co-access boost excludes deprecated entries; and vector compaction prunes stale embeddings.

## Resolved Decisions

| Decision | Resolution | Source | ADR |
|----------|-----------|--------|-----|
| Retrieval mode signaling | `RetrievalMode` enum (`Strict`, `Flexible`) on `ServiceSearchParams`, default `Flexible` | Architecture C1, SR-09 | ADR-001 |
| Successor similarity computation | Cosine similarity from stored embedding via `VectorIndex::get_embedding()`, not re-embedding | Architecture C2, SR-01 | ADR-002 |
| Supersession chain depth | Single-hop only — follow one level of `superseded_by`, no transitive chains | Architecture C2, SCOPE non-goal | ADR-003 |
| Co-access filtering interface | `deprecated_ids: &HashSet<u64>` parameter on boost functions — no server-crate types in engine | Architecture C3, SR-07 | ADR-004 |
| Penalty multiplier values | `DEPRECATED_PENALTY = 0.7`, `SUPERSEDED_PENALTY = 0.5` as named constants in `confidence.rs` | Architecture C7, SR-02 | ADR-005 |

## Files to Create/Modify

### New Files

| Path | Description |
|------|-------------|
| (none) | No new files — all changes are modifications to existing modules |

### Modified Files

| Path | Description |
|------|-------------|
| `crates/unimatrix-server/src/services/search.rs` | Add `RetrievalMode` enum, status filtering step (6a), supersession injection step (6b), penalty application during re-rank |
| `crates/unimatrix-engine/src/confidence.rs` | Add `DEPRECATED_PENALTY`, `SUPERSEDED_PENALTY` constants and `cosine_similarity()` helper |
| `crates/unimatrix-engine/src/coaccess.rs` | Add `deprecated_ids: &HashSet<u64>` to `compute_search_boost`, `compute_briefing_boost`, and internal `compute_boost_internal` |
| `crates/unimatrix-server/src/uds/listener.rs` | Set `RetrievalMode::Strict` in `handle_context_search`; filter deprecated from briefing injection history; remove `[deprecated]` indicator branch |
| `crates/unimatrix-server/src/mcp/tools.rs` | Set `RetrievalMode::Flexible` always; honor explicit `status` parameter to bypass penalties |
| `crates/unimatrix-server/src/infra/coherence.rs` | No changes — compaction already filters to Active (col-013). C6 verification test only. |
| `crates/unimatrix-server/src/services/status.rs` | No changes — `run_maintenance()` already receives `active_entries` filtered to `Status::Active` (`status.rs:175-181`). C6 verification test only. |
| `crates/unimatrix-vector/src/index.rs` | Add `get_embedding(entry_id: u64) -> Option<Vec<f32>>` method |
| `crates/unimatrix-core/src/async_wrappers.rs` | Add `AsyncVectorStore::get_embedding` async wrapper |
| `crates/unimatrix-server/src/services/briefing.rs` | Thread `deprecated_ids` to `compute_briefing_boost` calls |

## Data Structures

### New Types

```rust
// crates/unimatrix-server/src/services/search.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RetrievalMode {
    Strict,
    #[default]
    Flexible,
}
```

### Modified Types

```rust
// crates/unimatrix-server/src/services/search.rs
pub(crate) struct ServiceSearchParams {
    // ... existing fields ...
    pub(crate) retrieval_mode: RetrievalMode, // NEW — defaults to Flexible
}
```

### Existing Types (unchanged, referenced)

```rust
// crates/unimatrix-store/src/schema.rs
pub struct EntryRecord {
    pub status: Status,
    pub superseded_by: Option<u64>,
    pub confidence: f64,
    // ... other fields ...
}

pub enum Status { Active, Deprecated, Quarantined, Proposed }
```

## Function Signatures

### New Functions

```rust
// crates/unimatrix-engine/src/confidence.rs
pub const DEPRECATED_PENALTY: f64 = 0.7;
pub const SUPERSEDED_PENALTY: f64 = 0.5;

/// Cosine similarity between two f32 vectors, returned as f64 for scoring precision.
/// Assumes L2-normalized inputs. Returns 0.0 for zero-length or mismatched vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64;
```

```rust
// crates/unimatrix-vector/src/index.rs
impl VectorIndex {
    /// Retrieve the stored embedding for an entry. Returns None if entry has no mapping.
    pub fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>>;
}
```

```rust
// crates/unimatrix-core/src/async_wrappers.rs
impl AsyncVectorStore {
    pub async fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>>;
}
```

### Modified Signatures

```rust
// crates/unimatrix-engine/src/coaccess.rs — all gain deprecated_ids parameter
pub fn compute_search_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
    deprecated_ids: &HashSet<u64>,  // NEW
) -> HashMap<u64, f64>;

pub fn compute_briefing_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
    deprecated_ids: &HashSet<u64>,  // NEW
) -> HashMap<u64, f64>;

fn compute_boost_internal(
    // ... existing params ...
    deprecated_ids: &HashSet<u64>,  // NEW
) -> HashMap<u64, f64>;
```

## Pipeline Position

```
Query → Embed → HNSW Search → Quarantine Filter (existing)
  → [NEW Step 6a: Status Filter/Penalty Marking]
  → [NEW Step 6b: Supersession Candidate Injection]
  → Re-rank (with penalty multipliers)
  → [CHANGED: Co-access Boost with deprecated exclusion]
  → Truncate → Floors → Return
```

### Strict Mode (UDS) — Step 6a

Drop all entries where `status != Active` OR `superseded_by.is_some()`. Zero tolerance.

### Flexible Mode (MCP) — Step 6a

Keep deprecated entries. Mark for penalty at re-rank:
- Deprecated (no supersession): multiply final score by `DEPRECATED_PENALTY` (0.7)
- Superseded (`superseded_by.is_some()`): multiply final score by `SUPERSEDED_PENALTY` (0.5)

### Supersession Injection — Step 6b

1. **Skip entirely** if caller provided an explicit `status` filter set to `Deprecated` (FR-6.2) — the agent has a reason for requesting deprecated content and can follow `superseded_by` references themselves
2. Scan remaining results for entries with `superseded_by` set
3. Collect successor IDs, batch-fetch from store (single read)
4. For each successor: inject if Active, not already in results, not itself superseded
5. Compute cosine similarity between query embedding and successor's stored embedding
6. Injected successors enter re-rank pipeline with no special treatment

## Constraints

- No schema changes — `superseded_by`, `status`, all fields exist (NFR-3.1)
- No new MCP tools or parameters (NFR-4.1)
- No confidence formula changes — additive weighted composite (crt-002) unchanged
- Single-hop supersession only — no transitive chain traversal (ADR-003)
- Engine crate decoupled — `HashSet<u64>` interface, no server types (ADR-004)
- Backward-compatible SearchService API — default `Flexible` preserves current behavior (ADR-001)
- p95 search latency regression < 15% on 200-entry KB with 50% deprecation (NFR-1.1)
- Explicit `status: Deprecated` filter disables supersession injection and penalties (FR-6.2)
- Empty strict-mode results return empty set, never fallback to flexible (FR-1.5)

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `unimatrix-server` | Internal | Primary change target (SearchService, UDS, MCP, coherence) |
| `unimatrix-engine` | Internal | Co-access filtering, penalty constants, cosine similarity |
| `unimatrix-vector` | Internal | New `get_embedding` method on VectorIndex (SCOPE says "no changes" but ADR-002 requires it — accepted variance) |
| `unimatrix-core` | Internal | New `AsyncVectorStore::get_embedding` async wrapper (accepted variance) |
| `unimatrix-store` | Internal | No changes — existing fields used |
| crt-002 | Completed | Confidence scoring pipeline (unchanged) |
| crt-004 | Completed | Co-access storage and boost mechanism (extended) |
| crt-005 | Completed | Compaction infrastructure (extended) |
| ass-015 (GH #118) | Research | Validated findings on status signal gaps |

No external crate additions.

## NOT in Scope

- New MCP tools or parameters
- Confidence formula changes (crt-002 weights/factors)
- Schema migrations — no new tables, fields, or version bump
- Quarantine behavior changes — already working correctly
- Multi-hop supersession traversal — single-hop only, transitive chains deferred
- Restore-from-deprecated workflows — re-embedding is acceptable cost
- Embedding model changes
- Runtime-configurable penalty values — compile-time constants only
- Fallback from strict to flexible mode — UDS returns empty, not relaxed

## Alignment Status

**Overall: PASS with 1 WARN, 1 RESOLVED**

### WARN 1: VectorIndex "No Changes" Contradiction

SCOPE's Affected Crates table says `unimatrix-vector: No changes`. Architecture adds `VectorIndex::get_embedding()` — two new public methods in unimatrix-vector and unimatrix-core. This is an internal SCOPE inconsistency: Component 2 describes fetching embeddings from the vector index (option a), which necessarily requires a new method. Architecture correctly resolves via ADR-002. **Accepted** — no SCOPE amendment needed.

### WARN 2: R-08 Post-Compaction Supersession Injection Unreachable — RESOLVED

This is not a new behavior introduced by crt-010. Since col-013, compaction already runs on a background tick (`background.rs:234-257`) with entries filtered to `Status::Active` (`status.rs:175-181`). Deprecated entries are already excluded from HNSW rebuilds. The design tension (injection unreachable post-compaction) is the existing status quo — pre-compaction, injection provides recall boost; post-compaction, stale entries are gone entirely (net positive). C6 scope is reduced to a verification test confirming Active successors remain findable via their own embeddings.

### Open Questions from Architecture

1. **BriefingService co-access**: All call sites of `compute_briefing_boost` must thread `deprecated_ids`. Implementation should audit all callers.
2. **hnsw_rs point data retrieval**: `get_point_indexation().get_point_data()` needs verification. Fallback: re-embed via `EmbedService` (higher latency, functionally equivalent).

## Critical Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| R-01: hnsw_rs `get_embedding` API may not work | Critical | Verify API first; fallback to re-embedding via EmbedService |
| R-08: Post-compaction supersession injection unreachable | Resolved | Not a new behavior — col-013 background tick already excludes deprecated. Verification test only. |
| R-02: Penalty multipliers may cause incorrect ranking at extreme similarity gaps | High | Ranking invariant tests (AC-02, AC-03) |
| R-06: Co-access signature change breaks all callers | High | Update all call sites; pass empty HashSet for backward compat |
