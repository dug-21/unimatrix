//! Knowledge effectiveness analysis engine.
//!
//! Pure computation module that classifies entries into five effectiveness
//! categories based on injection/outcome data, computes confidence calibration
//! buckets, and aggregates results by trust source. Zero I/O, fully
//! deterministic. Pattern follows `confidence.rs`.

use serde::Serialize;
use std::collections::HashMap;

#[cfg(test)]
mod tests_aggregate;
#[cfg(test)]
mod tests_classify;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum distinct sessions with injection before an entry can be classified
/// as Ineffective.
pub const INEFFECTIVE_MIN_INJECTIONS: u32 = 3;

/// Outcome weight for successful sessions.
pub const OUTCOME_WEIGHT_SUCCESS: f64 = 1.0;

/// Outcome weight for rework sessions.
pub const OUTCOME_WEIGHT_REWORK: f64 = 0.5;

/// Outcome weight for abandoned sessions.
pub const OUTCOME_WEIGHT_ABANDONED: f64 = 0.0;

/// Trust sources considered "noisy" for classification (ADR-004).
pub const NOISY_TRUST_SOURCES: &[&str] = &["auto"];

/// Additive utility boost for Effective-classified entries at query time.
/// Applied inside the status_penalty multiplication (ADR-003).
pub const UTILITY_BOOST: f64 = 0.05;

/// Additive utility boost for Settled-classified entries at query time.
/// Must be strictly less than co-access boost maximum (0.03) per Constraint 5.
pub const SETTLED_BOOST: f64 = 0.01;

/// Additive utility penalty magnitude for Ineffective and Noisy entries.
/// Applied as `-UTILITY_PENALTY` at query time.
pub const UTILITY_PENALTY: f64 = 0.05;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Effectiveness classification category for a knowledge entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EffectivenessCategory {
    Effective,
    Settled,
    Unmatched,
    Ineffective,
    Noisy,
}

/// Per-entry effectiveness classification result.
#[derive(Debug, Clone, Serialize)]
pub struct EntryEffectiveness {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub category: EffectivenessCategory,
    pub injection_count: u32,
    pub success_rate: f64,
    pub helpfulness_ratio: f64,
}

/// Aggregated effectiveness per trust source.
#[derive(Debug, Clone, Serialize)]
pub struct SourceEffectiveness {
    pub trust_source: String,
    pub total_entries: u32,
    pub effective_count: u32,
    pub settled_count: u32,
    pub unmatched_count: u32,
    pub ineffective_count: u32,
    pub noisy_count: u32,
    pub aggregate_utility: f64,
}

/// Confidence calibration bucket.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationBucket {
    pub confidence_lower: f64,
    pub confidence_upper: f64,
    pub entry_count: u32,
    pub actual_success_rate: f64,
}

/// Data coverage indicator (ADR-003).
#[derive(Debug, Clone, Serialize)]
pub struct DataWindow {
    pub session_count: u32,
    pub earliest_session_at: Option<u64>,
    pub latest_session_at: Option<u64>,
}

/// Complete effectiveness analysis result.
#[derive(Debug, Clone, Serialize)]
pub struct EffectivenessReport {
    pub by_category: Vec<(EffectivenessCategory, u32)>,
    pub by_source: Vec<SourceEffectiveness>,
    pub calibration: Vec<CalibrationBucket>,
    pub top_ineffective: Vec<EntryEffectiveness>,
    pub noisy_entries: Vec<EntryEffectiveness>,
    pub unmatched_entries: Vec<EntryEffectiveness>,
    pub data_window: DataWindow,
    /// All classified entries from the most recent background tick.
    /// Used by the background tick to build the category map for `EffectivenessState`
    /// without re-querying the store. Empty when constructed outside the tick path.
    #[serde(default)]
    pub all_entries: Vec<EntryEffectiveness>,
    /// Entry IDs quarantined by the most recent background maintenance tick.
    /// Populated by maintenance_tick() after auto-quarantine SQL writes complete.
    /// Empty when auto-quarantine is disabled or no entries crossed the threshold.
    /// Surfaced in context_status output (FR-14).
    #[serde(default)]
    pub auto_quarantined_this_cycle: Vec<u64>,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Compute weighted success rate from outcome counts.
///
/// Returns 0.0 when total outcomes is zero (no division by zero).
pub fn utility_score(success: u32, rework: u32, abandoned: u32) -> f64 {
    let total = (success as u64) + (rework as u64) + (abandoned as u64);
    if total == 0 {
        return 0.0;
    }
    let weighted = (success as f64) * OUTCOME_WEIGHT_SUCCESS
        + (rework as f64) * OUTCOME_WEIGHT_REWORK
        + (abandoned as f64) * OUTCOME_WEIGHT_ABANDONED;
    weighted / (total as f64)
}

/// Classify a single entry given its injection/outcome stats and topic activity.
///
/// Classification priority: Noisy > Ineffective > Unmatched > Settled > Effective.
/// First matching rule wins (FR-01, R-01).
#[allow(clippy::too_many_arguments)]
pub fn classify_entry(
    entry_id: u64,
    title: &str,
    topic: &str,
    trust_source: &str,
    helpful_count: u32,
    unhelpful_count: u32,
    injection_count: u32,
    success_count: u32,
    rework_count: u32,
    abandoned_count: u32,
    topic_has_sessions: bool,
    noisy_trust_sources: &[&str],
) -> EntryEffectiveness {
    let rate = utility_score(success_count, rework_count, abandoned_count);

    let total_votes = (helpful_count as u64) + (unhelpful_count as u64);
    let helpfulness_ratio = if total_votes > 0 {
        (helpful_count as f64) / (total_votes as f64)
    } else {
        0.0
    };

    // ADR-002: map empty topic to "(unattributed)"
    let resolved_topic = if topic.is_empty() {
        "(unattributed)".to_string()
    } else {
        topic.to_string()
    };

    // Classification priority chain (R-01)
    let category = if noisy_trust_sources.contains(&trust_source)
        && helpful_count == 0
        && injection_count >= 1
    {
        EffectivenessCategory::Noisy
    } else if injection_count >= INEFFECTIVE_MIN_INJECTIONS && rate < 0.3 {
        EffectivenessCategory::Ineffective
    } else if injection_count == 0 && topic_has_sessions {
        EffectivenessCategory::Unmatched
    } else if !topic_has_sessions && injection_count > 0 && success_count > 0 {
        EffectivenessCategory::Settled
    } else {
        EffectivenessCategory::Effective
    };

    EntryEffectiveness {
        entry_id,
        title: title.to_string(),
        topic: resolved_topic,
        trust_source: trust_source.to_string(),
        category,
        injection_count,
        success_rate: rate,
        helpfulness_ratio,
    }
}

/// Aggregate per-entry classifications into source-level stats.
///
/// Results are sorted by trust source name for deterministic output.
pub fn aggregate_by_source(entries: &[EntryEffectiveness]) -> Vec<SourceEffectiveness> {
    let mut groups: HashMap<&str, Vec<&EntryEffectiveness>> = HashMap::new();
    for entry in entries {
        groups.entry(&entry.trust_source).or_default().push(entry);
    }

    let mut sources: Vec<&str> = groups.keys().copied().collect();
    sources.sort();

    let mut result = Vec::with_capacity(sources.len());
    for source in sources {
        let group = &groups[source];
        let total_entries = group.len() as u32;

        let mut effective_count = 0u32;
        let mut settled_count = 0u32;
        let mut unmatched_count = 0u32;
        let mut ineffective_count = 0u32;
        let mut noisy_count = 0u32;

        for entry in group {
            match entry.category {
                EffectivenessCategory::Effective => effective_count += 1,
                EffectivenessCategory::Settled => settled_count += 1,
                EffectivenessCategory::Unmatched => unmatched_count += 1,
                EffectivenessCategory::Ineffective => ineffective_count += 1,
                EffectivenessCategory::Noisy => noisy_count += 1,
            }
        }

        // Aggregate utility: average success_rate across entries with injections
        let injected: Vec<&&EntryEffectiveness> =
            group.iter().filter(|e| e.injection_count > 0).collect();

        let aggregate_utility = if injected.is_empty() {
            0.0
        } else {
            let sum: f64 = injected.iter().map(|e| e.success_rate).sum();
            sum / (injected.len() as f64)
        };

        result.push(SourceEffectiveness {
            trust_source: source.to_string(),
            total_entries,
            effective_count,
            settled_count,
            unmatched_count,
            ineffective_count,
            noisy_count,
            aggregate_utility,
        });
    }

    result
}

/// Build calibration buckets from injection-time confidence and session outcomes.
///
/// Produces 10 buckets of 0.1 width: [0.0, 0.1), [0.1, 0.2), ..., [0.9, 1.0].
/// Last bucket is inclusive on both ends. Values outside [0.0, 1.0] are clamped.
pub fn build_calibration_buckets(rows: &[(f64, bool)]) -> Vec<CalibrationBucket> {
    // (count, success_sum)
    let mut buckets = [(0u32, 0.0f64); 10];

    for &(confidence, succeeded) in rows {
        let index = if confidence >= 1.0 {
            9
        } else if confidence < 0.0 {
            0
        } else {
            ((confidence * 10.0).floor() as usize).min(9)
        };

        buckets[index].0 += 1;
        if succeeded {
            buckets[index].1 += 1.0;
        }
    }

    let mut result = Vec::with_capacity(10);
    for (i, &(count, success_sum)) in buckets.iter().enumerate() {
        let lower = i as f64 * 0.1;
        let upper = (i + 1) as f64 * 0.1;
        let actual_success_rate = if count > 0 {
            success_sum / (count as f64)
        } else {
            0.0
        };
        result.push(CalibrationBucket {
            confidence_lower: lower,
            confidence_upper: upper,
            entry_count: count,
            actual_success_rate,
        });
    }

    result
}

/// Assemble the full EffectivenessReport from raw components.
///
/// Caps: top 10 ineffective (by injection_count desc, then success_rate asc),
/// all noisy (no cap), top 10 unmatched (by topic then entry_id).
pub fn build_report(
    classifications: Vec<EntryEffectiveness>,
    calibration_rows: &[(f64, bool)],
    data_window: DataWindow,
) -> EffectivenessReport {
    use EffectivenessCategory::*;

    // Category counts in enum order
    let by_category: Vec<(EffectivenessCategory, u32)> =
        [Effective, Settled, Unmatched, Ineffective, Noisy]
            .iter()
            .map(|&cat| {
                let count = classifications.iter().filter(|e| e.category == cat).count() as u32;
                (cat, count)
            })
            .collect();

    let by_source = aggregate_by_source(&classifications);
    let calibration = build_calibration_buckets(calibration_rows);

    // Top ineffective: up to 10, sorted by injection_count desc, then success_rate asc
    let mut ineffective: Vec<EntryEffectiveness> = classifications
        .iter()
        .filter(|e| e.category == Ineffective)
        .cloned()
        .collect();
    ineffective.sort_by(|a, b| {
        b.injection_count.cmp(&a.injection_count).then_with(|| {
            a.success_rate
                .partial_cmp(&b.success_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    ineffective.truncate(10);

    // Noisy: all (no cap)
    let noisy_entries: Vec<EntryEffectiveness> = classifications
        .iter()
        .filter(|e| e.category == Noisy)
        .cloned()
        .collect();

    // Unmatched: up to 10, sorted by topic then entry_id
    let mut unmatched: Vec<EntryEffectiveness> = classifications
        .iter()
        .filter(|e| e.category == Unmatched)
        .cloned()
        .collect();
    unmatched.sort_by(|a, b| {
        a.topic
            .cmp(&b.topic)
            .then_with(|| a.entry_id.cmp(&b.entry_id))
    });
    unmatched.truncate(10);

    EffectivenessReport {
        by_category,
        by_source,
        calibration,
        top_ineffective: ineffective,
        noisy_entries,
        unmatched_entries: unmatched,
        data_window,
        all_entries: classifications,
        auto_quarantined_this_cycle: Vec::new(),
    }
}
