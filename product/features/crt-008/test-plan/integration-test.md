# Test Plan: integration-test (Wave 6)

## Tests

### T-INT-01: End-to-end feedback -> label -> reservoir -> retrain -> shadow
- **Location**: `crates/unimatrix-learn/tests/retraining_e2e.rs`
- **Setup**:
  1. Create tmpdir for model storage
  2. Create LearnConfig with classifier_retrain_threshold = 20
  3. Create ModelRegistry and TrainingService
  4. Record baseline classifier predictions
- **Action**:
  1. Generate 20 HelpfulVote feedback signals with varying digests
  2. Call training_service.record_feedback() for each
  3. Wait for spawn_blocking completion (tokio::time::sleep 3s)
- **Assert**:
  1. Shadow model exists in ModelRegistry
  2. Shadow model file exists on disk
  3. Shadow model produces different predictions from baseline
- **Runtime**: tokio::test
- **Dependencies**: tempfile, tokio (both already in workspace)
