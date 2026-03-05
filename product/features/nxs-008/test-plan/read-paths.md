# Test Plan: read-paths (Wave 1)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-03 (SQL Query Semantics) | RT-18 to RT-27 |
| RISK-10 (co_access Staleness) | RT-53 to RT-55 |
| RISK-17 (Time Index Shift) | RT-71 |
| RISK-18 (N+1 Elimination) | RT-72 |

## Integration Tests — Query Semantic Parity

### IT-read-01: Tag AND semantics (RT-18)
```
Setup: Insert entries:
  - E1: tags=["A", "B"]
  - E2: tags=["A", "C"]
  - E3: tags=["A"]
  - E4: tags=["B"]
Action: query_by_tags(["A", "B"])
Assert: Returns only E1 (has both A and B)
```

### IT-read-02: Tag superset matching (RT-19)
```
Setup: Insert entry with tags=["A", "B", "C"]
Action: query_by_tags(["A"])
Assert: Entry returned (superset of query tags is OK)
```

### IT-read-03: Empty filter defaults to Active (RT-20)
```
Setup: Insert 2 Active entries, 1 Deprecated entry
Action: query(QueryFilter::default()) — all fields None
Assert: Returns only the 2 Active entries
```

### IT-read-04: Empty tags bypass (RT-21)
```
Setup: Insert 2 Active entries (one with tags, one without)
Action: query(QueryFilter { tags: Some(vec![]), ..default() })
Assert: Returns both Active entries (tag filter skipped)
```

### IT-read-05: Invalid time range returns empty (RT-22)
```
Setup: Insert entries
Action: query_by_time_range(TimeRange { start: 2000, end: 1000 })
Assert: Returns empty vec (start > end)
```

### IT-read-06: Single-point time range (RT-23)
```
Setup: Insert entry with created_at=1500
Action: query_by_time_range(TimeRange { start: 1500, end: 1500 })
Assert: Returns the entry (inclusive both ends)
```

### IT-read-07: Multi-filter AND (RT-24)
```
Setup: Insert entries with various combinations of topic, category, status, tags, time
Action: query(QueryFilter { topic: Some("t1"), category: Some("c1"), status: Some(Active), tags: Some(vec!["tag1"]), time_range: Some(range) })
Assert: Only entries matching ALL filters returned
```

### IT-read-08: query_by_status for each variant (RT-25)
```
Setup: Insert entries with Status::Active, Deprecated, Quarantined
Action: query_by_status(Active), query_by_status(Deprecated), query_by_status(Quarantined)
Assert: Each returns correct subset
```

### IT-read-09: Zero-tag entries in non-tag queries (RT-26)
```
Setup: Insert entry with tags=[] (no tags)
Action: query_by_topic("some_topic")
Assert: Entry appears in results (no INNER JOIN exclusion on entry_tags)
```

### IT-read-10: Time range filters on created_at (RT-71)
```
Setup: Insert entry with created_at=1000, updated_at=2000
Action: query_by_time_range(TimeRange { start: 1500, end: 2500 })
Assert: Entry NOT returned (created_at=1000 is outside range)
Action: query_by_time_range(TimeRange { start: 500, end: 1500 })
Assert: Entry IS returned (created_at=1000 is in range)
```

### IT-read-11: query_by_topic returns entries with tags (batch load)
```
Setup: Insert entry with topic="test_topic", tags=["x", "y"]
Action: query_by_topic("test_topic")
Assert: Returned entry has tags=["x", "y"] (load_tags_for_entries called)
```

### IT-read-12: query_by_category returns correct entries
```
Setup: Insert entries in categories "cat1" and "cat2"
Action: query_by_category("cat1")
Assert: Only cat1 entries returned
```

## Integration Tests — Co-Access Reads (Wave 2)

### IT-read-13: get_co_access_partners staleness filter (RT-53)
```
Setup: Record co-access pairs at various timestamps
Action: get_co_access_partners(entry_id, staleness_cutoff)
Assert: Only pairs with last_updated >= cutoff returned
```

### IT-read-14: co_access_stats (RT-54)
```
Setup: 5 co-access pairs, 3 fresh, 2 stale
Action: co_access_stats(staleness_cutoff)
Assert: total=5, active=3
```

### IT-read-15: top_co_access_pairs ordering (RT-55)
```
Setup: Co-access pairs with counts 10, 5, 20, 1
Action: top_co_access_pairs(limit=2, staleness_cutoff)
Assert: Returns pairs with counts [20, 10] (descending order)
```

## Code Review Verification

### CR-read-01: No HashSet intersection (AC-11)
```
Action: grep "HashSet" in read.rs
Assert: Zero uses of HashSet for ID intersection
```

### CR-read-02: No N+1 fetch loop (AC-12)
```
Action: grep "fetch_entries\|collect_ids_by" in read.rs
Assert: Zero hits — all replaced by SQL WHERE
```

### CR-read-03: entry_from_row used everywhere
```
Action: Verify all query methods use entry_from_row for EntryRecord construction
Assert: No manual field extraction from rows outside entry_from_row
```
