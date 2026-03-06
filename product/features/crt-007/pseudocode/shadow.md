# Pseudocode: shadow (NeuralEnhancer + ShadowEvaluator)

## Pattern: Pipeline extension with mode-dependent behavior

NeuralEnhancer wraps models, ShadowEvaluator tracks accuracy.
Both live in unimatrix-observe's extraction module.

## Files

### crates/unimatrix-observe/src/extraction/neural.rs

```pseudo
use unimatrix_learn::models::{
    SignalClassifier, ConventionScorer, SignalDigest,
    ClassificationResult, SignalCategory,
};

/// Combined neural prediction for a single entry.
#[derive(Debug, Clone)]
pub struct NeuralPrediction {
    pub classification: ClassificationResult,
    pub convention_score: f32,
    pub digest: SignalDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnhancerMode {
    Shadow,
    Active,
}

/// Wraps classifier + scorer, produces NeuralPrediction.
pub struct NeuralEnhancer {
    classifier: SignalClassifier,
    scorer: ConventionScorer,
    mode: EnhancerMode,
}

impl NeuralEnhancer {
    pub fn new(
        classifier: SignalClassifier,
        scorer: ConventionScorer,
        mode: EnhancerMode,
    ) -> Self {
        Self { classifier, scorer, mode }
    }

    /// Build digest and run both models on a proposed entry.
    pub fn enhance(&self, entry: &super::ProposedEntry) -> NeuralPrediction {
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

    pub fn mode(&self) -> EnhancerMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: EnhancerMode) {
        self.mode = mode;
    }
}
```

### crates/unimatrix-observe/src/extraction/shadow.rs

```pseudo
use super::ProposedEntry;
use super::neural::{NeuralPrediction, EnhancerMode};

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
    pub per_category: std::collections::HashMap<String, f64>,
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

        let neural_cat = format!("{:?}", prediction.classification.category);
        let digest_bytes = bincode::serde::encode_to_vec(
            &prediction.digest,
            bincode::config::standard(),
        ).unwrap_or_default();

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
            let entry = per_cat_correct.entry(eval.rule_category.clone())
                .or_insert((0, 0));
            entry.1 += 1;
            // Neural agrees with rule if neural_category matches rule_category
            // (mapping: Convention<->convention, Pattern<->pattern, etc.)
            if neural_matches_rule(&eval.neural_category, &eval.rule_category) {
                correct += 1;
                entry.0 += 1;
            }
        }

        let total = self.evaluations.len() as u32;
        let overall = correct as f64 / total as f64;

        let per_category = per_cat_correct.into_iter()
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
        // No per-category regression check at this stage
        // (we compare against production model, which starts at baseline)
        acc.overall >= 0.5  // basic threshold for first promotion
    }

    /// Check if rollback should trigger.
    /// Returns true if rolling accuracy drops > threshold below baseline.
    pub fn should_rollback(&self) -> bool {
        let baseline = match self.baseline_accuracy {
            Some(b) => b,
            None => return false,
        };
        if self.evaluations.len() < self.rollback_window {
            return false;
        }
        // Rolling accuracy over last rollback_window predictions
        let window = &self.evaluations[self.evaluations.len() - self.rollback_window..];
        let correct = window.iter()
            .filter(|e| neural_matches_rule(&e.neural_category, &e.rule_category))
            .count();
        let rolling = correct as f64 / self.rollback_window as f64;
        (baseline - rolling) > self.rollback_threshold
    }

    pub fn set_baseline_accuracy(&mut self, accuracy: f64) {
        self.baseline_accuracy = Some(accuracy);
    }

    pub fn evaluation_count(&self) -> u32 {
        self.evaluations.len() as u32
    }

    /// Return evaluations for SQLite persistence.
    pub fn drain_evaluations(&mut self) -> Vec<ShadowLogEntry> {
        std::mem::take(&mut self.evaluations)
    }
}

/// Map neural SignalCategory debug name to rule category string.
fn neural_matches_rule(neural: &str, rule: &str) -> bool {
    matches!(
        (neural, rule),
        ("Convention", "convention")
        | ("Pattern", "pattern")
        | ("Gap", "gap" | "lesson-learned")
        | ("Dead", "decision")
        | ("Noise", _) // Noise is "no match" -- counts as incorrect unless rule also produced no match
    ) || neural.to_lowercase() == rule
}
```

### crates/unimatrix-observe/src/extraction/mod.rs

Add to module declarations:

```pseudo
pub mod neural;
pub mod shadow;
```

## Key Design Decisions

- NeuralEnhancer is stateless per-call -- mode is the only state
- ShadowEvaluator holds evaluations in memory, drained for persistence
- `from_fields()` on SignalDigest avoids cross-crate ProposedEntry dependency
- neural_matches_rule handles the mapping between enum variant names and
  extraction category strings
- drain_evaluations() enables batch SQLite writes (R-08 mitigation)
