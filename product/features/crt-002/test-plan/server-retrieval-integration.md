# Test Plan: server-retrieval-integration (C3)

## Location: `crates/unimatrix-server/` integration tests

### T-20: Confidence updated on retrieval (AC-09)
```
// Insert entry with confidence=0.0
// Trigger a retrieval (context_search or context_get)
// Read entry back from store
// Assert confidence > 0.0

// Note: Due to one-retrieval lag (ADR-004), the response shows
// the OLD confidence. The stored value is updated for the NEXT read.
```

### T-21: Fire-and-forget pattern (AC-19)
```
// Verify that:
// 1. record_usage_for_entries returns without blocking
// 2. If the confidence computation fails somehow, the retrieval response
//    is still delivered correctly
// 3. The error is logged (verified by checking tracing output or by
//    ensuring the response is well-formed regardless)

// This is primarily verified by the existing fire-and-forget test
// infrastructure from crt-001, extended to include the confidence path.
```

### T-22: Confidence matches expected formula (AC-09, AC-01)
```
// Insert entry with known field values
// Retrieve it (triggers usage recording + confidence)
// Read entry back
// Assert confidence matches compute_confidence(entry, now) within tolerance

// The tolerance accounts for the timestamp difference between
// record_usage_with_confidence's now and our verification timestamp.
```

### T-23: Multiple retrievals evolve confidence (AC-09)
```
// Insert entry
// First retrieval: confidence gets initial value
// Second retrieval: access_count=1, freshness=recent -> confidence changes
// Verify confidence changed between reads
```

Note: Most retrieval integration tests depend on the full server stack
(UnimatrixServer with all subsystems). If full server tests are not
practical in unit test scope, these tests belong in the integration
test suite that crt-001 established.
