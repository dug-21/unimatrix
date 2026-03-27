## ADR-001: Rank-Based Normalization for `phase_affinity_score`

### Context

`PhaseFreqTable` stores per-`(phase, category)` access frequencies from `query_log`.
The raw frequencies must be normalized to `[0.0, 1.0]` so they compose correctly with
`FusedScoreInputs.phase_explicit_norm` (which expects a `[0.0, 1.0]` value per
`compute_fused_score` invariants).

Two normalization strategies were considered:

**Min-max normalization:** `score = (freq - freq_min) / (freq_max - freq_min)`.
Access patterns are power-law distributed — a handful of entries dominate counts in
any given phase. Min-max collapses most entries near 0.0 with one or two outliers
near 1.0, producing a degenerate signal: near-uniform with a spike. For the PPR
personalization vector, this creates an almost-flat prior with one dominant seed,
giving PageRank little gradient to traverse.

**Rank-based normalization:** `score = 1.0 - (rank / N)`, where `rank` is 0-indexed
by descending frequency and `N` is the bucket size. The top entry (rank 0) scores 1.0;
the bottom entry (rank N-1) scores `1/N`. This spreads signal evenly across the bucket
regardless of the power-law shape of the underlying count distribution.

Entries absent from the `(phase, category)` bucket return `1.0` (neutral — no
evidence against them). This matches the cold-start invariant: `use_fallback = true`
produces `1.0` for all entries, which means `w_phase_explicit * 1.0` is a flat additive
term equal for all candidates, contributing no ranking signal. This is preferable to
`0.0` which would suppress unseen entries (see Unimatrix #3677).

### Decision

Use rank-based normalization with the formula:
```
score = 1.0 - (rank as f32 / N as f32)
```
where `rank` is 0-indexed within the `(phase, category)` bucket ordered by descending
frequency, and `N` is the total number of distinct entry IDs in the bucket.

Absent entries return `1.0` (neutral), not `0.0`.

The bucket is stored sorted descending by score (i.e., by ascending rank), so lookup
is a linear scan of the `Vec<(entry_id, f32)>`.

### Consequences

**Easier:**
- Richer gradient for PPR personalization vector — all ranked entries carry distinct
  signal rather than collapsing near 0.0.
- Cold-start and unknown-entry cases produce `1.0` (neutral) — no suppression of
  entries not seen in a given phase.
- Normalization is O(N) per bucket at rebuild time; no per-query computation required.

**Harder:**
- A high-frequency outlier does not receive a score proportionally higher than rank 2.
  If a single entry is accessed 1000× and the next is accessed once, both receive
  similar rank scores (1.0 and ~0.98 in a two-entry bucket). This trades raw-frequency
  fidelity for distribution robustness.
- Bucket size affects score granularity: a one-entry bucket always produces score 1.0
  for that entry. This is acceptable — a bucket with one entry means only one entry
  has been seen in that `(phase, category)`, which is 100% confident by revealed
  preference.
