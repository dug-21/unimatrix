# Test Plan: C1 -- Co-Access Storage

## Location

`crates/unimatrix-store/src/schema.rs` (unit tests)
`crates/unimatrix-store/tests/` (integration tests)

## Risk Coverage

- R-09: CoAccessRecord bincode serialization mismatch
- R-03: Full table scan for partner lookup latency
- R-07: Stale pair cleanup removes valuable patterns

## Test Scenarios

### T-C1-01: CoAccessRecord serialization roundtrip (R-09)

```
Given a CoAccessRecord with count=5, last_updated=1000
When serialized then deserialized
Then the result equals the original record
```

### T-C1-02: Serialization boundary -- count=0 (R-09)

```
Given a CoAccessRecord with count=0, last_updated=0
When roundtripped through serialize/deserialize
Then the result equals the original
```

### T-C1-03: Serialization boundary -- max values (R-09)

```
Given a CoAccessRecord with count=u32::MAX, last_updated=u64::MAX
When roundtripped through serialize/deserialize
Then the result equals the original
```

### T-C1-04: co_access_key ordering

```
Given co_access_key(10, 5) => (5, 10)
And co_access_key(5, 10) => (5, 10)
And co_access_key(5, 5) => (5, 5)
Then keys are always ordered (min, max)
```

### T-C1-05: CO_ACCESS table created on Store::open

```
Given a new Store opened with open_with_config
When we write a co-access record
Then no table-not-found error
```

### T-C1-06: record_co_access -- basic write and read

```
Given entry IDs [1, 2, 3]
When record_co_access(entry_ids, 10)
Then pairs (1,2), (1,3), (2,3) exist in CO_ACCESS table
And each has count=1
```

### T-C1-07: record_co_access -- increment existing

```
Given pair (1, 2) with count=3
When record_co_access([1, 2], 10) is called
Then pair (1, 2) has count=4
And last_updated is updated
```

### T-C1-08: record_co_access -- count saturating_add (R-09)

```
Given pair (1, 2) with count=u32::MAX
When record_co_access is called for the same pair
Then count remains u32::MAX (saturating)
```

### T-C1-09: record_co_access_pairs -- pre-computed pairs

```
Given pairs [(1,2), (3,4)]
When record_co_access_pairs is called
Then both pairs exist with count=1
```

### T-C1-10: record_co_access_pairs -- empty input

```
Given an empty pairs slice
When record_co_access_pairs is called
Then returns Ok(()) with no transaction
```

### T-C1-11: get_co_access_partners -- entry as min (R-03)

```
Given pairs (5,10) count=3 and (5,20) count=1
When get_co_access_partners(5, staleness_cutoff=0)
Then returns [(10, {count:3}), (20, {count:1})]
```

### T-C1-12: get_co_access_partners -- entry as max (R-03)

```
Given pairs (1,10) count=2 and (5,10) count=3
When get_co_access_partners(10, staleness_cutoff=0)
Then returns [(1, {count:2}), (5, {count:3})]
```

### T-C1-13: get_co_access_partners -- staleness filter (R-07)

```
Given pair (1,2) with last_updated=1000
When get_co_access_partners(1, staleness_cutoff=2000)
Then returns empty (pair is stale)
```

### T-C1-14: get_co_access_partners -- no partners (R-03)

```
Given an empty CO_ACCESS table
When get_co_access_partners(1, staleness_cutoff=0)
Then returns empty vec
```

### T-C1-15: cleanup_stale_co_access -- removes stale pairs (R-07)

```
Given pair (1,2) with last_updated=1000
And pair (3,4) with last_updated=5000
When cleanup_stale_co_access(cutoff=3000)
Then pair (1,2) is removed
And pair (3,4) is preserved
And returns removed_count=1
```

### T-C1-16: cleanup_stale_co_access -- boundary at cutoff (R-07)

```
Given pair (1,2) with last_updated=3000
When cleanup_stale_co_access(cutoff=3000)
Then pair (1,2) is removed (< cutoff means strictly less than)
```

### T-C1-17: co_access_stats (R-03)

```
Given 3 pairs: (1,2) fresh, (3,4) fresh, (5,6) stale
When co_access_stats(staleness_cutoff)
Then total=3, active=2
```

### T-C1-18: top_co_access_pairs -- ordering and limit

```
Given 5 pairs with different counts
When top_co_access_pairs(3, staleness_cutoff)
Then returns top 3 pairs ordered by count descending
And stale pairs excluded
```
