//! Feedback capture utilities for server-side integration (crt-008, Wave 4).
//!
//! Provides trust_source filtering and signal construction helpers that
//! server handlers call after their primary operations complete.

use crate::models::digest::SignalDigest;
use crate::training::{FeedbackSignal, OutcomeResult};

/// Trust sources that generate training signals.
const TRAINABLE_SOURCES: &[&str] = &["auto", "neural"];

/// Check if an entry's trust_source qualifies for training signal generation.
pub fn is_trainable_source(trust_source: &str) -> bool {
    TRAINABLE_SOURCES.contains(&trust_source)
}

/// Build a helpful/unhelpful vote feedback signal.
///
/// Returns `None` if the trust_source is not trainable.
pub fn vote_signal(
    entry_id: u64,
    category: &str,
    trust_source: &str,
    is_helpful: bool,
    digest: SignalDigest,
) -> Option<FeedbackSignal> {
    if !is_trainable_source(trust_source) {
        return None;
    }
    Some(if is_helpful {
        FeedbackSignal::HelpfulVote {
            entry_id,
            category: category.to_string(),
            digest,
        }
    } else {
        FeedbackSignal::UnhelpfulVote {
            entry_id,
            category: category.to_string(),
            digest,
        }
    })
}

/// Build a category correction feedback signal.
///
/// Returns `None` if the trust_source is not trainable.
pub fn correction_signal(
    entry_id: u64,
    old_category: &str,
    new_category: &str,
    trust_source: &str,
    digest: SignalDigest,
) -> Option<FeedbackSignal> {
    if !is_trainable_source(trust_source) {
        return None;
    }
    Some(FeedbackSignal::CategoryCorrection {
        entry_id,
        old_category: old_category.to_string(),
        new_category: new_category.to_string(),
        digest,
    })
}

/// Build a deprecation feedback signal.
///
/// Returns `None` if the trust_source is not trainable.
pub fn deprecation_signal(
    entry_id: u64,
    category: &str,
    trust_source: &str,
    digest: SignalDigest,
) -> Option<FeedbackSignal> {
    if !is_trainable_source(trust_source) {
        return None;
    }
    Some(FeedbackSignal::Deprecation {
        entry_id,
        category: category.to_string(),
        digest,
    })
}

/// Build a feature outcome feedback signal from a set of entries.
///
/// Filters entries to only those with trainable trust_sources.
/// Returns `None` if no entries qualify.
pub fn outcome_signal(
    feature_cycle: &str,
    result: OutcomeResult,
    entries: &[(u64, String, String, SignalDigest)], // (id, category, trust_source, digest)
) -> Option<FeedbackSignal> {
    let trainable: Vec<_> = entries
        .iter()
        .filter(|(_, _, ts, _)| is_trainable_source(ts))
        .collect();

    if trainable.is_empty() {
        return None;
    }

    let entry_ids: Vec<u64> = trainable.iter().map(|(id, _, _, _)| *id).collect();
    let digests: Vec<SignalDigest> = trainable.iter().map(|(_, _, _, d)| *d).collect();
    let categories: Vec<String> = trainable.iter().map(|(_, cat, _, _)| cat.clone()).collect();

    Some(FeedbackSignal::FeatureOutcome {
        feature_cycle: feature_cycle.to_string(),
        result,
        entry_ids,
        digests,
        categories,
    })
}

/// Build a stale entry feedback signal.
pub fn stale_signal(
    entry_id: u64,
    category: &str,
    trust_source: &str,
    digest: SignalDigest,
) -> Option<FeedbackSignal> {
    if !is_trainable_source(trust_source) {
        return None;
    }
    Some(FeedbackSignal::StaleEntry {
        entry_id,
        category: category.to_string(),
        digest,
    })
}

/// Reconstruct a SignalDigest from entry metadata.
///
/// Used when the original digest is not available from shadow_evaluations.
pub fn reconstruct_digest(
    confidence: f64,
    helpful_count: u32,
    content_length: usize,
    category: &str,
    topic: &str,
    tag_count: usize,
) -> SignalDigest {
    SignalDigest::from_fields(
        confidence,
        helpful_count as usize,
        content_length,
        category,
        "", // source_rule not available from entry metadata
        topic.len(),
        tag_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_digest() -> SignalDigest {
        SignalDigest::from_fields(0.7, 3, 500, "convention", "knowledge-gap", 50, 2)
    }

    // T-R04-01: Helpful vote on agent entry does NOT generate signal
    #[test]
    fn agent_entry_no_signal() {
        let result = vote_signal(1, "convention", "agent", true, test_digest());
        assert!(result.is_none(), "agent trust_source should not generate signal");
    }

    // T-R04-02: Helpful vote on auto entry DOES generate signal
    #[test]
    fn auto_entry_generates_signal() {
        let result = vote_signal(1, "convention", "auto", true, test_digest());
        assert!(result.is_some(), "auto trust_source should generate signal");
        match result.unwrap() {
            FeedbackSignal::HelpfulVote { entry_id, .. } => assert_eq!(entry_id, 1),
            _ => panic!("expected HelpfulVote"),
        }
    }

    // T-R04-03: Deprecation on neural entry DOES generate signal
    #[test]
    fn neural_entry_deprecation_signal() {
        let result = deprecation_signal(2, "convention", "neural", test_digest());
        assert!(result.is_some(), "neural trust_source should generate signal");
        match result.unwrap() {
            FeedbackSignal::Deprecation { entry_id, .. } => assert_eq!(entry_id, 2),
            _ => panic!("expected Deprecation"),
        }
    }

    #[test]
    fn is_trainable_source_values() {
        assert!(is_trainable_source("auto"));
        assert!(is_trainable_source("neural"));
        assert!(!is_trainable_source("agent"));
        assert!(!is_trainable_source("human"));
        assert!(!is_trainable_source("system"));
        assert!(!is_trainable_source(""));
    }

    #[test]
    fn correction_signal_filters_trust() {
        assert!(correction_signal(1, "noise", "convention", "auto", test_digest()).is_some());
        assert!(correction_signal(1, "noise", "convention", "agent", test_digest()).is_none());
    }

    #[test]
    fn outcome_signal_filters_entries() {
        let entries = vec![
            (1, "convention".to_string(), "auto".to_string(), test_digest()),
            (2, "pattern".to_string(), "agent".to_string(), test_digest()),
            (3, "gap".to_string(), "neural".to_string(), test_digest()),
        ];
        let result = outcome_signal("test-cycle", OutcomeResult::Success, &entries);
        assert!(result.is_some());
        match result.unwrap() {
            FeedbackSignal::FeatureOutcome { entry_ids, .. } => {
                assert_eq!(entry_ids, vec![1, 3]); // agent entry filtered out
            }
            _ => panic!("expected FeatureOutcome"),
        }
    }

    #[test]
    fn outcome_signal_empty_when_no_trainable() {
        let entries = vec![
            (1, "convention".to_string(), "agent".to_string(), test_digest()),
        ];
        assert!(outcome_signal("test-cycle", OutcomeResult::Success, &entries).is_none());
    }
}
