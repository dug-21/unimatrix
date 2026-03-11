//! Shadow evaluation: tracks neural vs rule-based ground truth.

use std::collections::HashMap;

use super::ProposedEntry;
use super::neural::NeuralPrediction;

/// Shadow evaluation log entry.
#[derive(Debug, Clone)]
pub struct ShadowLogEntry {
    pub timestamp: u64,
    pub rule_name: String,
    pub rule_category: String,
    pub neural_category: String,
    pub neural_confidence: f32,
    pub convention_score: f32,
    pub rule_accepted: bool,
    pub digest_bytes: Vec<u8>,
}

/// Per-category accuracy tracking.
#[derive(Debug, Clone, Default)]
pub struct ShadowAccuracy {
    pub overall: f64,
    pub per_category: HashMap<String, f64>,
    pub total_evaluations: u32,
}

/// Tracks neural predictions against rule-based ground truth.
pub struct ShadowEvaluator {
    evaluations: Vec<ShadowLogEntry>,
    min_evaluations: u32,
    rollback_threshold: f64,
    rollback_window: usize,
    baseline_accuracy: Option<f64>,
}

impl ShadowEvaluator {
    /// Create a new evaluator with promotion/rollback parameters.
    pub fn new(min_evaluations: u32, rollback_threshold: f64, rollback_window: usize) -> Self {
        Self {
            evaluations: Vec::new(),
            min_evaluations,
            rollback_threshold,
            rollback_window,
            baseline_accuracy: None,
        }
    }

    /// Log a neural prediction alongside the rule outcome.
    pub fn log_prediction(
        &mut self,
        entry: &ProposedEntry,
        prediction: &NeuralPrediction,
        rule_accepted: bool,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let neural_cat = prediction.classification.category.to_string();
        let digest_bytes =
            bincode::serde::encode_to_vec(&prediction.digest, bincode::config::standard())
                .unwrap_or_default();

        self.evaluations.push(ShadowLogEntry {
            timestamp: now,
            rule_name: entry.source_rule.clone(),
            rule_category: entry.category.clone(),
            neural_category: neural_cat,
            neural_confidence: prediction.classification.confidence,
            convention_score: prediction.convention_score,
            rule_accepted,
            digest_bytes,
        });
    }

    /// Compute accuracy: neural category matches rule category.
    pub fn accuracy(&self) -> ShadowAccuracy {
        if self.evaluations.is_empty() {
            return ShadowAccuracy::default();
        }

        let mut correct = 0u32;
        let mut per_cat_correct: HashMap<String, (u32, u32)> = HashMap::new();

        for eval in &self.evaluations {
            let entry = per_cat_correct
                .entry(eval.rule_category.clone())
                .or_insert((0, 0));
            entry.1 += 1;
            if neural_matches_rule(&eval.neural_category, &eval.rule_category) {
                correct += 1;
                entry.0 += 1;
            }
        }

        let total = self.evaluations.len() as u32;
        let overall = correct as f64 / total as f64;

        let per_category = per_cat_correct
            .into_iter()
            .map(|(cat, (c, t))| (cat, c as f64 / t as f64))
            .collect();

        ShadowAccuracy {
            overall,
            per_category,
            total_evaluations: total,
        }
    }

    /// Check if promotion criteria are met.
    pub fn can_promote(&self) -> bool {
        if (self.evaluations.len() as u32) < self.min_evaluations {
            return false;
        }
        let acc = self.accuracy();
        acc.overall >= 0.5
    }

    /// Check if rollback should trigger.
    ///
    /// Returns true if rolling accuracy drops > threshold below baseline.
    pub fn should_rollback(&self) -> bool {
        let baseline = match self.baseline_accuracy {
            Some(b) => b,
            None => return false,
        };
        if self.evaluations.len() < self.rollback_window {
            return false;
        }
        let window = &self.evaluations[self.evaluations.len() - self.rollback_window..];
        let correct = window
            .iter()
            .filter(|e| neural_matches_rule(&e.neural_category, &e.rule_category))
            .count();
        let rolling = correct as f64 / self.rollback_window as f64;
        (baseline - rolling) > self.rollback_threshold
    }

    /// Set the pre-promotion baseline accuracy.
    pub fn set_baseline_accuracy(&mut self, accuracy: f64) {
        self.baseline_accuracy = Some(accuracy);
    }

    /// Total number of evaluations logged.
    pub fn evaluation_count(&self) -> u32 {
        self.evaluations.len() as u32
    }

    /// Drain evaluations for SQLite persistence.
    pub fn drain_evaluations(&mut self) -> Vec<ShadowLogEntry> {
        std::mem::take(&mut self.evaluations)
    }
}

/// Map neural SignalCategory display name to rule category string.
fn neural_matches_rule(neural: &str, rule: &str) -> bool {
    matches!(
        (neural, rule),
        ("Convention", "convention")
            | ("Pattern", "pattern")
            | ("Gap", "gap")
            | ("Gap", "lesson-learned")
            | ("Dead", "decision")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::neural::{EnhancerMode, NeuralEnhancer};
    use unimatrix_learn::models::{ConventionScorer, SignalClassifier};

    fn make_test_entry(category: &str) -> ProposedEntry {
        ProposedEntry {
            title: "Valid title that is long enough".to_string(),
            content: "This is valid content with enough length for quality gate checks".to_string(),
            category: category.to_string(),
            topic: "test".to_string(),
            tags: vec!["auto-extracted".to_string()],
            source_rule: "knowledge-gap".to_string(),
            source_features: vec!["s1".to_string(), "s2".to_string()],
            extraction_confidence: 0.7,
        }
    }

    fn make_enhancer() -> NeuralEnhancer {
        NeuralEnhancer::new(
            SignalClassifier::new_with_baseline(),
            ConventionScorer::new_with_baseline(),
            EnhancerMode::Shadow,
        )
    }

    // T-SH-03: ShadowEvaluator tracks evaluations
    #[test]
    fn tracks_evaluations() {
        let enhancer = make_enhancer();
        let mut evaluator = ShadowEvaluator::new(20, 0.05, 50);

        for _ in 0..5 {
            let entry = make_test_entry("convention");
            let pred = enhancer.enhance(&entry);
            evaluator.log_prediction(&entry, &pred, true);
        }

        assert_eq!(evaluator.evaluation_count(), 5);
        let acc = evaluator.accuracy();
        assert_eq!(acc.total_evaluations, 5);
    }

    // T-SH-04: ShadowEvaluator accuracy computation
    #[test]
    fn accuracy_computation() {
        let mut evaluator = ShadowEvaluator::new(20, 0.05, 50);
        let enhancer = make_enhancer();

        // Log predictions -- baseline classifier classifies most as Noise
        // so accuracy against rule category will be low
        for _ in 0..10 {
            let entry = make_test_entry("convention");
            let pred = enhancer.enhance(&entry);
            evaluator.log_prediction(&entry, &pred, true);
        }

        let acc = evaluator.accuracy();
        assert_eq!(acc.total_evaluations, 10);
        assert!((0.0..=1.0).contains(&acc.overall));
    }

    // T-SH-05: can_promote requires min evaluations
    #[test]
    fn can_promote_requires_min_evaluations() {
        let mut evaluator = ShadowEvaluator::new(20, 0.05, 50);
        let enhancer = make_enhancer();

        for _ in 0..19 {
            let entry = make_test_entry("convention");
            let pred = enhancer.enhance(&entry);
            evaluator.log_prediction(&entry, &pred, true);
        }
        assert!(!evaluator.can_promote());

        let entry = make_test_entry("convention");
        let pred = enhancer.enhance(&entry);
        evaluator.log_prediction(&entry, &pred, true);
        // Now has 20 evaluations -- can_promote depends on accuracy
        assert_eq!(evaluator.evaluation_count(), 20);
    }

    // T-SH-06: should_rollback within tolerance
    #[test]
    fn should_rollback_within_tolerance() {
        let mut evaluator = ShadowEvaluator::new(20, 0.05, 5);
        evaluator.set_baseline_accuracy(0.80);

        // Simulate 5 evals where 4/5 match = 80% (within tolerance)
        let enhancer = make_enhancer();
        for i in 0..5 {
            let entry = make_test_entry(if i < 4 { "convention" } else { "pattern" });
            let mut pred = enhancer.enhance(&entry);
            // Force neural category to match for accuracy simulation
            if i < 4 {
                pred.classification.category = unimatrix_learn::models::SignalCategory::Convention;
            }
            evaluator.log_prediction(&entry, &pred, true);
        }

        // Rolling accuracy = 0.8, baseline = 0.8, drop = 0 < 0.05
        assert!(!evaluator.should_rollback());
    }

    // T-SH-07: should_rollback triggers on large drop
    #[test]
    fn should_rollback_triggers_on_large_drop() {
        let mut evaluator = ShadowEvaluator::new(20, 0.05, 5);
        evaluator.set_baseline_accuracy(0.80);

        let enhancer = make_enhancer();
        // 3/5 match = 60% accuracy, drop = 20% > 5%
        for i in 0..5 {
            let entry = make_test_entry(if i < 3 { "convention" } else { "pattern" });
            let mut pred = enhancer.enhance(&entry);
            if i < 3 {
                pred.classification.category = unimatrix_learn::models::SignalCategory::Convention;
            }
            evaluator.log_prediction(&entry, &pred, true);
        }

        assert!(evaluator.should_rollback());
    }

    // T-SH-08: should_rollback requires minimum window
    #[test]
    fn should_rollback_requires_min_window() {
        let mut evaluator = ShadowEvaluator::new(20, 0.05, 50);
        evaluator.set_baseline_accuracy(0.80);

        let enhancer = make_enhancer();
        // Only 49 evals (window = 50)
        for _ in 0..49 {
            let entry = make_test_entry("pattern"); // will mismatch
            let pred = enhancer.enhance(&entry);
            evaluator.log_prediction(&entry, &pred, true);
        }

        assert!(!evaluator.should_rollback()); // window not full
    }
}
