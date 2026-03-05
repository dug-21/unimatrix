# Test Plan: registry (Wave 4)

## Risk Coverage: R-06 (model file corruption or loss)

### T-R06-1: Registry loads with missing models directory
- **Type**: Unit
- **Method**: Create ModelRegistry with non-existent directory path
- **Pass criteria**: Directory created, registry functional (no crash)
- **Location**: `crates/unimatrix-learn/src/registry.rs` tests

### T-R06-2: Model load with corrupt file falls back to baseline
- **Type**: Unit
- **Method**: Write garbage bytes to model file path, attempt load
- **Pass criteria**: Returns baseline model (not error), logs warning
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests (load path)

### T-R06-3: Model load with empty file falls back to baseline
- **Type**: Unit
- **Method**: Write 0-byte file to model path, attempt load
- **Pass criteria**: Returns baseline model (not error)
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

### T-R06-4: Registry JSON corruption falls back to fresh
- **Type**: Unit
- **Method**: Write invalid JSON to registry.json, call load_registry
- **Pass criteria**: Returns fresh empty registry (no crash)
- **Location**: `crates/unimatrix-learn/src/registry.rs` tests

### T-R06-5: Retention policy deletes old versions
- **Type**: Unit
- **Method**: Register model v1, promote to v2, promote to v3.
  Verify v1 file is deleted but v2 (previous) and v3 (production) remain.
- **Pass criteria**: Only production + previous files exist after promotions
- **Location**: `crates/unimatrix-learn/src/registry.rs` tests

## Additional Unit Tests

### T-REG-01: Register and retrieve model
- Register "signal_classifier", verify get_production returns it

### T-REG-02: Model state transitions
- New model -> Observation
- After features_observed >= 5 -> Shadow
- After promotion -> Production
- After rollback -> RolledBack

### T-REG-03: Promotion flow
- production -> previous, shadow -> production
- previous_path set, shadow cleared

### T-REG-04: Rollback flow
- previous -> production, shadow cleared
- state = RolledBack

### T-REG-05: Registry save/load round-trip
- Save registry.json, load, verify all slots match

### T-REG-06: RollingMetrics accuracy
- Record 80 correct, 20 incorrect -> accuracy ~0.80
- Verify window respects capacity (evicts oldest)

### T-REG-07: RollingMetrics empty window
- accuracy() on empty window = 0.0

### T-REG-08: check_promotion criteria
- eval_count < 20: false
- eval_count >= 20 AND accuracy >= production: true
- eval_count >= 20 AND accuracy < production: false

### T-REG-09: check_rollback criteria
- accuracy drops > 5% below promotion accuracy: true
- accuracy stable: false

## Integration Test: integration_registry.rs

Full lifecycle test:
1. Create registry in tempdir
2. Register classifier with baseline weights
3. Verify state = Observation
4. Set features_observed = 10 -> state = Shadow
5. Record 25 evaluations (80% correct)
6. Check promotion = true
7. Promote -> state = Production
8. Record 100 evaluations (70% correct) -> accuracy drops
9. Check rollback = true
10. Rollback -> state = RolledBack
11. Save registry, reload, verify state preserved
