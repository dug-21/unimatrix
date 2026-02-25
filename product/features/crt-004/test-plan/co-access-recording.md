# Test Plan: C3 -- Co-Access Recording

## Location

`crates/unimatrix-server/src/coaccess.rs` (unit tests for generate_pairs)
`crates/unimatrix-server/tests/` (integration tests for recording pipeline)

## Risk Coverage

- R-04: Quadratic pair generation creates write amplification
- R-12: Co-access recording failure silently dropped

## Test Scenarios

### T-C3-01: generate_pairs -- cap enforcement (R-04)

```
Given 15 entry IDs [1..=15]
When generate_pairs(ids, MAX_CO_ACCESS_ENTRIES=10)
Then generates 45 pairs (from first 10 IDs only)
Not 105 pairs (from all 15)
```

### T-C3-02: generate_pairs -- single entry (R-04)

```
Given entry IDs [1]
When generate_pairs(ids, 10)
Then returns empty vec (need at least 2 for a pair)
```

### T-C3-03: generate_pairs -- two entries

```
Given entry IDs [5, 3]
When generate_pairs(ids, 10)
Then returns [(3, 5)] (one pair, ordered)
```

### T-C3-04: generate_pairs -- exactly 10 entries (R-04)

```
Given entry IDs [1..=10]
When generate_pairs(ids, 10)
Then returns exactly 45 pairs
```

### T-C3-05: generate_pairs -- empty input (R-04)

```
Given empty entry IDs []
When generate_pairs(ids, 10)
Then returns empty vec
```

### T-C3-06: generate_pairs -- pairs are ordered

```
Given entry IDs [10, 5, 8]
When generate_pairs(ids, 10)
Then all pairs have min < max:
  (5, 10), (8, 10), (5, 8)
```

### T-C3-07: recording failure does not affect tool response (R-12)

```
Integration test:
Given record_usage_for_entries is called with entry_ids=[1,2,3]
And CO_ACCESS write is simulated to fail
Then steps 1-4 (access dedup, votes, usage+confidence, feature entries) still succeed
And a tracing::warn is emitted
And the tool response is unaffected
```

Note: T-C3-07 is challenging to test in isolation because it requires simulating a store
failure. In practice, this is verified by ensuring the co-access step is wrapped in a
match on the spawn_blocking result, with warn on error. The integration test can verify
that the tool call succeeds even when CO_ACCESS is empty (no prior setup needed -- the
recording is fire-and-forget).
