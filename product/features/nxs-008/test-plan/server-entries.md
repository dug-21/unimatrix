# Test Plan: server-entries (Wave 1)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-02 (24-Column Bind Params) | RT-13 |
| RISK-04 (entry_tags Consistency) | RT-34 |
| RISK-06 (Cross-Crate Compilation) | RT-38, RT-39, RT-40 |
| RISK-21 (MCP Tool Parity) | RT-76 to RT-85 |

## Integration Tests

### IT-srv-01: store_ops insert creates correct entry (RT-13)
```
Setup: Initialize StoreService with fresh Store
Action: Call store_ops insert (server path) with all fields
Action: Read back via Store::get()
Assert: All 24 fields match, tags present
```

### IT-srv-02: store_correct creates correction with tags (RT-34)
```
Setup: Insert original entry with tags=["original_tag"]
Action: Call store_correct with new content and tags=["corrected_tag"]
Assert:
  - Original entry: status=Deprecated, superseded_by=correction_id
  - Correction entry: supersedes=original_id, tags=["corrected_tag"]
  - entry_tags rows exist for correction entry
```

### IT-srv-03: store_ops and Store::insert produce equivalent entries (RT-40)
```
Setup: Insert entry via Store::insert(), insert entry via store_ops
Action: Read both back
Assert: Same column structure, same tag handling, same counter updates
```

### IT-srv-04: status.rs uses direct column query
```
Setup: Insert entries with various statuses
Action: Call status scan endpoint
Assert: Correct counts returned
Assert: No reference to status_index table
```

### IT-srv-05: contradiction.rs uses direct entries query
```
Setup: Insert Active entries with known content
Action: Call contradiction detection path
Assert: Returns correct active entries
Assert: Tags loaded for each entry
```

### IT-srv-06: Cross-crate build gate (RT-38)
```
Action: cargo build --workspace
Assert: Zero errors after Wave 1 changes to both store and server crates
```

### IT-srv-07: Cross-crate test gate (RT-39)
```
Action: cargo test --workspace
Assert: All tests pass after Wave 1 changes
```

## MCP Tool Parity Tests (Wave 5)

### IT-srv-08: context_search parity (RT-76)
```
Setup: Store entries with embeddings
Action: context_search with known query
Assert: Same result entries and ordering as pre-normalization
```

### IT-srv-09: context_lookup parity (RT-77)
```
Setup: Store entries with various topics/categories
Action: context_lookup with filters
Assert: Same entries returned
```

### IT-srv-10: context_get parity (RT-78)
```
Setup: Store entry with all fields
Action: context_get(id)
Assert: Identical EntryRecord fields including tags
```

### IT-srv-11: context_store parity (RT-79)
```
Action: context_store with known input
Assert: Created entry has correct fields
```

### IT-srv-12: context_correct parity (RT-80)
```
Action: context_correct on existing entry
Assert: Correction chain intact (supersedes/superseded_by)
```

### IT-srv-13: context_deprecate/quarantine parity (RT-81)
```
Action: Deprecate then quarantine entries
Assert: Status updates correct, counters accurate
```

### IT-srv-14: context_status parity (RT-82)
```
Action: context_status
Assert: Accurate counts and lambda value
```

### IT-srv-15: context_briefing parity (RT-83)
```
Action: context_briefing for known agent/role
Assert: Same entries returned
```

### IT-srv-16: context_enroll parity (RT-84)
```
Action: Enroll agent via MCP tool
Assert: Agent registered with correct capabilities
```

### IT-srv-17: context_retrospective parity (RT-85)
```
Action: Run retrospective detection
Assert: Same signals produced
```

## Import Pattern Verification

After Wave 1, server code imports should include:
```rust
use unimatrix_store::{
    Store, EntryRecord, NewEntry, Status, StoreError,
    compute_content_hash, status_counter_key,
    entry_from_row, load_tags_for_entries, apply_tags, ENTRY_COLUMNS,
    counters,
};
```

No references to: `serialize_entry`, `deserialize_entry`, `ENTRIES`, `TOPIC_INDEX`, `CATEGORY_INDEX`, `TAG_INDEX`, `TIME_INDEX`, `STATUS_INDEX`.
