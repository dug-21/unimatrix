# Pseudocode Overview: crt-002 Confidence Evolution

## Component Interaction

```
confidence-module (C1)
  |-- Pure functions: 6 component scores + Wilson + composite + rerank
  |-- No I/O, no state, depends only on EntryRecord + Status from unimatrix-store
  |
  v
store-confidence (C2)
  |-- record_usage_with_confidence(): extends record_usage with inline confidence
  |-- update_confidence(): targeted confidence-only write to ENTRIES
  |-- Accepts confidence_fn as dyn Fn pointer (no dependency on server crate)
  |
  v
server-retrieval-integration (C3)      server-mutation-integration (C4)      search-reranking (C5)
  |-- Modifies record_usage_for_entries  |-- Adds confidence to insert           |-- Re-ranks context_search
  |   to pass confidence_fn              |-- Adds confidence to correct           |   results by blended score
  |-- Fire-and-forget pattern            |-- Adds confidence to deprecate         |-- context_search only
  |                                      |-- Fire-and-forget on all paths         |
```

## Data Flow

### Retrieval Path
```
1. context_search handler receives query
2. HNSW search returns top-k candidates with similarity scores
3. Fetch full EntryRecords for candidates
4. RE-RANK by blended score: 0.85*similarity + 0.15*confidence  [C5]
5. Format response (shows confidence from PREVIOUS computation)
6. Fire-and-forget: record_usage_for_entries()  [C3]
   a. Dedup checks (existing crt-001 logic)
   b. spawn_blocking: record_usage_with_confidence()  [C2]
      - Update counters (existing)
      - For each entry: confidence = confidence_fn(entry, now)  [C1]
      - Write updated entry (same transaction)
   c. Feature entries (existing, unchanged)
```

### Insert Path
```
1. context_store handler inserts entry (existing flow)
2. After insert commit: [C4]
   a. Read back entry
   b. confidence = compute_confidence(entry, now)  [C1]
   c. update_confidence(id, confidence)  [C2]
```

### Correction Path
```
1. context_correct handler creates correction + deprecates original (existing flow)
2. After correction commit: [C4]
   a. compute_confidence(new_correction, now)  [C1]
   b. update_confidence(new_id, confidence)  [C2]
   c. Read deprecated original
   d. compute_confidence(deprecated_original, now)  [C1]
   e. update_confidence(original_id, confidence)  [C2]
```

### Deprecation Path
```
1. context_deprecate handler deprecates entry (existing flow)
2. After deprecation: [C4]
   a. compute_confidence(deprecated_entry, now)  [C1]
   b. update_confidence(entry_id, confidence)  [C2]
```

## Shared Types

No new types introduced. All functions use existing types:
- `EntryRecord` from `unimatrix_store::schema`
- `Status` from `unimatrix_store::schema`
- `f32` / `f64` for scores
- `u64` for timestamps and entry IDs

## Implementation Order

1. C1 (confidence-module) -- no dependencies, fully testable in isolation
2. C2 (store-confidence) -- depends on C1 only via function pointer at call site
3. C3 + C4 + C5 in parallel -- all depend on C1 and C2
