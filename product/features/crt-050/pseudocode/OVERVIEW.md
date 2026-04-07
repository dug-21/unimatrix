# crt-050 Pseudocode Overview
# Phase-Conditioned Category Affinity (Explicit Read Rebuild)

GH Issue: #542

---

## Components Involved

| Component | File | Change Type |
|-----------|------|-------------|
| store-queries | `crates/unimatrix-store/src/query_log.rs` | Delete one fn, add two fns + struct + constant |
| phase-freq-table | `crates/unimatrix-server/src/services/phase_freq_table.rs` | Modify `rebuild()`, add two fns |
| config | `crates/unimatrix-server/src/infra/config.rs` | Rename field + serde alias, add field, update 5 sites |
| status-diagnostics | `crates/unimatrix-server/src/services/status.rs` + `background.rs` | Rename fn + field refs, add new diagnostic fn |

---

## Data Flow

```
background.rs::run_single_tick
  │
  ├─ reads inference_config.phase_freq_lookback_days   [renamed from query_log_lookback_days]
  │
  └─► PhaseFreqTable::rebuild(store, phase_freq_lookback_days)
        │
        ├─► store.query_phase_freq_observations(lookback_days)     [Query A — NEW]
        │     SQL: observations JOIN entries
        │     Filter: hook='PreToolUse', tool IN 4-entry clause, json_extract IS NOT NULL,
        │             phase IS NOT NULL, ts_millis > cutoff_millis
        │     Returns: Vec<PhaseFreqRow> (raw explicit-read counts)
        │
        ├─► [if rows empty] → return PhaseFreqTable { use_fallback: true, table: empty }
        │
        ├─► [COVERAGE GATE] count distinct (phase, session_id) from rows_a
        │     if count < min_phase_session_pairs → use_fallback=true, tracing::warn!, return
        │
        ├─► store.query_phase_outcome_map()                        [Query B — NEW]
        │     SQL: cycle_events JOIN sessions
        │     Filter: event_type='cycle_phase_end', phase IS NOT NULL,
        │             outcome IS NOT NULL, feature_cycle IS NOT NULL
        │     Returns: Vec<PhaseOutcomeRow>
        │     [ERROR PROPAGATES — do not treat as empty]
        │
        ├─► apply_outcome_weights(rows_a, rows_b)                  [Rust post-process — NEW]
        │     Build HashMap<String, f32> keyed by phase (per-phase MEAN of outcome_weight())
        │     Multiply each PhaseFreqRow.freq by per-phase weight (default 1.0)
        │     Returns: Vec<PhaseFreqRow> with weighted freq values
        │
        └─► [rank normalization — UNCHANGED col-031 ADR-001]
              Group by (phase, category), rank within bucket descending by freq
              score = 1.0 - ((rank-1) as f32 / N as f32)
              Return PhaseFreqTable { table, use_fallback: false }

PhaseFreqTable (in memory)
  ├─► phase_affinity_score(entry_id, category, phase)  [UNCHANGED — search hot path]
  └─► phase_category_weights()                          [NEW — W3-1 GNN cold-start, off hot path]

status.rs::run_maintenance (step 4 of tick)
  ├─► run_phase_freq_table_alignment_check(...)         [RENAMED — field ref updated]
  └─► run_observations_coverage_check(...)              [NEW — AC-11 diagnostic]
```

---

## Shared Types

### PhaseFreqRow (UNCHANGED shape — semantics change only)

Declared in `unimatrix-store/src/query_log.rs`, re-exported from crate root.

```
PhaseFreqRow {
    phase: String,
    category: String,
    entry_id: u64,       // read as i64 from SQL CAST, cast to u64
    freq: i64,           // semantics: now outcome-weighted explicit-read count
                         // (was: raw search-exposure count from query_log)
}
```

Note: `freq` is `i64` (not `u32` as IMPLEMENTATION-BRIEF shows) — SQLite `COUNT(*)`
maps to `i64` in sqlx 0.8; see the existing `row_to_phase_freq_row` in query_log.rs.

### PhaseOutcomeRow (NEW — not re-exported from crate root)

Internal to `unimatrix-store/src/query_log.rs`. Consumed only by `PhaseFreqTable::rebuild()`.

```
struct PhaseOutcomeRow {
    phase: String,
    feature_cycle: String,
    outcome: String,
}
```

### MILLIS_PER_DAY (NEW constant in unimatrix-store/src/query_log.rs)

```
const MILLIS_PER_DAY: i64 = 86_400 * 1_000;
// Must NOT be 86_400 (seconds) — observations.ts_millis is millisecond epoch (ADR-006)
```

### OutcomeWeightMap (ephemeral — built and discarded per rebuild)

```
HashMap<String, f32>   // keyed by phase; per-phase mean of outcome_weight() across cycles
```

### phase_category_weights return type

```
HashMap<(String, String), f32>   // (phase, category) → fraction of total entries for phase
                                  // sums to 1.0 per phase (up to f32 rounding)
                                  // empty when use_fallback = true
```

### InferenceConfig additions

```
// Renamed field with backward-compat alias:
#[serde(alias = "query_log_lookback_days")]
pub phase_freq_lookback_days: u32,   // was query_log_lookback_days

// New field:
pub min_phase_session_pairs: u32,    // default 5, range [1, 1000]
```

---

## Sequencing Constraints

1. **store-queries** must be implemented first — `phase-freq-table` calls the new store fns.
2. **config** can be implemented in parallel with **store-queries** — no cross-dependency.
3. **phase-freq-table** depends on **store-queries** (new fn signatures) and **config**
   (new field name).
4. **status-diagnostics** depends on **config** (field rename) and is otherwise independent.
5. All struct-literal `InferenceConfig { ..., query_log_lookback_days: N, ... }` sites in
   tests must be updated when **config** is implemented (compiler enforces — SR-04).

---

## Deleted Interface

```
// DELETED from unimatrix-store/src/query_log.rs:
pub async fn query_phase_freq_table(self: &SqlxStore, lookback_days: u32) -> Result<Vec<PhaseFreqRow>>
```

One call site: `PhaseFreqTable::rebuild()` in `phase_freq_table.rs`. Grep required after
implementation to confirm no remaining references.

---

## Constraints Summary (for all implementers)

| # | Constraint | Risk if violated |
|---|-----------|-----------------|
| C-1 | `o.hook = 'PreToolUse'` (NOT `o.hook_event`) | Runtime SQL error — column does not exist |
| C-2 | `CAST(json_extract(o.input, '$.id') AS INTEGER)` mandatory in JOIN | Silent zero-row return |
| C-3 | `cutoff_millis = now_millis - lookback_days * MILLIS_PER_DAY` bound as `i64` | 1000x window error |
| C-4 | 4-entry IN clause for tool names (no REPLACE/SUBSTR) | Missing prefixed-name rows |
| C-5 | Per-phase MEAN weighting in `apply_outcome_weights` (not per-cycle) | Rank ordering scrambled |
| C-6 | `outcome_weight()`: rework checked before fail | Incorrect weight for "rework_fail" strings |
| C-7 | Query B errors must propagate as `Err` — not silently treated as empty | Silent signal corruption |
| C-8 | `phase_category_weights()` uses `bucket.len()` (breadth), not weighted-freq sum | Wrong distribution |
| C-9 | `min_phase_session_pairs` gate: `use_fallback=true` + `tracing::warn!` when below threshold | Sparse data feeds scoring |
| C-10 | `PhaseOutcomeRow` not re-exported from crate root | Leaks internal type |
