# crt-013: Retrieval Calibration — Architecture

## System Overview

crt-013 calibrates the retrieval pipeline by resolving four independent issues that have accumulated across crt-004, crt-006, and crt-010:

1. **Co-access signal consolidation** — Four mechanisms consume CO_ACCESS data at different pipeline stages. Two are dead code. Architecture formalizes the surviving two-mechanism design.
2. **Status penalty validation** — crt-010 introduced multiplicative penalties but no integration tests. This feature adds behavior-based ranking tests.
3. **Briefing neighbor count** — Hardcoded `k: 3` limits recall. Made configurable.
4. **Status scan optimization** — Full entries table scan replaced with SQL aggregation queries.

The feature is subtractive (removing dead code), additive (new tests, new Store methods), and configurational (briefing k). No new cross-crate interfaces are introduced.

## Component Breakdown

### Component 1: Co-Access Signal Consolidation

**Crates:** `unimatrix-engine`, `unimatrix-adapt`

**Responsibility:** Remove dead code paths that consume CO_ACCESS data, leaving two well-defined mechanisms.

**Current state (4 mechanisms):**

| Mechanism | Stage | Location | Status |
|-----------|-------|----------|--------|
| MicroLoRA adaptation | Pre-HNSW | `unimatrix-adapt/src/service.rs` | **Keep** |
| Co-access boost | Post-rerank | `unimatrix-engine/src/coaccess.rs` | **Keep** |
| Co-access affinity | Query-time confidence | `unimatrix-engine/src/confidence.rs:239` | **Remove** (dead code) |
| Episodic augmentation | Post-search | `unimatrix-adapt/src/episodic.rs` | **Remove** (no-op stub) |

**Target state (2 mechanisms):**

| Mechanism | Stage | Max Effect | Rationale |
|-----------|-------|------------|-----------|
| MicroLoRA adaptation | Pre-HNSW embedding shift | Embedding-space adjustment | Most principled — operates in embedding space via learned LoRA weights |
| Co-access boost | Post-rerank additive | +0.03 (search), +0.01 (briefing) | Lightweight, interpretable, effective tiebreaker |

**Removals:**

1. **`co_access_affinity()` + `W_COAC` constant** (`confidence.rs:239-250`, `confidence.rs:28`)
   - Grep confirms: `W_COAC` referenced only in `confidence.rs`. `co_access_affinity()` called only from tests in `confidence.rs`. No dynamic dispatch, no conditional compilation. (SR-01 mitigated)
   - **W_COAC disposition: Option A** — Delete constant and function. Keep stored weights at 0.92. The 0.08 was never used in stored confidence computation. Zero behavioral change. (ADR-001)
   - Remove 8 test functions: `co_access_affinity_*` and `co_access_affinity_returns_f64`
   - Remove `weight_sum_effective_invariant` test (asserts stored + W_COAC = 1.0)
   - Keep `weight_sum_stored_invariant` test (asserts stored = 0.92)

2. **`episodic.rs` module** (`unimatrix-adapt/src/episodic.rs`)
   - Grep confirms: referenced from `lib.rs` (pub mod + no re-export), `service.rs` (field + constructor + method). `episodic_adjustments()` never called outside `service.rs`. (SR-04 mitigated)
   - Remove: `episodic.rs`, `pub mod episodic` from `lib.rs`, field/constructor/method from `service.rs`
   - Remove `episodic_adjustments()` method from `AdaptationService`

**Affected files:**

| File | Changes |
|------|---------|
| `crates/unimatrix-engine/src/confidence.rs` | Remove `W_COAC`, `co_access_affinity()`, 9 tests |
| `crates/unimatrix-adapt/src/episodic.rs` | Delete entire file |
| `crates/unimatrix-adapt/src/lib.rs` | Remove `pub mod episodic` and docstring reference |
| `crates/unimatrix-adapt/src/service.rs` | Remove `episodic` field, import, constructor init, `episodic_adjustments()` method |

### Component 2: Status Penalty Validation

**Crate:** `unimatrix-server` (integration tests)

**Responsibility:** Prove crt-010 penalties produce correct ranking outcomes via behavior-based tests.

**Design principle (human framing note #1):** Tests assert ranking outcomes (deprecated ranks below active), not specific score values. Graph Enablement will replace the hardcoded 0.7/0.5 constants with graph-topology-derived scoring — integration tests must survive that transition.

**Test cases:**

| Test | Assertion | Mode |
|------|-----------|------|
| T-SP-01: Deprecated below active | Deprecated entry (higher similarity) ranks below active entry | Flexible |
| T-SP-02: Superseded below active | Superseded entry (higher similarity) ranks below active entry | Flexible |
| T-SP-03: Strict mode exclusion | Deprecated and superseded entries absent from results | Strict |
| T-SP-04: Co-access exclusion | Deprecated entries receive no co-access boost | Flexible |
| T-SP-05: Deprecated-only query | Results returned (not empty) when only deprecated entries match | Flexible |
| T-SP-06: Superseded with successor | Successor injected and ranks above superseded entry | Flexible |

**Test design (SR-02, SR-06 mitigation):**

- Tests use `TestServiceContext` (existing integration test infrastructure)
- Inject pre-computed embeddings with known cosine similarity via direct vector store insertion
- Assert on **relative ranking** (`result[0].entry.status == Active`, `result[0].entry.id == active_id`) not absolute score values
- Use deterministic confidence values set directly on entries, isolating from crt-011 confidence computation
- Do not assert specific penalty constants (0.7, 0.5) — these will be replaced by graph-derived values

### Component 3: Configurable Briefing Neighbor Count

**Crate:** `unimatrix-server`

**Responsibility:** Replace hardcoded `k: 3` at `briefing.rs:228` with a configurable parameter.

**Design (SR-05 mitigation):** Minimal approach — no new config struct. Add `semantic_k: usize` field to `BriefingService` with default 3. Follow existing pattern (no server-wide config struct exists — only `ContradictionConfig` is a local config struct).

**Changes:**

| File | Change |
|------|--------|
| `services/briefing.rs` | Add `semantic_k: usize` field to `BriefingService`, wire to `ServiceSearchParams::k` at line 228, env var `UNIMATRIX_BRIEFING_K` with fallback to 3 |
| Server construction site | Pass `semantic_k` when constructing `BriefingService` |

**Configuration:** Read `UNIMATRIX_BRIEFING_K` env var at `BriefingService::new()`. Parse as usize, clamp to [1, 20], default 3.

### Component 4: Status Scan Optimization

**Crate:** `unimatrix-server`, `unimatrix-store`

**Responsibility:** Replace full entries table scan in `status.rs:136-144` with SQL aggregation queries.

**Current state:** `SELECT {ENTRY_COLUMNS} FROM entries` loads all entries, then iterates in Rust to compute:
- `entries_with_supersedes` (count where supersedes IS NOT NULL)
- `entries_with_superseded_by` (count where superseded_by IS NOT NULL)
- `total_correction_count` (SUM of correction_count)
- `trust_source_distribution` (GROUP BY trust_source)
- `entries_without_attribution` (count where created_by = '')
- `active_entries` (WHERE status = 'Active', with tags)

**Target state:** SQL aggregation queries for scalar metrics. Targeted query for active entries only.

**New Store method:**

```rust
/// Aggregate metrics computed via SQL, avoiding full entries deserialization.
pub struct StatusAggregates {
    pub entries_with_supersedes: u64,
    pub entries_with_superseded_by: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: Vec<(String, u64)>,
    pub entries_without_attribution: u64,
}

impl Store {
    pub fn compute_status_aggregates(&self) -> Result<StatusAggregates, StoreError>;
    pub fn load_active_entries_with_tags(&self) -> Result<Vec<EntryRecord>, StoreError>;
}
```

**SQL queries:**

```sql
-- Single compound query for scalar aggregates
SELECT
    SUM(CASE WHEN supersedes IS NOT NULL THEN 1 ELSE 0 END),
    SUM(CASE WHEN superseded_by IS NOT NULL THEN 1 ELSE 0 END),
    COALESCE(SUM(correction_count), 0),
    SUM(CASE WHEN created_by = '' OR created_by IS NULL THEN 1 ELSE 0 END)
FROM entries;

-- Trust source distribution
SELECT CASE WHEN trust_source = '' THEN '(none)' ELSE trust_source END,
       COUNT(*)
FROM entries
GROUP BY 1;

-- Active entries with tags (replaces filtered iteration)
SELECT {ENTRY_COLUMNS} FROM entries WHERE status = 'Active';
-- then load_tags_for_entries() on the resulting IDs
```

**Outcome stats** remain as Rust iteration over active entries (tags are needed, and outcome entries are a small subset). The full table scan is replaced; the active-only scan remains. (SR-03 mitigation)

**Equivalence contract (SR-03, AC-10):** Comparison test runs both paths (old full-scan and new SQL aggregation) on a test dataset and asserts field-by-field equality on `StatusAggregates`. Known divergences: none expected — NULL handling in SQL uses CASE expressions matching the Rust logic exactly (`trust_source = ''` → `(none)`, `created_by = ''` → counted).

**SR-07 (indexes):** The entries table already has indexes on `status` (used by counters). The aggregation queries scan the full table once (same I/O as before) but avoid Rust deserialization overhead. No new indexes required at current scale.

## Component Interactions

```
                    ┌─────────────────────────────────────────┐
                    │            Search Pipeline              │
                    │                                         │
  Query ──► Embed ──► MicroLoRA adapt ──► HNSW search        │
                    │         (C1: kept)       │              │
                    │                          ▼              │
                    │                    Fetch entries         │
                    │                          │              │
                    │                          ▼              │
                    │              Status filter/penalty      │
                    │              (C2: tests validate)       │
                    │                          │              │
                    │                          ▼              │
                    │                    Re-rank (rerank_score)│
                    │                          │              │
                    │                          ▼              │
                    │               Co-access boost           │
                    │             (C1: kept, +0.03 max)       │
                    │                          │              │
                    │                          ▼              │
                    │                    Truncate to k         │
                    └─────────────────────────────────────────┘

  Briefing: uses SearchService with configurable k (C3)
  Status:   SQL aggregates (C4) + active entries for coherence
```

**Removed from pipeline:** `co_access_affinity()` (was never in pipeline — dead code), `episodic_adjustments()` (was never called from pipeline).

## Integration Surface

| Integration Point | Type/Signature | Source | Change |
|-------------------|---------------|--------|--------|
| `co_access_affinity()` | `pub fn co_access_affinity(partner_count: usize, avg_partner_confidence: f64) -> f64` | `unimatrix-engine/src/confidence.rs:239` | **Remove** |
| `W_COAC` | `pub const W_COAC: f64 = 0.08` | `unimatrix-engine/src/confidence.rs:28` | **Remove** |
| `EpisodicAugmenter` | `pub struct EpisodicAugmenter` | `unimatrix-adapt/src/episodic.rs` | **Remove (entire module)** |
| `AdaptationService::episodic_adjustments()` | `pub fn episodic_adjustments(&self, result_ids: &[u64], result_scores: &[f64]) -> Vec<f64>` | `unimatrix-adapt/src/service.rs:240` | **Remove** |
| `BriefingService` | `pub(crate) struct BriefingService` | `unimatrix-server/src/services/briefing.rs` | **Add `semantic_k: usize` field** |
| `Store::compute_status_aggregates()` | `pub fn compute_status_aggregates(&self) -> Result<StatusAggregates, StoreError>` | `unimatrix-store/src/` | **New** |
| `Store::load_active_entries_with_tags()` | `pub fn load_active_entries_with_tags(&self) -> Result<Vec<EntryRecord>, StoreError>` | `unimatrix-store/src/` | **New** |
| `StatusAggregates` | `pub struct StatusAggregates { ... }` | `unimatrix-store/src/` | **New** |

## ADR Summary

| ADR | Title | Decision |
|-----|-------|----------|
| ADR-001 | W_COAC Disposition: Delete Dead Weight Constant | Option A — delete W_COAC and co_access_affinity(), keep stored weights at 0.92 |
| ADR-002 | Two-Mechanism Co-Access Architecture | Keep MicroLoRA (pre-HNSW) + scalar boost (post-rerank); transitional design pending Graph Enablement |
| ADR-003 | Behavior-Based Status Penalty Tests | Assert ranking outcomes not score values; tests survive constant changes |
| ADR-004 | Single StatusAggregates Store Method | One SQL round-trip returning aggregate struct, not per-metric methods |

## Risk Mitigations

| Risk | Mitigation |
|------|------------|
| SR-01: W_COAC removal cascades | Grep confirms W_COAC in confidence.rs only. co_access_affinity() in test-only call sites. Compiler verifies. |
| SR-02: crt-011 dependency | Component 2 tests inject deterministic confidence values. No dependency on live confidence computation. |
| SR-03: SQL equivalence | Comparison test runs both paths on same dataset, asserts field-by-field equality on StatusAggregates. |
| SR-04: Episodic removal breaks imports | Grep confirms 3 files: episodic.rs, lib.rs, service.rs. No external callers. Compiler catches. |
| SR-05: Briefing k scope creep | Minimal: field + env var. No config struct introduced. Follows existing pattern. |
| SR-06: Test determinism | Pre-computed embeddings with known similarity. Ranking assertions, not score assertions. |
| SR-07: Missing indexes | Queries scan full table (same as before) but avoid deserialization. No new indexes needed. |

## Constraints

- **No schema migrations.** All tables exist. New Store methods use existing schema.
- **No confidence formula changes.** Stored weights remain 0.92.
- **Backward compatible.** Default briefing k=3. Status scan optimization is internal.
- **Extend existing test infrastructure.** Use `TestServiceContext` and existing patterns.
- **Depends on crt-011 merged and CI green.** Component 2 tests use deterministic fixtures as isolation layer.
