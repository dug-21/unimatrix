# Pseudocode: write-security

## Purpose
Update Store::insert() and Store::update() to handle security fields.

## Modified File: crates/unimatrix-store/src/write.rs

### insert() Changes

Replace the EntryRecord construction:
```
let record = EntryRecord {
    id,
    title: entry.title,
    content: entry.content,
    topic: entry.topic,
    category: entry.category,
    tags: entry.tags,
    source: entry.source,
    status: entry.status,
    confidence: 0.0,
    created_at: now,
    updated_at: now,
    last_accessed_at: 0,
    access_count: 0,
    supersedes: None,
    superseded_by: None,
    correction_count: 0,
    embedding_dim: 0,
    // NEW: security fields
    created_by: entry.created_by.clone(),
    modified_by: entry.created_by,  // on insert, modifier = creator
    content_hash: compute_content_hash(&entry_title_ref, &entry_content_ref),
    previous_hash: String::new(),   // no previous on first insert
    version: 1,                     // first version
    feature_cycle: entry.feature_cycle,
    trust_source: entry.trust_source,
};
```

Note: Must compute content_hash BEFORE moving entry fields. Either clone title/content or compute hash first.

Recommended approach:
```
let content_hash = hash::compute_content_hash(&entry.title, &entry.content);
// Then move entry fields into record
```

### update() Changes

After reading old record, before writing updated record:
```
// Compute hash chain
let new_hash = hash::compute_content_hash(&updated.title, &updated.content);
updated.previous_hash = old.content_hash;
updated.content_hash = new_hash;

// Increment version
updated.version = old.version + 1;

// updated_at is already set to current_unix_timestamp_secs()
```

Note: `modified_by` is NOT set by the engine. The caller must set it on the EntryRecord before calling update(). The engine only sets engine-computed fields (content_hash, previous_hash, version, updated_at).

### update_status() -- NO CHANGES

update_status() does NOT increment version. It only changes status, STATUS_INDEX, counters, and updated_at. No content change means no hash change or version bump.

## Error Handling
No new error paths. compute_content_hash is infallible.

## Key Test Scenarios
- Insert: content_hash matches sha256("{title}: {content}")
- Insert: version = 1
- Insert: modified_by = created_by
- Insert: previous_hash = ""
- Update: previous_hash = old content_hash
- Update: new content_hash computed from updated title/content
- Update: version = old.version + 1
- Update: content change -> hash changes
- Update: no content change -> hash unchanged, version still increments
- update_status: version unchanged
- update_status: content_hash unchanged
