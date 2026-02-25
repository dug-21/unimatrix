# Test Plan: C4 -- Co-Access Boost

## Location

`crates/unimatrix-server/src/coaccess.rs` (unit tests for boost computation)
`crates/unimatrix-store/tests/` (integration tests with real store)

## Risk Coverage

- R-02: Co-access feedback loop
- R-06: Co-access boost overrides similarity for dissimilar entries

## Test Scenarios

### T-C4-01: Boost at count=0

```
Given co_access_boost(0, MAX_CO_ACCESS_BOOST)
Then returns 0.0
```

### T-C4-02: Boost at count=1 (R-02)

```
Given co_access_boost(1, 0.03)
Then returns approximately 0.007
  (ln(2)/ln(21) * 0.03 ~= 0.00682)
```

### T-C4-03: Boost at count=20 -- cap reached (R-02)

```
Given co_access_boost(20, 0.03)
Then returns 0.03 (max)
  (ln(21)/ln(21) = 1.0, capped at 1.0, * 0.03 = 0.03)
```

### T-C4-04: Boost at count=100 -- capped same as 20 (R-02)

```
Given co_access_boost(100, 0.03)
Then returns 0.03 (same as count=20, capped)
```

### T-C4-05: Boost at count=u32::MAX -- no overflow (R-02)

```
Given co_access_boost(u32::MAX, 0.03)
Then returns 0.03 (capped, no panic or overflow)
```

### T-C4-06: Boost diminishing returns -- count=10 vs count=20 (R-02)

```
Given boost_10 = co_access_boost(10, 0.03) ~= 0.024
And boost_20 = co_access_boost(20, 0.03) = 0.030
Then boost_20 - boost_10 < boost_10 - co_access_boost(0, 0.03)
(diminishing returns verified)
```

### T-C4-07: Briefing boost uses smaller max (R-13)

```
Given co_access_boost(20, MAX_BRIEFING_CO_ACCESS_BOOST=0.01)
Then returns 0.01 (not 0.03)
```

### T-C4-08: compute_search_boost -- basic scenario

```
Given:
  - anchor_ids = [1]
  - result_ids = [1, 2, 3]
  - CO_ACCESS: (1,2) count=5, (1,3) count=1
When compute_search_boost(anchors, results, store, staleness_cutoff)
Then boost_map contains:
  - 2 -> ~0.018
  - 3 -> ~0.007
  - 1 NOT in map (anchor not boosted by itself)
```

### T-C4-09: compute_search_boost -- multiple anchors, max wins

```
Given:
  - anchor_ids = [1, 2]
  - result_ids = [1, 2, 3]
  - CO_ACCESS: (1,3) count=2, (2,3) count=10
When compute_search_boost
Then boost_map[3] == co_access_boost(10, 0.03) (max of the two anchors)
Not co_access_boost(2, 0.03) + co_access_boost(10, 0.03)
```

### T-C4-10: compute_search_boost -- no co-access data

```
Given empty CO_ACCESS table
When compute_search_boost(anchors, results, store, staleness)
Then returns empty HashMap
```

### T-C4-11: compute_search_boost -- stale pairs excluded

```
Given CO_ACCESS: (1,2) count=5 with last_updated=1000
When compute_search_boost with staleness_cutoff=2000
Then boost_map is empty (pair is stale)
```

### T-C4-12: Similarity dominance -- high similarity beats max boost (R-06)

```
Given:
  - Entry A: similarity=0.95, no co-access boost
  - Entry B: similarity=0.85, max co-access boost=0.03
Then:
  - score_A = rerank_score(0.95, confidence_A) = 0.85*0.95 + 0.15*conf
  - score_B = rerank_score(0.85, confidence_B) + 0.03
  - score_A > score_B (similarity gap of 0.10 > boost of 0.03)
```

### T-C4-13: Tiebreaker behavior -- co-access breaks ties (R-06)

```
Given:
  - Entry A: similarity=0.90, confidence=0.5, co-access boost=0.02
  - Entry B: similarity=0.90, confidence=0.5, co-access boost=0.00
Then:
  - score_A = rerank_score(0.90, 0.5) + 0.02
  - score_B = rerank_score(0.90, 0.5) + 0.00
  - score_A > score_B
```

### T-C4-14: Anchor selection -- only top N results are anchors (R-06)

```
Given results ranked [E1, E2, E3, E4, E5] with anchor_count=3
When compute_search_boost is called
Then only E1, E2, E3 are used as anchors
E4 and E5 are not used as anchors (but may receive boost)
```
