# col-031: Phase-Conditioned Frequency Table — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-031/SCOPE.md |
| Scope Risk Assessment | product/features/col-031/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-031/architecture/ARCHITECTURE.md |
| Specification | product/features/col-031/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/col-031/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-031/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| phase_freq_table.rs | pseudocode/phase_freq_table.md | test-plan/phase_freq_table.md |
| query_log.rs (store method) | pseudocode/query_log_store_method.md | test-plan/query_log_store_method.md |
| search.rs (scoring wire-up) | pseudocode/search_scoring.md | test-plan/search_scoring.md |
| background.rs (tick integration) | pseudocode/background_tick.md | test-plan/background_tick.md |
| services/mod.rs (ServiceLayer wiring) | pseudocode/service_layer.md | test-plan/service_layer.md |
| infra/config.rs (InferenceConfig) | pseudocode/inference_config.md | test-plan/inference_config.md |
| eval/scenarios/replay.rs (AC-16 fix) | pseudocode/replay_fix.md | test-plan/replay_fix.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Activate the `w_phase_explicit = 0.0` placeholder in the fused scoring formula (reserved since
crt-026, ADR-003) by building a non-parametric in-memory frequency table — `PhaseFreqTable` —
that maps `(phase, category)` pairs to rank-normalized entry scores derived from `query_log`
access history (schema v17, col-028). The feature also publishes `phase_affinity_score` as
the integration contract for PPR (#398), fixes the eval harness so the regression gate is
non-vacuous (AC-16 / replay.rs), and raises `w_phase_explicit` default to `0.05`.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Normalization strategy | Rank-based: `score = 1.0 - ((rank-1) as f32 / N as f32)`, 1-indexed rank within each `(phase, category)` bucket. Absent entries return `1.0`. Single-entry bucket returns `1.0`. Min-max rejected due to power-law degeneration. | ADR-001 / Unimatrix #3685 | architecture/ADR-001-rank-based-normalization.md |
| Retention window | Time-based: `query_log_lookback_days: u32 = 30`. Filter: `WHERE q.ts > strftime('%s','now') - ?1 * 86400`. No cycle-based filter; no JOIN with `sessions`. `validate()` enforces range `[1, 3650]`. #409 owns cycle-aligned GC. | ADR-002 / Unimatrix #3686 | architecture/ADR-002-time-based-retention.md |
| Cold-start semantics | Two callers, one method: `phase_affinity_score` returns `1.0` on cold-start (PPR neutral multiplier). Fused scoring guards on `use_fallback` *before* calling the method, sets `phase_explicit_norm = 0.0` directly. Lock released before scoring loop begins. | ADR-003 / Unimatrix #3687 | architecture/ADR-003-two-cold-start-contracts.md |
| Weight activation and AC-16 gate ordering | `default_w_phase_explicit()` raised from `0.0` to `0.05`. AC-16 (`replay.rs` fix) is a hard non-separable prerequisite for AC-12 (eval gate). Gate 3b must reject any AC-12 PASS without verified non-null `current_phase` in eval output. FusionWeights doc-comment updated to `0.95 + 0.02 + 0.05 = 1.02`. | ADR-004 / Unimatrix #3688 | architecture/ADR-004-activate-w-phase-explicit.md |
| Handle threading enforcement | `PhaseFreqTableHandle` is a required non-optional constructor parameter at all 7 sites (SearchService::new, run_single_tick, background_tick_loop, spawn_background_tick, ServiceLayer, plus 5 test helpers). Missing wiring is a compile error. Lesson #3216 documents the silent-bypass failure mode. | ADR-005 / Unimatrix #3689 | architecture/ADR-005-required-handle-threading.md |

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/phase_freq_table.rs` | New module: `PhaseFreqTable` struct, `PhaseFreqTableHandle` type alias, `new()`, `new_handle()`, `rebuild()`, `phase_affinity_score()`. Max 500 lines. |

### Modified Files

| File | Change Summary |
|------|---------------|
| `crates/unimatrix-store/src/query_log.rs` | Add `PhaseFreqRow` struct and `SqlxStore::query_phase_freq_table(lookback_days: u32)` method with verified SQL aggregation. |
| `crates/unimatrix-server/src/services/mod.rs` | Add `phase_freq_table: PhaseFreqTableHandle` field to `ServiceLayer`; call `PhaseFreqTable::new_handle()` in `with_rate_config()`; expose `phase_freq_table_handle()` accessor. |
| `crates/unimatrix-server/src/services/search.rs` | Add `phase_freq_table: PhaseFreqTableHandle` field to `SearchService`; add `current_phase: Option<String>` to `ServiceSearchParams`; wire pre-loop snapshot extraction and per-entry `phase_explicit_norm` into scoring. |
| `crates/unimatrix-server/src/background.rs` | Thread `PhaseFreqTableHandle` through `spawn_background_tick` → `background_tick_loop` → `run_single_tick`; call `PhaseFreqTable::rebuild` after `TypedGraphState::rebuild` with retain-on-error semantics. |
| `crates/unimatrix-server/src/infra/config.rs` | Raise `default_w_phase_explicit()` to `0.05`; add `query_log_lookback_days: u32` (default 30); add `[1, 3650]` range check to `validate()`; update FusionWeights sum-check doc-comment to `0.95 + 0.02 + 0.05 = 1.02`. |
| `eval/scenarios/replay.rs` | One-line fix: add `current_phase: record.context.phase.clone()` to the `ServiceSearchParams` struct literal. No other change to `replay.rs`. |
| Test helpers: `server.rs`, `shutdown.rs`, `test_support.rs`, `listener.rs`, `eval/profile/layer.rs` | Pass `PhaseFreqTableHandle` to all `SearchService::new` and `spawn_background_tick` call sites (required by ADR-005). |

---

## Data Structures

### `PhaseFreqTable` (`crates/unimatrix-server/src/services/phase_freq_table.rs`)

```rust
pub struct PhaseFreqTable {
    /// (phase, category) → Vec<(entry_id, rank_score)>, sorted descending by score.
    pub table: HashMap<(String, String), Vec<(u64, f32)>>,
    /// true on cold-start or when rebuild returned zero rows.
    pub use_fallback: bool,
}

pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;
```

### `PhaseFreqRow` (`crates/unimatrix-store/src/query_log.rs`)

```rust
pub struct PhaseFreqRow {
    pub phase:    String,
    pub category: String,
    pub entry_id: u64,
    pub freq:     i64,   // COUNT(*) maps to i64 via sqlx 0.8
}
```

### `ServiceSearchParams` change (`crates/unimatrix-server/src/services/search.rs`)

New field added:

```rust
pub current_phase: Option<String>,
```

### `InferenceConfig` additions (`crates/unimatrix-server/src/infra/config.rs`)

```rust
// default raised from 0.0 to 0.05
#[serde(default = "default_w_phase_explicit")]
pub w_phase_explicit: f64,

// new field
#[serde(default = "default_query_log_lookback_days")]
pub query_log_lookback_days: u32,
```

---

## Function Signatures

### `PhaseFreqTable` public API

```rust
impl PhaseFreqTable {
    pub fn new() -> Self

    pub fn new_handle() -> PhaseFreqTableHandle

    pub async fn rebuild(
        store: &Store,
        lookback_days: u32,
    ) -> Result<Self, StoreError>

    /// Returns rank-based affinity score in [0.0, 1.0].
    ///
    /// # Integration Contract
    ///
    /// Two callers with distinct cold-start semantics:
    ///
    /// **PPR (#398, direct caller)**: Call this method directly.
    /// Returns `1.0` when `use_fallback = true` — neutral multiplier so
    /// `hnsw_score × 1.0 = hnsw_score` (no cold-start suppression).
    ///
    /// **Fused scoring (guarded caller)**: Check `use_fallback` on the handle
    /// BEFORE calling this method. When `use_fallback = true`, set
    /// `phase_explicit_norm = 0.0` directly and skip this call entirely.
    /// Preserves pre-col-031 score identity.
    ///
    /// Returns `1.0` also when `phase` is absent from the table, or when
    /// `entry_id` is absent from the `(phase, category)` bucket.
    pub fn phase_affinity_score(
        &self,
        entry_id: u64,
        entry_category: &str,
        phase: &str,
    ) -> f32
}
```

### `SqlxStore` new method

```rust
impl SqlxStore {
    pub async fn query_phase_freq_table(
        &self,
        lookback_days: u32,
    ) -> Result<Vec<PhaseFreqRow>>
}
```

### `ServiceLayer` accessor

```rust
pub fn phase_freq_table_handle(&self) -> PhaseFreqTableHandle {
    Arc::clone(&self.phase_freq_table)
}
```

---

## Rebuild SQL

The exact verified SQL for `query_phase_freq_table`:

```sql
SELECT
    q.phase,
    e.category,
    CAST(je.value AS INTEGER)  AS entry_id,
    COUNT(*)                   AS freq
FROM query_log q
  CROSS JOIN json_each(q.result_entry_ids) AS je
  JOIN entries e ON CAST(je.value AS INTEGER) = e.id
WHERE q.phase IS NOT NULL
  AND q.result_entry_ids IS NOT NULL
  AND q.ts > strftime('%s', 'now') - ?1 * 86400
GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)
ORDER BY q.phase, e.category, freq DESC
```

`?1` bound as `lookback_days as i64`. The `CAST(je.value AS INTEGER)` form is mandatory —
omitting it causes a text-to-integer JOIN mismatch that returns zero rows silently (R-05).

---

## Rank-Based Normalization Formula

Within each `(phase, category)` bucket, sorted by `freq DESC`:

```
score = 1.0 - ((rank - 1) as f32 / N as f32)
```

Where `rank` is 1-indexed (rank 1 = most frequent), `N` = total entries in bucket.

- Rank 1 → `1.0`
- Rank N → `(N-1)/N`
- Single-entry bucket (N=1, rank=1) → `1.0`
- Absent entry → `1.0` (neutral)

The formula `1 - rank/N` (zero-adjusted 1-indexed) must NOT be used; it returns `0.0`
for N=1 (single-entry buckets).

---

## Fused Scoring Integration Pattern

Lock acquired once before the scoring loop; released before the loop body executes:

```rust
// Pre-loop: acquire lock once, extract snapshot, release.
let phase_snapshot: Option<Vec<(u64, f32)>> = match &params.current_phase {
    None => None,  // lock never acquired
    Some(phase) => {
        let guard = self.phase_freq_table.read().unwrap_or_else(|e| e.into_inner());
        if guard.use_fallback {
            None   // cold-start: phase_explicit_norm = 0.0
        } else {
            Some(guard.extract_phase_snapshot(phase))
            // guard dropped here — lock released before scoring loop
        }
    }
};

// In scoring loop:
let phase_explicit_norm: f64 = match &phase_snapshot {
    None => 0.0,
    Some(snapshot) => snapshot.affinity(entry.id, &entry.category) as f64,
};
```

---

## Lock Acquisition Order (run_single_tick)

Required order — must be documented in a code comment at the lock sequence site:

```
EffectivenessStateHandle → TypedGraphStateHandle → PhaseFreqTableHandle
```

Each lock is acquired, data extracted, and released before the next is acquired.
No lock held across an await point or across the scoring loop.

---

## Constraints

| Constraint | Detail |
|------------|--------|
| col-028 prerequisite | `query_log.phase` (schema v17) confirmed COMPLETE (gate-3c PASS 2026-03-26). Pre-col-028 rows have `phase = NULL`; excluded by `WHERE phase IS NOT NULL`. Zero new migrations. |
| sqlx 0.8 integer mapping | `COUNT(*)` → `i64`. `PhaseFreqRow.freq: i64` (not u64). Bind `lookback_days as i64`. Row deserialization via `row.try_get::<T, _>(index)`. |
| No `feature_cycle` column | `query_log` has no `feature_cycle` column. Retention is time-based only. No JOIN with `sessions`. #409 owns cycle-aligned GC. |
| Phase vocabulary is runtime strings | No compile-time enum. Phase rename silently strands old key; new key starts cold. Cold-start fallback is the only recovery (CON-09, SR-04). |
| `w_phase_explicit` additive invariant | Outside the six-weight sum constraint (ADR-004, crt-026). `validate()` unchanged for sum check. FusionWeights doc-comment updated to `0.95 + 0.02 + 0.05 = 1.02`. |
| 500-line file limit | `phase_freq_table.rs` must not exceed 500 lines. SQL lives in `unimatrix-store/src/query_log.rs`. |
| `json_each` CAST form | `CAST(json_each.value AS INTEGER)` is mandatory. Verified against `mcp/knowledge_reuse.rs`. Omission silently returns zero rows (R-05). |
| AC-12 / AC-16 non-separable | AC-16 (`replay.rs` fix) must be complete and verified before AC-12 can be declared PASS. Gate 3b must reject any AC-12 PASS lacking evidence of non-null `current_phase` in eval output. |
| Poison recovery | All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())`. No bare `.unwrap()` on lock acquisitions. |
| `PhaseFreqTableHandle` non-optional | Required constructor parameter at all 7 wiring sites (ADR-005). Missing wiring is a compile error. |
| No PPR scaffolding | `phase_affinity_score` is the API contract only. No PPR internals, no personalization vector construction. Confirm #398 is not yet shipped at delivery start. |

---

## Dependencies

### Crate Dependencies (no new crates)

| Crate | Role |
|-------|------|
| `crates/unimatrix-store` | `SqlxStore`, sqlx 0.8 (`sqlite`, `runtime-tokio`, `macros`). Extension point: `query_log.rs`. |
| `crates/unimatrix-server` | Service layer, background tick, config, eval harness. Template: `services/typed_graph.rs`. |

### External Prerequisites

| Prerequisite | Status |
|-------------|--------|
| col-028 (schema v17, `query_log.phase`) | COMPLETE — gate-3c PASS 2026-03-26. |
| col-030 eval baselines (MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340) | Required for AC-12 gate thresholds. |
| `#398` (PPR) NOT yet shipped | Confirm at delivery start. col-031 must not implement PPR internals. |

### Future Consumers

| Consumer | Dependency |
|----------|-----------|
| #398 (PPR) | Will call `phase_affinity_score` directly. col-031 publishes the API contract. |
| #409 (cycle-aligned GC) | Will supersede `query_log_lookback_days` with cycle-based retention. |

---

## NOT in Scope

- PPR implementation — `phase_affinity_score` is the API contract only. PPR internals belong to #398.
- `query_log` schema changes — zero new migrations. Schema v17 is the prerequisite.
- `feature_cycle` column in `query_log` — does not exist. No cycle-based retention.
- `query_log` GC — `query_log_lookback_days` governs the rebuild SQL window only, not data deletion.
- Thompson Sampling — deferred until after PPR baseline ICD is measured.
- Gap detection (Loop 3) — separate feature after frequency table is operating.
- W3-1 GNN — frequency table is the non-parametric predecessor; GNN deferred until CC@k ≥ 0.7.
- BM25 hybrid retrieval — separate work item.
- Backfill of `phase = NULL` rows — pre-col-028 rows are filtered out, not backfilled.
- MCP tool or diagnostic endpoint — `PhaseFreqTable` is internal state only.
- `w_phase_histogram` changes — session histogram term (crt-026) is unaffected.
- `EffectivenessState` generation counter pattern — deferred for `PhaseFreqTable` until profiling shows need.
- Sessions JOIN — no JOIN with `sessions` table required.
- Changes to `extract.rs` or `output.rs` — both already handle `phase`. AC-16 touches `replay.rs` only.

---

## Critical Risk Summary for Delivery

The following risks from RISK-TEST-STRATEGY.md are flagged Critical or High and require
explicit attention before delivery gates are declared:

| Risk | Priority | Delivery Requirement |
|------|----------|---------------------|
| R-01: Silent wiring bypass — `run_single_tick` direct construction | Critical | Grep ALL `SearchService::new` and `spawn_background_tick` sites in `background.rs` before declaring wiring complete. ADR-005 makes this a compile error; confirm no `Option<PhaseFreqTableHandle>` at any site. |
| R-02: Vacuous AC-12 gate — `replay.rs` not forwarding `current_phase` | Critical | AC-16 must be in the same delivery wave as AC-12. Gate 3b must reject AC-12 PASS without non-null `current_phase` evidence in eval output. |
| R-03: `use_fallback` guard absent or fires too late in fused scoring | High | AC-11 Test 2 must confirm guard fires before `phase_affinity_score` is called. Score-identity assertion required. |
| R-04: Wrong cold-start return for PPR — `phase_affinity_score` returns `0.0` | High | AC-11 Test 3: `phase_affinity_score` on `use_fallback=true` table must return exactly `1.0`. |
| R-05: `CAST(json_each.value AS INTEGER)` omitted | High | AC-08 integration test must confirm non-empty result with correct `entry_id = 42`. SQL inspected in code review. |
| R-07: Rank normalization off-by-one (`1-rank/N` vs `1-(rank-1)/N`) | High | AC-13 single-entry bucket must assert `1.0`, not `0.0`. AC-14 multi-entry exact score assertions. |
| R-14: Test helper sites miss new constructor parameter | High | `cargo build --workspace` must pass. Grep all 7 sites enumerated in ADR-005 before declaring delivery complete. |

---

## Alignment Status

Vision alignment: **PASS**. Zero variances.

The ALIGNMENT-REPORT.md initially raised VARIANCE-1 (WARN): SPECIFICATION.md FR-11 named
`extract.rs` as the AC-16 target file, while ARCHITECTURE.md correctly identified `replay.rs`
as the target. **This variance has been resolved**: FR-11 and AC-16 have been corrected to
name `replay.rs` (not `extract.rs`) as the target file. The implementation brief reflects
the corrected scope: AC-16 is a one-line change in `replay.rs` only — `extract.rs` and
`output.rs` already select and propagate `phase`.

One minor open specification gap noted: SPECIFICATION FR-10 does not mention the `[1, 3650]`
range check for `query_log_lookback_days`, but ARCHITECTURE §6 and ADR-002 add it to
`validate()`. The architecture's guidance takes precedence. The range check is required
to close R-08.

---

## Tracking

GH Issue: #414
Draft PR: https://github.com/dug-21/unimatrix/pull/423
