# col-031: Phase-Conditioned Frequency Table — Pseudocode Overview

GH Issue: #414
Pseudocode Author: col-031-agent-1-pseudocode
Date: 2026-03-28

---

## Feature Summary

col-031 activates the `w_phase_explicit = 0.0` placeholder (reserved since crt-026,
ADR-003) by building `PhaseFreqTable`: a non-parametric in-memory frequency table
rebuilt each background tick from `query_log` access history. The feature also fixes
the eval replay path (AC-16) so the regression gate is non-vacuous.

---

## Components Involved

| Component File | Role | Status |
|----------------|------|--------|
| `services/phase_freq_table.rs` | New module: PhaseFreqTable struct, handle, rebuild, score | NEW |
| `unimatrix-store/src/query_log.rs` | Add PhaseFreqRow + query_phase_freq_table store method | MODIFIED |
| `services/search.rs` | Add phase_freq_table field + current_phase param + pre-loop snapshot | MODIFIED |
| `background.rs` | Thread PhaseFreqTableHandle + call rebuild after TypedGraphState | MODIFIED |
| `services/mod.rs` | Create handle in with_rate_config, add field + accessor | MODIFIED |
| `infra/config.rs` | Raise w_phase_explicit default, add query_log_lookback_days | MODIFIED |
| `eval/runner/replay.rs` | One-line fix: forward current_phase to ServiceSearchParams | MODIFIED |

---

## Shared Types

### `PhaseFreqTable` (new, `services/phase_freq_table.rs`)

```
struct PhaseFreqTable {
    table: HashMap<(String, String), Vec<(u64, f32)>>,
    //              ^phase  ^category    ^entry_id ^rank_score
    //              Vec sorted descending by rank_score
    use_fallback: bool,
}

type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>
```

### `PhaseFreqRow` (new, `unimatrix-store/src/query_log.rs`)

```
struct PhaseFreqRow {
    phase:    String,
    category: String,
    entry_id: u64,   // deserialized from i64 via as u64 cast
    freq:     i64,   // COUNT(*) -> i64 in sqlx 0.8
}
```

### `ServiceSearchParams` (modified field, `services/search.rs`)

New field added:
```
pub current_phase: Option<String>,
```

All existing construction sites (relay.rs, test helpers, etc.) must add this field;
the compile error from the required field is the primary wiring guard (ADR-005).

### `InferenceConfig` (modified defaults/fields, `infra/config.rs`)

```
w_phase_explicit: f64     // default raised from 0.0 to 0.05
query_log_lookback_days: u32  // new field, default 30, validated [1, 3650]
```

---

## Data Flow

```
query_log (schema v17)
    |
    | query_phase_freq_table(lookback_days)
    | SQL: query_log x json_each(result_entry_ids) JOIN entries
    v
Vec<PhaseFreqRow>
    |
    | PhaseFreqTable::rebuild(store, lookback_days)
    | group by (phase, category)
    | rank-normalize within each bucket: score = 1.0 - ((rank-1) / N)
    v
PhaseFreqTable { table: HashMap<(phase,category), Vec<(entry_id, score)>>, use_fallback: false }
    |
    | write lock swap under PhaseFreqTableHandle (background tick)
    v
PhaseFreqTableHandle (shared Arc<RwLock<PhaseFreqTable>>)
    |
    +-----> background.rs::run_single_tick (sole writer)
    |
    +-----> search.rs scoring path (reader)
    |           |
    |           | pre-loop: read lock once, extract phase snapshot, release lock
    |           | in scoring loop: phase_explicit_norm = snapshot.affinity(entry_id, category)
    |           v
    |       FusedScoreInputs.phase_explicit_norm
    |           |
    |           v
    |       compute_fused_score(inputs, effective_weights)
    |       (w_phase_explicit * phase_explicit_norm term now non-zero when active)
    |
    +-----> PPR #398 (future): phase_affinity_score(entry_id, category, phase) -> f32
```

---

## Sequencing Constraints

1. `PhaseFreqRow` + `query_phase_freq_table` (query_log.rs) must be implemented first —
   `PhaseFreqTable::rebuild` depends on it.

2. `PhaseFreqTable` module (phase_freq_table.rs) must be implemented before
   `search.rs` and `background.rs` modifications — both depend on the handle type.

3. `ServiceSearchParams.current_phase` field must be added before `replay.rs` fix —
   the fix is adding this field to the struct literal.

4. `InferenceConfig.query_log_lookback_days` must exist before `background.rs`
   reads it in the rebuild call.

5. All 7 construction sites (ADR-005) must be updated in the same wave to prevent
   compile failure.

---

## Lock Acquisition Order

In `run_single_tick`, the three Arc<RwLock<_>> handles must be acquired in this order:

```
EffectivenessStateHandle  -> acquire, extract, release
TypedGraphStateHandle     -> acquire, extract, release
PhaseFreqTableHandle      -> acquire (write), swap, release
```

No two locks are held simultaneously. No lock is held across an await point.
This order must be documented with a code comment at the lock sequence site (NFR-03, R-12).

---

## Cold-Start Invariants

Two distinct cold-start behaviors for two callers (ADR-003):

| Caller | use_fallback=true behavior | Reason |
|--------|---------------------------|--------|
| Fused scoring (search.rs) | Guard fires before method call; phase_explicit_norm = 0.0 | Preserve pre-col-031 score identity (NFR-04) |
| PPR (#398, future) | phase_affinity_score returns 1.0 | Neutral multiplier: hnsw_score * 1.0 = hnsw_score |

Both behaviors are correct and intentional. The `phase_affinity_score` doc comment
must name both callers explicitly (AC-17).

---

## Key Risk Mitigations in Pseudocode

| Risk | Mitigation in pseudocode |
|------|--------------------------|
| R-01: Silent wiring bypass | PhaseFreqTableHandle is non-optional at all 7 sites |
| R-02: Vacuous AC-12 | current_phase field in ServiceSearchParams; replay.rs fix |
| R-03: use_fallback guard fires late | Guard is before phase_affinity_score call in pre-loop block |
| R-04: PPR cold-start wrong value | phase_affinity_score returns 1.0 when use_fallback=true |
| R-05: json_each CAST omitted | CAST(je.value AS INTEGER) present in both SELECT and JOIN |
| R-07: rank formula off-by-one | Formula: 1.0 - ((rank-1) as f32 / N as f32), not 1 - rank/N |
| R-08: lookback_days unvalidated | validate() check [1, 3650] in inference_config.md |
| R-09: rebuild failure replaces state | Error branch has no write to handle |
| R-14: test helpers miss new param | current_phase: None at all ServiceSearchParams construction sites |
