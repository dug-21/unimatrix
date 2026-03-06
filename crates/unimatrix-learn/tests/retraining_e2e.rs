//! T-INT-01: End-to-end feedback -> label -> reservoir -> retrain -> shadow.

use std::sync::{Arc, Mutex};

use unimatrix_learn::models::classifier::SignalClassifier;
use unimatrix_learn::models::digest::SignalDigest;
use unimatrix_learn::models::traits::NeuralModel;
use unimatrix_learn::registry::{ModelRegistry, ModelSlot};
use unimatrix_learn::service::TrainingService;
use unimatrix_learn::training::FeedbackSignal;
use unimatrix_learn::LearnConfig;

#[test]
fn end_to_end_feedback_retrain_shadow() {
    let dir = tempfile::TempDir::new().expect("tmpdir");
    let config = LearnConfig {
        models_dir: dir.path().join("models"),
        classifier_retrain_threshold: 20,
        classifier_batch_size: 16,
        ..LearnConfig::default()
    };
    let registry = Arc::new(Mutex::new(ModelRegistry::new(config.models_dir.clone())));
    let service = Arc::new(TrainingService::new(config, registry.clone()));

    // Record baseline predictions
    let baseline_clf = SignalClassifier::new_with_baseline();
    let test_input = SignalDigest::from_fields(0.7, 3, 500, "convention", "knowledge-gap", 50, 2);
    let baseline_pred = baseline_clf.forward(test_input.as_slice());

    // Generate 20 HelpfulVote signals targeting signal_classifier
    for i in 0..20u64 {
        let digest = SignalDigest::from_fields(
            0.5 + (i as f64) * 0.02,
            (i % 5) as usize,
            300 + (i * 10) as usize,
            "convention",
            "knowledge-gap",
            30 + i as usize,
            2,
        );
        let signal = FeedbackSignal::HelpfulVote {
            entry_id: i,
            category: "convention".to_string(),
            digest,
        };
        service.record_feedback(signal);
    }

    // Wait for background training thread to complete
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Verify shadow model exists
    let reg = registry.lock().expect("registry lock");
    let shadow = reg.get_shadow("signal_classifier");
    assert!(shadow.is_some(), "shadow model should exist after training");

    // Load shadow model and verify different predictions
    let bytes = reg
        .load_model("signal_classifier", ModelSlot::Shadow)
        .expect("load model")
        .expect("model bytes present");
    let shadow_clf = SignalClassifier::deserialize(&bytes).expect("deserialize shadow");
    let shadow_pred = shadow_clf.forward(test_input.as_slice());

    // At least one output should differ
    let any_diff = baseline_pred
        .iter()
        .zip(shadow_pred.iter())
        .any(|(b, s)| (b - s).abs() > 1e-6);
    assert!(
        any_diff,
        "shadow predictions should differ from baseline: {:?} vs {:?}",
        baseline_pred, shadow_pred
    );
}
