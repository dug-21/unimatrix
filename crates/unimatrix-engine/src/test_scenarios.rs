//! Shared test infrastructure for pipeline validation.
//!
//! Provides reusable scenario builders, assertion helpers, and ranking metrics
//! for validating the intelligence pipeline across crates.
//!
//! Key types: [`EntryProfile`] (signal state), [`CalibrationScenario`] (ordering),
//! [`RetrievalScenario`] (re-ranking), [`kendall_tau`] (rank correlation).
//!
//! When golden values change: run calibration tests first to confirm the new
//! ordering is correct, then update `tests/pipeline_regression.rs`.

use std::collections::HashMap;

use unimatrix_core::{EntryRecord, Status};

use crate::confidence::compute_confidence;

/// Canonical timestamp for deterministic tests: ~2023-11-14.
pub const CANONICAL_NOW: u64 = 1_700_000_000;

/// Deterministic description of an entry's signal state for test scenarios.
#[derive(Debug, Clone)]
pub struct EntryProfile {
    pub label: &'static str,
    pub status: Status,
    pub access_count: u32,
    pub last_accessed_at: u64,
    pub created_at: u64,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
    pub correction_count: u32,
    pub trust_source: &'static str,
    pub category: &'static str,
}

/// A complete calibration scenario: entries plus expected ordering.
#[derive(Debug, Clone)]
pub struct CalibrationScenario {
    pub name: &'static str,
    pub description: &'static str,
    pub entries: Vec<EntryProfile>,
    pub now: u64,
    /// Indices into `entries`, highest confidence first.
    pub expected_ordering: Vec<usize>,
}

/// An entry with content for retrieval testing.
#[derive(Debug, Clone)]
pub struct RetrievalEntry {
    pub profile: EntryProfile,
    pub title: &'static str,
    pub content: &'static str,
    pub embedding: Option<Vec<f32>>,
    pub superseded_by: Option<usize>,
}

/// A retrieval scenario with query and expected ranking.
#[derive(Debug, Clone)]
pub struct RetrievalScenario {
    pub name: &'static str,
    pub description: &'static str,
    pub entries: Vec<RetrievalEntry>,
    pub query: &'static str,
    pub expected_top_k: Vec<usize>,
    pub pairwise_assertions: Vec<(usize, usize)>,
}

/// Convert an `EntryProfile` to a full `EntryRecord` with deterministic defaults.
pub fn profile_to_entry_record(profile: &EntryProfile, id: u64, now: u64) -> EntryRecord {
    EntryRecord {
        id,
        title: profile.label.to_string(),
        content: format!("Test content for {}", profile.label),
        topic: "test".to_string(),
        category: profile.category.to_string(),
        tags: vec![],
        source: "test".to_string(),
        status: profile.status,
        confidence: 0.0,
        created_at: profile.created_at,
        updated_at: now,
        last_accessed_at: profile.last_accessed_at,
        access_count: profile.access_count,
        supersedes: None,
        superseded_by: None,
        correction_count: profile.correction_count,
        embedding_dim: 0,
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        content_hash: String::new(),
        previous_hash: String::new(),
        version: 1,
        feature_cycle: String::new(),
        trust_source: profile.trust_source.to_string(),
        helpful_count: profile.helpful_count,
        unhelpful_count: profile.unhelpful_count,
        pre_quarantine_status: None,
    }
}

/// Active, high-access, recent, many helpful votes, human-authored.
pub fn expert_human_fresh() -> EntryProfile {
    EntryProfile {
        label: "expert-human-fresh",
        status: Status::Active,
        access_count: 30,
        last_accessed_at: CANONICAL_NOW - 3600, // 1 hour ago
        created_at: CANONICAL_NOW - 7 * 24 * 3600, // 1 week ago
        helpful_count: 10,
        unhelpful_count: 1,
        correction_count: 1,
        trust_source: "human",
        category: "decision",
    }
}

/// Active, moderate access, moderately fresh, some helpful votes, agent-authored.
pub fn good_agent_entry() -> EntryProfile {
    EntryProfile {
        label: "good-agent-entry",
        status: Status::Active,
        access_count: 15,
        last_accessed_at: CANONICAL_NOW - 3 * 24 * 3600, // 3 days ago
        created_at: CANONICAL_NOW - 14 * 24 * 3600, // 2 weeks ago
        helpful_count: 5,
        unhelpful_count: 1,
        correction_count: 2,
        trust_source: "agent",
        category: "convention",
    }
}

/// Proposed, low access, very recent, no votes, auto-extracted.
pub fn auto_extracted_new() -> EntryProfile {
    EntryProfile {
        label: "auto-extracted-new",
        status: Status::Proposed,
        access_count: 2,
        last_accessed_at: CANONICAL_NOW - 1800, // 30 minutes ago
        created_at: CANONICAL_NOW - 3600, // 1 hour ago
        helpful_count: 0,
        unhelpful_count: 0,
        correction_count: 0,
        trust_source: "auto",
        category: "pattern",
    }
}

/// Deprecated, moderate access, very stale, mixed votes, human-authored.
pub fn stale_deprecated() -> EntryProfile {
    EntryProfile {
        label: "stale-deprecated",
        status: Status::Deprecated,
        access_count: 10,
        last_accessed_at: CANONICAL_NOW - 90 * 24 * 3600, // 90 days ago
        created_at: CANONICAL_NOW - 180 * 24 * 3600, // 180 days ago
        helpful_count: 3,
        unhelpful_count: 3,
        correction_count: 4,
        trust_source: "human",
        category: "convention",
    }
}

/// Quarantined, low access, stale, mostly unhelpful, unknown source.
pub fn quarantined_bad() -> EntryProfile {
    EntryProfile {
        label: "quarantined-bad",
        status: Status::Quarantined,
        access_count: 1,
        last_accessed_at: CANONICAL_NOW - 30 * 24 * 3600, // 30 days ago
        created_at: CANONICAL_NOW - 60 * 24 * 3600, // 60 days ago
        helpful_count: 1,
        unhelpful_count: 8,
        correction_count: 7,
        trust_source: "unknown",
        category: "gap",
    }
}

/// 5 profiles, expected order: expert > good_agent > auto_new > stale > quarantined.
pub fn standard_ranking() -> CalibrationScenario {
    CalibrationScenario {
        name: "standard_ranking",
        description: "Full signal diversity: expert human-authored entry should rank highest, \
            followed by a decent agent entry, then a freshly extracted one with no votes, \
            then a stale deprecated entry, and finally a quarantined bad entry.",
        entries: vec![
            expert_human_fresh(),
            good_agent_entry(),
            auto_extracted_new(),
            stale_deprecated(),
            quarantined_bad(),
        ],
        now: CANONICAL_NOW,
        expected_ordering: vec![0, 1, 2, 3, 4],
    }
}

/// Same base signals except trust_source varies: human > system > agent > neural > auto.
pub fn trust_source_ordering() -> CalibrationScenario {
    let base = EntryProfile {
        label: "trust-base",
        status: Status::Active,
        access_count: 20,
        last_accessed_at: CANONICAL_NOW - 24 * 3600,
        created_at: CANONICAL_NOW - 7 * 24 * 3600,
        helpful_count: 5,
        unhelpful_count: 1,
        correction_count: 1,
        trust_source: "human",
        category: "decision",
    };

    let sources = ["human", "system", "agent", "neural", "auto"];
    let entries: Vec<EntryProfile> = sources
        .iter()
        .map(|&src| EntryProfile {
            label: match src {
                "human" => "trust-human",
                "system" => "trust-system",
                "agent" => "trust-agent",
                "neural" => "trust-neural",
                "auto" => "trust-auto",
                _ => "trust-unknown",
            },
            trust_source: src,
            ..base.clone()
        })
        .collect();

    CalibrationScenario {
        name: "trust_source_ordering",
        description: "All signals identical except trust_source. \
            Human (1.0) > system (0.7) > agent (0.5) > neural (0.4) > auto (0.35).",
        entries,
        now: CANONICAL_NOW,
        expected_ordering: vec![0, 1, 2, 3, 4],
    }
}

/// Same base signals except freshness varies: now > 1day > 1week > 1month > 1year.
pub fn freshness_dominance() -> CalibrationScenario {
    let offsets: Vec<u64> = vec![
        60,                // 1 minute ago
        24 * 3600,         // 1 day ago
        7 * 24 * 3600,     // 1 week ago
        30 * 24 * 3600,    // 1 month ago
        365 * 24 * 3600,   // 1 year ago
    ];

    let labels: Vec<&'static str> = vec![
        "fresh-just-now",
        "fresh-1-day",
        "fresh-1-week",
        "fresh-1-month",
        "fresh-1-year",
    ];

    let entries: Vec<EntryProfile> = offsets
        .iter()
        .zip(labels.iter())
        .map(|(&offset, &label)| EntryProfile {
            label,
            status: Status::Active,
            access_count: 20,
            last_accessed_at: CANONICAL_NOW - offset,
            created_at: CANONICAL_NOW - 365 * 24 * 3600,
            helpful_count: 5,
            unhelpful_count: 1,
            correction_count: 1,
            trust_source: "human",
            category: "decision",
        })
        .collect();

    CalibrationScenario {
        name: "freshness_dominance",
        description: "All signals identical except last_accessed_at. \
            Just now > 1 day > 1 week > 1 month > 1 year.",
        entries,
        now: CANONICAL_NOW,
        expected_ordering: vec![0, 1, 2, 3, 4],
    }
}

/// Kendall tau rank correlation in [-1.0, 1.0]. O(n^2), n <= 20.
/// Panics if rankings contain different elements or duplicates.
pub fn kendall_tau(ranking_a: &[u64], ranking_b: &[u64]) -> f64 {
    let n = ranking_a.len();
    assert_eq!(
        n,
        ranking_b.len(),
        "kendall_tau: rankings must have same length (a={}, b={})",
        n,
        ranking_b.len()
    );

    if n <= 1 {
        return 1.0;
    }

    // Build position maps
    let pos_a: HashMap<u64, usize> = ranking_a.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    let pos_b: HashMap<u64, usize> = ranking_b.iter().enumerate().map(|(i, &v)| (v, i)).collect();

    // Verify same elements
    assert_eq!(
        pos_a.len(),
        n,
        "kendall_tau: ranking_a contains duplicate elements"
    );
    assert_eq!(
        pos_b.len(),
        n,
        "kendall_tau: ranking_b contains duplicate elements"
    );
    for &v in ranking_a {
        assert!(
            pos_b.contains_key(&v),
            "kendall_tau: element {v} in ranking_a but not in ranking_b"
        );
    }

    let mut concordant: i64 = 0;
    let mut discordant: i64 = 0;

    for i in 0..n {
        for j in (i + 1)..n {
            let a_i = ranking_a[i];
            let a_j = ranking_a[j];

            let b_pos_i = pos_b[&a_i];
            let b_pos_j = pos_b[&a_j];

            // In ranking_a, a_i comes before a_j (i < j).
            // Check if b agrees or disagrees.
            if b_pos_i < b_pos_j {
                concordant += 1;
            } else {
                discordant += 1;
            }
        }
    }

    let total_pairs = (n * (n - 1)) as f64 / 2.0;
    (concordant - discordant) as f64 / total_pairs
}

/// Assert `higher_id` appears before `lower_id` in score-descending results.
pub fn assert_ranked_above(results: &[(u64, f64)], higher_id: u64, lower_id: u64) {
    let higher_pos = results.iter().position(|(id, _)| *id == higher_id);
    let lower_pos = results.iter().position(|(id, _)| *id == lower_id);

    let higher_pos = higher_pos.unwrap_or_else(|| {
        panic!("assert_ranked_above: higher_id {higher_id} not found in results")
    });
    let lower_pos = lower_pos.unwrap_or_else(|| {
        panic!("assert_ranked_above: lower_id {lower_id} not found in results")
    });

    let higher_score = results[higher_pos].1;
    let lower_score = results[lower_pos].1;

    assert!(
        higher_pos < lower_pos,
        "assert_ranked_above: expected id {higher_id} (pos={higher_pos}, score={higher_score:.6}) \
         above id {lower_id} (pos={lower_pos}, score={lower_score:.6})"
    );
}

/// Assert that `entry_id` appears in the first `k` results.
pub fn assert_in_top_k(results: &[(u64, f64)], entry_id: u64, k: usize) {
    let pos = results.iter().position(|(id, _)| *id == entry_id);
    let pos = pos
        .unwrap_or_else(|| panic!("assert_in_top_k: entry_id {entry_id} not found in results"));

    assert!(
        pos < k,
        "assert_in_top_k: entry_id {entry_id} at position {pos}, expected in top {k}. \
         Top-{k} IDs: {:?}",
        results.iter().take(k).map(|(id, _)| id).collect::<Vec<_>>()
    );
}

/// Assert Kendall tau between two rankings is at least `min_tau`.
pub fn assert_tau_above(ranking_a: &[u64], ranking_b: &[u64], min_tau: f64) {
    let tau = kendall_tau(ranking_a, ranking_b);
    assert!(
        tau >= min_tau,
        "assert_tau_above: tau = {tau:.4}, expected >= {min_tau:.4}. \
         Rankings differ: a={ranking_a:?}, b={ranking_b:?}"
    );
}

/// Compute confidence for each entry at `now`, verify ordering matches `expected_order`.
///
/// `expected_order` contains entry IDs in expected confidence order (highest first).
pub fn assert_confidence_ordering(entries: &[EntryRecord], expected_order: &[u64], now: u64) {
    let mut scored: Vec<(u64, f64)> = entries
        .iter()
        .map(|e| (e.id, compute_confidence(e, now)))
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let actual_order: Vec<u64> = scored.iter().map(|(id, _)| *id).collect();

    assert_eq!(
        actual_order, expected_order,
        "assert_confidence_ordering: ordering mismatch.\n  expected: {expected_order:?}\n  actual:   {actual_order:?}\n  scores:   {:?}",
        scored
    );
}
