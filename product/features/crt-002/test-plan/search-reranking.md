# Test Plan: search-reranking (C5)

## Location: `crates/unimatrix-server/src/confidence.rs` (unit) + integration tests

### T-29: rerank_score arithmetic (AC-13, AC-14, R-04)
```
// Already covered in T-11 (confidence-module test plan)
// Included here for completeness of component test plan.

// Similarity dominant: high similarity always beats low similarity
assert rerank_score(0.95, 0.0) > rerank_score(0.70, 1.0)

// Confidence tiebreaker: equal similarity, different confidence
assert rerank_score(0.90, 0.80) > rerank_score(0.90, 0.20)
```

### T-30: context_search re-ranks results (AC-13, R-04)
```
// Setup: Insert two entries with different confidence levels
// Entry A: high confidence (many accesses, helpful votes)
// Entry B: low confidence (never accessed, no votes)
// Both entries have similar content (close embeddings)

// Perform context_search with a query matching both
// Verify that if similarity scores are close, the higher-confidence
// entry appears first in results

// Note: This requires entries with similar embeddings but different
// confidence. Practically, this may need controlled setup.
```

### T-31: context_lookup not re-ranked (ADR-005)
```
// Verify context_lookup returns results in the same order as before crt-002
// Specifically: results are ordered by the deterministic index scan,
// not by confidence or similarity
```

### T-32: context_get unaffected (ADR-005)
```
// Verify context_get returns a single entry with computed confidence
// but no ranking change (there is only one entry)
```

### T-33: Displayed similarity is original, not blended (IR-03)
```
// After re-ranking, the similarity scores in the response should be
// the original vector similarities, NOT the blended scores.
// This is verified by checking that context_search JSON response
// "similarity" field matches what the HNSW index returned.
```

### T-34: Empty search results (EC-05)
```
// context_search with no matching results
// Verify: no error, no re-ranking, empty response
```

### T-35: Single result search (EC-04)
```
// context_search returns exactly 1 result
// Re-ranking is a no-op (single element sort)
// Verify: result returned correctly, confidence displayed
```
