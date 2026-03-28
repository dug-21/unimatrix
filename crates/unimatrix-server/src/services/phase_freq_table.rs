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

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use unimatrix_core::Store;
use unimatrix_store::StoreError;

// PhaseFreqRow is declared in unimatrix-store/src/query_log.rs and re-exported
// from the crate root.
use unimatrix_store::PhaseFreqRow;

// ---------------------------------------------------------------------------
// PhaseFreqTable
// ---------------------------------------------------------------------------

/// In-memory tick-rebuild frequency table keyed by (phase, category).
///
/// Background tick is the sole writer. Search hot path reads under a
/// short read lock — extracts snapshot, releases, then scores.
///
/// Cold-start: `use_fallback = true`, empty `table`.
#[derive(Debug)]
pub struct PhaseFreqTable {
    /// `(phase, category)` → `Vec<(entry_id, rank_score)>`
    ///
    /// Vec is sorted descending by rank_score (highest affinity first).
    /// Rank scores are in `[0.0, 1.0]` computed by the formula:
    ///   `score = 1.0 - ((rank - 1) as f32 / N as f32)`
    /// where rank is 1-indexed (rank 1 = most frequent), N = bucket size.
    pub table: HashMap<(String, String), Vec<(u64, f32)>>,

    /// When true: no phase history available (cold-start or empty rebuild).
    ///
    /// Two callers, two cold-start contracts:
    ///   - Fused scoring: guards on this field BEFORE calling `phase_affinity_score`.
    ///     When true, sets `phase_explicit_norm = 0.0` directly (score identity).
    ///   - PPR (#398): calls `phase_affinity_score` directly; receives `1.0` (neutral).
    pub use_fallback: bool,
}

// ---------------------------------------------------------------------------
// PhaseFreqTableHandle
// ---------------------------------------------------------------------------

/// Thread-safe shared reference to `PhaseFreqTable`.
///
/// Held by `ServiceLayer`, `SearchService`, and the background tick.
/// Background tick is sole writer. All other consumers are readers.
///
/// All lock acquisitions must use `.unwrap_or_else(|e| e.into_inner())` —
/// never `.unwrap()` or `.expect()`.
pub type PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>;

// ---------------------------------------------------------------------------
// impl PhaseFreqTable
// ---------------------------------------------------------------------------

impl PhaseFreqTable {
    /// Create a cold-start `PhaseFreqTable`: `use_fallback = true`, empty table.
    ///
    /// Called by `new_handle()` and (implicitly) the retain-on-error path
    /// when the server first starts. The search path applies
    /// `phase_explicit_norm = 0.0` until the first background tick populates state.
    pub fn new() -> Self {
        PhaseFreqTable {
            table: HashMap::new(),
            use_fallback: true,
        }
    }

    /// Create a new `PhaseFreqTableHandle` wrapping a cold-start empty state.
    ///
    /// Called once by `ServiceLayer::with_rate_config()` to create the shared handle,
    /// then `Arc::clone`'d into `SearchService` and `spawn_background_tick`. All components
    /// share the same backing `RwLock<PhaseFreqTable>`.
    pub fn new_handle() -> PhaseFreqTableHandle {
        Arc::new(RwLock::new(PhaseFreqTable::new()))
    }

    /// Rebuild `PhaseFreqTable` from the store.
    ///
    /// Steps:
    /// 1. Call `store.query_phase_freq_table(lookback_days)` → `Vec<PhaseFreqRow>`.
    /// 2. If result is empty, return `Ok(PhaseFreqTable { use_fallback: true, table: empty })`.
    /// 3. Group rows by `(phase, category)` key.
    /// 4. Within each group, the rows are already sorted by `freq DESC` (SQL ORDER BY).
    ///    Apply rank-based normalization: for rank `1..=N` (1-indexed):
    ///      `score = 1.0 - ((rank - 1) as f32 / N as f32)`
    ///    - Rank 1 (most frequent): score = `1.0`
    ///    - Rank N (least frequent): score = `(N-1)/N`
    ///    - N=1 (single entry): score = `1.0` (NOT `1 - 1/1 = 0.0` — use the `(rank-1)/N` form)
    /// 5. Store computed `(entry_id, score)` pairs per bucket in descending-score order.
    /// 6. Return `Ok(PhaseFreqTable { table, use_fallback: false })`.
    ///
    /// On store error: return `Err(e)`. Caller retains existing state and emits
    /// `tracing::error!` (retain-on-error semantics, R-09).
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

        // Step 3: group rows by (phase, category).
        // Rows are pre-sorted by (phase, category, freq DESC) from SQL ORDER BY.
        // We accumulate into a HashMap, preserving the SQL sort order within each group.
        let mut grouped: HashMap<(String, String), Vec<PhaseFreqRow>> = HashMap::new();
        for row in rows {
            grouped
                .entry((row.phase.clone(), row.category.clone()))
                .or_default()
                .push(row);
        }

        // Step 4+5: rank-normalize each bucket.
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
                    // CRITICAL: use (rank - 1) / N, NOT rank / N.
                    // The (rank-1)/N form ensures:
                    //   - rank=1 (top): (1-1)/N = 0.0 component -> score 1.0
                    //   - N=1, rank=1 (single entry): (1-1)/1 = 0.0 -> score 1.0
                    //   - rank=N (last): (N-1)/N -> score = 1/N (always > 0)
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

    /// Return rank-based affinity score for an entry in a given phase, in `[0.0, 1.0]`.
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
    /// method from the fused scoring path when `use_fallback = true`.
    ///
    /// Returns `1.0` also when:
    /// - `phase` is absent as a key in `table`
    /// - `entry_id` is absent from the `(phase, entry_category)` bucket
    ///
    /// The `1.0` return for absent entries ensures cold-start and missing-history
    /// states are neutral, not suppressive.
    pub fn phase_affinity_score(&self, entry_id: u64, entry_category: &str, phase: &str) -> f32 {
        // Cold-start path (PPR contract): return neutral 1.0
        if self.use_fallback {
            return 1.0;
        }

        // Phase not in table: this phase has no history -> return neutral 1.0
        let bucket = match self
            .table
            .get(&(phase.to_string(), entry_category.to_string()))
        {
            Some(b) => b,
            None => return 1.0,
        };

        // Entry not in bucket: unknown entry for this (phase, category) -> neutral 1.0.
        // Linear scan is acceptable: buckets are expected to be small (top-k entries
        // per phase/category pair from the bounded lookback window).
        match bucket.iter().find(|(id, _)| *id == entry_id) {
            Some((_, score)) => *score,
            None => 1.0,
        }
    }
}

impl Default for PhaseFreqTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // helper: build a populated (not cold-start) table with one bucket
    fn table_with(phase: &str, cat: &str, bucket: Vec<(u64, f32)>) -> PhaseFreqTable {
        let mut m = HashMap::new();
        m.insert((phase.to_string(), cat.to_string()), bucket);
        PhaseFreqTable {
            table: m,
            use_fallback: false,
        }
    }

    // helper: apply rank normalization to a slice of entry_ids (pre-sorted desc by freq)
    fn rank_bucket(ids: &[u64]) -> Vec<(u64, f32)> {
        let n = ids.len();
        ids.iter()
            .enumerate()
            .map(|(idx, &id)| {
                let score = 1.0_f32 - ((idx) as f32 / n as f32);
                (id, score)
            })
            .collect()
    }

    // AC-01: cold-start construction
    #[test]
    fn test_phase_freq_table_new_returns_cold_start() {
        let t = PhaseFreqTable::new();
        assert!(t.use_fallback);
        assert!(t.table.is_empty());
    }

    #[test]
    fn test_phase_freq_table_default_matches_new() {
        let d = PhaseFreqTable::default();
        assert!(d.use_fallback);
        assert!(d.table.is_empty());
    }

    // AC-03: handle mechanics
    #[test]
    fn test_new_handle_wraps_cold_start_state() {
        let h = PhaseFreqTable::new_handle();
        let g = h.read().unwrap_or_else(|e| e.into_inner());
        assert!(g.use_fallback);
        assert!(g.table.is_empty());
    }

    #[test]
    fn test_new_handle_write_then_read_reflects_change() {
        let h = PhaseFreqTable::new_handle();
        h.write().unwrap_or_else(|e| e.into_inner()).use_fallback = false;
        assert!(!h.read().unwrap_or_else(|e| e.into_inner()).use_fallback);
    }

    #[test]
    fn test_new_handle_returns_independent_handles() {
        let h1 = PhaseFreqTable::new_handle();
        let h2 = PhaseFreqTable::new_handle();
        h1.write().unwrap_or_else(|e| e.into_inner()).use_fallback = false;
        assert!(h2.read().unwrap_or_else(|e| e.into_inner()).use_fallback);
    }

    #[test]
    fn test_arc_clone_shares_state() {
        let h = PhaseFreqTable::new_handle();
        let c = Arc::clone(&h);
        h.write().unwrap_or_else(|e| e.into_inner()).use_fallback = false;
        assert!(!c.read().unwrap_or_else(|e| e.into_inner()).use_fallback);
    }

    // Poison recovery
    #[test]
    fn test_phase_freq_table_handle_poison_recovery() {
        let h = PhaseFreqTable::new_handle();
        let hc = Arc::clone(&h);
        let _ = std::panic::catch_unwind(move || {
            let _g = hc.write().unwrap_or_else(|e| e.into_inner());
            panic!("intentional");
        });
        // must not panic
        let g = h.read().unwrap_or_else(|e| e.into_inner());
        assert!(g.use_fallback);
    }

    // AC-07 / R-04: three 1.0-return paths
    #[test]
    fn test_phase_affinity_score_use_fallback_returns_one() {
        let t = PhaseFreqTable::new(); // use_fallback=true
        assert_eq!(t.phase_affinity_score(42, "decision", "delivery"), 1.0_f32);
        assert_eq!(t.phase_affinity_score(99, "pattern", "scope"), 1.0_f32);
    }

    #[test]
    fn test_phase_affinity_score_absent_phase_returns_one() {
        let t = table_with("scope", "decision", vec![(42, 1.0)]);
        assert_eq!(t.phase_affinity_score(42, "decision", "delivery"), 1.0_f32);
    }

    #[test]
    fn test_phase_affinity_score_absent_entry_returns_one() {
        let t = table_with("delivery", "decision", vec![(100, 1.0)]);
        assert_eq!(t.phase_affinity_score(99, "decision", "delivery"), 1.0_f32);
    }

    #[test]
    fn test_phase_affinity_score_present_entry_returns_rank_score() {
        let t = table_with("delivery", "decision", vec![(42, 2.0 / 3.0)]);
        let score = t.phase_affinity_score(42, "decision", "delivery");
        assert!(
            (score - 2.0_f32 / 3.0_f32).abs() < f32::EPSILON,
            "got {score}"
        );
    }

    // AC-13 / R-07: single-entry bucket must yield 1.0, NOT 0.0
    #[test]
    fn test_phase_affinity_score_single_entry_bucket_returns_one() {
        // N=1, rank=1: 1.0 - (1-1)/1 = 1.0
        let t = table_with("scope", "decision", rank_bucket(&[7]));
        assert_eq!(t.phase_affinity_score(7, "decision", "scope"), 1.0_f32);
    }

    // AC-14: exact scores for N=3 bucket
    #[test]
    fn test_rebuild_normalization_three_entry_bucket_exact_scores() {
        // entry_ids pre-sorted desc by freq: [10, 20, 30]
        let bucket = rank_bucket(&[10, 20, 30]);
        assert!(
            (bucket[0].1 - 1.0_f32).abs() < 1e-6,
            "rank-1={}",
            bucket[0].1
        );
        assert!(
            (bucket[1].1 - 2.0_f32 / 3.0_f32).abs() < 1e-5,
            "rank-2={}",
            bucket[1].1
        );
        assert!(
            (bucket[2].1 - 1.0_f32 / 3.0_f32).abs() < 1e-5,
            "rank-3={}",
            bucket[2].1
        );
        assert!(bucket[0].1 >= bucket[1].1 && bucket[1].1 >= bucket[2].1);

        let t = table_with("delivery", "decision", bucket);
        let s1 = t.phase_affinity_score(10, "decision", "delivery");
        let s2 = t.phase_affinity_score(20, "decision", "delivery");
        let s3 = t.phase_affinity_score(30, "decision", "delivery");
        assert!((s1 - 1.0_f32).abs() < 1e-6, "s1={s1}");
        assert!((s2 - 2.0_f32 / 3.0_f32).abs() < 1e-5, "s2={s2}");
        assert!((s3 - 1.0_f32 / 3.0_f32).abs() < 1e-5, "s3={s3}");
    }

    // R-07: 5-bucket last rank = 0.8, never 0.0
    #[test]
    fn test_rebuild_normalization_last_entry_in_five_bucket() {
        let bucket = rank_bucket(&[1, 2, 3, 4, 5]);
        let t = table_with("delivery", "pattern", bucket);
        let last = t.phase_affinity_score(5, "pattern", "delivery");
        assert!(
            (last - 0.8_f32).abs() < 1e-6,
            "rank-5 of 5 must be 0.8, got {last}"
        );
        assert!(last > 0.0_f32);
    }

    // AC-14: N=2 bucket
    #[test]
    fn test_rebuild_normalization_two_entry_bucket() {
        let bucket = rank_bucket(&[1, 2]);
        let t = table_with("scope", "pattern", bucket);
        assert_eq!(t.phase_affinity_score(1, "pattern", "scope"), 1.0_f32);
        assert!((t.phase_affinity_score(2, "pattern", "scope") - 0.5_f32).abs() < 1e-6);
    }

    // R-10: phase rename -> 1.0 (graceful degradation)
    #[test]
    fn test_phase_affinity_score_unknown_phase_returns_one() {
        let t = table_with("delivery", "decision", vec![(42, 1.0)]);
        assert_eq!(t.phase_affinity_score(42, "decision", "implement"), 1.0_f32);
    }
}
