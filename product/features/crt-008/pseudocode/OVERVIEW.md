# Pseudocode Overview: crt-008 Continuous Self-Retraining

## Component Interaction

```
[MCP Handlers] --emit FeedbackSignal--> [TrainingService]
                                            |
                                    [LabelGenerator] --generate--> Vec<(model_name, TrainingSample)>
                                            |
                                    route to per-model [TrainingReservoir]
                                            |
                                    threshold check --> [try_train_step]
                                            |
                                    spawn_blocking: compute_gradients + EWC + apply_gradients
                                            |
                                    NaN check --> [ModelRegistry::register_shadow]
```

## Data Flow

1. Server handlers (usage, correct, deprecate, store, background) detect auto/neural entries
2. Build FeedbackSignal variant, call `training_service.record_feedback(signal)`
3. LabelGenerator converts signal to 0-2 TrainingSamples with model routing
4. Samples added to per-model reservoir
5. If reservoir.len() >= threshold AND training lock free: spawn_blocking training
6. Training: for each sample: compute_gradients -> add EWC contribution -> apply_gradients
7. NaN/Inf check on final params; if clean: save as shadow, register in ModelRegistry
8. Ground truth backfill on CategoryCorrection and consistent multi-vote signals

## Shared Types (training.rs)

- `TrainingSample { digest, target, weight, source, entry_id, timestamp }` -- Clone, Debug
- `TrainingTarget::Classification([f32; 5])` | `ConventionScore(f32)` -- Clone, Debug
- `FeedbackSignal` -- 9 variants matching spec -- Clone, Debug
- `OutcomeResult::Success` | `Rework` -- Clone, Debug
- `LabelGenerator { weak_label_weight: f32 }` -- stateless converter

## Component List

| Component | Wave | Crate | Files Modified/Created |
|-----------|------|-------|----------------------|
| trait-refactor | 0 | unimatrix-learn | models/traits.rs, models/classifier.rs, models/scorer.rs |
| training-types | 1 | unimatrix-learn | training.rs (NEW) |
| training-service | 2 | unimatrix-learn | service.rs (NEW), config.rs |
| rollback-enhancements | 3 | unimatrix-learn | service.rs (extend) |
| feedback-hooks | 4 | unimatrix-server | services/usage.rs, mcp/correct.rs, mcp/deprecate.rs, mcp/store.rs, background.rs, lib.rs |
| ground-truth-backfill | 5 | unimatrix-learn | service.rs (extend) |
| integration-test | 6 | unimatrix-learn | tests/retraining_e2e.rs (NEW) |

## Integration Harness Plan

- Existing crt-007 tests in `crates/unimatrix-learn/src/models/classifier.rs::tests` and `scorer.rs::tests` must pass unchanged after Wave 0
- New unit tests added inline in each new/modified file
- Integration test (Wave 6) in `crates/unimatrix-learn/tests/retraining_e2e.rs`
- No product/test/infra-001 suites apply (unimatrix-learn is a pure Rust crate with no MCP integration test dependency)

## Patterns Used

- Unimatrix #428: Shared ML Infrastructure Crate structure (models/, config.rs, etc.)
- Unimatrix #429: How to add ndarray MLP model (trait impl pattern)
- Existing crt-007 patterns: Xavier init, forward_layers, flat_parameters ordering
- TrainingReservoir<T: Clone> generic pattern for reservoir sampling
- EwcState::update_from_flat for flat gradient interface
- ModelRegistry::register_shadow + save_model for shadow installation

## Open Questions

None. All design decisions are captured in ADR-001 through ADR-006.
