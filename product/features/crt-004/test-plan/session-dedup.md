# Test Plan: C2 -- Session Dedup Extension

## Location

`crates/unimatrix-server/src/usage_dedup.rs` (unit tests)

## Risk Coverage

- R-05: Session dedup race condition between concurrent agents

## Test Scenarios

### T-C2-01: filter_co_access_pairs -- first call passes through

```
Given a fresh UsageDedup
When filter_co_access_pairs([(1,2), (3,4)])
Then returns [(1,2), (3,4)] (both new)
```

### T-C2-02: filter_co_access_pairs -- second call filters duplicates

```
Given filter_co_access_pairs([(1,2), (3,4)]) was called
When filter_co_access_pairs([(1,2), (5,6)])
Then returns [(5,6)] only (pair (1,2) already recorded)
```

### T-C2-03: filter_co_access_pairs -- empty input

```
Given a fresh UsageDedup
When filter_co_access_pairs([])
Then returns empty vec
```

### T-C2-04: filter_co_access_pairs -- agent independent (R-05)

```
Given agent A recorded usage for entries [1,2,3] (via filter_access)
And pairs (1,2), (1,3), (2,3) were filtered via filter_co_access_pairs
When agent B calls filter_co_access_pairs([(1,2)])
Then returns empty (pair already recorded, regardless of agent)
```

Note: co-access dedup uses (u64, u64) keys, NOT (String, u64) keys like access_counted.
This is agent-independent by design (SCOPE non-goal: no per-agent profiles).

### T-C2-05: filter_co_access_pairs -- concurrent access (R-05)

```
Given two threads both calling filter_co_access_pairs([(1,2)])
When both execute
Then exactly one thread gets [(1,2)] returned
And the other gets empty
(Mutex ensures serialization)
```

### T-C2-06: co_access_recorded does not interfere with existing dedup

```
Given filter_access("agent-a", [1, 2]) was called
When filter_co_access_pairs([(1,2)])
Then returns [(1,2)] (co-access dedup is independent of access dedup)
And filter_access("agent-a", [1, 2]) returns empty (still deduped)
```
