# crt-051: contradiction_density_score() — Architecture

## System Overview

`contradiction_density_score()` is one of three pure functions that feed the Lambda
coherence metric in `infra/coherence.rs`. Lambda is a composite health score in [0.0,
1.0] reported by `context_status`. The contradiction dimension carries weight 0.31 —
the second-highest of the three Lambda dimensions (graph_quality 0.46, embedding 0.23,
contradiction 0.31).

The function currently takes `total_quarantined: u64` as its "contradiction" signal.
Quarantine count is a status counter for entries flagged for policy or quality review;
it has no causal relationship to whether entries contradict each other. The scan that
actually detects contradiction pairs (`scan_contradictions`, running on a background
tick every ~60 minutes) writes its results to an in-memory cache
(`ContradictionScanCacheHandle`). That cache is already read in `compute_report()` Phase
2 and populates `report.contradiction_count`, but that value is never passed into
`contradiction_density_score()`. This feature closes that gap.

## Component Breakdown

### 1. `infra/coherence.rs` — Scoring function (the primary change)

**Responsibility:** Pure, deterministic scoring functions for all three Lambda dimensions.
No I/O, no state.

**Change:** Replace the `total_quarantined: u64` parameter on
`contradiction_density_score()` with `contradiction_pair_count: usize`. Update the
formula and doc comment. The function remains pure.

**Before:**
```rust
pub fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64 {
    if total_active == 0 { return 1.0; }
    let score = 1.0 - (total_quarantined as f64 / total_active as f64);
    score.clamp(0.0, 1.0)
}
```

**After:**
```rust
/// Contradiction density dimension: complement of contradiction pair ratio.
///
/// Returns 1.0 if `total_active` is zero (empty database guard).
/// Returns 1.0 if `contradiction_pair_count` is zero (cold-start or no contradictions
/// detected — optimistic default until the scan produces evidence).
/// Score is `1.0 - contradiction_pair_count / total_active`, clamped to [0.0, 1.0].
/// When `contradiction_pair_count > total_active` (degenerate: many pairs from a
/// small active set), the clamp produces 0.0.
///
/// `contradiction_pair_count` comes from `ContradictionScanCacheHandle` read in Phase 2
/// of `compute_report()`. It reflects detected contradiction pairs from the background
/// heuristic scan (HNSW nearest-neighbour + negation/directive/sentiment signals).
/// The cache is rebuilt approximately every 60 minutes. A stale cache is a known
/// limitation (SR-07); this function is not responsible for cache freshness.
pub fn contradiction_density_score(
    contradiction_pair_count: usize,
    total_active: u64,
) -> f64 {
    if total_active == 0 {
        return 1.0;
    }
    let score = 1.0 - (contradiction_pair_count as f64 / total_active as f64);
    score.clamp(0.0, 1.0)
}
```

**Unit tests in `coherence.rs` — full rewrite required (SR-01):**

Three existing tests encode quarantine-based semantics by name and by value:
- `contradiction_density_zero_active` — rename + update args from `(0, 0)` to `(0, 0)`
  (args happen to be identical; rename only)
- `contradiction_density_quarantined_exceeds_active` — rename to
  `contradiction_density_pairs_exceed_active`; args `(200, 100)` stay numerically valid
  but mean pair count, not quarantine count — update test comment to reflect meaning
- `contradiction_density_no_quarantined` — rename to `contradiction_density_no_pairs`;
  args `(0, 100)` stay unchanged

No new numeric values change because the formula structure is identical — only the
semantic meaning of the first argument shifts. The tests can reuse the same input
numbers but MUST have updated names and doc comments that reference pair count, not
quarantine count.

### 2. `services/status.rs` — Call site (one-line change)

**Responsibility:** `compute_report()` orchestrates all phases of status data collection
and assembles `StatusReport`. Phase 2 reads the contradiction cache; Phase 5 computes
Lambda scores.

**Change:** At line 747–748, replace `report.total_quarantined` with
`report.contradiction_count` as the first argument:

**Before (line 747–748):**
```rust
report.contradiction_density_score =
    coherence::contradiction_density_score(report.total_quarantined, report.total_active);
```

**After:**
```rust
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

No other changes to `status.rs`. `generate_recommendations()` at line 784–790 continues
to receive `report.total_quarantined` unchanged — that path is about quarantine
management recommendations, not Lambda scoring, and must not be altered (AC-08).

### 3. `mcp/response/mod.rs` — Fixture correction (SR-02)

**Responsibility:** Serialization and formatting of `StatusReport` into MCP response
text. Contains test fixtures that construct `StatusReport` with hardcoded field values.

**Change:** The `make_coherence_status_report()` fixture (around line 1397) currently has:
- `total_quarantined: 3`
- `contradiction_count: 0`
- `contradiction_density_score: 0.7000`

The value 0.7000 was derived from the old formula: `1.0 - (3 / 10)` is not even that
because `total_active: 50` gives `1.0 - 3/50 = 0.940`. The 0.7000 appears to be a
manually assigned scenario value, not computed from the formula. After the fix, a fixture
with `total_quarantined: 3`, `contradiction_count: 0`, `total_active: 50` would compute
`contradiction_density_score` as `1.0 - (0 / 50) = 1.0`. The fixture value must be
updated to `1.0000` if it is meant to represent the computed score, **or** `contradiction_count`
must be set to a non-zero value to produce a non-trivial score.

The correct resolution: the fixture represents a scenario where contradictions were
detected. Update `contradiction_count: 0` to a value that produces a recognizable score.
The simplest coherent fixture: set `contradiction_count: 15` so
`contradiction_density_score = 1.0 - (15/50) = 0.7000`. This preserves the existing
hardcoded score value and gives the fixture a semantically valid state.

The seven other fixtures all have `contradiction_density_score: 1.0` and
`contradiction_count: 0` — these are consistent with the new semantics (0 pairs → score
1.0) and require no change.

## Component Interactions

```
ContradictionScanCacheHandle (Arc<RwLock<Option<ContradictionScanResult>>>)
    |
    | background tick every ~60 min writes scan results
    |
    v
compute_report() Phase 2 (services/status.rs ~line 583)
    reads cache → sets report.contradiction_count: usize
    reads cache → sets report.contradiction_scan_performed: bool
    |
    | report.contradiction_count passed forward in-struct
    |
    v
compute_report() Phase 5 (services/status.rs ~line 747)
    calls coherence::contradiction_density_score(
        report.contradiction_count,   // <-- THE FIX (was report.total_quarantined)
        report.total_active
    )
    → report.contradiction_density_score: f64
    |
    v
coherence::compute_lambda(
    report.graph_quality_score,
    embed_dim,
    report.contradiction_density_score,
    &coherence::DEFAULT_WEIGHTS,
)
    → report.coherence: f64 (Lambda)
```

**Separate path (unchanged):**
```
compute_report() Phase 5 (~line 784)
    calls coherence::generate_recommendations(
        report.coherence,
        coherence::DEFAULT_LAMBDA_THRESHOLD,
        report.graph_stale_ratio,
        report.embedding_inconsistencies.len(),
        report.total_quarantined,   // <-- UNCHANGED, quarantine recs are correct here
    )
```

## Technology Decisions

No new dependencies. No schema changes. No async boundary changes. The function remains
pure. See ADR-001 for the decision rationale.

## Integration Points

- `ContradictionScanCacheHandle`: defined in `infra/contradiction.rs`. Type alias for
  `Arc<RwLock<Option<ContradictionScanResult>>>`. Not modified by this feature.
- `StatusReport.contradiction_count: usize`: field already exists at `status.rs:533`,
  initialized to `0`, populated in Phase 2. Type matches the new parameter type directly
  — no cast required.
- `StatusReport.total_quarantined: u64`: field remains in `StatusReport` and continues
  to be passed to `generate_recommendations()`. Removed only from the
  `contradiction_density_score()` call site.
- `coherence::DEFAULT_WEIGHTS`: unchanged. `contradiction_density: 0.31` weight is
  correct per ADR-001 crt-048; only the input data is wrong.

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `contradiction_density_score` (new) | `fn(contradiction_pair_count: usize, total_active: u64) -> f64` | `infra/coherence.rs` |
| `contradiction_density_score` (old, removed) | `fn(total_quarantined: u64, total_active: u64) -> f64` | `infra/coherence.rs` |
| `report.contradiction_count` | `usize` | `StatusReport`, `services/status.rs:533` |
| `report.total_quarantined` | `u64` | `StatusReport` — unchanged, still used by `generate_recommendations()` |
| `ContradictionScanResult.pairs` | `Vec<ContradictionPair>` | `infra/contradiction.rs` |

## Phase Ordering Invariant

The ordering is load-bearing. Phase 2 sets `contradiction_count` and Phase 5 reads it.
This is currently guaranteed by sequential code in `compute_report()` (Phase 2 at
~line 583, Phase 5 at ~line 747). The invariant is not type-enforced; a future refactor
that reorders phases would silently break this. This is documented as a known structural
risk (SR-03) — no code change is required for crt-051, but a comment should be added at
the Phase 5 call site to make the dependency explicit:

```rust
// report.contradiction_count is populated in Phase 2 (contradiction cache read);
// Phase 5 must not be reordered above Phase 2. See crt-051 ADR-001.
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

## Open Questions

None. All SCOPE open questions are resolved by the spawn prompt:
- Pair count (not unique entry count) — confirmed by human.
- Cold-start = 1.0 (optimistic) — confirmed by human. This is identical to what the
  code already produces because `contradiction_count` defaults to `0` when the cache is
  `None`.
