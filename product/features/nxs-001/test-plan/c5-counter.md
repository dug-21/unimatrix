# C5: Counter Test Plan

## R5/AC-05: Monotonic ID Generation

### test_first_id_is_one
- Open fresh database
- Insert one entry
- Assert returned ID == 1

### test_100_sequential_inserts_monotonic
- Insert 100 entries sequentially
- Assert each ID > previous ID
- Assert first ID == 1, last ID == 100

### test_counter_matches_last_id
- Insert N entries
- Read "next_entry_id" counter
- Assert counter == last_assigned_id + 1

## Counter Read

### test_read_counter_missing_key
- Read a counter key that doesn't exist
- Assert returns 0

### test_read_counter_after_inserts
- Insert entries with Status::Active
- Read "total_active" counter
- Assert matches count of inserted active entries
