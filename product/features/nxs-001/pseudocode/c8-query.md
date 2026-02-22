# C8: Query Pseudocode

## Purpose

Combined `QueryFilter` multi-index intersection. Uses C7's internal `collect_ids_by_*` functions to gather ID sets per filter field, intersects them, then batch-fetches.

## Module: query.rs

### Store::query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>>

```
fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>>:
    let txn = self.db.begin_read()?

    // Determine effective filter (empty = all active)
    let effective_status = if filter.topic.is_none()
                           && filter.category.is_none()
                           && filter.tags.is_none()
                           && filter.status.is_none()
                           && filter.time_range.is_none():
        Some(Status::Active)   // default: all active entries
    else:
        filter.status

    // Collect ID sets for each present filter field
    let mut sets: Vec<HashSet<u64>> = Vec::new()

    if let Some(ref topic) = filter.topic:
        sets.push(read::collect_ids_by_topic(&txn, topic)?)

    if let Some(ref category) = filter.category:
        sets.push(read::collect_ids_by_category(&txn, category)?)

    if let Some(ref tags) = filter.tags:
        if !tags.is_empty():
            sets.push(read::collect_ids_by_tags(&txn, tags)?)

    if let Some(status) = effective_status:
        sets.push(read::collect_ids_by_status(&txn, status)?)

    if let Some(range) = filter.time_range:
        if range.start <= range.end:
            sets.push(read::collect_ids_by_time_range(&txn, range)?)

    // Intersect all sets
    if sets.is_empty():
        // No filters and no default status -- shouldn't happen with effective_status logic
        // But handle gracefully: return all active
        let ids = read::collect_ids_by_status(&txn, Status::Active)?
        return read::fetch_entries(&txn, &ids)

    let mut result_ids = sets.remove(0)
    for set in sets:
        result_ids = result_ids.intersection(&set).copied().collect()

    // Batch fetch
    read::fetch_entries(&txn, &result_ids)
```

## Integration with C7

C8 depends on C7's internal `collect_ids_by_*` functions and `fetch_entries`. These are `pub(crate)` in read.rs. The query function creates its own ReadTransaction and passes it to C7's helpers.

## Error Handling

- All errors from index queries and fetch propagate via `?`
- Empty result is Ok(vec![]), not an error
- Inverted time range treated as empty filter contribution

## Key Test Scenarios

- AC-17: Multi-field filter returns intersection
- AC-17: Empty QueryFilter returns all active entries
- AC-17: Disjoint filters return empty
- R7: All field combinations (single, double, triple, all five)
- R7: 50 entries, varied fields, verify correct subsets
- Property tests: random entries + random filters vs brute-force filter
