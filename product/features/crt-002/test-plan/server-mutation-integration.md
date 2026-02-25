# Test Plan: server-mutation-integration (C4)

## Location: `crates/unimatrix-server/` integration tests (or inline)

### T-24: Confidence seeded on insert (AC-10, R-02)
```
// Call context_store to create a new entry
// Read entry back from store
// Assert confidence > 0.0
// Assert confidence approximately matches:
//   For agent-authored: ~0.525 (0.20*0.5 + 0.15*0 + 0.20*1.0 + 0.15*0.5 + 0.15*0.5 + 0.15*0.5)
//   For human-authored: ~0.60
```

### T-25: Confidence recomputed on correction (AC-11, R-02)
```
// Insert original entry
// Trigger a retrieval (to give it some confidence)
// Correct the entry via context_correct
// Read new correction entry: has computed confidence
// Read deprecated original: has recomputed confidence with base_score=0.2
// Assert new correction confidence differs from original's
// Assert deprecated original's confidence is lower (base_score dropped)
```

### T-26: Confidence recomputed on deprecation (AC-12, R-02)
```
// Insert entry, trigger retrieval (builds some confidence)
// Record the confidence before deprecation
// Deprecate entry via context_deprecate
// Read entry back
// Assert confidence < previous confidence (base_score 0.5 -> 0.2)
```

### T-27: Mutation confidence failure does not fail mutation (R-02, AC-19)
```
// This test verifies fire-and-forget on mutation paths:
// If update_confidence fails (e.g., concurrent delete), the mutation
// (insert/correct/deprecate) itself should still succeed.
//
// Practically tested by verifying that insert/correct/deprecate return
// successfully even when confidence is fire-and-forget.
```

### T-28: Deprecated entry has lower confidence than active (AC-18)
```
// Insert two entries with identical content/trust/usage
// Deprecate one
// Trigger retrieval of both
// Assert deprecated entry confidence < active entry confidence
```
