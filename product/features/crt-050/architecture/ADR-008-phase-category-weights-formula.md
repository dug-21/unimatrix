## ADR-008: phase_category_weights() — Normalized Bucket Size Formula

### Context

Goal 4 of crt-050 requires a public method `phase_category_weights()` on
`PhaseFreqTable` that returns `HashMap<(String, String), f32>` — a
`(phase, category)` → weight map for W3-1 GNN cold-start initialization.
The internal `table` field stores `HashMap<(String, String), Vec<(u64, f32)>>`
(per-entry rank scores per bucket). A different projection is needed: a scalar
weight per `(phase, category)` pair.

Two aggregation strategies were considered:

**Option A (mean rank score):** For each `(phase, category)` bucket, compute
the mean of the rank scores across all entries in the bucket. This is
proportional to average affinity within the bucket.
- Pro: uses the already-computed rank scores directly.
- Con: mean rank score is not normalized across categories within a phase —
  two categories with identical bucket sizes would have the same mean score,
  making it impossible to distinguish "phase uses mostly decision entries" from
  "phase uses mostly pattern entries" at the category level.

**Option B (normalized bucket size):** For each `(phase, category)` bucket,
the weight is `bucket.len() / total_entries_for_phase` where
`total_entries_for_phase` is the sum of bucket sizes across all categories for
that phase. This forms a probability distribution over categories per phase,
summing to `1.0` within each phase.
- Pro: directly answers "given phase P, what fraction of reads were from
  category C?" — the natural question for GNN cold-start ("how much attention
  should the GNN initially allocate to each category for this phase?").
- Con: discards intra-bucket rank ordering (the distinction between frequently
  and infrequently read entries within a category is not represented). This is
  acceptable for a cold-start initialization vector.

The SCOPE.md Proposed Approach (Step 3) mandates Option B: "normalized bucket
size — for each (phase, category) bucket, the weight is the fraction of total
explicit reads for that phase attributable to that category."

### Decision

Implement `phase_category_weights()` using normalized bucket size (Option B).

**Formula:**

For each phase `p` in `self.table`:
```
total_entries(p) = Σ bucket.len() for all (p, c) buckets
weight(p, c) = bucket(p, c).len() as f32 / total_entries(p) as f32
```

This produces a probability distribution over categories within each phase:
`Σ_c weight(p, c) = 1.0` for each phase `p`.

**Edge cases:**
- `use_fallback = true`: return empty `HashMap` (AC-08).
- A phase with a single category: weight = `1.0` (trivially).
- Floating-point precision: weights may not sum to exactly `1.0` due to f32
  rounding; this is acceptable for a cold-start initialization vector.

**Method signature:**

```rust
/// Return a learned (phase, category) weight map for W3-1 GNN cold-start.
///
/// Weight = fraction of explicit reads for the phase attributable to the
/// category. Sums to 1.0 per phase (up to f32 rounding).
///
/// Returns an empty map when `use_fallback = true` (no signal available).
/// Not called on the search hot path — called at GNN initialization only.
pub fn phase_category_weights(&self) -> HashMap<(String, String), f32> {
    // ...
}
```

**Visibility:** `pub` on `PhaseFreqTable`. `PhaseFreqTable` lives in
`unimatrix-server/src/services/`. W3-1 (ASS-029) may need this from a
different context — if cross-crate access is required at W3-1 implementation
time, visibility must be re-evaluated then. This is a tracked open item for
W3-1 (SR-07).

### Consequences

- The weight map is a probability distribution: interpretable, bounded [0, 1],
  and normalization is self-documenting.
- Intra-bucket rank ordering is not exposed through this method. W3-1 can
  access the full `table` field (pub) if it needs entry-level scores.
- The method is `O(buckets)` — linear in the number of `(phase, category)`
  pairs, not in total entry count. Acceptable for an off-hot-path method.
- W3-1 cold-start: the empty-map return on `use_fallback = true` means W3-1
  must handle an empty map and fall back to its own initialization (e.g.,
  uniform distribution). This is a W3-1 responsibility, not crt-050's.
