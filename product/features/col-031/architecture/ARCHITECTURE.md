# col-031: Phase-Conditioned Frequency Table — Architecture

## System Overview

Unimatrix's fused scoring formula (`compute_fused_score`) includes a named field
`FusedScoreInputs.phase_explicit_norm` that has been hardcoded to `0.0` since
crt-026 (ADR-003: "W3-1 reserved placeholder"). The field exists, the weight
`w_phase_explicit` exists, the formula includes the term — only the signal source
is absent.

col-031 provides that signal source non-parametrically: a `PhaseFreqTable` rebuilt
each background tick from `query_log` rows (populated with `phase` since col-028,
schema v17). The table maps `(phase, category) → Vec<(entry_id, rank_score)>` and
exposes `phase_affinity_score(entry_id, category, phase) → f32` as the integration
contract for both the fused scoring loop (col-031) and future PPR personalization
(#398).

This supersedes ADR-003 (crt-026, Unimatrix #3163) for the explicit phase term
placeholder strategy. The placeholder is now filled with a non-parametric signal;
W3-1 (GNN) remains the parametric successor after `CC@k ≥ 0.7`.

### Position in the Intelligence Pipeline

```
query_log.phase (col-028 data)
         ↓
    [background tick]
         ↓
  PhaseFreqTable::rebuild(store, retention_cycles)
         ↓
  PhaseFreqTableHandle (Arc<RwLock<_>>)
         ↓ (read under short lock, clone nothing — score read directly)
  SearchService scoring loop
         ↓
  FusedScoreInputs.phase_explicit_norm  ← was 0.0, now computed
         ↓
  compute_fused_score(inputs, weights)
```

---

## Component Breakdown

### 1. `PhaseFreqTable` (new — `services/phase_freq_table.rs`)

Single-responsibility: hold the in-memory phase-conditioned frequency data and
provide the scoring API.

**Responsibilities:**
- Contain `HashMap<(String, String), Vec<(u64, f32)>>` keyed by `(phase, category)`,
  values sorted descending by rank score.
- `use_fallback: bool` flag for cold-start and empty-table detection.
- `new()` / `new_handle()` constructors following `TypedGraphState` exact pattern.
- `rebuild(store, retention_cycles)` — async SQL aggregation delegated to store layer.
- `phase_affinity_score(entry_id, category, phase)` — public API returning `f32 ∈ [0.0, 1.0]`.
  Returns `1.0` (neutral) when `use_fallback = true`, phase absent, or entry absent from bucket.

**File limit:** 500 lines (`services/phase_freq_table.rs`). SQL aggregation moves to
`unimatrix-store`.

### 2. `Store::query_phase_freq_table` (new — `unimatrix-store/src/query_log.rs`)

Single-responsibility: execute the SQL aggregation and return raw rows.

**Responsibilities:**
- SQL aggregation over `query_log` joined with `entries` via `json_each`.
- Filters `WHERE phase IS NOT NULL`.
- Retention window via `feature_cycle` subquery (last N completed cycles or open cycles).
- Returns `Vec<PhaseFreqRow>` — `(phase: String, category: String, entry_id: u64, freq: u64)`.

**Location:** `crates/unimatrix-store/src/query_log.rs` — extends the existing
`query_log` module rather than creating a new file. Follows the pattern of
`scan_query_log_by_sessions` in the same file.

### 3. `ServiceLayer` wiring (modified — `services/mod.rs`)

**Responsibilities:**
- Create `PhaseFreqTableHandle` via `PhaseFreqTable::new_handle()` in `with_rate_config()`.
- `Arc::clone` the handle into `SearchService` and expose via `phase_freq_table_handle()` accessor.
- `ServiceLayer` holds `phase_freq_table: PhaseFreqTableHandle` as a field alongside
  `typed_graph_state` and `effectiveness_state`.

### 4. `SearchService` scoring loop (modified — `services/search.rs`)

**Responsibilities:**
- Accept `PhaseFreqTableHandle` as a constructor parameter (alongside `TypedGraphStateHandle`).
- Before the scoring loop: acquire read lock, read `use_fallback` flag. If not fallback,
  capture a reference sufficient to compute scores inline (no clone of the HashMap).
- Replace the hardcoded `phase_explicit_norm: 0.0` assignment with a call to
  `freq_table.phase_affinity_score(entry.id, &entry.category, phase)` when
  `params.current_phase` is `Some`.

### 5. `Background tick` (modified — `background.rs`)

**Responsibilities:**
- Accept `PhaseFreqTableHandle` in `spawn_background_tick` and `run_single_tick` signatures.
- After `TypedGraphState::rebuild`: call `PhaseFreqTable::rebuild(store, retention_cycles).await`.
- On success: swap handle under write lock with `*guard = new_state`.
- On failure: log via `tracing::error!` and retain existing state.

### 6. `InferenceConfig` (modified — `infra/config.rs`)

**Responsibilities:**
- `default_w_phase_explicit()` changed from `0.0` to `0.05`.
- New field `query_log_retention_cycles: u32` with default `20`.
- Config merge logic extended to include `query_log_retention_cycles` (same project/global
  merge pattern as all other `InferenceConfig` fields).
- Sum-check comment in `FusionWeights` updated: `0.95 + 0.02 + 0.05 = 1.02` total with defaults.

### 7. `eval/scenarios/extract.rs` (modified — eval harness)

**Responsibilities:**
- Add `current_phase` selection to scenario extraction SQL so AC-12 regression gate
  uses phase-aware scenarios rather than silently producing vacuous `0.0` signal.
- This is a non-separable deliverable from the scoring activation (SR-03). AC-12 depends
  on AC-16 (eval harness fix) being complete.

---

## Component Interactions

```
ServiceLayer::with_rate_config()
    │
    ├── PhaseFreqTable::new_handle()  ──────────────────────────────────┐
    │       ↓                                                           │
    │   phase_freq_table_handle (Arc<RwLock<PhaseFreqTable>>)           │
    │       │                                                           │
    │       ├── Arc::clone → SearchService (read hot path)             │
    │       └── exposed via phase_freq_table_handle()                  │
    │                                                                   │
    └─── (in main.rs)                                                  │
             └── spawn_background_tick(..., phase_freq_table_handle)   │
                     │                                                  │
                     └── run_single_tick                               │
                             │                                          │
                             ├── [after TypedGraphState::rebuild]      │
                             │                                          │
                             └── PhaseFreqTable::rebuild(store, K)     │
                                     │ delegates SQL to store layer     │
                                     └── Store::query_phase_freq_table │
                                              ↓                         │
                                         swap *guard = new_state ──────┘

SearchService::handle_search(params)
    │
    ├── [acquire PhaseFreqTableHandle read lock]
    ├── [extract use_fallback + borrow table ref]
    ├── [release lock BEFORE scoring loop]  ← lock ordering rule
    │
    └── scoring loop per candidate:
            phase_explicit_norm = freq_table.phase_affinity_score(
                entry.id, &entry.category, phase)  // f32 → f64
```

---

## Lock Ordering (SR-06 — Mandatory)

Three `Arc<RwLock<_>>` handles exist on the hot path in `SearchService`:
`EffectivenessStateHandle`, `TypedGraphStateHandle`, and `PhaseFreqTableHandle`.
The sole-writer contract for each is the background tick.

**Acquisition order (both read and write paths must follow this order):**

```
1. EffectivenessStateHandle    (acquire and release)
2. TypedGraphStateHandle       (acquire and release)
3. PhaseFreqTableHandle        (acquire and release)
```

**Rules enforced structurally:**
- Each lock is acquired, data is cloned or referenced, and the lock is released
  BEFORE the next lock is acquired or before any scoring work begins.
- The background tick NEVER holds `PhaseFreqTableHandle` write lock while holding
  any other write lock. Each handle's swap block (`{ let mut guard = ...; *guard = new_state; }`)
  is a separate, non-nested scope.
- Write lock acquisition order in the background tick: TypedGraphState is swapped
  first (existing code), PhaseFreqTable is swapped second (new code). These are
  separate scopes — never nested.

---

## Data Flow

### Rebuild (background tick, once per cycle)

```
query_log (SQLite)
    WHERE phase IS NOT NULL
    AND feature_cycle IN (last K completed cycles OR open cycles)
    CROSS JOIN json_each(result_entry_ids)   -- integer array expansion
    JOIN entries ON entries.id = CAST(json_each.value AS INTEGER)
    GROUP BY phase, entries.category, entries.id
    ORDER BY phase, entries.category, freq DESC
    ↓
Vec<PhaseFreqRow { phase, category, entry_id, freq }>
    ↓
PhaseFreqTable::from_rows(rows):
    for each (phase, category) group:
        rank each entry 0-indexed by freq desc
        score = 1.0 - (rank / N)  [rank-based normalization, N = group size]
    HashMap<(phase, category), Vec<(entry_id, f32)>> sorted desc
    ↓
*guard = PhaseFreqTable { table, use_fallback: false }
```

### Query (search hot path, per candidate in scoring loop)

```
PhaseFreqTableHandle.read()  → guard (held briefly)
    if guard.use_fallback → release, use 1.0 (neutral) for all
    else → borrow guard.table reference, release lock

per candidate entry:
    freq_table.phase_affinity_score(entry_id, entry.category, phase)
        → lookup (phase, category) in HashMap
        → binary search entry_id in Vec<(entry_id, score)> (sorted by score, not id)
             NOTE: linear scan acceptable given typical bucket size (<50 entries)
        → return score if found; 1.0 (neutral) if not found
    phase_explicit_norm = score as f64
```

### PPR Integration (#398, future)

```
PPR personalization vector construction:
    for v in candidates:
        personalization[v] = hnsw_score[v] * phase_affinity_score(v, v.category, phase)
    normalize(personalization)
```

`phase_affinity_score` is the published contract. PPR (#398) calls it directly
on the `PhaseFreqTable` borrowed through the handle.

---

## Technology Decisions

See ADR files:
- ADR-001: Rank-based normalization for `phase_affinity_score`
- ADR-002: Store layer placement for `query_phase_freq_table` SQL method
- ADR-003: `json_each` integer-cast form pinned to `CAST(json_each.value AS INTEGER)`
- ADR-004: Lock ordering — three-handle acquisition sequence
- ADR-005: `w_phase_explicit` default raised to 0.05; eval harness fix as non-separable deliverable

---

## Integration Points

### Existing components consumed

| Component | How used |
|-----------|----------|
| `query_log` table (col-028, schema v17) | Source data for frequency aggregation |
| `entries` table | Join source for `category` lookup keyed on `id` |
| `TypedGraphState` pattern (`services/typed_graph.rs`) | Exact template for handle / rebuild / cold-start |
| `EffectivenessState` pattern (`services/effectiveness.rs`) | Template for ServiceLayer wiring and accessor pattern |
| `FusedScoreInputs.phase_explicit_norm` (crt-026) | Named field already in struct; col-031 populates it |
| `FusionWeights.w_phase_explicit` (crt-026) | Weight field already in config; col-031 raises default |
| `InferenceConfig` (crt-026) | Extended with `query_log_retention_cycles: u32` |
| `background.rs` `run_single_tick` | Tick placement: after `TypedGraphState::rebuild`, before NLI tick |
| `ServiceSearchParams.current_phase` (crt-025/crt-026) | Phase string consumed by `phase_affinity_score` |

### New integration surfaces

| Integration Point | Type/Signature | Defined In |
|-------------------|---------------|------------|
| `PhaseFreqTable::new()` | `fn new() -> Self` | `services/phase_freq_table.rs` |
| `PhaseFreqTable::new_handle()` | `fn new_handle() -> PhaseFreqTableHandle` | `services/phase_freq_table.rs` |
| `PhaseFreqTable::rebuild` | `async fn rebuild(store: &Store, retention_cycles: u32) -> Result<Self, StoreError>` | `services/phase_freq_table.rs` |
| `PhaseFreqTable::phase_affinity_score` | `fn phase_affinity_score(&self, entry_id: u64, entry_category: &str, phase: &str) -> f32` | `services/phase_freq_table.rs` |
| `PhaseFreqTableHandle` | `type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` | `services/phase_freq_table.rs` |
| `Store::query_phase_freq_table` | `async fn query_phase_freq_table(&self, retention_cycles: u32) -> Result<Vec<PhaseFreqRow>>` | `unimatrix-store/src/query_log.rs` |
| `PhaseFreqRow` | `struct PhaseFreqRow { phase: String, category: String, entry_id: u64, freq: u64 }` | `unimatrix-store/src/query_log.rs` |
| `ServiceLayer::phase_freq_table_handle` | `fn phase_freq_table_handle(&self) -> PhaseFreqTableHandle` | `services/mod.rs` |
| `InferenceConfig::query_log_retention_cycles` | `u32`, default `20` | `infra/config.rs` |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `PhaseFreqTableHandle` | `Arc<RwLock<PhaseFreqTable>>` | `services/phase_freq_table.rs` |
| `phase_affinity_score` | `(&self, entry_id: u64, entry_category: &str, phase: &str) -> f32` | `services/phase_freq_table.rs` |
| `PhaseFreqTable.table` | `HashMap<(String, String), Vec<(u64, f32)>>` | `services/phase_freq_table.rs` |
| `PhaseFreqTable.use_fallback` | `bool` | `services/phase_freq_table.rs` |
| `PhaseFreqRow` | `{ phase: String, category: String, entry_id: u64, freq: u64 }` | `unimatrix-store/src/query_log.rs` |
| `query_phase_freq_table` | `async fn(&self, retention_cycles: u32) -> Result<Vec<PhaseFreqRow>>` | `unimatrix-store/src/query_log.rs` |
| `FusedScoreInputs.phase_explicit_norm` | `f64` (was hardcoded `0.0`; now computed from `phase_affinity_score as f64`) | `services/search.rs` |
| `InferenceConfig.w_phase_explicit` | `f64`, default changed `0.0 → 0.05` | `infra/config.rs` |
| `InferenceConfig.query_log_retention_cycles` | `u32`, default `20` (new field) | `infra/config.rs` |

---

## Constraints Applied

- **No new crate dependencies.** `PhaseFreqTable` uses only `std::collections::HashMap`,
  `std::sync::{Arc, RwLock}`, and the existing `unimatrix-store` / `unimatrix-core` types.
- **No new SQLite extensions.** `json_each` is already available in the bundled SQLite build.
- **No schema migration.** All data is in schema v17 (`query_log.phase`), shipped by col-028.
- **500-line file limit.** SQL aggregation is in `unimatrix-store`; service module contains
  only struct, handle, and scoring logic.
- **Cold-start safety.** Empty table → all `phase_explicit_norm = 0.0` → score bit-for-bit
  identical to pre-col-031 behavior (AC-11).
- **Sole-writer contract.** Background tick is the only writer; search hot path is read-only.
- **Eval harness fix is non-separable from scoring activation** (SR-03). AC-12 gates on AC-16.

---

## Open Questions

1. **json_each integer casting** (SR-01): The SQL form
   `CAST(json_each.value AS INTEGER)` is specified in ADR-003 based on analysis
   of how `result_entry_ids` is stored (`serde_json::to_string(entry_ids)` on
   `&[u64]`). The implementer MUST verify this form against a live `query_log` row
   before finalising the SQL — the `query_log.rs` unit test (AC-08) will catch
   mismatches in CI.

2. **Tick wall time budget** (SR-07): The `<5ms` estimate for the SQL aggregation
   assumes a warm SQLite page cache. Implementers should add a `tracing::debug!`
   timing log for the rebuild step to establish an empirical baseline at
   representative `query_log` sizes.

3. **PPR status check** (SR-05): At delivery start, confirm whether #398 PPR has
   shipped concurrently. If it has, AC-07/AC-08 wire-up ACs become immediately
   applicable rather than deferred integration points.

4. **`w_phase_explicit = 0.05` empirical grounding** (SR-02): ASS-032 research
   provides directional calibration but no numerically derived value for `0.05`
   specifically. The risk is accepted and documented in ADR-005: configurable via
   `InferenceConfig`; cold-start degrades to near-zero net effect; AC-12 is the
   safety gate.
