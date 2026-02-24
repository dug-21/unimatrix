# Pseudocode: confidence-module (C1)

## File: `crates/unimatrix-server/src/confidence.rs`

### Constants

```
W_BASE: f32 = 0.20
W_USAGE: f32 = 0.15
W_FRESH: f32 = 0.20
W_HELP: f32 = 0.15
W_CORR: f32 = 0.15
W_TRUST: f32 = 0.15

MAX_MEANINGFUL_ACCESS: f64 = 50.0
FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0
MINIMUM_SAMPLE_SIZE: u32 = 5
WILSON_Z: f64 = 1.96
SEARCH_SIMILARITY_WEIGHT: f32 = 0.85
```

### base_score(status: Status) -> f64

```
match status:
    Active => 0.5
    Proposed => 0.5
    Deprecated => 0.2
```

Note: Use exhaustive match (no wildcard) so new Status variants cause a compile error.

### usage_score(access_count: u32) -> f64

```
if access_count == 0:
    return 0.0
numerator = ln(1.0 + access_count as f64)
denominator = ln(1.0 + MAX_MEANINGFUL_ACCESS)
result = numerator / denominator
return min(result, 1.0)  // clamp
```

### freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f64

```
// Determine reference timestamp
reference = if last_accessed_at > 0 then last_accessed_at else created_at

// Edge case: no reference timestamp
if reference == 0:
    return 0.0

// Edge case: clock skew (reference in future)
if now <= reference:
    return 1.0

age_seconds = now - reference
age_hours = age_seconds as f64 / 3600.0
return exp(-age_hours / FRESHNESS_HALF_LIFE_HOURS)
```

### helpfulness_score(helpful_count: u32, unhelpful_count: u32) -> f64

```
total = helpful_count + unhelpful_count
if total < MINIMUM_SAMPLE_SIZE:
    return 0.5  // neutral prior

return wilson_lower_bound(helpful_count as f64, total as f64)
```

### wilson_lower_bound(positive: f64, total: f64) -> f64

```
// Private helper. Called only when total >= MINIMUM_SAMPLE_SIZE (>= 5).
z = WILSON_Z  // 1.96
p_hat = positive / total
z_sq = z * z

numerator = p_hat + z_sq / (2.0 * total)
             - z * sqrt(p_hat * (1.0 - p_hat) / total + z_sq / (4.0 * total * total))
denominator = 1.0 + z_sq / total

result = numerator / denominator

// Clamp to [0.0, 1.0] for safety (should be in range but floating point)
return max(0.0, min(1.0, result))
```

### correction_score(correction_count: u32) -> f64

```
match correction_count:
    0 => 0.5
    1 | 2 => 0.8
    3..=5 => 0.6
    _ => 0.3
```

### trust_score(trust_source: &str) -> f64

```
match trust_source:
    "human" => 1.0
    "system" => 0.7
    "agent" => 0.5
    _ => 0.3
```

### compute_confidence(entry: &EntryRecord, now: u64) -> f32

```
// All computation in f64
b = base_score(entry.status) as f64
u = usage_score(entry.access_count)
f = freshness_score(entry.last_accessed_at, entry.created_at, now)
h = helpfulness_score(entry.helpful_count, entry.unhelpful_count)
c = correction_score(entry.correction_count)
t = trust_score(&entry.trust_source)

composite = W_BASE as f64 * b
          + W_USAGE as f64 * u
          + W_FRESH as f64 * f
          + W_HELP as f64 * h
          + W_CORR as f64 * c
          + W_TRUST as f64 * t

// Clamp to [0.0, 1.0] and cast to f32
return clamp(composite, 0.0, 1.0) as f32
```

### rerank_score(similarity: f32, confidence: f32) -> f32

```
return SEARCH_SIMILARITY_WEIGHT * similarity
     + (1.0 - SEARCH_SIMILARITY_WEIGHT) * confidence
```

## Module Declaration

In `crates/unimatrix-server/src/lib.rs`, add:
```
pub mod confidence;
```

## Dependencies

- `unimatrix_store::schema::{EntryRecord, Status}` (via unimatrix_core re-export)
- `std::f64` for ln(), exp(), sqrt()
- No external crates
