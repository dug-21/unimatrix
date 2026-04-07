//! PhaseFreqTable: in-memory tick-rebuild cache of phase-conditioned access
//! frequency data.
//!
//! Rebuilt each background tick by `PhaseFreqTable::rebuild()` from the
//! `observations` table (explicit agent reads via `context_get` /
//! `context_lookup` `PreToolUse` events) with outcome weighting applied from
//! `cycle_events`. Replaces the former `query_log` search-exposure source
//! (crt-050 ADR-001).
//!
//! The search hot path acquires a short read lock, extracts a snapshot, and
//! releases before scoring — it never rebuilds per query.
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

// PhaseOutcomeRow is re-exported from the store crate root with #[doc(hidden)]
// to keep it an implementation detail while allowing cross-crate passing
// (Option A from pseudocode visibility note — weighting logic stays in server).
use unimatrix_store::PhaseOutcomeRow;

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

    /// Rebuild `PhaseFreqTable` from explicit-read observations with outcome weighting.
    ///
    /// Two-query path (crt-050 ADR-001):
    ///
    /// **Query A** — `store.query_phase_freq_observations(lookback_days)`:
    ///   Aggregates `(phase, category, entry_id, freq)` from deliberate agent reads
    ///   (`context_get` / `context_lookup` `PreToolUse` events) in `observations`.
    ///
    /// **Coverage gate** — `store.count_phase_session_pairs(lookback_days)`:
    ///   Counts distinct `(phase, session_id)` pairs. If below
    ///   `min_phase_session_pairs`, returns cold-start table with `use_fallback=true`
    ///   and emits `tracing::warn!` (AC-14).
    ///
    /// **Query B** — `store.query_phase_outcome_map()`:
    ///   Fetches `(phase, feature_cycle, outcome)` triples from `cycle_events` joined
    ///   to `sessions`. Error MUST propagate — never silently ignored (constraint C-7).
    ///
    /// **Rust post-process** — `apply_outcome_weights(rows_a, rows_b)`:
    ///   Applies per-phase MEAN outcome weight to each row's `freq` (ADR-001, R-03).
    ///
    /// **Rank normalization** (unchanged col-031 ADR-001 formula):
    ///   Groups by `(phase, category)`, applies `score = 1.0 - ((rank-1)/N)` per bucket.
    ///
    /// On store error: return `Err(e)`. Caller retains existing state (retain-on-error, R-09).
    pub async fn rebuild(
        store: &Store,
        lookback_days: u32,
        min_phase_session_pairs: u32,
    ) -> Result<Self, StoreError> {
        // Step 1: Query A — explicit-read aggregates from observations
        let rows_a: Vec<PhaseFreqRow> = store.query_phase_freq_observations(lookback_days).await?;

        // Step 2: Empty result → cold-start table (unchanged behavior)
        if rows_a.is_empty() {
            return Ok(PhaseFreqTable {
                table: HashMap::new(),
                use_fallback: true,
            });
        }

        // Step 3: Coverage gate — count distinct (phase, session_id) pairs.
        // rows_a does not carry session_id (aggregated away by GROUP BY), so a
        // separate COUNT query is required (pseudocode Option a).
        let coverage_count: i64 = store.count_phase_session_pairs(lookback_days).await?;
        if coverage_count < min_phase_session_pairs as i64 {
            tracing::warn!(
                coverage_count = coverage_count,
                threshold = min_phase_session_pairs,
                "PhaseFreqTable: distinct (phase, session_id) pairs ({}) below minimum \
                 threshold ({}); falling back to neutral scoring",
                coverage_count,
                min_phase_session_pairs,
            );
            return Ok(PhaseFreqTable {
                table: HashMap::new(),
                use_fallback: true,
            });
        }

        // Step 4: Query B — outcome map from cycle_events + sessions.
        // ERROR MUST PROPAGATE — do not catch and return empty (constraint C-7).
        let rows_b: Vec<PhaseOutcomeRow> = store.query_phase_outcome_map().await?;

        // Step 5: Apply per-phase mean outcome weights (Rust post-process, ADR-001)
        let weighted_rows = apply_outcome_weights(rows_a, rows_b);

        // Step 6: Group rows by (phase, category).
        // Rows are pre-sorted by (phase, category, freq DESC) from SQL ORDER BY.
        // We accumulate into a HashMap, preserving the SQL sort order within each group.
        let mut grouped: HashMap<(String, String), Vec<PhaseFreqRow>> = HashMap::new();
        for row in weighted_rows {
            grouped
                .entry((row.phase.clone(), row.category.clone()))
                .or_default()
                .push(row);
        }

        // Step 7: Rank-normalize each bucket (unchanged col-031 ADR-001 formula).
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

        // Step 8: Return populated table
        Ok(PhaseFreqTable {
            table,
            use_fallback: false,
        })
    }

    /// Return a learned `(phase, category)` weight map for W3-1 GNN cold-start.
    ///
    /// Weight = fraction of total explicit-read entries for the phase attributable
    /// to the category. Formula: `bucket.len() / total_entries_for_phase` — this is
    /// categorical **breadth** (distinct entries accessed per category), NOT categorical
    /// depth (how often entries were accessed). The distribution sums to 1.0 per phase
    /// (up to f32 rounding) — ADR-008 constraint #9.
    ///
    /// W3-1 implementers: if you need a weighted-sum (freq-based) projection, access
    /// `self.table` directly. This method is breadth-only by design.
    ///
    /// Returns an empty map when `use_fallback = true` (no signal available, AC-08).
    ///
    /// NOT called on the search hot path — GNN initialization only (NFR-07).
    pub fn phase_category_weights(&self) -> HashMap<(String, String), f32> {
        // Cold-start: no data, return empty map (AC-08)
        if self.use_fallback {
            return HashMap::new();
        }

        // Step 1: Compute total distinct-entry count per phase
        // total_entries_for_phase[phase] = sum of bucket.len() across all categories
        let mut phase_totals: HashMap<String, usize> = HashMap::new();
        for ((phase, _category), bucket) in &self.table {
            *phase_totals.entry(phase.clone()).or_insert(0) += bucket.len();
        }

        // Step 2: For each (phase, category) bucket, weight = bucket.len() / phase_total
        let mut result: HashMap<(String, String), f32> = HashMap::with_capacity(self.table.len());
        for ((phase, category), bucket) in &self.table {
            // unwrap_or(1) guards zero-divide; in practice impossible — a phase key
            // exists only if it has at least one bucket (defensive coding required).
            let total = *phase_totals.get(phase).unwrap_or(&1);
            let weight = bucket.len() as f32 / total as f32;
            result.insert((phase.clone(), category.clone()), weight);
        }

        result
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

// ---------------------------------------------------------------------------
// Private free functions
// ---------------------------------------------------------------------------

/// Map a cycle outcome string to a weighting factor.
///
/// Priority order: "rework" checked before "fail" (ADR-003 constraint #7, crt-050).
/// This mirrors the priority ordering in `infer_gate_result()` in `mcp/tools.rs`
/// (col-026 R-03). Any future change to the canonical outcome vocabulary must
/// update BOTH this function and `infer_gate_result()`.
///
/// Mapping (case-insensitive substring match):
///   contains "rework" → 0.5  (checked FIRST)
///   contains "fail"   → 0.5
///   contains "pass"   → 1.0
///   anything else (including "unknown", "") → 1.0 (graceful degradation, AC-05)
fn outcome_weight(outcome: &str) -> f32 {
    let lower = outcome.to_lowercase();
    // rework checked BEFORE fail — priority order (ADR-003 constraint #7)
    if lower.contains("rework") {
        return 0.5;
    }
    if lower.contains("fail") {
        return 0.5;
    }
    if lower.contains("pass") {
        return 1.0;
    }
    // All other strings (unknown, empty, unrecognized): graceful degradation = 1.0
    // AC-05 contract: missing outcome = unweighted = weight 1.0
    1.0
}

/// Apply per-phase mean outcome weights to explicit-read frequency rows.
///
/// Builds a per-phase weight by averaging `outcome_weight()` across all
/// `cycle_phase_end` rows for each phase (per-phase MEAN, not per-cycle —
/// ADR-001 constraint #6, R-03). This preserves rank ordering invariant
/// within buckets: all rows for the same phase share the same multiplier,
/// so relative ordering is unchanged before rank normalization.
///
/// When no outcome rows exist for a phase, the default weight `1.0` is used
/// (AC-05 contract: missing outcome = unweighted).
///
/// The weighted `freq` is stored back as `i64` (truncating via `as i64` cast).
/// Rank normalization uses only ordering, not absolute magnitude — the cast is
/// invariant to the normalization formula (col-031 ADR-001).
fn apply_outcome_weights(
    rows: Vec<PhaseFreqRow>,
    outcome_rows: Vec<PhaseOutcomeRow>,
) -> Vec<PhaseFreqRow> {
    // Step 1: Collect outcome weights per phase
    let mut raw_weights: HashMap<String, Vec<f32>> = HashMap::new();
    for outcome_row in outcome_rows {
        let w = outcome_weight(&outcome_row.outcome);
        raw_weights.entry(outcome_row.phase).or_default().push(w);
    }

    // Step 2: Compute per-phase MEAN weight (ADR-001 OQ-1, constraint #6)
    let phase_weights: HashMap<String, f32> = raw_weights
        .into_iter()
        .map(|(phase, weight_vec)| {
            let mean = weight_vec.iter().sum::<f32>() / weight_vec.len() as f32;
            (phase, mean)
        })
        .collect();

    // Step 3: Apply per-phase mean weight to each row's freq
    rows.into_iter()
        .map(|mut row| {
            let weight = phase_weights.get(&row.phase).copied().unwrap_or(1.0_f32);
            row.freq = (row.freq as f32 * weight) as i64;
            row
        })
        .collect()
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

    // R-07: 5-bucket last rank = 0.2 (= 1 - 4/5), never 0.0.
    // Formula: 1.0 - ((rank-1)/N). Rank 5 of 5: 1.0 - 4/5 = 0.2.
    // (The banned `1-rank/N` form would give 0.0 for rank=N — this test guards against that.)
    #[test]
    fn test_rebuild_normalization_last_entry_in_five_bucket() {
        let bucket = rank_bucket(&[1, 2, 3, 4, 5]);
        let t = table_with("delivery", "pattern", bucket);
        let last = t.phase_affinity_score(5, "pattern", "delivery");
        assert!(
            (last - 0.2_f32).abs() < 1e-5,
            "rank-5 of 5 must be ~0.2, got {last}"
        );
        assert!(last > 0.0_f32, "last-rank entry must never be 0.0");
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

    // -----------------------------------------------------------------------
    // New tests: outcome_weight() — R-02, AC-13b/c/d/e
    // -----------------------------------------------------------------------

    // Build PhaseFreqRow test helper
    fn make_freq_row(phase: &str, category: &str, entry_id: u64, freq: i64) -> PhaseFreqRow {
        PhaseFreqRow {
            phase: phase.to_string(),
            category: category.to_string(),
            entry_id,
            freq,
        }
    }

    // Build PhaseOutcomeRow test helper
    fn make_outcome_row(phase: &str, feature_cycle: &str, outcome: &str) -> PhaseOutcomeRow {
        PhaseOutcomeRow {
            phase: phase.to_string(),
            feature_cycle: feature_cycle.to_string(),
            outcome: outcome.to_string(),
        }
    }

    // T-PFT-14 / R-02 scenario 1: pass variants return 1.0
    #[test]
    fn test_outcome_weight_pass_variants_return_1_0() {
        assert_eq!(outcome_weight("pass"), 1.0_f32);
        assert_eq!(outcome_weight("PASS"), 1.0_f32);
        assert_eq!(outcome_weight("Pass"), 1.0_f32);
    }

    // T-PFT-14 / R-02 scenario 1: rework variants return 0.5
    #[test]
    fn test_outcome_weight_rework_variants_return_0_5() {
        assert_eq!(outcome_weight("rework"), 0.5_f32);
        assert_eq!(outcome_weight("REWORK"), 0.5_f32);
        assert_eq!(outcome_weight("Rework"), 0.5_f32);
    }

    // T-PFT-14 / R-02 scenario 1: fail variants return 0.5
    #[test]
    fn test_outcome_weight_fail_variants_return_0_5() {
        assert_eq!(outcome_weight("fail"), 0.5_f32);
        assert_eq!(outcome_weight("FAIL"), 0.5_f32);
        assert_eq!(outcome_weight("FAILED"), 0.5_f32);
    }

    // T-PFT-14 / R-02 scenario 1: unknown/empty return 1.0 (graceful degradation)
    #[test]
    fn test_outcome_weight_unknown_and_empty_return_1_0() {
        assert_eq!(outcome_weight("unknown"), 1.0_f32);
        assert_eq!(outcome_weight("abandoned"), 1.0_f32);
        assert_eq!(outcome_weight(""), 1.0_f32);
    }

    // T-PFT-15 / R-02 scenario 2+3: rework checked before fail (priority order, ADR-003)
    #[test]
    fn test_outcome_weight_rework_checked_before_fail() {
        // "rework-and-fail" contains both "rework" and "fail"
        // rework branch must fire first → returns 0.5 (not double-penalized)
        assert_eq!(outcome_weight("rework-and-fail"), 0.5_f32);
        assert_eq!(outcome_weight("rework_fail"), 0.5_f32);
        // "fail_rework" — rework check (via contains) fires first too
        assert_eq!(outcome_weight("fail_rework"), 0.5_f32);
    }

    // -----------------------------------------------------------------------
    // New tests: apply_outcome_weights() — R-03 / AC-04
    // -----------------------------------------------------------------------

    // T-PFT (AC-13b): single cycle pass → weight 1.0, freq unchanged
    #[test]
    fn test_apply_outcome_weights_single_cycle_pass_weights_1_0() {
        let freq_rows = vec![make_freq_row("delivery", "decision", 1, 10)];
        let outcome_rows = vec![make_outcome_row("delivery", "c-1", "PASS")];
        let result = apply_outcome_weights(freq_rows, outcome_rows);
        assert_eq!(result[0].freq, 10); // 10 * 1.0 = 10
    }

    // T-PFT (AC-13c): single cycle rework → weight 0.5
    #[test]
    fn test_apply_outcome_weights_single_cycle_rework_weights_0_5() {
        let freq_rows = vec![make_freq_row("delivery", "decision", 1, 10)];
        let outcome_rows = vec![make_outcome_row("delivery", "c-1", "REWORK")];
        let result = apply_outcome_weights(freq_rows, outcome_rows);
        assert_eq!(result[0].freq, 5); // 10 * 0.5 = 5
    }

    // T-PFT (AC-05): no outcome rows → default weight 1.0, freq unchanged
    #[test]
    fn test_apply_outcome_weights_no_outcome_rows_defaults_to_1_0() {
        let freq_rows = vec![make_freq_row("delivery", "decision", 1, 8)];
        let result = apply_outcome_weights(freq_rows, vec![]);
        assert_eq!(result[0].freq, 8); // default weight 1.0
    }

    // T-PFT (AC-13e): phase not in outcome rows → default weight 1.0
    #[test]
    fn test_apply_outcome_weights_missing_phase_defaults_to_1_0() {
        let freq_rows = vec![make_freq_row("delivery", "decision", 1, 6)];
        // outcome row is for "scope", not "delivery"
        let outcome_rows = vec![make_outcome_row("scope", "c-1", "REWORK")];
        let result = apply_outcome_weights(freq_rows, outcome_rows);
        assert_eq!(result[0].freq, 6); // default 1.0 for unmatched phase
    }

    // T-PFT-06 (R-03 key test): per-phase MEAN not per-cycle
    // Phase "delivery": cycle-A (pass=1.0), cycle-B (rework=0.5) → mean=0.75
    #[test]
    fn test_apply_outcome_weights_mixed_cycles_uses_per_phase_mean() {
        let freq_rows = vec![
            make_freq_row("delivery", "decision", 10, 18),
            make_freq_row("delivery", "decision", 20, 15),
        ];
        let outcome_rows = vec![
            make_outcome_row("delivery", "cycle-A", "PASS"),
            make_outcome_row("delivery", "cycle-B", "REWORK"),
        ];
        let result = apply_outcome_weights(freq_rows, outcome_rows);
        // per-phase mean = (1.0 + 0.5) / 2 = 0.75
        // 18 * 0.75 = 13.5 → as i64 = 13
        // 15 * 0.75 = 11.25 → as i64 = 11
        assert!(
            result[0].freq == 13 || result[0].freq == 14,
            "got {}",
            result[0].freq
        );
        assert_eq!(result[1].freq, 11);
        // rank ordering preserved: entry 10 still above entry 20
        assert!(result[0].freq > result[1].freq);
    }

    // T-PFT-07 (R-03 ordering invariant): per-phase mean preserves relative ordering
    #[test]
    fn test_apply_outcome_weights_per_phase_mean_not_per_cycle() {
        let freq_rows = vec![
            make_freq_row("scope", "decision", 1, 10),
            make_freq_row("scope", "decision", 2, 8),
        ];
        let outcome_rows = vec![
            make_outcome_row("scope", "ca", "PASS"),   // weight 1.0
            make_outcome_row("scope", "cb", "REWORK"), // weight 0.5
        ];
        let result = apply_outcome_weights(freq_rows, outcome_rows);
        // mean = 0.75; entry 1 gets 10*0.75=7, entry 2 gets 8*0.75=6
        assert_eq!(result[0].entry_id, 1, "entry 1 must remain first");
        assert!(
            result[0].freq > result[1].freq,
            "relative ordering must be preserved"
        );
        // weight was applied: 10*1.0=10 would indicate no weighting
        assert!(result[0].freq < 10, "weight must be applied (not 1.0 path)");
    }

    // -----------------------------------------------------------------------
    // New tests: phase_category_weights() — AC-08, R-07
    // -----------------------------------------------------------------------

    // Build a table with multiple buckets for testing phase_category_weights
    fn table_with_buckets(buckets: Vec<(&str, &str, Vec<(u64, f32)>)>) -> PhaseFreqTable {
        let mut m = HashMap::new();
        for (phase, cat, bucket) in buckets {
            m.insert((phase.to_string(), cat.to_string()), bucket);
        }
        PhaseFreqTable {
            table: m,
            use_fallback: false,
        }
    }

    // T-PFT-11: cold-start → empty map (AC-08a)
    #[test]
    fn test_phase_category_weights_cold_start_returns_empty_map() {
        let t = PhaseFreqTable::new(); // use_fallback = true
        assert!(t.phase_category_weights().is_empty());
    }

    // T-PFT-13: single category → weight 1.0 (R-07 edge)
    #[test]
    fn test_phase_category_weights_single_category_returns_1_0() {
        let t = table_with("delivery", "decision", vec![(1, 1.0)]);
        let weights = t.phase_category_weights();
        let w = weights
            .get(&("delivery".to_string(), "decision".to_string()))
            .copied()
            .unwrap_or(0.0);
        assert!(
            (w - 1.0_f32).abs() < 1e-6,
            "single category must be 1.0, got {w}"
        );
    }

    // T-PFT-12: two categories — correct distribution and sum=1.0 (AC-08b, R-07)
    #[test]
    fn test_phase_category_weights_two_categories_sums_to_1_0() {
        // "decision": 2 entries, "pattern": 1 entry → total 3
        let t = table_with_buckets(vec![
            ("delivery", "decision", vec![(1, 1.0), (2, 0.5)]),
            ("delivery", "pattern", vec![(3, 1.0)]),
        ]);
        let weights = t.phase_category_weights();

        let w_decision = weights
            .get(&("delivery".to_string(), "decision".to_string()))
            .copied()
            .unwrap_or(0.0);
        let w_pattern = weights
            .get(&("delivery".to_string(), "pattern".to_string()))
            .copied()
            .unwrap_or(0.0);

        assert!(
            (w_decision - 2.0_f32 / 3.0_f32).abs() < 1e-6,
            "decision={w_decision}"
        );
        assert!(
            (w_pattern - 1.0_f32 / 3.0_f32).abs() < 1e-6,
            "pattern={w_pattern}"
        );
        assert!(
            (w_decision + w_pattern - 1.0_f32).abs() < 1e-6,
            "sum must be 1.0"
        );
    }

    // R-07 explicit test: breadth-based (entry count), NOT frequency-weighted
    #[test]
    fn test_phase_category_weights_breadth_not_freq_sum() {
        // 1 entry in "decision" (freq=10), 10 entries in "pattern" (freq=1 each)
        // breadth: decision=1/11, pattern=10/11
        // (NOT frequency-weighted: which would give decision=10/20, pattern=10/20)
        let pattern_bucket: Vec<(u64, f32)> = (1u64..=10).map(|i| (i, 1.0)).collect();
        let t = table_with_buckets(vec![
            ("scope", "decision", vec![(100, 1.0)]),
            ("scope", "pattern", pattern_bucket),
        ]);
        let weights = t.phase_category_weights();

        let w_decision = weights
            .get(&("scope".to_string(), "decision".to_string()))
            .copied()
            .unwrap_or(0.0);
        let w_pattern = weights
            .get(&("scope".to_string(), "pattern".to_string()))
            .copied()
            .unwrap_or(0.0);

        assert!(
            (w_decision - 1.0_f32 / 11.0_f32).abs() < 1e-5,
            "decision={w_decision}"
        );
        assert!(
            (w_pattern - 10.0_f32 / 11.0_f32).abs() < 1e-5,
            "pattern={w_pattern}"
        );
    }

    // T-PFT-12 variant: multiple phases are independent (each sums to 1.0)
    #[test]
    fn test_phase_category_weights_multiple_phases_independent() {
        // "delivery": "decision"=2 entries, "pattern"=1 entry (total 3)
        // "scope":    "decision"=1 entry, "lesson-learned"=1 entry (total 2)
        let t = table_with_buckets(vec![
            ("delivery", "decision", vec![(1, 1.0), (2, 0.5)]),
            ("delivery", "pattern", vec![(3, 1.0)]),
            ("scope", "decision", vec![(4, 1.0)]),
            ("scope", "lesson-learned", vec![(5, 1.0)]),
        ]);
        let weights = t.phase_category_weights();

        // delivery sum
        let delivery_sum: f32 = weights
            .iter()
            .filter(|((p, _), _)| p == "delivery")
            .map(|(_, &w)| w)
            .sum();
        // scope sum
        let scope_sum: f32 = weights
            .iter()
            .filter(|((p, _), _)| p == "scope")
            .map(|(_, &w)| w)
            .sum();

        assert!(
            (delivery_sum - 1.0_f32).abs() < 1e-6,
            "delivery sum={delivery_sum}"
        );
        assert!((scope_sum - 1.0_f32).abs() < 1e-6, "scope sum={scope_sum}");
    }
}
