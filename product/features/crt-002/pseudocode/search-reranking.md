# Pseudocode: search-reranking (C5)

## File: `crates/unimatrix-server/src/tools.rs`

### Modified: context_search handler

Insert a re-ranking step between fetching full entries (step 9) and formatting the response (step 10):

```
// Step 9: Fetch full entries for results (EXISTING, unchanged)
let mut results_with_scores: Vec<(EntryRecord, f32)> = Vec::new();
for sr in &search_results {
    match self.entry_store.get(sr.entry_id).await {
        Ok(entry) => results_with_scores.push((entry, sr.similarity)),
        Err(_) => continue,
    }
}

// Step 9b: Re-rank by blended score (NEW)
results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
    let score_a = confidence::rerank_score(*sim_a, entry_a.confidence);
    let score_b = confidence::rerank_score(*sim_b, entry_b.confidence);
    // Descending order: higher score first
    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
});

// Step 10: Format response (EXISTING, unchanged)
let result = format_search_results(&results_with_scores, format);
```

### Scope: context_search ONLY

Re-ranking applies ONLY to the `context_search` handler. The following handlers are NOT modified:
- `context_lookup` -- deterministic, no similarity scores
- `context_get` -- single entry by ID
- `context_briefing` -- its internal search component calls context_search logic, so re-ranking is inherited naturally for the search portion

### Import Addition

Add to tools.rs imports:
```
use crate::confidence;
```

### Behavioral Change

- Search results may appear in different order compared to pre-crt-002
- The `similarity` score displayed in responses is the ORIGINAL vector similarity, NOT the blended score
- The blended score is used for ordering only, not for display

## Dependencies

- `crate::confidence::rerank_score` (from C1)
- No other new dependencies
