# Test Plan: episodic (Episodic Augmentation)

## Component Under Test

`crates/unimatrix-adapt/src/episodic.rs` -- EpisodicAugmenter, score adjustment computation.

## Risks Covered

None directly. Episodic augmentation is the lowest-priority component (SR-04, SCOPE.md). If scope is cut, this component becomes a no-op stub. Tests are designed to pass for both full implementation and no-op stub.

## Test Cases

### T-EPI-01: Construction with defaults

**Purpose**: Verify EpisodicAugmenter initializes correctly.
**Setup**: Create EpisodicAugmenter with max_boost=0.02, min_affinity=3.
**Assertions**:
- max_boost == 0.02
- min_affinity == 3

### T-EPI-02: No adjustment for single result

**Purpose**: Verify degenerate case (one result) returns zero adjustment.
**Setup**: Create augmenter. Call with 1 result ID and score.
**Assertions**:
- Adjustment vector has length 1
- Adjustment[0] == 0.0

### T-EPI-03: No adjustment when no co-access affinity

**Purpose**: Verify results without co-access affinity get zero adjustment.
**Setup**: Create augmenter. Call with 5 result IDs. Provide a store that returns None for all co-access lookups.
**Assertions**:
- All adjustments are 0.0

### T-EPI-04: Adjustment applied for high-affinity results

**Purpose**: Verify results with co-access affinity >= min_affinity get a positive boost.
**Setup**: Create augmenter with min_affinity=3. Call with 5 results. Provide a store where result[3] has co-access count=10 with result[0] (an anchor).
**Assertions**:
- Adjustments[0..3] == 0.0 (anchors get no adjustment)
- Adjustments[3] > 0.0 (boosted by affinity)
- Adjustments[3] <= 0.02 (capped at max_boost)

### T-EPI-05: Adjustment capped at max_boost

**Purpose**: Verify the cap is enforced.
**Setup**: Create augmenter with max_boost=0.02. Provide co-access count=10000 (very high affinity).
**Assertions**:
- Adjustment <= 0.02 regardless of affinity value

### T-EPI-06: Stale co-access records ignored

**Purpose**: Verify staleness_cutoff filters old records.
**Setup**: Create augmenter. Provide co-access records with last_updated=100. Call with staleness_cutoff=200.
**Assertions**:
- All adjustments are 0.0 (records are stale)

### T-EPI-07: Below min_affinity threshold

**Purpose**: Verify low-affinity records do not trigger a boost.
**Setup**: Create augmenter with min_affinity=3. Provide co-access count=2 (below threshold).
**Assertions**:
- Adjustment == 0.0 for that result

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-01 Empty KB | N/A | No search results, no augmentation |
| EC-02 Single entry | T-EPI-02 | Single result, no adjustment |

## Note on Implementation Priority

Per SR-04, if scope proves too large, episodic augmentation may be implemented as a no-op stub returning zero adjustments. All tests except T-EPI-04 and T-EPI-05 should pass for both implementations. T-EPI-04 and T-EPI-05 can be marked `#[ignore]` or conditionally compiled if the stub path is taken.

## Total: 7 unit tests
