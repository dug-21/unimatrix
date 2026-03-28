# col-031: Phase-Conditioned Frequency Table — Architecture

## System Overview

col-031 activates the `w_phase_explicit` placeholder that has been at `0.0` since
crt-026 (ADR-003, Unimatrix #3163). The placeholder reserved a named slot in
`FusedScoreInputs.phase_explicit_norm` for a phase-category signal that would be
provided by W3-1 (GNN). ASS-032 identified the non-parametric predecessor: a
frequency table rebuilt each background tick from `query_log.phase` and
`result_entry_ids`, requiring no ML training and no schema change.

The feature introduces one new in-memory structure (`PhaseFreqTable`), one new
store method (`query_phase_freq_table`), two new `InferenceConfig` fields, a
wiring change in `ServiceLayer` and `background.rs`, a scoring change in
`search.rs`, and a fix to the eval replay path so the regression gate is
non-vacuous.

### Position in the Intelligence Pipeline

```
query_log (schema v17) ──► PhaseFreqTable::rebuild() ──► PhaseFreqTableHandle
                                                                │
                         ┌──────────────────────────────────────┤
                         │  (background tick, ~15 min interval)  │
                         ▼                                       ▼
        ServiceSearchParams.current_phase          background.rs::run_single_tick
                         │
                         ▼
         search.rs scoring loop
         phase_explicit_norm = phase_affinity_score(entry.id, category, phase)
                         │
                         ▼
         FusedScoreInputs.phase_explicit_norm  (weight: w_phase_explicit = 0.05)
                         │
                         ▼
         compute_fused_score(inputs, weights)
```

`PhaseFreqTable` also publishes `phase_affinity_score` as the integration
contract for PPR (#398, not yet implemented). Two callers, two cold-start
contracts — documented explicitly in the method's doc comment (SR-06).

---

## Component Breakdown

### 1. PhaseFreqTable (`crates/unimatrix-server/src/services/phase_freq_table.rs`)

New file, max 500 lines. Analogous to `services/typed_graph.rs`.

**Responsibility**: In-memory rank-normalized frequency table keyed by
`(phase: String, category: String)`. Rebuilt each tick from `query_log`.
Exposes `phase_affinity_score` as a public integration contract.

**Key types**:

```rust
pub struct PhaseFreqTable {
    pub table: HashMap<(String, String), Vec<(u64, f32)>>, // sorted desc by score
    pub use_fallback: bool,
}

pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;
```

**Public API surface**:

```rust
impl PhaseFreqTable {
    pub fn new() -> Self
    pub fn new_handle() -> PhaseFreqTableHandle
    pub async fn rebuild(store: &Store, lookback_days: u32) -> Result<Self, StoreError>

    /// Returns rank-based affinity score in [0.0, 1.0].
    ///
    /// # Integration Contract
    ///
    /// Two callers with distinct cold-start semantics:
    ///
    /// **PPR (#398, direct caller)**: Call `phase_affinity_score` directly.
    /// Returns `1.0` when `use_fallback = true` — a neutral multiplier so
    /// `hnsw_score × 1.0 = hnsw_score` (no distortion on cold start).
    ///
    /// **Fused scoring (guarded caller)**: Check `use_fallback` on the handle
    /// *before* calling this method. When `use_fallback = true`, set
    /// `phase_explicit_norm = 0.0` directly and skip this call entirely.
    /// This preserves score identity with pre-col-031 behavior.
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

### 2. Store Method (`crates/unimatrix-store/src/query_log.rs`)

New method on `SqlxStore`. SQL aggregation lives here, not in the service layer.

```rust
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    pub entry_id: u64,
    pub freq: i64,  // COUNT(*) → i64 in sqlx
}

impl SqlxStore {
    pub async fn query_phase_freq_table(
        &self,
        lookback_days: u32,
    ) -> Result<Vec<PhaseFreqRow>>
}
```

SQL:
```sql
SELECT
    q.phase,
    e.category,
    CAST(je.value AS INTEGER) AS entry_id,
    COUNT(*)                  AS freq
FROM query_log q
  CROSS JOIN json_each(q.result_entry_ids) AS je
  JOIN entries e ON CAST(je.value AS INTEGER) = e.id
WHERE q.phase IS NOT NULL
  AND q.result_entry_ids IS NOT NULL
  AND q.ts > strftime('%s', 'now') - ?1 * 86400
GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)
ORDER BY q.phase, e.category, freq DESC
```

`?1` = `lookback_days as i64`. Row fetch: `sqlx::query(&sql).bind(lookback_days as i64).fetch_all(self.read_pool()).await`.
Column access: `row.try_get::<T, _>(index)` — same pattern as existing `query_log.rs` methods.

### 3. ServiceLayer (`crates/unimatrix-server/src/services/mod.rs`)

`with_rate_config()` creates `PhaseFreqTableHandle` once and threads it to
`SearchService` and exposes it via accessor for `background.rs`:

```rust
// In ServiceLayer struct:
phase_freq_table: PhaseFreqTableHandle,  // col-031: shared with SearchService and background tick

// In with_rate_config():
let phase_freq_table = PhaseFreqTable::new_handle();

// Passed to SearchService::new() as required (non-optional) parameter.
// Accessor:
pub fn phase_freq_table_handle(&self) -> PhaseFreqTableHandle {
    Arc::clone(&self.phase_freq_table)
}
```

**SR-01 mitigation**: `PhaseFreqTableHandle` is a required, non-optional
parameter in `SearchService::new()`. Missing wiring is a compile error.

### 4. Background Tick (`crates/unimatrix-server/src/background.rs`)

`PhaseFreqTableHandle` added to:
- `spawn_background_tick` signature
- `background_tick_loop` signature
- `run_single_tick` signature

`run_single_tick` rebuilds the table after `TypedGraphState::rebuild`:

```rust
// Lock acquisition order (SR-07):
// EffectivenessStateHandle → TypedGraphStateHandle → PhaseFreqTableHandle
// Each lock acquired, data extracted, released before next is acquired.
// Never hold any of these locks across the scoring loop.
//
// PhaseFreqTableHandle rebuild (col-031):
match PhaseFreqTable::rebuild(store, inference_config.query_log_lookback_days).await {
    Ok(new_table) => {
        let mut guard = phase_freq_table.write().unwrap_or_else(|e| e.into_inner());
        *guard = new_table;
    }
    Err(e) => {
        tracing::error!("PhaseFreqTable rebuild failed: {e}; retaining existing state");
    }
}
```

### 5. SearchService (`crates/unimatrix-server/src/services/search.rs`)

**Field**: `phase_freq_table: PhaseFreqTableHandle` added to `SearchService` struct.

**Scoring integration** (pre-loop, lock acquired once, released before loop):

```rust
// Acquire lock once before the scoring loop; extract bucket snapshot; release.
let phase_snapshot: Option<Vec<(u64, f32)>> = match &params.current_phase {
    None => None,
    Some(phase) => {
        let guard = self.phase_freq_table.read().unwrap_or_else(|e| e.into_inner());
        if guard.use_fallback {
            None   // cold-start: phase_explicit_norm = 0.0 (score identity with pre-col-031)
        } else {
            // Clone the (phase, category) buckets needed for scoring.
            // Key is (phase, category); we need all categories for this phase.
            Some(guard.extract_phase_snapshot(phase))
        }
    }
};

// In scoring loop:
let phase_explicit_norm: f64 = match &phase_snapshot {
    None => 0.0,
    Some(snapshot) => snapshot.affinity(entry.id, &entry.category) as f64,
};
```

**`ServiceSearchParams`** gains a new field:

```rust
pub current_phase: Option<String>,
```

This is the col-031 activation field. The eval runner (`replay.rs`) must populate
it from `record.context.phase` (AC-16 fix — see below).

### 6. InferenceConfig (`crates/unimatrix-server/src/infra/config.rs`)

Two changes:
1. `default_w_phase_explicit()` returns `0.05` (raised from `0.0`).
2. New field: `query_log_lookback_days: u32`, default `30`.

No changes to `validate()` — per-field range check already covers
`w_phase_explicit`. `query_log_lookback_days` needs a range check added:
`[1, 3650]` (1 day to 10 years). FusionWeights sum-check comment updated:
`0.95 + 0.02 + 0.05 = 1.02`.

### 7. Eval Harness Fix (AC-16 — non-separable from AC-12)

The eval runner currently sets `phase` only as metadata passthrough, never
forwarding it to `ServiceSearchParams` (line 80, `replay.rs`). Without this
fix, all eval replays have `current_phase = None`, so `phase_explicit_norm = 0.0`
always, making AC-12 a vacuous gate.

**Change in `replay.rs`**: Add `current_phase: record.context.phase.clone()` to
the `ServiceSearchParams` struct literal.

**Extract.rs status**: `phase` is already selected in the SQL (`output.rs` line
108) and already populated in `ScenarioContext.phase`. No change to `extract.rs`
is needed. The gap is entirely in `replay.rs`.

---

## Component Interactions

```
main.rs
  └─ ServiceLayer::with_rate_config()
       ├─ PhaseFreqTable::new_handle()          → PhaseFreqTableHandle (cold start)
       ├─ SearchService::new(…, phase_freq_table_handle)
       └─ service_layer.phase_freq_table_handle() → Arc::clone to spawn_background_tick

background_tick_loop
  └─ run_single_tick(…, phase_freq_table_handle)
       ├─ TypedGraphState::rebuild()             (existing)
       └─ PhaseFreqTable::rebuild(store, lookback_days)
            └─ store.query_phase_freq_table(lookback_days)
                 └─ SQL: query_log ✕ json_each ✕ entries
            → Ok(new_table) → write lock swap
            → Err(e)        → tracing::error!, retain existing state

SearchService::search(params)
  ├─ params.current_phase = Some(phase) ──► read lock (once, pre-loop)
  │    └─ guard.use_fallback? → None (phase_explicit_norm = 0.0)
  │    └─ !use_fallback       → extract bucket snapshot, release lock
  └─ scoring loop:
       phase_explicit_norm = snapshot.affinity(entry.id, category)
       FusedScoreInputs { …, phase_explicit_norm }
       compute_fused_score(inputs, weights)

PPR (#398, future):
  └─ phase_freq_table_handle.read()
       └─ guard.phase_affinity_score(entry_id, category, phase)
            → 1.0 on use_fallback=true (neutral multiplier)
            → rank score otherwise
```

---

## Technology Decisions

See ADR files for full rationale.

| Decision | Choice | ADR |
|----------|--------|-----|
| Normalization | Rank-based: `1.0 - ((rank-1) / N)` | ADR-001 |
| Retention | Time-based: `query_log_lookback_days = 30` | ADR-002 |
| Cold-start (fused scoring) | `use_fallback` guard → `phase_explicit_norm = 0.0` | ADR-003 |
| Cold-start (PPR) | `phase_affinity_score` returns `1.0` | ADR-003 |
| Weight activation | `w_phase_explicit = 0.05`; AC-16 non-separable | ADR-004 |
| Handle threading | Required non-optional constructor param | ADR-005 |

---

## Integration Points

### Existing components modified

| Component | Change |
|-----------|--------|
| `services/mod.rs` (ServiceLayer) | Create `PhaseFreqTableHandle`; add field; add accessor |
| `services/search.rs` (SearchService) | Add `phase_freq_table` field; add `current_phase` to `ServiceSearchParams`; wire scoring |
| `infra/config.rs` (InferenceConfig) | Raise `w_phase_explicit` default to `0.05`; add `query_log_lookback_days: u32` |
| `background.rs` | Thread `PhaseFreqTableHandle` through `spawn_background_tick` → `background_tick_loop` → `run_single_tick`; call `PhaseFreqTable::rebuild` |
| `eval/runner/replay.rs` | Forward `record.context.phase` to `ServiceSearchParams.current_phase` |

### New components

| Component | Crate |
|-----------|-------|
| `services/phase_freq_table.rs` | `unimatrix-server` |
| `SqlxStore::query_phase_freq_table` + `PhaseFreqRow` | `unimatrix-store` |

### Pre-existing no-change

| Component | Reason |
|-----------|--------|
| `eval/scenarios/output.rs` | Already selects `phase` in SQL |
| `eval/scenarios/extract.rs` | Already reads `phase` from row and populates `ScenarioContext.phase` |
| `eval/scenarios/types.rs` | `ScenarioContext.phase: Option<String>` already present |
| `query_log` schema | `phase` column present since col-028 (schema v17); zero migrations |
| `FusionWeights::effective()` | `w_phase_explicit` already excluded from re-normalization denominator (ADR-004 crt-026) |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `PhaseFreqTableHandle` | `Arc<RwLock<PhaseFreqTable>>` | `services/phase_freq_table.rs` |
| `PhaseFreqTable::new_handle()` | `() -> PhaseFreqTableHandle` | `services/phase_freq_table.rs` |
| `PhaseFreqTable::rebuild()` | `async (&Store, u32) -> Result<PhaseFreqTable, StoreError>` | `services/phase_freq_table.rs` |
| `PhaseFreqTable::phase_affinity_score()` | `(&self, u64, &str, &str) -> f32` | `services/phase_freq_table.rs` |
| `PhaseFreqTable.use_fallback` | `bool` | `services/phase_freq_table.rs` |
| `PhaseFreqTable.table` | `HashMap<(String, String), Vec<(u64, f32)>>` | `services/phase_freq_table.rs` |
| `SqlxStore::query_phase_freq_table()` | `async (&self, u32) -> Result<Vec<PhaseFreqRow>>` | `unimatrix-store/src/query_log.rs` |
| `PhaseFreqRow` | `{ phase: String, category: String, entry_id: u64, freq: i64 }` | `unimatrix-store/src/query_log.rs` |
| `ServiceSearchParams.current_phase` | `Option<String>` (new field) | `services/search.rs` |
| `ServiceLayer::phase_freq_table_handle()` | `(&self) -> PhaseFreqTableHandle` | `services/mod.rs` |
| `InferenceConfig.query_log_lookback_days` | `u32`, default `30` | `infra/config.rs` |
| `InferenceConfig.w_phase_explicit` default | `0.05` (raised from `0.0`) | `infra/config.rs` |

---

## Cross-Cutting Concerns

### Lock Ordering (SR-07)

The required acquisition order in `run_single_tick` is:

```
EffectivenessStateHandle → TypedGraphStateHandle → PhaseFreqTableHandle
```

Each lock is acquired, data extracted, and released before the next is
acquired. No lock is held across an await point or across the scoring loop.
A code comment at the lock sequence site in `run_single_tick` must name this
order explicitly.

### Poison Recovery

All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())`.
This is consistent with `TypedGraphState`, `EffectivenessState`, and
`CategoryAllowlist` conventions.

### Error Handling

`PhaseFreqTable::rebuild` returns `Result<PhaseFreqTable, StoreError>`.
On error, `run_single_tick` emits `tracing::error!` and retains the existing
(possibly cold-start) state. Cold-start state has `use_fallback = true`, which
produces `phase_explicit_norm = 0.0` — score-identical to pre-col-031.

### Phase Vocabulary

Phase strings are runtime values with no compile-time enum. A phase rename
silently strands historical data under the old key; the new key starts cold.
Cold-start fallback (`use_fallback = true`) is the only recovery path. This is
intentional and documented; it is not a bug.

---

## Open Questions

None. All questions from SCOPE.md and SCOPE-RISK-ASSESSMENT.md are resolved:

- **SR-01** (hidden run_single_tick bypass): Addressed by making
  `PhaseFreqTableHandle` a required non-optional parameter at every construction
  site and grepping for all `SearchService::new` call sites before declaring
  wiring complete. Pattern #3213 and lesson #3216 govern.

- **SR-03** (AC-12 / AC-16 non-separability): Addressed by architecture — AC-16
  (forward `current_phase` in `replay.rs`) is a prerequisite of AC-12. The two
  must ship in the same delivery wave. Gate 3b must reject any AC-12 PASS claim
  that precedes AC-16.

- **SR-06** (two cold-start values): Addressed in `phase_affinity_score` doc
  comment contract above and in ADR-003.

- **SR-07** (lock ordering): Addressed with explicit comment at tick lock sequence.

- **AC-16 scope clarification**: `extract.rs` and `output.rs` already select and
  propagate `phase`. The gap is in `replay.rs` — `current_phase` is not forwarded
  to `ServiceSearchParams`. AC-16 is a one-line fix in `replay.rs` plus adding the
  `current_phase` field to `ServiceSearchParams`.
