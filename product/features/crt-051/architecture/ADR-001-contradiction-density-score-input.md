## ADR-001: contradiction_density_score uses scan pair count instead of quarantine counter

Feature: crt-051
Status: Accepted

### Context

`contradiction_density_score()` in `infra/coherence.rs` computes the contradiction
dimension of the Lambda coherence metric. Lambda uses this dimension at weight 0.31 —
the second-highest of three dimensions (graph_quality 0.46, embedding 0.23,
contradiction 0.31, per ADR-001 crt-048, Unimatrix entry #4199).

The function currently accepts `total_quarantined: u64` as its primary input and scores
as `1.0 - (total_quarantined / total_active)`. Quarantine count is a status counter for
entries flagged for policy or quality review via `context_quarantine`. It has no causal
relationship to whether entries semantically contradict each other. A knowledge base
with many quarantined entries (e.g., from a bulk policy purge) receives an incorrectly
penalized Lambda contradiction dimension; a knowledge base with many real contradictions
but no quarantined entries receives a perfect 1.0 score on this dimension.

The contradiction scan (`scan_contradictions` in `infra/contradiction.rs`) runs on a
background tick every ~60 minutes. It uses HNSW nearest-neighbour search plus
negation/directive/sentiment signals to produce `Vec<ContradictionPair>`. Its output is
written to `ContradictionScanCacheHandle` (`Arc<RwLock<Option<ContradictionScanResult>>>`).
`compute_report()` Phase 2 already reads this cache and populates
`report.contradiction_count: usize` (the length of detected pairs). However,
`contradiction_count` is never passed to `contradiction_density_score()`. Instead,
`total_quarantined` is passed — an unrelated counter.

This is the bug: the correct data exists, is already extracted in Phase 2, and sits in
`report.contradiction_count`, but is not wired into the Lambda computation.

Note: Contradicts edges in `GRAPH_EDGES` have never been written in production. The
write path (`run_post_store_nli`) was deleted in crt-038. The scan-based cache is the
only available source of contradiction evidence and is sufficient for this fix.

### Decision

Replace the `total_quarantined: u64` parameter with `contradiction_pair_count: usize`
in `contradiction_density_score()`. Use `report.contradiction_count` at the call site
in `compute_report()` Phase 5 instead of `report.total_quarantined`.

**New signature:**
```rust
pub fn contradiction_density_score(
    contradiction_pair_count: usize,
    total_active: u64,
) -> f64
```

**Formula:** `1.0 - (contradiction_pair_count as f64 / total_active as f64)`, clamped
to [0.0, 1.0]. Structure is identical to the old formula; only the first argument's
meaning changes.

**Cold-start behavior:** When the background scan has not yet completed (server just
started), `ContradictionScanCacheHandle` is `None`. Phase 2 leaves
`report.contradiction_count` at its default of `0`. `contradiction_density_score(0, N)`
returns `1.0` for any `N > 0`. This is an optimistic default: no penalty from absence
of scan data. It is semantically correct — "we have no evidence of contradictions" is
not the same as "contradictions exist." The `contradiction_scan_performed: bool` field
in `StatusReport` gives operators visibility into whether the scan has run.

**Normalization choice — pair count, not unique entry count:** The formula uses raw pair
count (`pairs.len()`) rather than the count of unique entries that appear in any pair.
Rationale: pair count is directly available from the cache without a second pass over
the data; it is simpler and unambiguous; at expected contradiction counts (single digits
to low tens) the difference between pair count and unique entry count is negligible. A
knowledge base with N detected pairs and M active entries scores `1.0 - N/M`. This
choice was confirmed by the human in the crt-051 spawn prompt.

**Call site change (services/status.rs ~line 747):**
```rust
// Before
report.contradiction_density_score =
    coherence::contradiction_density_score(report.total_quarantined, report.total_active);

// After — report.contradiction_count is set in Phase 2 (contradiction cache read);
// Phase 5 must not be reordered above Phase 2. See crt-051 ADR-001.
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

**generate_recommendations() is unchanged:** It receives `total_quarantined: u64`
separately and uses it to recommend "review quarantined entries." Quarantine management
is distinct from Lambda scoring and must not be conflated.

**GRAPH_EDGES writing is out of scope:** Re-enabling the `run_post_store_nli` write path
or adding a new path to persist Contradicts edges to `GRAPH_EDGES` would require NLI
infrastructure that was deliberately removed in crt-038. The scan-based cache is an
independent detection mechanism. Wiring cache output to Lambda is orthogonal to edge
persistence. Edge persistence is deferred to a future decision when NLI infrastructure
is re-evaluated.

**Fixture correction in `mcp/response/mod.rs` (SR-02):** The `make_coherence_status_report()`
fixture has `total_quarantined: 3`, `contradiction_count: 0`, and a hardcoded
`contradiction_density_score: 0.7000`. Under the new semantics, `contradiction_count: 0`
produces score `1.0`, not `0.7000`. To preserve the non-trivial score value (which tests
formatting of a sub-1.0 score), set `contradiction_count: 15` with `total_active: 50`:
`1.0 - 15/50 = 0.7000`. The seven other fixtures in `response/mod.rs` have
`contradiction_density_score: 1.0` and `contradiction_count: 0` — consistent with the
new semantics and require no change.

### Consequences

**Easier:**
- Lambda contradiction dimension now reflects actual knowledge-base contradiction health
  as detected by the background scan, not an unrelated quarantine counter.
- Operators can trust that a low `contradiction_density_score` indicates detected
  semantic conflicts, not administrative quarantine activity.
- Healthy deployments with zero detected contradictions continue to receive score 1.0
  (no regression for the common case).
- The scan infrastructure already runs; no new infrastructure is required.

**Harder / Known Limitations:**
- The score is bounded by cache freshness (~60 min staleness window). A burst of
  contradictions added between scans will not affect Lambda until the next scan
  completes. This is a pre-existing property of the scan architecture, not introduced
  by this change (SR-07).
- The contradiction_scan_performed = false state (cold-start window) is indistinguishable
  from "scan ran and found zero contradictions" at the score level (both → 1.0).
  Operators who need this distinction must inspect the `contradiction_scan_performed`
  boolean in the status response.
- Pair count normalization (`pairs / active`) does not account for multi-entry
  contradiction clusters. A single entry contradicting many others produces many pairs
  but only one "bad actor." This is accepted as a simplification at current scale.
