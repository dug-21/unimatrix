//! Neural enhancement wrapper for extraction pipeline.
//!
//! Wraps Signal Classifier + Convention Scorer, produces NeuralPrediction.

use unimatrix_learn::models::{
    ClassificationResult, ConventionScorer, SignalClassifier, SignalDigest,
};

use super::ProposedEntry;

/// Combined neural prediction for a single entry.
#[derive(Debug, Clone)]
pub struct NeuralPrediction {
    pub classification: ClassificationResult,
    pub convention_score: f32,
    pub digest: SignalDigest,
}

/// Operating mode for neural enhancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnhancerMode {
    /// Log predictions, pass entries unchanged.
    Shadow,
    /// Apply neural decisions (suppress noise, override confidence).
    Active,
}

/// Wraps classifier + scorer, produces NeuralPrediction.
pub struct NeuralEnhancer {
    classifier: SignalClassifier,
    scorer: ConventionScorer,
    mode: EnhancerMode,
}

impl NeuralEnhancer {
    /// Create a new enhancer with the given models and mode.
    pub fn new(classifier: SignalClassifier, scorer: ConventionScorer, mode: EnhancerMode) -> Self {
        Self {
            classifier,
            scorer,
            mode,
        }
    }

    /// Build digest and run both models on a proposed entry.
    pub fn enhance(&self, entry: &ProposedEntry) -> NeuralPrediction {
        let digest = SignalDigest::from_fields(
            entry.extraction_confidence,
            entry.source_features.len(),
            entry.content.len(),
            &entry.category,
            &entry.source_rule,
            entry.title.len(),
            entry.tags.len(),
        );

        let classification = self.classifier.classify(&digest);
        let convention_score = self.scorer.score(&digest);

        NeuralPrediction {
            classification,
            convention_score,
            digest,
        }
    }

    /// Current operating mode.
    pub fn mode(&self) -> EnhancerMode {
        self.mode
    }

    /// Change operating mode.
    pub fn set_mode(&mut self, mode: EnhancerMode) {
        self.mode = mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_entry() -> ProposedEntry {
        ProposedEntry {
            title: "Valid title that is long enough".to_string(),
            content: "This is valid content with enough length for the quality gate checks"
                .to_string(),
            category: "convention".to_string(),
            topic: "test".to_string(),
            tags: vec!["auto-extracted".to_string()],
            source_rule: "knowledge-gap".to_string(),
            source_features: vec!["session-1".to_string(), "session-2".to_string()],
            extraction_confidence: 0.7,
        }
    }

    // T-SH-01: NeuralEnhancer shadow mode passes entry unchanged
    #[test]
    fn shadow_mode_passes_unchanged() {
        let enhancer = NeuralEnhancer::new(
            SignalClassifier::new_with_baseline(),
            ConventionScorer::new_with_baseline(),
            EnhancerMode::Shadow,
        );
        let entry = make_test_entry();
        let prediction = enhancer.enhance(&entry);

        // Prediction is produced
        assert!(prediction.convention_score >= 0.0);
        assert!(prediction.convention_score <= 1.0);
        assert_eq!(prediction.classification.probabilities.len(), 5);
        // Mode is shadow
        assert_eq!(enhancer.mode(), EnhancerMode::Shadow);
    }

    // T-SH-02: NeuralEnhancer produces valid prediction
    #[test]
    fn produces_valid_prediction() {
        let enhancer = NeuralEnhancer::new(
            SignalClassifier::new_with_baseline(),
            ConventionScorer::new_with_baseline(),
            EnhancerMode::Shadow,
        );
        let entry = make_test_entry();
        let prediction = enhancer.enhance(&entry);

        let prob_sum: f32 = prediction.classification.probabilities.iter().sum();
        assert!(
            (prob_sum - 1.0).abs() < 1e-4,
            "probabilities sum to {prob_sum}"
        );
        assert!((0.0..=1.0).contains(&prediction.convention_score));

        // Digest should match entry fields
        assert!(
            (prediction.digest.features[0] - 0.7).abs() < 1e-5,
            "extraction_confidence mismatch"
        );
    }
}
