# C5: Filter Module -- Test Plan

## Tests

```
test_filter_accepts_allowed_id:
    filter = EntryIdFilter::new(vec![1, 5, 10])
    ASSERT filter.hnsw_filter(&5) == true
    ASSERT filter.hnsw_filter(&1) == true
    ASSERT filter.hnsw_filter(&10) == true

test_filter_rejects_disallowed_id:
    filter = EntryIdFilter::new(vec![1, 5, 10])
    ASSERT filter.hnsw_filter(&2) == false
    ASSERT filter.hnsw_filter(&0) == false
    ASSERT filter.hnsw_filter(&100) == false

test_filter_empty_rejects_all:
    filter = EntryIdFilter::new(vec![])
    ASSERT filter.hnsw_filter(&0) == false
    ASSERT filter.hnsw_filter(&1) == false

test_filter_single_element:
    filter = EntryIdFilter::new(vec![42])
    ASSERT filter.hnsw_filter(&42) == true
    ASSERT filter.hnsw_filter(&41) == false
    ASSERT filter.hnsw_filter(&43) == false

test_filter_unsorted_input:
    // Constructor should sort internally
    filter = EntryIdFilter::new(vec![10, 1, 5])
    ASSERT filter.hnsw_filter(&1) == true
    ASSERT filter.hnsw_filter(&5) == true
    ASSERT filter.hnsw_filter(&10) == true

test_filter_duplicates:
    // Constructor should deduplicate
    filter = EntryIdFilter::new(vec![5, 5, 5, 1, 1])
    ASSERT filter.hnsw_filter(&5) == true
    ASSERT filter.hnsw_filter(&1) == true
    ASSERT filter.hnsw_filter(&2) == false
```

## Risks Covered
- R-03 (Filtered search correctness): Filter unit tests ensure the FilterT implementation is correct in isolation. Integration tests in C4 verify end-to-end filter behavior.
