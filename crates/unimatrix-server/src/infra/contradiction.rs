//! Contradiction detection and embedding consistency checks.
//!
//! Scans active entries for semantic conflicts using HNSW nearest-neighbor
//! search combined with a multi-signal conflict heuristic (ADR-003).
//! Also provides embedding consistency verification for relevance hijacking defense.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use unimatrix_core::Store;
use unimatrix_core::{EmbedService, VectorStore};
use unimatrix_store::{EntryRecord, Status};

use crate::error::ServerError;

/// HNSW search expansion factor (matches tools.rs EF_SEARCH).
const EF_SEARCH: usize = 32;

/// Minimum cosine similarity to consider two entries as semantic neighbors.
const SIMILARITY_THRESHOLD: f32 = 0.85;

/// Default conflict sensitivity (0.0 = most lenient, 1.0 = most sensitive).
const DEFAULT_CONFLICT_SENSITIVITY: f32 = 0.5;

/// Number of nearest neighbors to check per entry.
const NEIGHBORS_PER_ENTRY: usize = 10;

/// Minimum self-match similarity for embedding consistency.
const EMBEDDING_CONSISTENCY_THRESHOLD: f32 = 0.99;

/// Weight for negation opposition signal in conflict heuristic.
const NEGATION_WEIGHT: f32 = 0.6;

/// Weight for incompatible directives signal in conflict heuristic.
const DIRECTIVE_WEIGHT: f32 = 0.3;

/// Weight for opposing sentiment signal in conflict heuristic.
const SENTIMENT_WEIGHT: f32 = 0.1;

/// A pair of entries flagged as potentially contradictory.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContradictionPair {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub similarity: f32,
    pub conflict_score: f32,
    pub explanation: String,
}

/// An entry whose stored embedding does not match its current content.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingInconsistency {
    pub entry_id: u64,
    pub title: String,
    pub expected_similarity: f32,
}

/// Configuration for contradiction scanning and embedding checks.
#[derive(Debug, Clone)]
pub struct ContradictionConfig {
    pub similarity_threshold: f32,
    pub conflict_sensitivity: f32,
    pub neighbors_per_entry: usize,
    pub embedding_consistency_threshold: f32,
}

impl Default for ContradictionConfig {
    fn default() -> Self {
        ContradictionConfig {
            similarity_threshold: SIMILARITY_THRESHOLD,
            conflict_sensitivity: DEFAULT_CONFLICT_SENSITIVITY,
            neighbors_per_entry: NEIGHBORS_PER_ENTRY,
            embedding_consistency_threshold: EMBEDDING_CONSISTENCY_THRESHOLD,
        }
    }
}

/// Check a single piece of content for contradictions against existing entries.
///
/// Embeds the given title+content, searches HNSW for neighbors, and applies
/// the conflict heuristic. Returns the highest-scoring contradiction pair,
/// or None if no contradiction detected (col-013 ADR-006).
pub fn check_entry_contradiction(
    content: &str,
    title: &str,
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Option<ContradictionPair>, ServerError> {
    let embedding = embed_adapter
        .embed_entry(title, content)
        .map_err(ServerError::Core)?;

    let neighbors = vector_store
        .search(&embedding, config.neighbors_per_entry, EF_SEARCH)
        .map_err(ServerError::Core)?;

    let mut best: Option<ContradictionPair> = None;

    for neighbor in &neighbors {
        if (neighbor.similarity as f32) < config.similarity_threshold {
            continue;
        }

        let neighbor_entry =
            match tokio::runtime::Handle::current().block_on(store.get(neighbor.entry_id)) {
                Ok(e) => e,
                Err(_) => continue,
            };

        if neighbor_entry.status != Status::Active {
            continue;
        }

        let (conflict_score, explanation) = conflict_heuristic(
            content,
            &neighbor_entry.content,
            config.conflict_sensitivity,
        );

        if conflict_score > 0.0 {
            let pair = ContradictionPair {
                entry_id_a: 0, // proposed entry has no ID yet
                entry_id_b: neighbor_entry.id,
                title_a: title.to_string(),
                title_b: neighbor_entry.title.clone(),
                similarity: neighbor.similarity as f32,
                conflict_score,
                explanation,
            };
            if best
                .as_ref()
                .map_or(true, |b| conflict_score > b.conflict_score)
            {
                best = Some(pair);
            }
        }
    }

    Ok(best)
}

/// Scan all active entries for potential contradictions.
///
/// For each active entry, re-embeds its content (ADR-002), searches for
/// nearest neighbors in HNSW, and applies the multi-signal conflict
/// heuristic (ADR-003) to each high-similarity pair. Results are sorted
/// by conflict score descending.
pub fn scan_contradictions(
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Vec<ContradictionPair>, ServerError> {
    let active_entries = read_active_entries(store)?;

    let mut seen_pairs: HashSet<(u64, u64)> = HashSet::new();
    let mut results: Vec<ContradictionPair> = Vec::new();

    for entry in &active_entries {
        // Re-embed from title + content (ADR-002: re-embed from text)
        let embedding = match embed_adapter.embed_entry(&entry.title, &entry.content) {
            Ok(v) => v,
            Err(_) => continue, // graceful degradation
        };

        // Search HNSW for neighbors
        let neighbors = match vector_store.search(&embedding, config.neighbors_per_entry, EF_SEARCH)
        {
            Ok(n) => n,
            Err(_) => continue,
        };

        for neighbor in &neighbors {
            // Skip self-match
            if neighbor.entry_id == entry.id {
                continue;
            }

            // Skip below similarity threshold (cast f64 similarity to f32 for contradiction-local comparison)
            if (neighbor.similarity as f32) < config.similarity_threshold {
                continue;
            }

            // Canonical pair key for dedup
            let pair_key = (
                entry.id.min(neighbor.entry_id),
                entry.id.max(neighbor.entry_id),
            );
            if seen_pairs.contains(&pair_key) {
                continue;
            }
            seen_pairs.insert(pair_key);

            // Fetch neighbor entry
            let neighbor_entry =
                match tokio::runtime::Handle::current().block_on(store.get(neighbor.entry_id)) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

            // Skip non-active neighbors
            if neighbor_entry.status != Status::Active {
                continue;
            }

            // Run conflict heuristic
            let (conflict_score, explanation) = conflict_heuristic(
                &entry.content,
                &neighbor_entry.content,
                config.conflict_sensitivity,
            );

            if conflict_score > 0.0 {
                let (title_a, title_b) = if entry.id == pair_key.0 {
                    (entry.title.clone(), neighbor_entry.title.clone())
                } else {
                    (neighbor_entry.title.clone(), entry.title.clone())
                };

                results.push(ContradictionPair {
                    entry_id_a: pair_key.0,
                    entry_id_b: pair_key.1,
                    title_a,
                    title_b,
                    similarity: neighbor.similarity as f32,
                    conflict_score,
                    explanation,
                });
            }
        }
    }

    // Sort by conflict_score descending
    results.sort_by(|a, b| {
        b.conflict_score
            .partial_cmp(&a.conflict_score)
            .unwrap_or(Ordering::Equal)
    });

    Ok(results)
}

/// Read all active entries from the store.
///
/// Bridges async sqlx queries to sync context via `block_on` (nxs-011).
/// Called from `spawn_blocking` closures where the tokio handle is available.
fn read_active_entries(store: &Store) -> Result<Vec<EntryRecord>, ServerError> {
    tokio::runtime::Handle::current()
        .block_on(store.query_by_status(Status::Active))
        .map_err(|e| ServerError::Core(unimatrix_core::CoreError::Store(e)))
}

/// Check embedding consistency for all active entries.
///
/// Re-embeds each entry's content and verifies that the entry appears as
/// its own top-1 nearest neighbor with similarity above the threshold.
/// Entries that fail this check may have been subject to relevance hijacking.
pub fn check_embedding_consistency(
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Vec<EmbeddingInconsistency>, ServerError> {
    let active_entries = read_active_entries(store)?;

    let mut inconsistencies = Vec::new();

    for entry in &active_entries {
        // Re-embed from title + content
        let embedding = match embed_adapter.embed_entry(&entry.title, &entry.content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Search for top-1 (self-match expected)
        let results = match vector_store.search(&embedding, 1, EF_SEARCH) {
            Ok(r) => r,
            Err(_) => {
                // No match at all -- flag as inconsistent
                inconsistencies.push(EmbeddingInconsistency {
                    entry_id: entry.id,
                    title: entry.title.clone(),
                    expected_similarity: 0.0,
                });
                continue;
            }
        };

        if results.is_empty() {
            inconsistencies.push(EmbeddingInconsistency {
                entry_id: entry.id,
                title: entry.title.clone(),
                expected_similarity: 0.0,
            });
            continue;
        }

        let top_result = &results[0];

        if top_result.entry_id != entry.id {
            // Another entry is more similar than self -- suspicious
            inconsistencies.push(EmbeddingInconsistency {
                entry_id: entry.id,
                title: entry.title.clone(),
                expected_similarity: top_result.similarity as f32,
            });
        } else if (top_result.similarity as f32) < config.embedding_consistency_threshold {
            // Self-match but similarity too low
            inconsistencies.push(EmbeddingInconsistency {
                entry_id: entry.id,
                title: entry.title.clone(),
                expected_similarity: top_result.similarity as f32,
            });
        }
    }

    Ok(inconsistencies)
}

/// Multi-signal conflict heuristic (ADR-003).
///
/// Combines three weighted signals: negation opposition (0.6), incompatible
/// directives (0.3), and opposing sentiment (0.1). Returns `(score, explanation)`
/// where score is 0.0 if below the sensitivity threshold.
pub fn conflict_heuristic(content_a: &str, content_b: &str, sensitivity: f32) -> (f32, String) {
    let mut signals: Vec<(&str, f32)> = Vec::new();
    let mut explanations: Vec<String> = Vec::new();

    // Signal 1: Negation opposition (weight: 0.6)
    let neg_score = check_negation_opposition(content_a, content_b);
    if neg_score > 0.0 {
        let weighted = neg_score * NEGATION_WEIGHT;
        signals.push(("negation", weighted));
        explanations.push(format!("negation opposition ({neg_score:.2})"));
    }

    // Signal 2: Incompatible directives (weight: 0.3)
    let dir_score = check_incompatible_directives(content_a, content_b);
    if dir_score > 0.0 {
        let weighted = dir_score * DIRECTIVE_WEIGHT;
        signals.push(("directive", weighted));
        explanations.push(format!("incompatible directives ({dir_score:.2})"));
    }

    // Signal 3: Opposing sentiment (weight: 0.1)
    let sent_score = check_opposing_sentiment(content_a, content_b);
    if sent_score > 0.0 {
        let weighted = sent_score * SENTIMENT_WEIGHT;
        signals.push(("sentiment", weighted));
        explanations.push(format!("opposing sentiment ({sent_score:.2})"));
    }

    // Composite score
    let total: f32 = signals.iter().map(|(_, w)| *w).sum();
    let total = total.clamp(0.0, 1.0);

    // Apply sensitivity threshold: flag if score >= (1.0 - sensitivity)
    let threshold = 1.0 - sensitivity;
    if total < threshold {
        return (0.0, String::new());
    }

    let explanation = explanations.join("; ");
    (total, explanation)
}

/// Global directive regex singleton.
static DIRECTIVE_REGEX: OnceLock<Regex> = OnceLock::new();

/// Get or compile the directive extraction regex.
fn directive_regex() -> &'static Regex {
    DIRECTIVE_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(use|always|prefer|should|must|enable|avoid|never|do\s+not|don't|should\s+not|must\s+not|disable)\s+(\w[\w\s\-]*)"
        ).expect("directive regex must compile")
    })
}

/// Check for negation opposition between two content strings.
///
/// Extracts directive phrases from each and checks for pairs where one
/// is affirmative and the other negative, with matching subjects.
fn check_negation_opposition(content_a: &str, content_b: &str) -> f32 {
    let directives_a = extract_directives(content_a);
    let directives_b = extract_directives(content_b);

    let mut max_score: f32 = 0.0;

    for (verb_a, subject_a) in &directives_a {
        for (verb_b, subject_b) in &directives_b {
            let a_affirm = is_affirmative(verb_a);
            let b_affirm = is_affirmative(verb_b);

            if a_affirm == b_affirm {
                continue; // same polarity, no opposition
            }

            let subject_match = compare_subjects(subject_a, subject_b);
            if subject_match > 0.0 {
                max_score = max_score.max(subject_match);
            }
        }
    }

    max_score
}

/// Extract directive phrases (verb, subject) from content.
///
/// Matches patterns like "use X", "avoid Y", "should not Z".
fn extract_directives(content: &str) -> Vec<(String, String)> {
    let re = directive_regex();
    let mut directives = Vec::new();

    for cap in re.captures_iter(content) {
        let verb = cap[1].to_lowercase();
        let subject = cap[2].trim().to_lowercase();
        let subject = first_n_words(&subject, 4);
        directives.push((verb, subject));
    }

    directives
}

/// Returns true if the directive verb is affirmative, false if negative.
fn is_affirmative(verb: &str) -> bool {
    match verb {
        "use" | "always" | "prefer" | "should" | "must" | "enable" => true,
        "avoid" | "never" | "do not" | "don't" | "should not" | "must not" | "disable" => false,
        _ => true, // default to affirmative
    }
}

/// Compare two subject phrases for similarity.
///
/// Returns 1.0 for exact match, 0.5 for substring match, 0.0 for no match.
fn compare_subjects(subject_a: &str, subject_b: &str) -> f32 {
    if subject_a == subject_b {
        return 1.0;
    }
    if subject_a.contains(subject_b) || subject_b.contains(subject_a) {
        return 0.5;
    }
    0.0
}

/// Check for incompatible directives between two content strings.
///
/// Detects cases where both entries have affirmative directives with
/// different subjects (e.g., "use X" in A and "use Y" in B).
fn check_incompatible_directives(content_a: &str, content_b: &str) -> f32 {
    let directives_a = extract_directives(content_a);
    let directives_b = extract_directives(content_b);

    let affirm_a: Vec<&str> = directives_a
        .iter()
        .filter(|(v, _)| is_affirmative(v))
        .map(|(_, s)| s.as_str())
        .collect();

    let affirm_b: Vec<&str> = directives_b
        .iter()
        .filter(|(v, _)| is_affirmative(v))
        .map(|(_, s)| s.as_str())
        .collect();

    for sub_a in &affirm_a {
        for sub_b in &affirm_b {
            if sub_a != sub_b && !sub_a.contains(sub_b) && !sub_b.contains(sub_a) {
                return 1.0;
            }
        }
    }

    0.0
}

/// Check for opposing sentiment between two content strings.
///
/// Detects cases where one entry uses positive markers and the other
/// uses negative markers.
fn check_opposing_sentiment(content_a: &str, content_b: &str) -> f32 {
    const POSITIVE_MARKERS: &[&str] = &[
        "recommended",
        "best practice",
        "preferred",
        "ideal",
        "excellent",
    ];
    const NEGATIVE_MARKERS: &[&str] = &[
        "anti-pattern",
        "discouraged",
        "problematic",
        "risky",
        "avoid",
        "bad practice",
    ];

    let a_lower = content_a.to_lowercase();
    let b_lower = content_b.to_lowercase();

    let a_positive = POSITIVE_MARKERS.iter().any(|m| a_lower.contains(m));
    let a_negative = NEGATIVE_MARKERS.iter().any(|m| a_lower.contains(m));
    let b_positive = POSITIVE_MARKERS.iter().any(|m| b_lower.contains(m));
    let b_negative = NEGATIVE_MARKERS.iter().any(|m| b_lower.contains(m));

    if (a_positive && b_negative) || (a_negative && b_positive) {
        return 1.0;
    }

    0.0
}

/// Extract the first N words from a string.
fn first_n_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contradiction_config_default() {
        let config = ContradictionConfig::default();
        assert!((config.similarity_threshold - 0.85).abs() < f32::EPSILON);
        assert!((config.conflict_sensitivity - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.neighbors_per_entry, 10);
        assert!((config.embedding_consistency_threshold - 0.99).abs() < f32::EPSILON);
    }

    #[test]
    fn test_first_n_words_normal() {
        assert_eq!(
            first_n_words("hello world foo bar baz", 4),
            "hello world foo bar"
        );
    }

    #[test]
    fn test_first_n_words_fewer_than_n() {
        assert_eq!(first_n_words("two words", 4), "two words");
    }

    #[test]
    fn test_first_n_words_empty() {
        assert_eq!(first_n_words("", 4), "");
    }

    #[test]
    fn test_is_affirmative_positive_verbs() {
        assert!(is_affirmative("use"));
        assert!(is_affirmative("always"));
        assert!(is_affirmative("prefer"));
        assert!(is_affirmative("should"));
        assert!(is_affirmative("must"));
        assert!(is_affirmative("enable"));
    }

    #[test]
    fn test_is_affirmative_negative_verbs() {
        assert!(!is_affirmative("avoid"));
        assert!(!is_affirmative("never"));
        assert!(!is_affirmative("do not"));
        assert!(!is_affirmative("don't"));
        assert!(!is_affirmative("should not"));
        assert!(!is_affirmative("must not"));
        assert!(!is_affirmative("disable"));
    }

    #[test]
    fn test_is_affirmative_unknown_defaults_true() {
        assert!(is_affirmative("consider"));
    }

    #[test]
    fn test_compare_subjects_exact_match() {
        assert!((compare_subjects("bincode v2", "bincode v2") - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compare_subjects_substring_match() {
        assert!((compare_subjects("bincode v2 serde", "bincode v2") - 0.5).abs() < f32::EPSILON);
        assert!((compare_subjects("bincode", "bincode v2") - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compare_subjects_no_match() {
        assert!((compare_subjects("bincode", "serde")).abs() < f32::EPSILON);
    }

    #[test]
    fn test_extract_directives_basic() {
        let content = "Always use bincode v2 for serialization. Never use JSON for storage.";
        let directives = extract_directives(content);
        assert!(directives.len() >= 2);

        // Check that we found "always" + subject and "never" + subject
        let has_always = directives.iter().any(|(v, _)| v == "always");
        let has_never = directives.iter().any(|(v, _)| v == "never");
        assert!(has_always, "should find 'always' directive");
        assert!(has_never, "should find 'never' directive");
    }

    #[test]
    fn test_extract_directives_empty_content() {
        let directives = extract_directives("No directives here at all.");
        assert!(directives.is_empty());
    }

    #[test]
    fn test_check_negation_opposition_opposing() {
        let a = "Use bincode for serialization.";
        let b = "Avoid bincode for serialization.";
        let score = check_negation_opposition(a, b);
        assert!(
            score > 0.0,
            "should detect negation opposition, got {score}"
        );
    }

    #[test]
    fn test_check_negation_opposition_same_polarity() {
        let a = "Use bincode for serialization.";
        let b = "Must use bincode for storage.";
        let score = check_negation_opposition(a, b);
        assert!(score == 0.0, "same polarity should not flag, got {score}");
    }

    #[test]
    fn test_check_incompatible_directives_different_subjects() {
        let a = "Use bincode for serialization.";
        let b = "Use JSON for serialization.";
        let score = check_incompatible_directives(a, b);
        assert!(
            (score - 1.0).abs() < f32::EPSILON,
            "different affirmative subjects should flag, got {score}"
        );
    }

    #[test]
    fn test_check_incompatible_directives_same_subject() {
        let a = "Use bincode for serialization.";
        let b = "Prefer bincode for serialization.";
        let score = check_incompatible_directives(a, b);
        // Both have "bincode for serialization" as subject -- exact match
        assert!(score == 0.0, "same subject should not flag, got {score}");
    }

    #[test]
    fn test_check_opposing_sentiment_opposite() {
        let a = "This approach is recommended and a best practice.";
        let b = "This approach is an anti-pattern and problematic.";
        let score = check_opposing_sentiment(a, b);
        assert!(
            (score - 1.0).abs() < f32::EPSILON,
            "opposing sentiment should score 1.0, got {score}"
        );
    }

    #[test]
    fn test_check_opposing_sentiment_same_positive() {
        let a = "This approach is recommended.";
        let b = "This approach is preferred and ideal.";
        let score = check_opposing_sentiment(a, b);
        assert!(score == 0.0, "same sentiment should score 0.0, got {score}");
    }

    #[test]
    fn test_check_opposing_sentiment_neutral() {
        let a = "Set timeout to 30 seconds.";
        let b = "Configure retries to 3 attempts.";
        let score = check_opposing_sentiment(a, b);
        assert!(
            score == 0.0,
            "neutral content should score 0.0, got {score}"
        );
    }

    #[test]
    fn test_conflict_heuristic_no_conflict() {
        let a = "Set timeout to 30 seconds.";
        let b = "Configure logging level to debug.";
        let (score, explanation) = conflict_heuristic(a, b, 0.5);
        assert!(score == 0.0, "no conflict should score 0.0, got {score}");
        assert!(explanation.is_empty());
    }

    #[test]
    fn test_conflict_heuristic_strong_conflict() {
        let a = "Always use bincode for serialization. This is recommended and best practice.";
        let b = "Never use bincode for serialization. This is an anti-pattern and problematic.";
        let (score, explanation) = conflict_heuristic(a, b, 0.5);
        assert!(
            score > 0.0,
            "strong conflict should have positive score, got {score}"
        );
        assert!(!explanation.is_empty(), "should have explanation");
    }

    #[test]
    fn test_conflict_heuristic_below_sensitivity() {
        // With sensitivity 0.0, threshold = 1.0 - 0.0 = 1.0
        // Only a perfect score would pass
        let a = "Use JSON for config files.";
        let b = "Use YAML for config files.";
        let (score, _) = conflict_heuristic(a, b, 0.0);
        assert!(
            score == 0.0,
            "with sensitivity 0.0, low scores should be filtered, got {score}"
        );
    }

    #[test]
    fn test_conflict_heuristic_high_sensitivity() {
        // With sensitivity 1.0, threshold = 0.0
        // Any non-zero score passes
        let a = "Use JSON for config files.";
        let b = "Use YAML for config files.";
        let (score, _) = conflict_heuristic(a, b, 1.0);
        assert!(
            score > 0.0,
            "with sensitivity 1.0, incompatible directives should flag, got {score}"
        );
    }

    #[test]
    fn test_directive_regex_compiles() {
        let re = directive_regex();
        assert!(re.is_match("Always use the standard library"));
        assert!(re.is_match("avoid global state"));
        assert!(re.is_match("Do not use unsafe code"));
    }

    #[test]
    fn test_dedup_canonical_pair_order() {
        let pair_key_ab = (5u64.min(10), 5u64.max(10));
        let pair_key_ba = (10u64.min(5), 10u64.max(5));
        assert_eq!(pair_key_ab, pair_key_ba);
        assert_eq!(pair_key_ab, (5, 10));
    }

    #[test]
    fn test_sensitivity_high_flags_more() {
        // Weak conflict: only opposing sentiment
        let a = "This approach is recommended and considered best practice.";
        let b = "This approach is problematic and discouraged.";

        // At default sensitivity (0.5): threshold = 0.5, sentiment alone (0.1) may not pass
        let (score_default, _) = conflict_heuristic(a, b, 0.5);

        // At high sensitivity (0.95): threshold = 0.05, sentiment signal (0.1) should pass
        let (score_sensitive, _) = conflict_heuristic(a, b, 0.95);

        assert!(
            score_sensitive >= score_default,
            "higher sensitivity should flag more: default={score_default}, sensitive={score_sensitive}"
        );
        assert!(
            score_sensitive > 0.0,
            "high sensitivity should flag opposing sentiment: {score_sensitive}"
        );
    }

    #[test]
    fn test_no_conflict_complementary_entries() {
        let a = "Use tokio for async runtime management.";
        let b = "Use tokio with multi-threaded runtime for best performance.";

        // Same subject, same polarity -- not a contradiction
        let (score, _) = conflict_heuristic(a, b, 0.5);
        assert_eq!(
            score, 0.0,
            "complementary entries should not conflict, got {score}"
        );
    }

    #[test]
    fn test_no_conflict_agreement() {
        let a = "Use serde for serialization.";
        let b = "Serde is a recommended choice for serialization.";

        let (score, _) = conflict_heuristic(a, b, 0.5);
        assert_eq!(score, 0.0, "agreement should not conflict, got {score}");
    }

    #[test]
    fn test_negation_always_vs_never() {
        let a = "Always enable strict mode.";
        let b = "Never enable strict mode.";

        let score = check_negation_opposition(a, b);
        assert!(
            score > 0.0,
            "always vs never should detect opposition, got {score}"
        );
    }

    #[test]
    fn test_incompatible_directives_reqwest_vs_ureq() {
        let a = "Use reqwest for HTTP clients.";
        let b = "Use ureq for HTTP clients.";

        let score = check_incompatible_directives(a, b);
        assert!(
            score > 0.0,
            "different HTTP clients should be incompatible, got {score}"
        );
    }

    #[test]
    fn test_contradiction_pair_clone() {
        let pair = ContradictionPair {
            entry_id_a: 1,
            entry_id_b: 2,
            title_a: "A".to_string(),
            title_b: "B".to_string(),
            similarity: 0.9,
            conflict_score: 0.5,
            explanation: "test".to_string(),
        };
        let cloned = pair.clone();
        assert_eq!(cloned.entry_id_a, 1);
        assert_eq!(cloned.conflict_score, 0.5);
    }

    #[test]
    fn test_embedding_inconsistency_clone() {
        let inc = EmbeddingInconsistency {
            entry_id: 42,
            title: "Test".to_string(),
            expected_similarity: 0.95,
        };
        let cloned = inc.clone();
        assert_eq!(cloned.entry_id, 42);
    }

    #[test]
    fn test_contradiction_config_clone() {
        let config = ContradictionConfig::default();
        let cloned = config.clone();
        assert!((cloned.similarity_threshold - config.similarity_threshold).abs() < f32::EPSILON);
    }
}
