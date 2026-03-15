//! Feedback-to-label pipeline: types and conversion logic for continuous
//! self-retraining (crt-008).
//!
//! Transforms utilization events into typed training samples routed to
//! the appropriate neural model.

use crate::models::digest::SignalDigest;

/// Result of a feature lifecycle outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeResult {
    /// Feature completed without rework.
    Success,
    /// Feature required rework iterations.
    Rework,
}

/// Model-specific training target.
#[derive(Clone, Debug)]
pub enum TrainingTarget {
    /// One-hot: [convention, pattern, gap, dead, noise].
    Classification([f32; 5]),
    /// Target score in [0.0, 1.0].
    ConventionScore(f32),
}

/// Utilization event that produces training labels.
#[derive(Clone, Debug)]
pub enum FeedbackSignal {
    HelpfulVote {
        entry_id: u64,
        category: String,
        digest: SignalDigest,
    },
    UnhelpfulVote {
        entry_id: u64,
        category: String,
        digest: SignalDigest,
    },
    CategoryCorrection {
        entry_id: u64,
        old_category: String,
        new_category: String,
        digest: SignalDigest,
    },
    ContentCorrection {
        entry_id: u64,
        category: String,
        digest: SignalDigest,
    },
    Deprecation {
        entry_id: u64,
        category: String,
        digest: SignalDigest,
    },
    StaleEntry {
        entry_id: u64,
        category: String,
        digest: SignalDigest,
    },
    ConventionFollowed {
        entry_id: u64,
        digest: SignalDigest,
    },
    ConventionDeviated {
        entry_id: u64,
        digest: SignalDigest,
    },
    FeatureOutcome {
        feature_cycle: String,
        result: OutcomeResult,
        entry_ids: Vec<u64>,
        digests: Vec<SignalDigest>,
        categories: Vec<String>,
    },
}

/// A labeled training example for a neural model.
#[derive(Clone, Debug)]
pub struct TrainingSample {
    pub digest: SignalDigest,
    pub target: TrainingTarget,
    pub weight: f32,
    pub source: FeedbackSignal,
    pub entry_id: u64,
    pub timestamp: u64,
}

/// Stateless converter from `FeedbackSignal` to routed training samples.
pub struct LabelGenerator {
    weak_label_weight: f32,
}

impl LabelGenerator {
    /// Create a new label generator with the given weak label weight.
    pub fn new(weak_label_weight: f32) -> Self {
        Self { weak_label_weight }
    }

    /// Convert a feedback signal into zero or more `(model_name, sample)` pairs.
    pub fn generate(&self, signal: &FeedbackSignal) -> Vec<(String, TrainingSample)> {
        let now = now_millis();
        match signal {
            FeedbackSignal::HelpfulVote {
                entry_id,
                category,
                digest,
            } => {
                vec![sample(
                    "signal_classifier",
                    *digest,
                    TrainingTarget::Classification(one_hot_for_category(category)),
                    1.0,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::UnhelpfulVote {
                entry_id, digest, ..
            } => {
                vec![sample(
                    "signal_classifier",
                    *digest,
                    TrainingTarget::Classification(NOISE_ONE_HOT),
                    1.0,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::CategoryCorrection {
                entry_id,
                new_category,
                digest,
                ..
            } => {
                vec![sample(
                    "signal_classifier",
                    *digest,
                    TrainingTarget::Classification(one_hot_for_category(new_category)),
                    1.0,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::ContentCorrection {
                entry_id, digest, ..
            } => {
                vec![sample(
                    "signal_classifier",
                    *digest,
                    TrainingTarget::Classification(NOISE_ONE_HOT),
                    0.7,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::Deprecation {
                entry_id, digest, ..
            } => {
                vec![
                    sample(
                        "signal_classifier",
                        *digest,
                        TrainingTarget::Classification(NOISE_ONE_HOT),
                        1.0,
                        signal,
                        *entry_id,
                        now,
                    ),
                    sample(
                        "convention_scorer",
                        *digest,
                        TrainingTarget::ConventionScore(0.0),
                        1.0,
                        signal,
                        *entry_id,
                        now,
                    ),
                ]
            }
            FeedbackSignal::StaleEntry {
                entry_id, digest, ..
            } => {
                vec![sample(
                    "signal_classifier",
                    *digest,
                    TrainingTarget::Classification(DEAD_ONE_HOT),
                    self.weak_label_weight,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::ConventionFollowed { entry_id, digest } => {
                vec![sample(
                    "convention_scorer",
                    *digest,
                    TrainingTarget::ConventionScore(1.0),
                    1.0,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::ConventionDeviated { entry_id, digest } => {
                vec![sample(
                    "convention_scorer",
                    *digest,
                    TrainingTarget::ConventionScore(0.0),
                    1.0,
                    signal,
                    *entry_id,
                    now,
                )]
            }
            FeedbackSignal::FeatureOutcome {
                entry_ids,
                digests,
                categories,
                result,
                ..
            } => entry_ids
                .iter()
                .zip(digests.iter())
                .zip(categories.iter())
                .map(|((id, digest), cat)| {
                    let target = match result {
                        OutcomeResult::Success => {
                            TrainingTarget::Classification(one_hot_for_category(cat))
                        }
                        OutcomeResult::Rework => TrainingTarget::Classification(NOISE_ONE_HOT),
                    };
                    sample(
                        "signal_classifier",
                        *digest,
                        target,
                        self.weak_label_weight,
                        signal,
                        *id,
                        now,
                    )
                })
                .collect(),
        }
    }
}

// -- Helpers --

const NOISE_ONE_HOT: [f32; 5] = [0.0, 0.0, 0.0, 0.0, 1.0];
const DEAD_ONE_HOT: [f32; 5] = [0.0, 0.0, 0.0, 1.0, 0.0];

/// Map category string to one-hot [f32; 5].
fn one_hot_for_category(category: &str) -> [f32; 5] {
    match category.to_lowercase().as_str() {
        "convention" => [1.0, 0.0, 0.0, 0.0, 0.0],
        "pattern" => [0.0, 1.0, 0.0, 0.0, 0.0],
        "gap" => [0.0, 0.0, 1.0, 0.0, 0.0],
        "dead" => [0.0, 0.0, 0.0, 1.0, 0.0],
        _ => NOISE_ONE_HOT,
    }
}

fn sample(
    model_name: &str,
    digest: SignalDigest,
    target: TrainingTarget,
    weight: f32,
    source: &FeedbackSignal,
    entry_id: u64,
    timestamp: u64,
) -> (String, TrainingSample) {
    (
        model_name.to_string(),
        TrainingSample {
            digest,
            target,
            weight,
            source: source.clone(),
            entry_id,
            timestamp,
        },
    )
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_digest() -> SignalDigest {
        SignalDigest::from_fields(0.7, 3, 500, "convention", "knowledge-gap", 50, 2)
    }

    fn make_generator() -> LabelGenerator {
        LabelGenerator::new(0.3)
    }

    // T-FR01-01: TrainingSample type construction
    #[test]
    fn training_sample_construction_and_clone() {
        let sample = TrainingSample {
            digest: test_digest(),
            target: TrainingTarget::Classification([1.0, 0.0, 0.0, 0.0, 0.0]),
            weight: 1.0,
            source: FeedbackSignal::HelpfulVote {
                entry_id: 1,
                category: "convention".to_string(),
                digest: test_digest(),
            },
            entry_id: 1,
            timestamp: 12345,
        };
        let cloned = sample.clone();
        assert_eq!(cloned.entry_id, 1);
        assert_eq!(cloned.weight, 1.0);
    }

    // T-FR02-01: HelpfulVote generates positive classifier label
    #[test]
    fn helpful_vote_positive_label() {
        let signal = FeedbackSignal::HelpfulVote {
            entry_id: 1,
            category: "convention".to_string(),
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "signal_classifier");
        assert_eq!(labels[0].1.weight, 1.0);
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => {
                assert_eq!(*t, [1.0, 0.0, 0.0, 0.0, 0.0]);
            }
            _ => panic!("expected Classification"),
        }
    }

    // T-FR02-02: UnhelpfulVote generates noise classifier label
    #[test]
    fn unhelpful_vote_noise_label() {
        let signal = FeedbackSignal::UnhelpfulVote {
            entry_id: 2,
            category: "pattern".to_string(),
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "signal_classifier");
        assert_eq!(labels[0].1.weight, 1.0);
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => {
                assert_eq!(*t, NOISE_ONE_HOT);
            }
            _ => panic!("expected Classification"),
        }
    }

    // T-FR02-03: CategoryCorrection generates ground truth re-label
    #[test]
    fn category_correction_relabel() {
        let signal = FeedbackSignal::CategoryCorrection {
            entry_id: 3,
            old_category: "noise".to_string(),
            new_category: "convention".to_string(),
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "signal_classifier");
        assert_eq!(labels[0].1.weight, 1.0);
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => {
                assert_eq!(*t, [1.0, 0.0, 0.0, 0.0, 0.0]);
            }
            _ => panic!("expected Classification"),
        }
    }

    // T-FR02-04: Deprecation generates dual model labels
    #[test]
    fn deprecation_dual_labels() {
        let signal = FeedbackSignal::Deprecation {
            entry_id: 4,
            category: "convention".to_string(),
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].0, "signal_classifier");
        assert_eq!(labels[1].0, "convention_scorer");
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => assert_eq!(*t, NOISE_ONE_HOT),
            _ => panic!("expected Classification"),
        }
        match &labels[1].1.target {
            TrainingTarget::ConventionScore(s) => assert_eq!(*s, 0.0),
            _ => panic!("expected ConventionScore"),
        }
    }

    // T-FR02-05: FeatureOutcome success generates weak labels
    #[test]
    fn feature_outcome_success_weak_labels() {
        let d1 = SignalDigest::from_fields(0.6, 2, 400, "convention", "knowledge-gap", 30, 1);
        let d2 = SignalDigest::from_fields(0.8, 4, 600, "pattern", "implicit-convention", 60, 3);
        let signal = FeedbackSignal::FeatureOutcome {
            feature_cycle: "test-cycle".to_string(),
            result: OutcomeResult::Success,
            entry_ids: vec![10, 11],
            digests: vec![d1, d2],
            categories: vec!["convention".to_string(), "pattern".to_string()],
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 2);
        for label in &labels {
            assert_eq!(label.0, "signal_classifier");
            assert!((label.1.weight - 0.3).abs() < 1e-6);
        }
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => assert_eq!(*t, [1.0, 0.0, 0.0, 0.0, 0.0]),
            _ => panic!("expected Classification"),
        }
        match &labels[1].1.target {
            TrainingTarget::Classification(t) => assert_eq!(*t, [0.0, 1.0, 0.0, 0.0, 0.0]),
            _ => panic!("expected Classification"),
        }
    }

    // T-FR02-06: ConventionFollowed generates positive scorer label
    #[test]
    fn convention_followed_positive() {
        let signal = FeedbackSignal::ConventionFollowed {
            entry_id: 20,
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "convention_scorer");
        assert_eq!(labels[0].1.weight, 1.0);
        match &labels[0].1.target {
            TrainingTarget::ConventionScore(s) => assert_eq!(*s, 1.0),
            _ => panic!("expected ConventionScore"),
        }
    }

    // T-FR02-07: ConventionDeviated generates negative scorer label
    #[test]
    fn convention_deviated_negative() {
        let signal = FeedbackSignal::ConventionDeviated {
            entry_id: 21,
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "convention_scorer");
        assert_eq!(labels[0].1.weight, 1.0);
        match &labels[0].1.target {
            TrainingTarget::ConventionScore(s) => assert_eq!(*s, 0.0),
            _ => panic!("expected ConventionScore"),
        }
    }

    // T-FR02-08: StaleEntry generates weak dead label
    #[test]
    fn stale_entry_weak_dead() {
        let signal = FeedbackSignal::StaleEntry {
            entry_id: 30,
            category: "convention".to_string(),
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "signal_classifier");
        assert!((labels[0].1.weight - 0.3).abs() < 1e-6);
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => assert_eq!(*t, DEAD_ONE_HOT),
            _ => panic!("expected Classification"),
        }
    }

    // T-FR02-09: ContentCorrection generates noise label with 0.7 weight
    #[test]
    fn content_correction_noise() {
        let signal = FeedbackSignal::ContentCorrection {
            entry_id: 40,
            category: "convention".to_string(),
            digest: test_digest(),
        };
        let labels = make_generator().generate(&signal);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, "signal_classifier");
        assert!((labels[0].1.weight - 0.7).abs() < 1e-6);
        match &labels[0].1.target {
            TrainingTarget::Classification(t) => assert_eq!(*t, NOISE_ONE_HOT),
            _ => panic!("expected Classification"),
        }
    }
}
