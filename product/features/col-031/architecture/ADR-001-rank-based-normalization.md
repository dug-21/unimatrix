## ADR-001: Rank-Based Normalization for phase_affinity_score

### Context

`PhaseFreqTable` stores per-`(phase, category)` access frequencies aggregated
from `query_log`. Raw frequency counts must be normalized to `[0.0, 1.0]` for
composition with `FusedScoreInputs.phase_explicit_norm` and use as a PPR
personalization seed weight.

Two strategies were considered:

**Min-max normalization**: `(freq - min) / (max - min)`. Access patterns in
production are power-law distributed — a small number of entries dominate
retrieval in any given phase. Min-max collapses all non-outlier entries near
`0.0` with one outlier at `1.0`, producing a near-degenerate PPR
personalization vector with no usable gradient for PageRank traversal. A
floor parameter was considered to alleviate this but introduces an additional
tunable with no principled calibration basis.

**Rank-based normalization**: Entries are sorted descending by frequency within
each `(phase, category)` bucket. Score is computed from 1-indexed rank:

```
score = 1.0 - ((rank - 1) as f32 / N as f32)
```

where `rank` is 1-indexed (1 = most frequent), `N` = bucket size.

- Rank 1 (most frequent) → `1.0`
- Rank N (least frequent) → `(N-1)/N`
- Single-entry bucket (N=1, rank=1) → `1.0` — full signal, not zero
- Absent entry (not in bucket) → `1.0` (neutral)

Note: the formula `1 - rank/N` with 1-indexed rank produces `0.0` for N=1
(`1 - 1/1 = 0`). The form `1 - (rank-1)/N` is used explicitly to make the
single-entry case correct and safe.

The neutral absent-entry value (`1.0`) is not arbitrary — it is the PPR
contract: `hnsw_score × 1.0 = hnsw_score` (no cold-start suppression).

An existing ADR from a pre-design pass (Unimatrix #3679) captured an earlier
version of this decision using 0-indexed rank (`1.0 - rank/N`). The formula
above supersedes that entry; the 1-indexed form is used in SCOPE.md, the
SCOPE.md verified fact list, and is the authoritative implementation contract.

### Decision

Use rank-based normalization: `score = 1.0 - ((rank - 1) as f32 / N as f32)`,
1-indexed rank, within each `(phase, category)` bucket. Buckets are stored
sorted descending by score (i.e., most-frequent entry first). Absent entries
return `1.0`. Single-entry buckets return `1.0` for that entry.

### Consequences

**Easier**:
- PPR receives a rich gradient across all ranked entries regardless of the
  raw count distribution. High-volume outliers do not dominate.
- Cold-start and unknown-entry cases produce `1.0` (neutral multiplier) —
  no PPR distortion.
- No additional tunable parameters (no floor, no smoothing constant).

**Harder**:
- Raw frequency magnitude is lost in the normalization. Two entries appearing
  100× and 2× respectively rank adjacent if they are rank-1 and rank-2 in a
  2-entry bucket. For fused scoring this is acceptable; for future diagnostic
  tooling the raw `freq` column from `PhaseFreqRow` is available.
