# Test Plan: C5 -- Confidence Factor Extension

## Location

`crates/unimatrix-server/src/confidence.rs` (unit tests)

## Risk Coverage

- R-01: Confidence weight redistribution causes ranking regression
- R-10: Co-access affinity computation produces NaN or out-of-range value

## Test Scenarios

### T-C5-01: Stored weight sum invariant (R-01)

```
Assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92
(Updated from 1.0 to 0.92)
```

### T-C5-02: Effective weight sum invariant (R-01)

```
Assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST + W_COAC == 1.00
```

### T-C5-03: compute_confidence all-defaults -- updated expected value (R-01)

```
Given entry with all defaults (Status::Active, access=0, timestamps=0, votes=0, corrections=0, trust="")
When compute_confidence(entry, now=1_000_000)
Then result matches:
  0.18*0.5 + 0.14*0.0 + 0.18*0.0 + 0.14*0.5 + 0.14*0.5 + 0.14*0.3
  = 0.09 + 0 + 0 + 0.07 + 0.07 + 0.042 = 0.272
(Previous expected: 0.295 with old weights)
```

### T-C5-04: compute_confidence all-max -- updated range (R-01)

```
Given entry with max values (Active, access=1000, fresh timestamps, 100 helpful votes, 1 correction, trust="human")
When compute_confidence(entry, now)
Then result > 0.7 and result <= 0.92
(Previous upper bound was 1.0, now 0.92)
```

### T-C5-05: compute_confidence range -- always in [0.0, 0.92] (R-01)

```
For various entry configurations:
  compute_confidence always returns value in [0.0, 0.92]
```

### T-C5-06: Existing crt-002 tests pass with updated expected values (R-01)

```
All existing confidence tests must pass after weight redistribution.
Update expected values in:
  - weight_sum_invariant
  - compute_confidence_all_defaults
  - compute_confidence_all_max
Other tests use ranges/relative comparisons and should pass without changes.
```

### T-C5-07: co_access_affinity -- zero partners (R-10)

```
Given co_access_affinity(partner_count=0, avg_confidence=0.8)
Then returns 0.0
```

### T-C5-08: co_access_affinity -- max partners and max confidence (R-10)

```
Given co_access_affinity(partner_count=10, avg_confidence=1.0)
Then returns W_COAC (0.08)
  ln(11)/ln(11) = 1.0, capped at 1.0
  0.08 * 1.0 * 1.0 = 0.08
```

### T-C5-09: co_access_affinity -- large partner count saturated (R-10)

```
Given co_access_affinity(partner_count=100, avg_confidence=1.0)
Then returns 0.08 (same as partner_count=10, capped)
```

### T-C5-10: co_access_affinity -- zero confidence (R-10)

```
Given co_access_affinity(partner_count=5, avg_confidence=0.0)
Then returns 0.0
```

### T-C5-11: co_access_affinity -- negative confidence clamped (R-10)

```
Given co_access_affinity(partner_count=5, avg_confidence=-0.5)
Then returns 0.0 (negative confidence clamped to 0)
```

### T-C5-12: Effective confidence sum clamped to [0.0, 1.0] (R-10)

```
Given stored_confidence = compute_confidence(max_entry, now) = 0.92
And affinity = co_access_affinity(10, 1.0) = 0.08
Then effective = (stored_confidence + affinity).clamp(0.0, 1.0) = 1.00
```

### T-C5-13: rerank_score with effective confidence (R-01)

```
Given effective_confidence = 0.92 + 0.08 = 1.0
When rerank_score(0.90, 1.0)
Then result = 0.85 * 0.90 + 0.15 * 1.0 = 0.915
(Same as before crt-004 for max-confidence entry)
```

### T-C5-14: No co-access data -- confidence slightly lower (R-01)

```
Given stored_confidence = 0.85 (typical high-confidence entry under new weights)
And affinity = 0.0 (no co-access partners)
Then effective = 0.85
And rerank_score(0.90, 0.85) = 0.85*0.90 + 0.15*0.85 = 0.8925
(Previously with old weights the stored confidence would have been ~0.924)
(The ranking impact is small: ~0.005 difference in rerank score)
```
