# col-031: phase_freq_table.rs — Pseudocode

File: `crates/unimatrix-server/src/services/phase_freq_table.rs`
Status: NEW
Max 500 lines (NFR-01, AC-15)
Template: `services/typed_graph.rs`

---

## Purpose

In-memory rank-normalized frequency table keyed by `(phase, category)` pairs,
rebuilt each background tick from `query_log` access history. Exposes
`phase_affinity_score` as the public API contract for PPR (#398). The background
tick is the sole writer; search.rs reads under a short lock, extracts a snapshot,
and releases before the scoring loop.

---

## Module-Level Doc Comment

```
//! PhaseFreqTable: in-memory tick-rebuild cache of phase-conditioned access
//! frequency data.
//!
//! Rebuilt each background tick by `PhaseFreqTable::rebuild()` from the
//! `query_log` table. The search hot path acquires a short read lock,
//! extracts a snapshot, and releases before scoring — it never rebuilds
//! per query.
//!
//! Cold-start: empty table, `use_fallback = true` until first successful tick.
//!
//! # Phase vocabulary
//!
//! Phase strings are runtime values with no compile-time enum. A phase rename
//! silently strands historical data under the old key; the new key starts cold
//! and falls through to `use_fallback` behavior. This is expected operational
//! degradation, not a bug (CON-09, SR-04).
//!
//! All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())` for
//! poison recovery (consistent with `TypedGraphState`, `EffectivenessState`,
//! and `CategoryAllowlist` conventions).
```

---

## Imports

```
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use unimatrix_core::Store;
use unimatrix_store::StoreError;

// PhaseFreqRow is declared in unimatrix-store but re-exported or used
// via the store trait — exact import path depends on how store trait exposes it.
// Use: unimatrix_store::PhaseFreqRow  (see query_log_store_method.md)
use unimatrix_store::PhaseFreqRow;
```

---

## Structs

### `PhaseFreqTable`

```
/// In-memory tick-rebuild frequency table keyed by (phase, category).
///
/// Background tick is the sole writer. Search hot path reads under a
/// short read lock — extracts snapshot, releases, then scores.
///
/// Cold-start: `use_fallback = true`, empty `table`.
#[derive(Debug)]
pub struct PhaseFreqTable {
    /// (phase, category) -> Vec<(entry_id, rank_score)>
    ///
    /// Vec is sorted descending by rank_score (highest affinity first).
    /// Rank scores are in [0.0, 1.0] computed by the formula:
    ///   score = 1.0 - ((rank - 1) as f32 / N as f32)
    /// where rank is 1-indexed (rank 1 = most frequent), N = bucket size.
    pub table: HashMap<(String, String), Vec<(u64, f32)>>,

    /// When true: no phase history available (cold-start or empty rebuild).
    ///
    /// Two callers, two cold-start contracts:
    ///   - Fused scoring: guards on this field BEFORE calling phase_affinity_score.
    ///     When true, sets phase_explicit_norm = 0.0 directly (score identity).
    ///   - PPR (#398): calls phase_affinity_score directly; receives 1.0 (neutral).
    pub use_fallback: bool,
}
```

### `PhaseFreqTableHandle` type alias

```
/// Thread-safe shared reference to PhaseFreqTable.
///
/// Held by ServiceLayer, SearchService, and the background tick.
/// Background tick is sole writer. All other consumers are readers.
///
/// All lock acquisitions must use .unwrap_or_else(|e| e.into_inner()) —
/// never .unwrap() or .expect().
pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;
```

---

## `impl PhaseFreqTable`

### `new()`

```
/// Create a cold-start PhaseFreqTable: use_fallback = true, empty table.
///
/// Called by new_handle() and (implicitly) the retain-on-error path
/// when the server first starts. The search path applies
/// phase_explicit_norm = 0.0 until the first background tick populates state.
pub fn new() -> Self {
    PhaseFreqTable {
        table: HashMap::new(),
        use_fallback: true,
    }
}
```

### `new_handle()`

```
/// Create a new PhaseFreqTableHandle wrapping a cold-start empty state.
///
/// Called once by ServiceLayer::with_rate_config() to create the shared handle,
/// then Arc::clone'd into SearchService and spawn_background_tick. All components
/// share the same backing RwLock<PhaseFreqTable>.
pub fn new_handle() -> PhaseFreqTableHandle {
    Arc::new(RwLock::new(PhaseFreqTable::new()))
}
```

### `rebuild()`

```
/// Rebuild PhaseFreqTable from the store.
///
/// Steps:
/// 1. Call store.query_phase_freq_table(lookback_days) -> Vec<PhaseFreqRow>.
/// 2. If result is empty, return Ok(PhaseFreqTable { use_fallback: true, table: empty }).
/// 3. Group rows by (phase, category) key.
/// 4. Within each group, the rows are already sorted by freq DESC (SQL ORDER BY).
///    Apply rank-based normalization: for rank 1..=N (1-indexed):
///      score = 1.0 - ((rank - 1) as f32 / N as f32)
///    Where N = bucket size.
///    - Rank 1 (most frequent): score = 1.0
///    - Rank N (least frequent): score = (N-1)/N
///    - N=1 (single entry): score = 1.0  [NOT 1 - 1/1 = 0.0 — use (rank-1)/N form]
/// 5. Store computed (entry_id, score) pairs per bucket.
///    Resulting Vec is already in descending-score order (rank 1 first).
/// 6. Return Ok(PhaseFreqTable { table, use_fallback: false }).
///
/// On store error: return Err(e). Caller retains existing state and emits
/// tracing::error! (retain-on-error semantics, R-09).
pub async fn rebuild(store: &Store, lookback_days: u32) -> Result<Self, StoreError> {
    // Step 1: query store
    let rows: Vec<PhaseFreqRow> = store.query_phase_freq_table(lookback_days).await?;

    // Step 2: empty result -> cold-start table
    if rows.is_empty() {
        return Ok(PhaseFreqTable {
            table: HashMap::new(),
            use_fallback: true,
        });
    }

    // Step 3: group rows by (phase, category)
    // Rows are pre-sorted by (phase, category, freq DESC) from SQL.
    // We accumulate into a HashMap, preserving the SQL sort order within each group.
    let mut grouped: HashMap<(String, String), Vec<PhaseFreqRow>> = HashMap::new();
    for row in rows {
        grouped
            .entry((row.phase.clone(), row.category.clone()))
            .or_default()
            .push(row);
    }

    // Step 4+5: rank-normalize each bucket
    let mut table: HashMap<(String, String), Vec<(u64, f32)>> =
        HashMap::with_capacity(grouped.len());

    for (key, bucket_rows) in grouped {
        let n = bucket_rows.len();
        let bucket: Vec<(u64, f32)> = bucket_rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                // rank is 1-indexed: idx=0 -> rank=1
                let rank = idx + 1;
                // CRITICAL: use (rank - 1) / N, NOT rank / N
                // (rank - 1) / N ensures rank-1 (top) = 0.0 / N = 0.0 component -> score 1.0
                // and N=1 single entry: (1-1)/1 = 0.0 -> score 1.0
                let score = 1.0_f32 - ((rank - 1) as f32 / n as f32);
                (row.entry_id, score)
            })
            .collect();
        // bucket is already sorted descending (rank 1 first = highest score first)
        table.insert(key, bucket);
    }

    // Step 6: return populated table
    Ok(PhaseFreqTable {
        table,
        use_fallback: false,
    })
}
```

### `phase_affinity_score()`

```
/// Return rank-based affinity score for an entry in a given phase, in [0.0, 1.0].
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
/// Preserves pre-col-031 score identity (NFR-04). Do NOT call this
/// method from the fused scoring path when use_fallback = true.
///
/// Returns `1.0` also when:
/// - `phase` is absent as a key in `table`
/// - `entry_id` is absent from the `(phase, entry_category)` bucket
///
/// The `1.0` return for absent entries ensures cold-start and missing-history
/// states are neutral, not suppressive.
pub fn phase_affinity_score(
    &self,
    entry_id: u64,
    entry_category: &str,
    phase: &str,
) -> f32 {
    // Cold-start path (PPR contract): return neutral 1.0
    if self.use_fallback {
        return 1.0;
    }

    // Phase not in table: this phase has no history -> return neutral 1.0
    let bucket = match self.table.get(&(phase.to_string(), entry_category.to_string())) {
        Some(b) => b,
        None => return 1.0,
    };

    // Entry not in bucket: unknown entry for this (phase, category) -> neutral 1.0
    // Linear scan: buckets are expected to be small (top-k entries per phase/category)
    match bucket.iter().find(|(id, _)| *id == entry_id) {
        Some((_, score)) => *score,
        None => 1.0,
    }
}
```

### `Default` impl

```
impl Default for PhaseFreqTable {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Helper Method for Search Path

The architecture documents `extract_phase_snapshot` and `snapshot.affinity()` as
conceptual operations in the pre-loop block. These are NOT separate public methods —
they are inline operations in the search.rs pre-loop. The implementation agent must
NOT add extra methods to PhaseFreqTable beyond the four listed above. Instead:

- "extract_phase_snapshot(phase)" means: clone all (phase, *) buckets from the table
  into a local HashMap or Vec that can be used without holding the lock.
- "snapshot.affinity(entry_id, category)" means: look up the cloned local data.

The search_scoring.md pseudocode specifies the exact inline form.

---

## `impl` block for tests module

```
#[cfg(test)]
mod tests {
    use super::*;

    // AC-01: cold-start
    fn test_new_returns_use_fallback_true_and_empty_table() {
        let t = PhaseFreqTable::new();
        assert!(t.use_fallback);
        assert!(t.table.is_empty());
    }

    // AC-03: new_handle wraps cold-start state
    fn test_new_handle_is_cold_start() {
        let h = PhaseFreqTable::new_handle();
        let g = h.read().unwrap_or_else(|e| e.into_inner());
        assert!(g.use_fallback);
        assert!(g.table.is_empty());
    }

    // AC-11 Test 3: phase_affinity_score on use_fallback=true returns 1.0 (PPR contract)
    fn test_phase_affinity_score_use_fallback_returns_1_0() {
        let t = PhaseFreqTable::new();  // use_fallback = true
        assert_eq!(t.phase_affinity_score(42, "decision", "delivery"), 1.0_f32);
        assert_eq!(t.phase_affinity_score(99, "pattern", "scope"), 1.0_f32);
    }

    // AC-07: absent phase -> 1.0
    fn test_phase_affinity_score_absent_phase_returns_1_0() {
        let t = PhaseFreqTable {
            table: HashMap::new(),  // empty but use_fallback = false
            use_fallback: false,
        };
        assert_eq!(t.phase_affinity_score(42, "decision", "delivery"), 1.0_f32);
    }

    // AC-07: absent entry_id in bucket -> 1.0
    fn test_phase_affinity_score_absent_entry_returns_1_0() {
        let mut table = HashMap::new();
        table.insert(
            ("delivery".to_string(), "decision".to_string()),
            vec![(100_u64, 1.0_f32)],
        );
        let t = PhaseFreqTable { table, use_fallback: false };
        // entry 99 is not in the bucket
        assert_eq!(t.phase_affinity_score(99, "decision", "delivery"), 1.0_f32);
    }

    // AC-13: single-entry bucket scores 1.0 (R-07 guard)
    fn test_rank_normalization_single_entry_bucket() {
        let mut table = HashMap::new();
        table.insert(
            ("scope".to_string(), "decision".to_string()),
            vec![(42_u64, 1.0_f32)],  // rank=1, N=1: 1.0 - (0/1) = 1.0
        );
        let t = PhaseFreqTable { table, use_fallback: false };
        assert_eq!(t.phase_affinity_score(42, "decision", "scope"), 1.0_f32);
    }

    // AC-14: multi-entry bucket normalization
    // Bucket with freqs [10, 5, 1] (N=3):
    //   rank1 -> score = 1.0 - 0/3 = 1.0
    //   rank2 -> score = 1.0 - 1/3 = 0.666...
    //   rank3 -> score = 1.0 - 2/3 = 0.333...
    fn test_rank_normalization_multi_entry_bucket() {
        let mut table = HashMap::new();
        table.insert(
            ("delivery".to_string(), "decision".to_string()),
            vec![
                (1_u64, 1.0_f32),
                (2_u64, 2.0/3.0_f32),
                (3_u64, 1.0/3.0_f32),
            ],
        );
        let t = PhaseFreqTable { table, use_fallback: false };
        let score1 = t.phase_affinity_score(1, "decision", "delivery");
        let score2 = t.phase_affinity_score(2, "decision", "delivery");
        let score3 = t.phase_affinity_score(3, "decision", "delivery");
        assert!((score1 - 1.0_f32).abs() < 1e-6, "rank-1 must be 1.0");
        assert!((score2 - 2.0/3.0_f32).abs() < 1e-5, "rank-2 must be ~0.666");
        assert!((score3 - 1.0/3.0_f32).abs() < 1e-5, "rank-3 must be ~0.333");
    }

    // R-07: last entry in 5-bucket must score (5-1)/5 = 0.8, NOT 0.0
    fn test_rank_normalization_last_entry_is_not_zero() {
        let bucket: Vec<(u64, f32)> = (1_u64..=5)
            .enumerate()
            .map(|(idx, id)| {
                let n = 5_usize;
                let rank = idx + 1;
                let score = 1.0_f32 - ((rank - 1) as f32 / n as f32);
                (id, score)
            })
            .collect();
        let mut table = HashMap::new();
        table.insert(("delivery".to_string(), "pattern".to_string()), bucket);
        let t = PhaseFreqTable { table, use_fallback: false };
        let last_score = t.phase_affinity_score(5, "pattern", "delivery");
        assert!((last_score - 0.8_f32).abs() < 1e-6, "rank-5 of 5 must be 0.8, got {last_score}");
    }

    // Poison recovery: poisoned handle must not panic on read
    fn test_handle_poison_recovery() {
        let handle = PhaseFreqTable::new_handle();
        let handle_clone = Arc::clone(&handle);
        let _ = std::panic::catch_unwind(move || {
            let _g = handle_clone.write().unwrap_or_else(|e| e.into_inner());
            panic!("intentional");
        });
        let g = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(g.use_fallback);
    }

    // R-10: phase rename -> absent key -> 1.0 (graceful cold-start, not error)
    fn test_phase_rename_returns_1_0() {
        let mut table = HashMap::new();
        table.insert(
            ("delivery".to_string(), "decision".to_string()),
            vec![(42_u64, 1.0_f32)],
        );
        let t = PhaseFreqTable { table, use_fallback: false };
        assert_eq!(t.phase_affinity_score(42, "decision", "implement"), 1.0_f32);
    }
}
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `store.query_phase_freq_table` returns `Err` | `rebuild` returns `Err(e)`; caller retains existing state, emits `tracing::error!` |
| SQL returns zero rows | `rebuild` returns `Ok(PhaseFreqTable { use_fallback: true, .. })` |
| Lock poison on read | `.unwrap_or_else(|e| e.into_inner())` recovers; no panic |
| Lock poison on write | `.unwrap_or_else(|e| e.into_inner())` recovers; swap proceeds |

---

## Key Test Scenarios

- AC-01: `new()` returns `use_fallback=true`, empty table
- AC-03: `new_handle()` returns Arc<RwLock<_>> in cold-start state
- AC-07: `phase_affinity_score` returns 1.0 for all three absent-entry paths
- AC-11 Test 3: `phase_affinity_score` on `use_fallback=true` returns exactly 1.0_f32
- AC-13: single-entry bucket scores 1.0 (NOT 0.0 — guards against R-07)
- AC-14: multi-entry bucket scores are exact (rank 1 = 1.0, rank N = (N-1)/N)
- R-07: 5-entry bucket last-rank = 0.8
- R-10: phase rename -> 1.0 neutral (not error)
- Poison recovery: poisoned handle does not panic
