# Test Plan: confidence-module (C1)

## Location: `crates/unimatrix-server/src/confidence.rs` (inline #[cfg(test)])

### T-01: Weight sum invariant (R-05, AC-02)
```
assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 1.0
```
Exact f32 comparison. The constants are chosen to sum exactly.

### T-02: base_score values (AC-08)
```
assert base_score(Active) == 0.5
assert base_score(Proposed) == 0.5
assert base_score(Deprecated) == 0.2
```

### T-03: usage_score values (R-08, AC-03)
```
assert usage_score(0) == 0.0
assert usage_score(1) > 0.0 and < 0.5
assert abs(usage_score(50) - 1.0) < 0.01  // approximately 1.0
assert usage_score(500) == 1.0  // clamped
assert usage_score(u32::MAX) == 1.0  // clamped
```

### T-04: freshness_score values (R-07, AC-04)
```
now = 1_000_000
// Just accessed
assert abs(freshness_score(now, now, now) - 1.0) < 0.001

// 1 week ago (168 hours * 3600)
one_week_ago = now - 168 * 3600
assert abs(freshness_score(one_week_ago, 0, now) - 0.3679) < 0.01

// Fallback to created_at when last_accessed_at == 0
assert freshness_score(0, now, now) is approximately 1.0

// Edge: both timestamps 0
assert freshness_score(0, 0, now) == 0.0

// Edge: clock skew (reference in future)
assert freshness_score(now + 100, 0, now) == 1.0

// Edge: very old entry
very_old = now - 365 * 24 * 3600
result = freshness_score(very_old, 0, now)
assert result >= 0.0 and result < 0.001  // nearly zero but not NaN
```

### T-05: helpfulness_score minimum sample guard (R-01, AC-05, AC-21)
```
// Below minimum: always 0.5
assert helpfulness_score(0, 0) == 0.5
assert helpfulness_score(3, 0) == 0.5
assert helpfulness_score(2, 2) == 0.5
assert helpfulness_score(4, 0) == 0.5  // total=4 < 5

// At minimum: Wilson kicks in
result = helpfulness_score(5, 0)
assert result != 0.5  // Wilson deviates from neutral
assert result < 1.0

// All helpful
result = helpfulness_score(100, 0)
assert result > 0.5 and result < 1.0  // Wilson < 1.0

// All unhelpful
result = helpfulness_score(0, 100)
assert result == 0.0  // Wilson lower bound

// Mixed
result = helpfulness_score(80, 20)
assert result > 0.5  // more helpful than not
```

### T-06: Wilson score reference values (R-01, AC-15)
```
// Compare against known Wilson lower bound at z=1.96
// Reference: n=100, p_hat=0.8 -> Wilson lower ~0.714
result = wilson_lower_bound(80.0, 100.0)
assert abs(result - 0.714) < 0.002

// n=10, p_hat=0.8 -> Wilson lower ~0.494
result = wilson_lower_bound(8.0, 10.0)
assert abs(result - 0.494) < 0.02

// n=100000, p_hat=0.5 -> should be stable, close to 0.5
result = wilson_lower_bound(50000.0, 100000.0)
assert abs(result - 0.497) < 0.005  // very close to 0.5
```

### T-07: correction_score values (AC-06)
```
assert correction_score(0) == 0.5
assert correction_score(1) == 0.8
assert correction_score(2) == 0.8
assert correction_score(3) == 0.6
assert correction_score(4) == 0.6
assert correction_score(5) == 0.6
assert correction_score(6) == 0.3
assert correction_score(100) == 0.3
```

### T-08: trust_score values (AC-07)
```
assert trust_score("human") == 1.0
assert trust_score("system") == 0.7
assert trust_score("agent") == 0.5
assert trust_score("") == 0.3
assert trust_score("unknown") == 0.3
assert trust_score("Human") == 0.3  // case-sensitive
```

### T-09: compute_confidence composite (AC-01, AC-02, R-05)
```
// All components at known values
entry = make_test_entry(
    status=Active, access_count=0, last_accessed_at=0, created_at=0,
    helpful=0, unhelpful=0, correction_count=0, trust_source=""
)
result = compute_confidence(entry, 1_000_000)
// base=0.5, usage=0.0, fresh=0.0, help=0.5, corr=0.5, trust=0.3
expected = 0.20*0.5 + 0.15*0.0 + 0.20*0.0 + 0.15*0.5 + 0.15*0.5 + 0.15*0.3
assert abs(result - expected as f32) < 0.001

// All max values
entry_max = make_test_entry(
    status=Active, access_count=1000, last_accessed_at=now, created_at=now,
    helpful=100, unhelpful=0, correction_count=1, trust_source="human"
)
result_max = compute_confidence(entry_max, now)
assert result_max > 0.8  // high confidence
assert result_max <= 1.0
```

### T-10: compute_confidence range (AC-01, R-12)
```
// Must always return [0.0, 1.0]
for various EntryRecord configurations:
    result = compute_confidence(entry, now)
    assert result >= 0.0
    assert result <= 1.0
```

### T-11: rerank_score blend (AC-13, AC-14)
```
assert rerank_score(1.0, 1.0) == 1.0
assert rerank_score(0.0, 0.0) == 0.0
assert rerank_score(1.0, 0.0) == 0.85
assert rerank_score(0.0, 1.0) == 0.15

// Confidence tiebreaker
assert rerank_score(0.90, 0.80) > rerank_score(0.90, 0.20)

// Similarity dominant
assert rerank_score(0.95, 0.0) > rerank_score(0.70, 1.0)
```
