# Pseudocode: integration-test (Wave 6)

## Purpose

End-to-end integration test: feedback -> label -> reservoir -> retrain -> shadow model. New file: `crates/unimatrix-learn/tests/retraining_e2e.rs`.

## Test: T-INT-01

```pseudo
#[tokio::test]
async fn end_to_end_feedback_retrain_shadow() {
    // Setup
    let tmpdir = tempfile::TempDir::new().unwrap();
    let config = LearnConfig {
        models_dir: tmpdir.path().join("models"),
        classifier_retrain_threshold: 20,
        classifier_batch_size: 16,
        ..LearnConfig::default()
    };
    let registry = Arc::new(Mutex::new(ModelRegistry::new(config.models_dir.clone())));
    let service = Arc::new(TrainingService::new(config, registry.clone()));

    // Record baseline predictions
    let baseline_clf = SignalClassifier::new_with_baseline();
    let test_input = SignalDigest::from_fields(0.7, 3, 500, "convention", "topic", 5, 2);
    let baseline_pred = baseline_clf.forward(test_input.as_slice());

    // Generate 20 HelpfulVote signals targeting signal_classifier
    for i in 0..20u64 {
        let digest = SignalDigest::from_fields(
            0.5 + (i as f32) * 0.02, // varying confidence
            (i % 5) as u32,
            300 + (i * 10) as u32,
            "convention",
            "test-topic",
            3,
            1,
        );
        let signal = FeedbackSignal::HelpfulVote {
            entry_id: i,
            category: "convention".to_string(),
            digest,
        };
        service.record_feedback(signal);
    }

    // Wait for spawn_blocking to complete
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Verify shadow model exists
    let reg = registry.lock().unwrap();
    let shadow = reg.get_shadow("signal_classifier");
    assert!(shadow.is_some(), "shadow model should exist after training");

    // Load shadow model and verify different predictions
    if let Ok(Some(bytes)) = reg.load_model("signal_classifier", ModelSlot::Shadow) {
        let shadow_clf = SignalClassifier::deserialize(&bytes).unwrap();
        let shadow_pred = shadow_clf.forward(test_input.as_slice());

        // At least one output should differ
        let any_diff = baseline_pred.iter().zip(shadow_pred.iter())
            .any(|(b, s)| (b - s).abs() > 1e-6);
        assert!(any_diff, "shadow predictions should differ from baseline");
    }
}
```

## Notes

- Uses tokio::test for async runtime (spawn_blocking needs tokio)
- 3-second sleep is generous; training of 16 samples should complete in < 1s
- Test verifies the full pipeline: signal -> label -> reservoir -> threshold -> train -> shadow save
