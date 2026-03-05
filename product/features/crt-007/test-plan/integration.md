# Test Plan: integration (Wave 6)

## Risk Coverage: R-05 (col-013 integration surface)

### T-R05-1: SignalDigest from mock ProposedEntry
- **Type**: Unit
- **Method**: Create a mock ProposedEntry (no col-013 internal types).
  Build SignalDigest from it. Verify all slots populated correctly.
- **Pass criteria**: Digest construction succeeds, all slots in [0, 1]
- **Location**: `crates/unimatrix-server/src/background.rs` tests or
  `crates/unimatrix-learn/src/digest.rs` tests

### T-R05-2: Neural enhancement callable with mock inputs
- **Type**: Unit
- **Method**: Call neural_enhance with mock ProposedEntries, mock registry,
  mock evaluator. Verify it returns successfully.
- **Pass criteria**: Function returns Ok, entries unchanged in shadow mode
- **Location**: `crates/unimatrix-server/src/background.rs` tests

### T-R05-3: End-to-end shadow mode pipeline
- **Type**: Integration
- **Method**: Synthetic observations -> extraction rules -> digest -> predict -> log.
  Verify shadow_evaluations table has records, entries unchanged.
- **Pass criteria**: Evaluation records exist, no entries stored by neural step
- **Location**: `crates/unimatrix-learn/tests/integration_shadow.rs`

## Risk Coverage: R-09 (performance)

### T-R09-1: Classifier inference < 50ms
- **Type**: Benchmark
- **Method**: Time 1000 classifier predictions, compute average
- **Pass criteria**: Average < 50ms
- **Location**: `crates/unimatrix-learn/src/classifier.rs` tests

### T-R09-2: Scorer inference < 10ms
- **Type**: Benchmark
- **Method**: Time 1000 scorer predictions, compute average
- **Pass criteria**: Average < 10ms
- **Location**: `crates/unimatrix-learn/src/scorer.rs` tests

### T-R09-3: 100 entries neural enhancement < 10 seconds
- **Type**: Integration benchmark
- **Method**: Create 100 mock ProposedEntries, run through neural_enhance
  (classifier + scorer + shadow log for each)
- **Pass criteria**: Total time < 10 seconds
- **Location**: `crates/unimatrix-learn/tests/integration_shadow.rs`

### T-R09-4: No lock contention with MCP requests
- **Type**: Unit
- **Method**: Verify neural_enhance does not acquire Store lock during
  prediction (only during shadow evaluation INSERT). Verify INSERTs use
  non-blocking pattern.
- **Pass criteria**: Architecture review -- prediction is CPU-only, no locks
- **Location**: Code review (not a runtime test)

## trust_source Tests

### T-TST-01: trust_score("neural") = 0.40
- **Location**: `crates/unimatrix-engine/src/confidence.rs` tests
- Verify exact value

### T-TST-02: trust_score("neural") ordering
- "auto" (0.35) < "neural" (0.40) < "agent" (0.50)

### T-TST-03: Existing trust scores unchanged
- "human" = 1.0, "system" = 0.7, "agent" = 0.5, "auto" = 0.35, unknown = 0.3
