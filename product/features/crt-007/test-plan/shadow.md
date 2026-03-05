# Test Plan: shadow (Wave 5)

## Risk Coverage: R-04 (shadow mode false confidence)

### T-R04-1: ShadowEvaluator records both predictions
- **Type**: Unit
- **Method**: Call evaluate() with known rule and neural predictions.
  Query shadow_evaluations table, verify both predictions stored.
- **Pass criteria**: Row exists with correct model_name, rule_prediction, neural_prediction
- **Location**: `crates/unimatrix-learn/src/shadow.rs` tests

### T-R04-2: Per-class accuracy computation
- **Type**: Unit
- **Method**: Insert 10 evaluations: 8 where rule=neural (correct), 2 where they differ.
  Call per_class_accuracy().
- **Pass criteria**: Returns correct per-class accuracy values
- **Location**: `crates/unimatrix-learn/src/shadow.rs` tests

### T-R04-3: Divergence rate tracked and queryable
- **Type**: Unit
- **Method**: Insert evaluations with known agreement/disagreement.
  Call divergence_rate().
- **Pass criteria**: Returns expected fraction (e.g., 0.2 for 2/10 disagreements)
- **Location**: `crates/unimatrix-learn/src/shadow.rs` tests

### T-R04-4: No promotion on per-class regression
- **Type**: Unit
- **Method**: Simulate: aggregate accuracy improves but one class drops >10%.
  Verify check_promotion returns false.
- **Pass criteria**: Promotion blocked when any single class regresses >10%
- **Location**: `crates/unimatrix-learn/src/registry.rs` tests
- **Note**: This test validates the interaction between ShadowEvaluator per_class_accuracy
  and ModelRegistry check_promotion.

## Risk Coverage: R-08 (schema migration)

### T-R08-1: v7->v8 migration creates table
- **Type**: Unit
- **Method**: Open in-memory SQLite, set schema_version=7, run migration.
  Verify shadow_evaluations table exists with correct columns.
- **Pass criteria**: Table exists, INSERT works, indexes exist
- **Location**: `crates/unimatrix-store/src/migration.rs` tests

### T-R08-2: Migration is idempotent
- **Type**: Unit
- **Method**: Run migration twice. Verify no error on second run.
- **Pass criteria**: Second run succeeds without error
- **Location**: `crates/unimatrix-store/src/migration.rs` tests

### T-R08-3: v8 database opens correctly
- **Type**: Unit
- **Method**: Open database via Store::open (creates fresh v8), insert shadow evaluation, query it.
- **Pass criteria**: Full round-trip works
- **Location**: `crates/unimatrix-store/src/migration.rs` tests or `crates/unimatrix-learn/src/shadow.rs` tests

### T-R08-4: Existing v7 data unaffected
- **Type**: Unit
- **Method**: Create v7 database with observations, run migration to v8.
  Query observations table, verify data intact.
- **Pass criteria**: All pre-existing data queryable after migration
- **Location**: `crates/unimatrix-store/src/migration.rs` tests

## Additional Unit Tests

### T-SHD-01: ShadowEvaluator evaluation_count
- Insert N records, verify count returns N

### T-SHD-02: ShadowEvaluator accuracy
- Insert known mix of agree/disagree, verify accuracy computation

### T-SHD-03: Signal digest stored as BLOB
- Insert evaluation, query signal_digest column, verify 128 bytes
- Decode back to [f32; 32], verify matches original

### T-SHD-04: Feature cycle stored and queryable
- Insert with feature_cycle = "crt-007", query with WHERE, verify found

## Integration Test: integration_shadow.rs

End-to-end shadow pipeline:
1. Create in-memory SQLite via Store::open (tempdir)
2. Run migration (auto on open)
3. Create SignalClassifier and ConventionScorer with baseline weights
4. Build SignalDigest from test values
5. Run classifier.predict and scorer.predict
6. Log evaluations via ShadowEvaluator
7. Query accuracy and per_class_accuracy
8. Verify all values match expected
9. Verify no entries were modified (shadow mode = observation only)
