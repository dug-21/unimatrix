# Test Plan: prototypes (Domain Prototypes)

## Component Under Test

`crates/unimatrix-adapt/src/prototypes.rs` -- PrototypeManager, PrototypeKey, Prototype, soft pull, running-mean update, LRU eviction, serialization.

## Risks Covered

- **R-08** (Medium): Prototype centroid instability under rapid corrections

## Test Cases

### T-PRO-01: Construction and empty state

**Purpose**: Verify PrototypeManager initializes correctly.
**Setup**: Create PrototypeManager with max_count=256, min_entries=3, pull_strength=0.1, dimension=384.
**Assertions**:
- Empty prototypes map
- `apply_pull(any_vector, None, None)` returns a copy of the input (no pull)
- `apply_pull(any_vector, Some("cat"), None)` returns input copy (no prototype exists)

### T-PRO-02: Prototype creation on first update

**Purpose**: Verify first update creates a new prototype.
**Setup**: Create PrototypeManager. Generate a 384d vector.
**Method**: Call `update(vector, Some("decision"), None, timestamp)`.
**Assertions**:
- Prototype with key Category("decision") exists
- entry_count == 1
- centroid equals the input vector

### T-PRO-03: Running-mean centroid update

**Purpose**: Verify online running mean formula.
**Setup**: Create PrototypeManager. Update same category with 3 vectors: [1,0,...], [0,1,...], [0,0,...].
**Assertions**:
- After 1 update: centroid = [1,0,...]
- After 2 updates: centroid = [(1+0)/2, (0+1)/2,...] = [0.5, 0.5,...]
- After 3 updates: centroid = [(0.5*2+0)/3, (0.5*2+0)/3,...] = [0.333, 0.333,...]
- entry_count == 3

### T-PRO-04: Soft pull below minimum entries threshold

**Purpose**: Verify prototypes with fewer than min_entries do not apply pull.
**Setup**: Create PrototypeManager with min_entries=3. Update category "test" with 2 entries.
**Method**: Call `apply_pull(vector, Some("test"), None)`.
**Assertions**:
- Output equals input (no pull applied, entry_count < min_entries)

### T-PRO-05: Soft pull above minimum entries threshold

**Purpose**: Verify soft pull adjusts embedding toward prototype.
**Setup**: Create PrototypeManager with min_entries=3, pull_strength=0.1. Update category "test" with 5 similar vectors to establish a centroid.
**Method**: Call `apply_pull(vector, Some("test"), None)` with a vector near the centroid.
**Assertions**:
- Output differs from input
- Output is closer to centroid than input was (cosine similarity to centroid increases)
- Adjustment magnitude is proportional to pull_strength * cosine_sim

### T-PRO-06: Prototype stability under rapid updates (R-08)

**Purpose**: Verify centroid does not oscillate wildly under diverse inputs.
**Setup**: Create PrototypeManager. Alternate updates with two very different vectors for 100 iterations.
**Assertions**:
- Centroid converges to the mean of the two vectors
- After 100 updates, centroid is stable (not oscillating)
- entry_count == 100

### T-PRO-07: Centroid convergence

**Purpose**: Verify centroid approaches true mean over many samples.
**Setup**: Create PrototypeManager. Generate 1000 vectors from a known distribution (Gaussian around a target mean).
**Method**: Update prototype with each vector.
**Assertions**:
- Final centroid is within L2 distance < 0.05 of the target mean
- entry_count == 1000

### T-PRO-08: LRU eviction at capacity (AC-15)

**Purpose**: Verify least-recently-updated prototype is evicted when max is reached.
**Setup**: Create PrototypeManager with max_count=3.
**Method**:
- Add prototypes for categories "a" (timestamp 100), "b" (timestamp 200), "c" (timestamp 300)
- Add prototype for category "d" (timestamp 400) -- should evict "a"
**Assertions**:
- Category "a" no longer exists
- Categories "b", "c", "d" exist
- Total prototype count == 3

### T-PRO-09: Category and topic prototypes independent

**Purpose**: Verify category and topic prototypes are maintained separately.
**Setup**: Update with category="decision", topic="architecture" using different vectors.
**Assertions**:
- Category("decision") prototype exists with its centroid
- Topic("architecture") prototype exists with its centroid
- The two centroids are independent (topic centroid != category centroid)

### T-PRO-10: apply_pull selects best prototype

**Purpose**: Verify best matching prototype is used when both category and topic exist.
**Setup**: Create category prototype with centroid near vector A, topic prototype with centroid near vector B.
**Method**: Call apply_pull with a vector closer to the topic prototype.
**Assertions**:
- Pull is toward the topic prototype (higher cosine similarity), not category

### T-PRO-11: Serialization round-trip

**Purpose**: Verify to_serialized and from_serialized preserve state.
**Setup**: Create PrototypeManager, add 5 category and 5 topic prototypes.
**Method**: Call `to_serialized()`, then `from_serialized()`.
**Assertions**:
- Same number of prototypes
- Same keys (category and topic names)
- Centroids match element-wise
- entry_count and last_updated match

### T-PRO-12: apply_pull with None category and None topic

**Purpose**: Verify query path (no category/topic) returns input unchanged.
**Setup**: Create PrototypeManager with several prototypes.
**Method**: Call `apply_pull(vector, None, None)`.
**Assertions**:
- Output equals input (no prototype lookup without key)

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-05 Corrected entry | T-PRO-06 | Running mean absorbs corrections gracefully |
| EC-09 Eviction during read | T-PRO-08 | Eviction is write-locked, reads are concurrent |

## Total: 12 unit tests
