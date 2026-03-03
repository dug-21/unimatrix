# Test Plan: SearchService

## Unit Tests

### TS-01: SearchService result equivalence (AC-01, R-01)
- Setup: Seed store with 10+ entries spanning multiple topics, with varying confidence levels
- Action: Run search through SearchService with specific query
- Verify: Result IDs, ordering, and scores match the expected ranking behavior
- Verify: Provenance boost applied for lesson-learned entries
- Verify: Co-access boost computed correctly
- Note: This is a comparison test against known expected ordering, not against the old inline path (since both are being modified simultaneously)

### TS-02: SearchService with floors
- Setup: Seed store with entries at varying similarity/confidence levels
- Action: Search with `similarity_floor=0.7` and `confidence_floor=0.5`
- Verify: Only entries meeting both floors appear in results
- Verify: Entries below either floor are excluded

### TS-03: SearchService with filters
- Setup: Seed store with entries in different topics and categories
- Action: Search with `filters = Some(QueryFilter { topic: "rust", ... })`
- Verify: Only entries matching filter appear in results
- Verify: Correct use of `search_filtered` vs `search`

### TS-03b: SearchService returns query_embedding
- Action: Any search
- Verify: `SearchResults.query_embedding` is a non-empty Vec<f32>
- Verify: Embedding is L2-normalized (magnitude ~1.0)

## Integration Tests

### TS-22: Quarantine exclusion in SearchService (AC-09, R-12)
- Setup: Create store, insert entry, quarantine it
- Action: Search with query matching the quarantined entry
- Verify: Quarantined entry never appears in results
- Verify: Active entries with similar content still appear

### TS-22b: Empty store search
- Setup: Empty store
- Action: Search with any query
- Verify: Returns empty SearchResults, no error

### TS-22c: All results quarantined
- Setup: Store where all matching entries are quarantined
- Action: Search
- Verify: Returns empty results, no error
