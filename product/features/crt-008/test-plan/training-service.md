# Test Plan: training-service (Wave 2)

## Tests

### T-FR04-01: Reservoir routing by model name
- **Setup**: Create TrainingService with default config
- **Action**: Record 5 HelpfulVote signals + 3 ConventionFollowed signals
- **Assert**: classifier reservoir has 5 items, scorer reservoir has 3

### T-FR04-02: Classifier threshold triggers training
- **Setup**: TrainingService with classifier_retrain_threshold = 20
- **Action**: Record 20 HelpfulVote signals
- **Assert**: Training triggered (shadow model appears in registry after brief wait)

### T-FR04-03: Scorer threshold triggers training
- **Setup**: TrainingService with scorer_retrain_threshold = 5
- **Action**: Record 5 ConventionFollowed signals
- **Assert**: Training triggered (shadow model appears in registry after brief wait)

### T-FR05-01: Training completes without blocking
- **Setup**: Record enough signals to trigger training
- **Action**: Measure wall time of record_feedback calls
- **Assert**: All record_feedback calls return in < 10ms (training runs in background)

### T-FR05-02: EWC penalty active during training
- **Setup**: Initialize EWC state with known reference params
- **Action**: Train for 5 steps with EWC
- **Assert**: EWC penalty at step 5 > penalty at step 1 (weights diverging from reference)
- **Note**: This tests EWC integration, not TrainingService directly. Can be a unit test on the training loop logic.

### T-FR05-03: Retrained model saved as shadow
- **Setup**: Trigger training via threshold crossing
- **Action**: Wait for training completion
- **Assert**: ModelRegistry has shadow for model name, shadow file exists on disk

### T-R02-01: Concurrent training lock prevents double execution
- **Setup**: Manually set training lock to true
- **Action**: Call try_train_step
- **Assert**: Returns immediately without training
- **Cleanup**: Release lock, call again, verify training proceeds

### T-FR-CONFIG-01: Default config values
- **Setup**: `LearnConfig::default()`
- **Assert**: All new fields match spec defaults (threshold=20, scorer_threshold=5, ewc_alpha=0.95, etc.)

### T-R05-01: Custom threshold triggers training at configured value
- **Setup**: Config with classifier_retrain_threshold = 5
- **Action**: Record 5 signals
- **Assert**: Training triggers at 5 (not default 20)
