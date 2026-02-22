# C7: Read Test Plan

## R1/AC-06: Point Lookup

### test_get_returns_inserted_entry
- Insert entry, get by ID, all fields match

### test_get_nonexistent_returns_error
- get(999) -> StoreError::EntryNotFound(999)

## AC-07: Topic Index Query

### test_query_by_topic_returns_matching
- Insert 5 entries across 3 topics
- query_by_topic("auth") returns exactly entries with topic "auth"

### test_query_by_topic_nonexistent
- query_by_topic("nonexistent") returns empty vec

## AC-08: Category Index Query

### test_query_by_category_returns_matching
- Insert entries across categories
- query_by_category("convention") returns correct set

### test_query_by_category_nonexistent
- Returns empty vec

## R9/AC-09: Tag Intersection

### test_query_single_tag
- Returns all entries with that tag

### test_query_two_tag_intersection
- Insert entries with overlapping tags
- query_by_tags(["rust", "error"]) returns only entries with BOTH

### test_query_three_tag_intersection
- Only 1 entry matches all three tags

### test_query_nonexistent_tag
- Returns empty vec

### test_query_empty_tags
- query_by_tags([]) returns empty vec

## AC-10: Time Range Query

### test_time_range_inclusive
- Insert at timestamps 1000, 2000, 3000, 4000, 5000
- Range 2000..=4000 returns exactly 3

### test_time_range_single_point
- Range where start == end, returns entries at that exact timestamp

### test_time_range_inverted
- start > end returns empty vec

### test_time_range_empty
- Range with no entries returns empty vec

## AC-11: Status Query

### test_query_by_status_active
- Insert Active + Deprecated entries
- query_by_status(Active) returns only Active entries

### test_query_by_status_deprecated
- Returns only Deprecated entries

## AC-13: Vector Mapping Lookup

### test_get_vector_mapping_exists
- After put_vector_mapping, get returns correct value

### test_get_vector_mapping_missing
- Returns None

## Exists Check

### test_exists_true
- Insert entry, exists(id) -> true

### test_exists_false
- exists(999) -> false

## Counter Read

### test_read_counter_via_store
- Insert entries, read_counter("total_active") returns correct count
