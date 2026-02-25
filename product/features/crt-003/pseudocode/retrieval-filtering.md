# Pseudocode: C2 Retrieval Filtering

## File: crates/unimatrix-server/src/tools.rs

### context_search modifications

```
// After HNSW search returns candidates and metadata filtering is applied:
// Current: results are filtered by topic, category, tags
// Add: filter out Quarantined entries

fn context_search(params):
    // ... existing identity, capability, validation, embed ...
    // ... search HNSW ...
    // ... metadata filtering (topic, category, tags) ...

    // NEW: exclude quarantined entries from results
    results.retain(|entry| entry.status != Status::Quarantined)

    // ... usage tracking, confidence update, format response ...
```

### context_lookup modifications

```
// QueryFilter already defaults to Some(Status::Active) when no status param provided.
// This naturally excludes Quarantined entries.
// No code change needed for default behavior.
// parse_status() (C1) handles "quarantined" as a valid value for explicit queries.
```

### context_briefing modifications

```
// Briefing internally calls lookup (defaults to Active) and search.
// Lookup: already filtered by Active default.
// Search: gains the new Quarantined filter from context_search changes.
// No additional changes needed in the briefing handler itself.
```

### context_get modifications

```
// No changes. Direct ID access returns any entry regardless of status.
```

### context_correct modifications

```
// Current: rejects if original.status == Deprecated
// Add: also reject if original.status == Quarantined

fn context_correct(params):
    // ... fetch original entry ...
    if original.status == Status::Deprecated:
        return Err("cannot correct deprecated entry")
    if original.status == Status::Quarantined:     // NEW
        return Err("cannot correct quarantined entry; restore first")
    // ... proceed with correction ...
```
