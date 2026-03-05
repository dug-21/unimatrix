# Test Plan: schema-ddl (Wave 1)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-02 (24-Column Bind Params) | RT-17 |
| RISK-04 (entry_tags Consistency) | RT-28, RT-31, RT-32, RT-33 |
| RISK-09 (PRAGMA FK Side Effects) | RT-51, RT-52 |

## Integration Tests

### IT-ddl-01: entries table has 24 columns (AC-01)
```
Setup: Fresh Store::open
Action: PRAGMA table_info(entries)
Assert: 25 rows (id + 24 data columns), no "data BLOB" column
  Columns: id, title, content, topic, category, source, status, confidence,
  created_at, updated_at, last_accessed_at, access_count, supersedes,
  superseded_by, correction_count, embedding_dim, created_by, modified_by,
  content_hash, previous_hash, version, feature_cycle, trust_source,
  helpful_count, unhelpful_count
```

### IT-ddl-02: entry_tags table structure (AC-02)
```
Setup: Fresh Store::open
Action: PRAGMA table_info(entry_tags)
Assert: 2 columns (entry_id INTEGER, tag TEXT)
Action: PRAGMA foreign_key_list(entry_tags)
Assert: FK to entries(id) with ON DELETE CASCADE
```

### IT-ddl-03: Index tables eliminated (AC-03)
```
Setup: Fresh Store::open
Action: SELECT name FROM sqlite_master WHERE type='table'
Assert: No topic_index, category_index, tag_index, time_index, status_index
```

### IT-ddl-04: SQL indexes exist (AC-04)
```
Setup: Fresh Store::open
Action: SELECT name FROM sqlite_master WHERE type='index'
Assert: Includes idx_entries_topic, idx_entries_category, idx_entries_status,
  idx_entries_created_at, idx_entry_tags_tag, idx_entry_tags_entry_id
```

### IT-ddl-05: PRAGMA foreign_keys is ON (RT-32)
```
Setup: Fresh Store::open
Action: PRAGMA foreign_keys
Assert: Returns 1
```

### IT-ddl-06: FK CASCADE works (RT-51)
```
Setup: Insert entry with tags
Action: Delete entry from entries table
Assert: entry_tags rows for that entry_id are gone (CASCADE)
Assert: No FK violation errors
```

### IT-ddl-07: vector_map unaffected by FK (RT-52)
```
Setup: Insert entry, insert vector_map row
Action: Delete vector_map row manually
Assert: No FK constraint error (vector_map has no FK)
```

### IT-ddl-08: entry_from_row uses column names not positions (RT-17)
```
Verification: Static analysis
Action: grep for `get::<_, T>(n)` (positional) in entry_from_row
Assert: Zero positional gets; all use `get::<_, T>("column_name")` or `get("column_name")`
```

### IT-ddl-09: entry with 0 tags (RT-31)
```
Setup: Insert entry with empty tags vec
Action: Store::get()
Assert: entry.tags == vec![]
```

### IT-ddl-10: Tags populated in query results (RT-33)
```
Setup: Insert entry with tags=["a", "b"]
Action: query_by_tags(["a"])
Assert: Returned entry has tags=["a", "b"] (full tag set, not just queried tags)
```

### IT-ddl-11: Fresh database at v6 schema (RT-75)
```
Setup: Create brand new Store (no pre-existing DB)
Action: Check schema_version
Assert: schema_version = 6, all v6 tables present
```

## Helper Functions

### entry_from_row
```
Signature: fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<EntryRecord>
- Reads all 24 columns by name
- Sets tags = vec![] (populated separately by load_tags_for_entries)
- Handles NULL for Option fields (supersedes, superseded_by, embedding_dim, previous_hash, ended_at)
```

### load_tags_for_entries
```
Signature: fn load_tags_for_entries(conn: &Connection, ids: &[u64]) -> Result<HashMap<u64, Vec<String>>>
- Batch query: SELECT entry_id, tag FROM entry_tags WHERE entry_id IN (...)
- Returns map of entry_id -> Vec<tag>
- Empty ids -> empty map (no query)
```

### apply_tags
```
Signature: fn apply_tags(entries: &mut [EntryRecord], tag_map: &HashMap<u64, Vec<String>>)
- For each entry, set entry.tags from tag_map lookup
```

### ENTRY_COLUMNS
```
Constant: &str listing all 24 column names for SELECT queries
```
