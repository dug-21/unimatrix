# Test Plan: service (AdaptationService)

## Component Under Test

`crates/unimatrix-adapt/src/service.rs` -- AdaptationService, the top-level public API that orchestrates MicroLoRA, EWC, prototypes, reservoir, and persistence.

## Risks Covered

- **R-03** (High): Training-induced regression (cross-topic interference)
- **R-05** (High): Concurrent read/write race
- **R-12** (Low): Cold-start near-identity

## Test Cases

### T-SVC-01: Cold-start identity output (R-12, EC-01)

**Purpose**: Verify fresh AdaptationService produces near-identity embeddings.
**Setup**: Create AdaptationService with default config.
**Method**: Call adapt_embedding on 10 random 384d vectors.
**Assertions**:
- For each: cosine_similarity(input, output) > 0.99
- Output has same dimension as input (384)
- No NaN or Inf in output

### T-SVC-02: adapt_embedding with category and topic

**Purpose**: Verify category/topic are passed through to prototype system.
**Setup**: Create AdaptationService. Update prototypes for category "decision" (5+ entries to exceed min_entries).
**Method**: Call adapt_embedding with category=Some("decision").
**Assertions**:
- Output differs from adapt_embedding with category=None
- The prototype pull has been applied

### T-SVC-03: record_training_pairs and reservoir accumulation

**Purpose**: Verify pairs flow into the reservoir.
**Setup**: Create AdaptationService.
**Method**: Call record_training_pairs with 10 pairs.
**Assertions**:
- Reservoir has 10 pairs (verify via try_train_step threshold behavior)
- Training does not trigger (10 < batch_size 32)

### T-SVC-04: Cross-topic training does not degrade unrelated topics (R-03)

**Purpose**: Verify training on topic A does not worsen topic B retrieval.
**Setup**: Create AdaptationService. Generate two clusters of vectors (topic A and topic B) at dimension 16 (for speed).
**Method**:
- Establish baseline: compute cosine similarities within each topic
- Train on topic A pairs only (50 pairs, multiple training steps)
- Measure topic B similarities after training
**Assertions**:
- Topic B intra-cluster similarities have not decreased by more than 5%
- Topic A intra-cluster similarities have improved or stayed the same
- EWC regularization prevents topic B degradation

### T-SVC-05: EWC prevents catastrophic forgetting (R-03)

**Purpose**: Verify sequential training on different topics preserves earlier learning.
**Setup**: Create AdaptationService at dimension 16.
**Method**:
- Train on topic A pairs (30 steps)
- Record topic A quality (intra-cluster similarity)
- Train on topic B pairs (30 steps)
- Measure topic A quality again
**Assertions**:
- Topic A quality after topic B training >= 90% of topic A quality after topic A training
- This validates EWC regularization is working

### T-SVC-06: Concurrent read during training (R-05)

**Purpose**: Verify no race condition between adapt_embedding and try_train_step.
**Setup**: Create AdaptationService. Fill reservoir with 100 pairs.
**Method**: Using threads:
- Spawn 100 threads each calling adapt_embedding on random inputs
- Simultaneously call try_train_step on the main thread
**Assertions**:
- No panics from any thread
- All adapt_embedding calls return valid (finite, correct dimension) results
- No NaN in any output
- All threads complete successfully

### T-SVC-07: try_train_step fires when reservoir is full

**Purpose**: Verify training triggers when reservoir reaches batch_size.
**Setup**: Create AdaptationService with batch_size=32.
**Method**: Add 40 pairs via record_training_pairs. Call try_train_step with valid embed_fn.
**Assertions**:
- training_generation() returns 1 (incremented from 0)
- total_steps incremented

### T-SVC-08: try_train_step does not fire below threshold

**Purpose**: Verify training does not trigger below batch_size.
**Setup**: Create AdaptationService with batch_size=32.
**Method**: Add 10 pairs. Call try_train_step.
**Assertions**:
- training_generation() returns 0 (unchanged)

### T-SVC-09: Debounced save counter

**Purpose**: Verify should_save and reset_save_counter work correctly.
**Setup**: Create AdaptationService.
**Assertions**:
- should_save() returns false initially
- After 9 training steps: should_save() still false
- After 10th training step: should_save() returns true
- After reset_save_counter(): should_save() returns false

### T-SVC-10: Persistence round-trip through service API

**Purpose**: Verify save_state and load_state preserve learned adaptation.
**Setup**: Create AdaptationService. Train for several steps to get non-trivial weights.
**Method**: Save state to temp dir. Create new AdaptationService, load state from same dir.
**Assertions**:
- training_generation matches
- adapt_embedding on same input produces same output as pre-save service
- EWC penalty on same params matches

### T-SVC-11: adapt_embedding dimension validation

**Purpose**: Verify dimension mismatch is caught.
**Setup**: Create AdaptationService with dimension=384.
**Method**: Call adapt_embedding with a 100d vector.
**Assertions**:
- Panics or returns error (programmer error, not runtime)

### T-SVC-12: Send + Sync (NFR-07)

**Purpose**: Verify AdaptationService can be shared via Arc across threads.
**Setup**: Create Arc<AdaptationService>.
**Method**: Pass Arc clone to a spawned thread. Call adapt_embedding from the spawned thread.
**Assertions**:
- Compiles (Send + Sync bound satisfied)
- Returns valid result from spawned thread

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-01 Empty KB | T-SVC-01 | Near-identity output |
| EC-03 Fewer pairs than batch | T-SVC-08 | Training does not trigger |
| EC-04 All pairs from one topic | T-SVC-04 variant | Single-topic training works |
| EC-08 Concurrent train + use | T-SVC-06 | No deadlock, no race |

## Total: 12 unit tests
