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
#[path = "phase_freq_table_tests.rs"]
mod tests;
