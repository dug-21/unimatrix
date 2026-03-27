# col-031: Phase-Conditioned Frequency Table — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-031/SCOPE.md |
| Architecture | product/features/col-031/architecture/ARCHITECTURE.md |
| Specification | product/features/col-031/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-031/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-031/ALIGNMENT-REPORT.md |

---

## Goal

Activate the `w_phase_explicit * phase_explicit_norm` placeholder term in Unimatrix's
fused scoring formula — hardcoded to `0.0` since crt-026 — by building a non-parametric
`PhaseFreqTable` from `query_log` history (available since col-028, schema v17). The table
maps `(phase, category) → ranked entry list`, is rebuilt each background tick, and exposes
`phase_affinity_score` as the integration contract for fused scoring and future PPR
personalization (#398). Simultaneously, a known gap in the eval harness (`extract.rs`
omitting `current_phase` from scenario extraction) is fixed so the regression gate is
non-vacuous.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `PhaseFreqTable` + `PhaseFreqTableHandle` | pseudocode/phase_freq_table.md | test-plan/phase_freq_table.md |
| `Store::query_phase_freq_table` | pseudocode/store_query_phase_freq.md | test-plan/store_query_phase_freq.md |
| `ServiceLayer` wiring | pseudocode/service_layer_wiring.md | test-plan/service_layer_wiring.md |
| `SearchService` scoring loop | pseudocode/search_scoring.md | test-plan/search_scoring.md |
| Background tick integration | pseudocode/background_tick.md | test-plan/background_tick.md |
| `InferenceConfig` changes | pseudocode/inference_config.md | test-plan/inference_config.md |
| `eval/scenarios/extract.rs` fix | pseudocode/eval_extract.md | test-plan/eval_extract.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture — actual file paths are filled during delivery.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Normalization strategy for `phase_affinity_score` | Rank-based: `score = 1.0 - (rank / N)`, 0-indexed, top entry → 1.0, absent entries → 1.0 (neutral). Chosen over min-max to handle power-law access patterns without collapsing signal. | ADR-001 (Unimatrix #3679) | product/features/col-031/architecture/ADR-001-rank-based-normalization.md |
| SQL aggregation placement | `Store::query_phase_freq_table` in `unimatrix-store/src/query_log.rs` (Option B), not in the service module. Preserves crate boundary, enables `TestDb` testing, keeps `phase_freq_table.rs` under 500 lines. | ADR-002 (Unimatrix #3680) | product/features/col-031/architecture/ADR-002-store-layer-sql-placement.md |
| `json_each` expansion form | `CAST(json_each.value AS INTEGER)` pinned explicitly. `result_entry_ids` is stored as unquoted JSON integers by `serde_json`. The cast prevents affinity mismatch across SQLite versions. NULL guard added: `AND ql.result_entry_ids IS NOT NULL`. | ADR-003 (Unimatrix #3681) | product/features/col-031/architecture/ADR-003-json-each-integer-cast.md |
| Lock ordering — three handles on hot path | `EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`. Each lock acquired, data extracted, lock released before the next is acquired or before scoring begins. Background tick swap blocks are non-nested sequential scopes. | ADR-004 (Unimatrix #3682) | product/features/col-031/architecture/ADR-004-lock-ordering-three-handles.md |
| `w_phase_explicit` default and eval gate | Raise default `0.0 → 0.05`. Ship active. AC-16 (eval harness `extract.rs` fix) is a non-separable deliverable from AC-12 (eval regression gate). Accept SR-02 calibration risk; AC-12 is the safety net. | ADR-005 (Unimatrix #3683) | product/features/col-031/architecture/ADR-005-w-phase-explicit-default-and-eval-gate.md |

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/phase_freq_table.rs` | `PhaseFreqTable` struct, `PhaseFreqTableHandle` type alias, `new()`, `new_handle()`, `rebuild()`, `phase_affinity_score()`. Max 500 lines. |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-store/src/query_log.rs` | Add `PhaseFreqRow` struct and `Store::query_phase_freq_table(retention_cycles: u32)` method; SQL aggregation with `json_each`, retention window subquery, and `entries` join. |
| `crates/unimatrix-server/src/services/mod.rs` | Add `phase_freq_table: PhaseFreqTableHandle` field to `ServiceLayer`; call `PhaseFreqTable::new_handle()` in `with_rate_config()`; add `phase_freq_table_handle()` accessor; `Arc::clone` into `SearchService` and background tick. |
| `crates/unimatrix-server/src/services/search.rs` | Accept `PhaseFreqTableHandle` constructor parameter; before scoring loop acquire read lock, extract `use_fallback` and relevant bucket data, release lock; replace hardcoded `phase_explicit_norm: 0.0` with computed value from `phase_affinity_score`. |
| `crates/unimatrix-server/src/background.rs` | Accept `PhaseFreqTableHandle` in `spawn_background_tick` and `run_single_tick`; after `TypedGraphState::rebuild`, call `PhaseFreqTable::rebuild`; swap handle on success; log error and retain state on failure. |
| `crates/unimatrix-server/src/infra/config.rs` | Change `default_w_phase_explicit()` from `0.0` to `0.05`; add `query_log_retention_cycles: u32` field with default `20`; update `FusionWeights` sum-check comment to `0.95 + 0.02 + 0.05 = 1.02`. |
| `crates/unimatrix-server/src/eval/scenarios/extract.rs` | Add `query_log.phase` to scenario extraction SQL; populate `current_phase` field in emitted scenario output. Bounded change — nothing else in `extract.rs` changes. |

---

## Data Structures

### `PhaseFreqTable` (new — `services/phase_freq_table.rs`)

```rust
pub struct PhaseFreqTable {
    /// (phase, category) -> Vec<(entry_id, rank_score)>, sorted descending by rank score.
    /// Empty on cold-start; use_fallback = true in that state.
    pub table: HashMap<(String, String), Vec<(u64, f32)>>,
    /// true on cold-start or when query_log has no phase-tagged rows.
    pub use_fallback: bool,
}

pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;
```

### `PhaseFreqRow` (new — `unimatrix-store/src/query_log.rs`)

```rust
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    pub entry_id: u64,
    /// Raw COUNT(*) from SQL aggregation.
    /// NOTE: delivery agent must confirm whether sqlx maps SQLite INTEGER COUNT(*)
    /// to i64 or u64 and reconcile the field type accordingly (see delivery note below).
    pub freq: u64,  // WARN: may need to be i64 — see delivery note
}
```

### Key existing structures consumed

- `FusedScoreInputs.phase_explicit_norm: f64` — previously hardcoded `0.0`; now populated from `phase_affinity_score as f64`.
- `FusionWeights.w_phase_explicit: f64` — default changes `0.0 → 0.05`.
- `InferenceConfig.query_log_retention_cycles: u32` — new field, default `20`.
- `ServiceSearchParams.current_phase: Option<String>` — already present since crt-025/crt-026; source of phase key for scoring.

---

## Function Signatures

```rust
// services/phase_freq_table.rs
impl PhaseFreqTable {
    pub fn new() -> Self;
    pub fn new_handle() -> PhaseFreqTableHandle;
    pub async fn rebuild(store: &Store, retention_cycles: u32) -> Result<Self, StoreError>;
    /// Rank-normalized affinity score in [0.0, 1.0].
    /// Returns 1.0 (neutral) when use_fallback=true, phase absent, or entry absent in bucket.
    pub fn phase_affinity_score(
        &self,
        entry_id: u64,
        entry_category: &str,
        phase: &str,
    ) -> f32;
}

// unimatrix-store/src/query_log.rs
impl Store {
    pub async fn query_phase_freq_table(
        &self,
        retention_cycles: u32,
    ) -> Result<Vec<PhaseFreqRow>>;
}

// services/mod.rs (ServiceLayer)
pub fn phase_freq_table_handle(&self) -> PhaseFreqTableHandle;

// background.rs
pub fn spawn_background_tick(
    store: Store,
    config: InferenceConfig,
    typed_graph_handle: TypedGraphStateHandle,
    effectiveness_handle: EffectivenessStateHandle,
    phase_freq_table_handle: PhaseFreqTableHandle,  // new
    // ... other existing params
);
```

---

## SQL Aggregation (pinned form — ADR-003)

```sql
SELECT
    ql.phase,
    e.category,
    CAST(je.value AS INTEGER) AS entry_id,
    COUNT(*) AS freq
FROM query_log ql
CROSS JOIN json_each(ql.result_entry_ids) je
JOIN entries e ON e.id = CAST(je.value AS INTEGER)
WHERE ql.phase IS NOT NULL
  AND ql.result_entry_ids IS NOT NULL
  AND (
      ql.feature_cycle IS NULL
      OR ql.feature_cycle IN (
          SELECT DISTINCT feature_cycle
          FROM query_log
          ORDER BY query_id DESC
          LIMIT ?1
      )
  )
GROUP BY ql.phase, e.category, e.id
ORDER BY ql.phase, e.category, freq DESC
```

Where `?1` = `retention_cycles`. The `CAST` appears in both `SELECT` and `JOIN` — both required.

---

## Normalization Formula (ADR-001)

```
score = 1.0 - (rank as f32 / N as f32)
```

- `rank`: 0-indexed position in descending-frequency order within `(phase, category)` bucket.
- `N`: total distinct entry IDs in the bucket.
- Top entry (rank 0) → 1.0. Last entry (rank N-1) → 1/N.
- Absent entry → 1.0 (neutral, no cold-start penalty).
- Single-entry bucket: N=1, rank=0 → score = 1.0 (100% confident by revealed preference).

---

## Lock Ordering (ADR-004 — mandatory)

Acquisition order for both `SearchService` hot path and background tick:

```
1. EffectivenessStateHandle    — acquire, extract, release
2. TypedGraphStateHandle       — acquire, extract, release
3. PhaseFreqTableHandle        — acquire, extract, release
```

Rules:
- Each lock is a separate non-nested scope; guard is dropped before the next is acquired.
- Background tick swap blocks are non-nested sequential scopes — `TypedGraphState` swap
  completes before `PhaseFreqTable` swap block opens.
- All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())` for poison recovery.
- If `params.current_phase` is `None`, `PhaseFreqTableHandle` is never acquired.

---

## Constraints

| Constraint | Source |
|-----------|--------|
| `query_log.phase` (schema v17) must exist — col-028 is a hard prerequisite. Pre-col-028 rows (`phase = NULL`) contribute zero signal. | SCOPE.md, C-01 |
| `services/phase_freq_table.rs` must not exceed 500 lines. SQL aggregation belongs in `unimatrix-store`. | SCOPE.md, C-07 |
| AC-16 (`extract.rs` fix) and AC-12 (eval regression gate) are a non-separable deliverable. Neither AC is independently shippable. | ADR-005, C-03 |
| `w_phase_explicit` is an additive term outside the six-weight sum constraint. `validate()` logic is unchanged; only the FusionWeights sum-check comment requires updating. | SCOPE.md, NFR-09 |
| Background tick is the sole writer of `PhaseFreqTableHandle`. Search hot path is read-only. | SCOPE.md, NFR-04 |
| Phase vocabulary is runtime strings — no compile-time enum. Phase rename makes old key go cold (neutral); new key starts empty. Silent degradation is accepted behavior; document in code comments. | SCOPE.md, C-05, NFR-08 |
| No new crate dependencies. Uses only `std::collections::HashMap`, `std::sync::{Arc, RwLock}`, existing `tokio`, `rusqlite`/`sqlx`, `tracing`. | NFR-10 |
| No schema migration. Zero new tables, columns, or migrations. | NFR-07 |
| No `query_log` GC implementation. `query_log_retention_cycles` governs lookback window only; GC belongs to #409. | C-09 |
| Existing `TICK_TIMEOUT` applies to the full tick including the frequency table rebuild. No separate inner timeout. | C-11 |
| `json_each` form must be verified against a live `query_log` row (AC-08 integration test is the gate). Do not assume correctness from SCOPE.md prose. | ADR-003, C-02 |
| Confirm whether `COUNT(*)` via sqlx returns `i64` or `u64` from SQLite and set `PhaseFreqRow.freq` type accordingly. | See delivery note. |

---

## Dependencies

| Dependency | Status | Notes |
|-----------|--------|-------|
| `query_log.phase` column (schema v17, col-028) | Shipped 2026-03-26 | Source of all phase signal. Gate-3c PASS confirmed. |
| `TypedGraphStateHandle` pattern (`services/typed_graph.rs`) | Active | Exact template for `PhaseFreqTable`. Follow it precisely. |
| `EffectivenessStateHandle` pattern (`services/effectiveness.rs`) | Active | Reference for poison recovery and ServiceLayer wiring pattern. |
| `CategoryAllowlist` poison recovery convention | Active | `.unwrap_or_else(|e| e.into_inner())` on all lock acquisitions. |
| `FusedScoreInputs.phase_explicit_norm` (crt-026) | Active, pre-existing | Field already exists; previously hardcoded `0.0`. |
| `FusionWeights.w_phase_explicit` (crt-026) | Active, pre-existing | Default changes `0.0 → 0.05`. |
| `ServiceSearchParams.current_phase` (crt-025/crt-026) | Active | Already in search params; source of phase key. |
| `json_each` SQLite built-in | Active | Used in `mcp/knowledge_reuse.rs`. Form verified in ADR-003. |
| `PPR phase_affinity_score` call site (#398) | Not yet shipped | `phase_affinity_score` is the public integration contract; PPR itself is separate. Confirm #398 status at delivery start. |
| `eval/scenarios/extract.rs` | To be modified | Add `current_phase` to scenario extraction. Bounded change. |
| `tracing` crate | Active | `tracing::error!` on tick rebuild failure; `tracing::debug!` for timing. |
| `tokio` | Active | `rebuild` is `async fn`. |
| `rusqlite` / `sqlx` | Active | SQL aggregation in `unimatrix-store`. |
| GH #409 retention framework | Not yet shipped | `query_log_retention_cycles` default (20) aligns with #409; GC logic belongs there, not here. |

---

## NOT in Scope

- No changes to `query_log` schema (schema v17 is the prerequisite, not a deliverable). Zero migrations.
- No Thompson Sampling — deferred until after PPR baseline ICD is measured (ROADMAP.md).
- No gap detection (Loop 3 — separate feature after frequency table is operating).
- No PPR implementation — `phase_affinity_score` is the integration API; the PPR algorithm is #398.
- No W3-1 GNN — frequency table is the non-parametric predecessor; GNN deferred until CC@k ≥ 0.7.
- No BM25 hybrid retrieval.
- No `query_log` GC implementation — `query_log_retention_cycles` is lookback window only.
- No backfill of pre-col-028 rows — `phase = NULL` rows contribute zero signal; accepted.
- No new MCP tool — `PhaseFreqTable` is internal state; no diagnostic endpoint.
- No change to `w_phase_histogram` — session histogram term (crt-026) is unaffected.
- No `EffectivenessSnapshot` generation-counter optimization for `PhaseFreqTable` — defer until profiling shows cost.
- No new evaluation JSONL scenarios — AC-12 uses existing scenarios with `current_phase` from AC-16 fix.

---

## Alignment Status

**Overall: PASS — zero variances requiring approval.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly activates the Wave 1A explicit phase signal; resolves the "No session-conditioned relevance" critical gap in the product vision. |
| Milestone Fit | PASS | Correctly sequenced in Wave 1A; all deferred items (W3-1 GNN, Thompson Sampling, PPR, gap detection) remain explicitly out of scope. |
| Scope Gaps | PASS | All 7 SCOPE.md Goals addressed; all 8 Non-Goals preserved. Open Questions 2, 3, and 4 from SCOPE.md are resolved in the architecture. |
| Architecture Consistency | PASS | Exact `TypedGraphState` pattern followed; crate boundaries respected; lock ordering documented in ADR-004. |
| Risk Completeness | PASS | 14 risks catalogued; all SR-01 through SR-07 scope risks traced; Critical and High risks have multi-scenario coverage. |
| Scope Additions | WARN (non-blocking) | Architecture adds lock ordering formalization (ADR-004) and timing observability instrumentation (SR-07) not explicitly in SCOPE.md. Both are necessary elaborations that resolve risks explicitly identified in SCOPE-RISK-ASSESSMENT.md. No approval required. |

### Delivery Note — Type Inconsistency (must resolve before implementation)

SPECIFICATION.md FR-06 specifies `Store::query_phase_freq_table` returns `Vec<(String, String, u64, i64)>` (note `i64` for `freq_count`), while ARCHITECTURE.md Integration Point table shows `PhaseFreqRow.freq: u64`. The discrepancy arises because SQLite's `COUNT(*)` returns an INTEGER, and sqlx typically deserializes SQLite INTEGER as `i64`. RISK-TEST-STRATEGY.md R-13 scenario 1 flags this exact concern.

**Delivery agent must:**
1. Confirm what type sqlx returns for `COUNT(*)` on a SQLite INTEGER column in this codebase.
2. Set `PhaseFreqRow.freq` to that type (`i64` or `u64`) consistently across `PhaseFreqRow`, the SQL query mapping, and `PhaseFreqTable::rebuild` conversion logic.
3. If `i64`, add a runtime non-negative assertion or `as u64` cast with a comment explaining the safe cast (COUNT is always ≥ 0).

---

## Critical Test Coverage Requirements

The following risks are **Critical priority** and must have test coverage before gate-3c:

| Risk | Coverage Required |
|------|------------------|
| R-01: `json_each` cast produces no rows | AC-08 integration test against real SQLite `TestDb` — not a mock. A mock cannot catch `json_each` expansion failures. |
| R-02: Cold-start semantic drift | AC-11 must test both paths: `current_phase = None` (bit-for-bit identical to pre-col-031) AND `current_phase = Some(...)` + cold-start (ranking-preserving, uniform +0.05 offset — NOT score-identical). |
| R-04: Vacuous eval gate | AC-12 must include a pre-check: generated scenario JSONL contains `current_phase != null` for at least one scenario. If all are null, AC-16 is not complete and AC-12 must NOT be declared passing. |
| R-05: AC-16 and scoring activation shipped separately | Delivery protocol must treat AC-12 and AC-16 as a single wave. Gate-3c must not declare AC-12 passing without AC-16 already passing in the same or preceding wave. |

---

## Tracking

GH Issue: #414
