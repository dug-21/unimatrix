# Pseudocode: C5 -- Confidence Factor Extension

## Crate: unimatrix-server

### confidence.rs changes

**Weight redistribution** (ADR-003):

```rust
// -- Weight constants (stored factors must sum to exactly 0.92) --

/// Weight for base quality (status-dependent).
pub const W_BASE: f32 = 0.18;   // was 0.20
/// Weight for usage frequency.
pub const W_USAGE: f32 = 0.14;  // was 0.15
/// Weight for freshness (recency of access).
pub const W_FRESH: f32 = 0.18;  // was 0.20
/// Weight for helpfulness (Wilson score).
pub const W_HELP: f32 = 0.14;   // was 0.15
/// Weight for correction chain quality.
pub const W_CORR: f32 = 0.14;   // was 0.15
/// Weight for creator trust level.
pub const W_TRUST: f32 = 0.14;  // was 0.15

/// Weight for co-access affinity (applied at query time, NOT in compute_confidence).
pub const W_COAC: f32 = 0.08;

// Sum of stored weights: 0.18 + 0.14 + 0.18 + 0.14 + 0.14 + 0.14 = 0.92
// Effective total: 0.92 + 0.08 = 1.00
```

**compute_confidence** -- NO changes to function body or signature. The formula automatically produces values in [0.0, 0.92] because the six weights now sum to 0.92 instead of 1.0. The function pointer signature `fn(&EntryRecord, u64) -> f32` is preserved.

**New function: co_access_affinity**

```rust
/// Compute the co-access affinity component for an entry.
///
/// This is computed at query time and added to the stored confidence value.
/// The result is in [0.0, W_COAC] (i.e., [0.0, 0.08]).
///
/// Formula (ADR-003):
///   partner_score = min(ln(1 + partner_count) / ln(1 + MAX_MEANINGFUL_PARTNERS), 1.0)
///   affinity = W_COAC * partner_score * avg_partner_confidence
///
/// Where MAX_MEANINGFUL_PARTNERS = 10.0 (from coaccess module).
///
/// Returns 0.0 when partner_count is 0.
/// Returns at most W_COAC (0.08) when partner_count >= 10 and avg_partner_confidence = 1.0.
pub fn co_access_affinity(
    partner_count: usize,
    avg_partner_confidence: f32,
) -> f32 {
    if partner_count == 0 || avg_partner_confidence <= 0.0 {
        return 0.0;
    }

    let partner_score = (1.0 + partner_count as f64).ln()
        / (1.0 + crate::coaccess::MAX_MEANINGFUL_PARTNERS).ln();
    let capped = partner_score.min(1.0);
    let affinity = W_COAC as f64 * capped * avg_partner_confidence.clamp(0.0, 1.0) as f64;

    affinity.clamp(0.0, W_COAC as f64) as f32
}
```

Key design notes:
- `compute_confidence` returns [0.0, 0.92] with the new weights. No code change needed in the function body -- only the weight constants change.
- `co_access_affinity` is a pure function (no I/O). The CO_ACCESS lookups happen in the caller (tools.rs), which passes partner_count and avg_partner_confidence.
- The log-transform on partner_count uses the same pattern as usage_score and co_access_boost.
- avg_partner_confidence modulates the affinity: partners with high confidence contribute more.
- The function is clamped to [0.0, W_COAC] for safety.
- Existing tests for compute_confidence need updated expected values (weights changed).
- The weight_sum_invariant test changes from asserting sum==1.0 to asserting stored sum==0.92 and total sum==1.00.
