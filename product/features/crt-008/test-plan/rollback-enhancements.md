# Test Plan: rollback-enhancements (Wave 3)

## Tests

### T-R03-01: NaN/Inf detection discards model
- **Setup**: Create model, manually set one parameter to NaN via set_parameters
- **Action**: Run NaN/Inf check (the check from try_train_step)
- **Assert**: Model is discarded, no shadow registered in ModelRegistry

### T-R06-01: Per-class regression prevents promotion
- **Setup**: Shadow per-class accuracy = [0.95, 0.85, 0.90, 0.80, 0.95], Production = [0.95, 0.95, 0.90, 0.90, 0.95]
- **Action**: Call check_promotion_safe() with threshold 0.10
- **Assert**: Returns false (class 1 drops 0.10, class 3 drops 0.10 -- at boundary)
- **Note**: Test with exact boundary (0.10 drop) and with clear regression (0.15 drop)
