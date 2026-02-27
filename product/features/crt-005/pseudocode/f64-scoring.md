# Pseudocode: C2 f64 Scoring Constants

## Purpose

Promote all scoring pipeline constants and function signatures from f32 to f64 across the workspace. Eliminates silent truncation at `compute_confidence`'s return boundary and JSON precision artifacts.

## Files Modified

- `crates/unimatrix-server/src/confidence.rs` -- constants + function signatures
- `crates/unimatrix-server/src/coaccess.rs` -- boost constants + function signatures
- `crates/unimatrix-vector/src/index.rs` -- SearchResult.similarity type + cast
- `crates/unimatrix-core/src/traits.rs` -- implicit via SearchResult type change
- `crates/unimatrix-store/src/write.rs` -- update_confidence signature
- `crates/unimatrix-server/src/tools.rs` -- all call sites

## confidence.rs Changes

### Constants: f32 -> f64

```
pub const W_BASE: f64 = 0.18;
pub const W_USAGE: f64 = 0.14;
pub const W_FRESH: f64 = 0.18;
pub const W_HELP: f64 = 0.14;
pub const W_CORR: f64 = 0.14;
pub const W_TRUST: f64 = 0.14;
pub const W_COAC: f64 = 0.08;
pub const SEARCH_SIMILARITY_WEIGHT: f64 = 0.85;
```

### compute_confidence: return f64 directly

```
pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f64:
    b = base_score(entry.status)
    u = usage_score(entry.access_count)
    f = freshness_score(entry.last_accessed_at, entry.created_at, now)
    h = helpfulness_score(entry.helpful_count, entry.unhelpful_count)
    c = correction_score(entry.correction_count)
    t = trust_score(&entry.trust_source)

    // Weights are now f64 natively -- no `as f64` casts needed
    composite = W_BASE * b + W_USAGE * u + W_FRESH * f
              + W_HELP * h + W_CORR * c + W_TRUST * t

    return composite.clamp(0.0, 1.0)
    // NOTE: Remove the `as f32` that was on the old return
```

### rerank_score: f64 signature

```
pub fn rerank_score(similarity: f64, confidence: f64) -> f64:
    SEARCH_SIMILARITY_WEIGHT * similarity + (1.0 - SEARCH_SIMILARITY_WEIGHT) * confidence
```

### co_access_affinity: f64 signature

```
pub fn co_access_affinity(partner_count: usize, avg_partner_confidence: f64) -> f64:
    if partner_count == 0 || avg_partner_confidence <= 0.0:
        return 0.0
    partner_score = ln(1.0 + partner_count as f64) / ln(1.0 + MAX_MEANINGFUL_PARTNERS)
    capped = partner_score.min(1.0)
    affinity = W_COAC * capped * avg_partner_confidence.clamp(0.0, 1.0)
    return affinity.clamp(0.0, W_COAC)
```

## coaccess.rs Changes

```
pub const MAX_CO_ACCESS_BOOST: f64 = 0.03;
pub const MAX_BRIEFING_CO_ACCESS_BOOST: f64 = 0.01;

fn co_access_boost(count: u32, max_boost: f64) -> f64:
    if count == 0: return 0.0
    raw = ln(1.0 + count as f64) / ln(1.0 + MAX_MEANINGFUL_CO_ACCESS)
    capped = raw.min(1.0)
    return capped * max_boost

pub fn compute_search_boost(...) -> HashMap<u64, f64>
pub fn compute_briefing_boost(...) -> HashMap<u64, f64>
fn compute_boost_internal(..., max_boost: f64) -> HashMap<u64, f64>
```

All internal `HashMap<u64, f32>` become `HashMap<u64, f64>`.

## index.rs Changes (vector crate)

```
pub struct SearchResult {
    pub entry_id: u64,
    pub similarity: f64,   // WAS: f32
}
```

In `map_neighbours_to_results`:
```
similarity: 1.0_f64 - n.distance as f64
// NOT: (1.0 - n.distance) as f64   (R-04: cast order matters)
```

## write.rs Changes (store crate)

```
pub fn update_confidence(&self, entry_id: u64, confidence: f64) -> Result<()>:
    // Same logic, f64 assigned to f64 field
```

## tools.rs Call Site Updates

Mechanical: all compute_confidence/rerank_score/update_confidence/compute_search_boost calls use f64 types. Remove any `as f64` or `as f32` casts in the scoring path.

## Test Update Strategy (~60-80 tests)

- `f32` type annotations -> `f64` (or remove, since f64 is default)
- `f32::EPSILON` -> `f64::EPSILON`
- `0.95_f32` -> `0.95`
- Test helpers accepting f32 confidence -> f64
- Exact f32 representation values -> exact f64 values

## Key Test Scenarios

1. Weight sums: W_BASE..W_TRUST = 0.92, + W_COAC = 1.0
2. compute_confidence precision beyond 7 decimal digits
3. rerank_score(1.0, 1.0) == 1.0 in f64
4. SearchResult carries f64 precision
5. update_confidence roundtrips f64 (store 0.123456789012345, read back exact)
6. All 811 existing tests pass
7. No `as f32` in scoring pipeline (grep verification)
