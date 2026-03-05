# Test Plan: write-paths (Wave 1)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-02 (24-Column Bind Params) | RT-11, RT-12, RT-14, RT-15, RT-16 |
| RISK-04 (entry_tags Consistency) | RT-28, RT-29, RT-30 |

## Integration Tests

### IT-write-01: 24-field round-trip via Store API (RT-11, RT-12)
```
Setup: Fresh Store
Action: Insert EntryRecord with ALL 24 fields set to distinct non-default values:
  id=auto, title="unique_title", content="unique_content", topic="unique_topic",
  category="unique_cat", source="unique_src", status=Active, confidence=0.87,
  created_at=1234567890, updated_at=1234567891, last_accessed_at=1234567892,
  access_count=42, supersedes=Some(999), superseded_by=None,
  correction_count=3, embedding_dim=Some(384), created_by="agent_x",
  modified_by="agent_y", content_hash="hash_abc", previous_hash=Some("prev_hash"),
  version=5, feature_cycle="nxs-test", trust_source="system",
  helpful_count=10, unhelpful_count=2, tags=["alpha", "beta"]
Action: Store::get(id)
Assert: Every field matches exactly (field-by-field comparison)
```

### IT-write-02: Update every field (RT-14)
```
Setup: Insert entry with known values
Action: Store::update() changing every mutable field:
  - title, content, topic, category, source, confidence, tags
  - updated_at, last_accessed_at, access_count
  - status (Active -> Deprecated), superseded_by=Some(new_id)
Action: Store::get()
Assert: All changed fields reflect updates, unchanged fields preserved
```

### IT-write-03: Update tags replaces correctly (RT-30)
```
Setup: Insert entry with tags=["a", "b", "c"]
Action: Update to tags=["b", "d"]
Assert: get() returns tags=["b", "d"] (not ["a", "b", "c", "b", "d"])
```

### IT-write-04: Delete with CASCADE (RT-29)
```
Setup: Insert entry with tags=["x", "y"]
Action: Store::delete(id)
Assert:
  - Entry gone from entries table
  - entry_tags rows for this id gone (CASCADE)
  - No orphan rows in entry_tags
```

### IT-write-05: Insert entry with tags (RT-28)
```
Setup: Fresh Store
Action: Insert entry with tags=["tag1", "tag2", "tag3"]
Assert: get() returns entry with tags=["tag1", "tag2", "tag3"]
Assert: SELECT COUNT(*) FROM entry_tags WHERE entry_id=? returns 3
```

### IT-write-06: u64 boundary handling (RT-16)
```
Setup: Fresh Store
Action: Insert entry with created_at = u64::MAX / 2 (max safe i64)
Assert: Round-trip preserves value
Action: Verify behavior with values near i64::MAX boundary
Assert: No silent truncation or overflow panic
```

### IT-write-07: named_params used in all INSERT/UPDATE (RT-15)
```
Verification: Static analysis
Action: grep -n "named_params" in write.rs
Assert: All INSERT and UPDATE for entries table use named_params!{}
Action: grep for positional params (?1, ?2...) in entries INSERT/UPDATE
Assert: Zero positional params for entries table operations
```

### IT-write-08: update_status single column UPDATE
```
Setup: Insert Active entry
Action: update_status(id, Status::Deprecated)
Assert:
  - get() returns status=Deprecated
  - Other fields unchanged
  - Status counter decremented for Active, incremented for Deprecated
```

### IT-write-09: write_ext record_usage_with_confidence (direct SQL)
```
Setup: Insert entry
Action: record_usage_with_confidence(id, ...)
Assert:
  - access_count incremented
  - last_accessed_at updated
  - confidence updated
  - No bincode serialize/deserialize
```

### IT-write-10: write_ext record_co_access_pairs (SQL columns)
```
Setup: Insert 3 entries
Action: record_co_access_pairs([1, 2, 3])
Assert:
  - co_access table has 3 rows (1-2, 1-3, 2-3)
  - count=1, last_updated set correctly
Action: Record again
Assert: count incremented to 2
```

### IT-write-11: write_ext cleanup_stale_co_access (SQL WHERE)
```
Setup: Record co-access pairs at time T
Action: cleanup_stale_co_access(T + 1000)
Assert: All pairs deleted (last_updated < cutoff)
Action: Record new pairs, cleanup with future cutoff
Assert: Only stale pairs deleted, fresh pairs retained
```
