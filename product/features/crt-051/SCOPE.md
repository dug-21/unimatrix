# crt-051: Fix contradiction_density_score() — Replace Quarantine Proxy with Real Contradiction Count

## Problem Statement

`contradiction_density_score()` in the Lambda coherence metric scores "contradiction
health" as `1.0 - (total_quarantined / total_active)`. Quarantine count has no causal
relationship to contradiction density. Quarantine is a status for entries that have been
flagged for removal (often for policy or quality reasons, not because they contradict
another entry). A knowledge base with zero contradictions but many quarantined entries
receives a misleadingly low Lambda; conversely a database with many real contradictions
but no quarantined entries gets a perfect 1.0 score on this dimension.

Separately, the `contradiction_density` dimension carries a weight of 0.31 (post-crt-048
re-normalization) — the second-highest Lambda weight. This makes the proxy distortion
impactful: it is not a cosmetic rounding error but a structural misallocation of 31% of
the Lambda signal.

Contradicts edges — the data that would make this dimension meaningful — have never been
written to `GRAPH_EDGES` in production. The sole write path (`run_post_store_nli` in
`nli_detection.rs`) was deleted in crt-038 because NLI was disabled and the function was
dead code. The contradiction scan (`scan_contradictions`) runs on a background tick, but
it writes to an in-memory cache (`ContradictionScanCacheHandle`), not to `GRAPH_EDGES`.
The cache is consumed by `context_status` to report detected pairs to operators but has
no influence on `contradiction_density_score()`.

This feature resolves GH #540.

## Goals

1. Replace `contradiction_density_score(total_quarantined, total_active)` with a function
   that uses a meaningful metric: the count of detected contradiction pairs from the
   contradiction scan cache.
2. Ensure the contradiction dimension tracks actual knowledge-base contradiction health,
   not an unrelated quarantine counter.
3. Preserve the Lambda weight for the contradiction dimension (0.31) — the dimension is
   worth retaining; only its input data source is wrong.
4. Keep the function pure and testable (no I/O, deterministic given inputs).
5. Update all callers and tests to reflect the new signature and semantics.
6. Confirm no Lambda regression for healthy databases (zero contradictions → score 1.0).

## Non-Goals

- Changing Lambda weights. The 0.31 weight for contradiction_density is intentional
  (ADR-001 crt-048) and is not being revisited here.
- Writing Contradicts edges to `GRAPH_EDGES`. The scan-based approach already detects
  contradictions; unblocking Contradicts edge persistence is a separate future decision
  with its own NLI infrastructure requirements.
- Re-enabling the NLI contradiction-write path (`run_post_store_nli`). That path was
  deleted in crt-038 for sound reasons; crt-051 does not reverse that decision.
- Changing the contradiction scan itself (`scan_contradictions` in `infra/contradiction.rs`,
  `ContradictionScanCacheHandle`). The scan is already correct; its outputs are simply not
  reaching `contradiction_density_score()`.
- Removing the quarantine counter from the status report. `total_quarantined` remains in
  `StatusReport` and in `generate_recommendations()` — it is useful to operators, just not
  as a Lambda input.
- Changing the `context_status` output schema or any JSON response shape beyond what falls
  out of the signature change.
- Implementing a new contradiction detection strategy (e.g. cosine-only, NLI-based).

## Background Research

### contradiction_density_score() — Confirmed Broken

**Location:** `crates/unimatrix-server/src/infra/coherence.rs:68–74`

```rust
pub fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64 {
    if total_active == 0 {
        return 1.0;
    }
    let score = 1.0 - (total_quarantined as f64 / total_active as f64);
    score.clamp(0.0, 1.0)
}
```

The call site in `status.rs:747–748`:
```rust
report.contradiction_density_score =
    coherence::contradiction_density_score(report.total_quarantined, report.total_active);
```

This is confirmed wrong in production. `total_quarantined` is read from the `COUNTERS`
table (via `read_counter("total_quarantined")`). It counts entries in `Quarantined` status
— no connection to whether those entries contradict anything.

### Lambda Weight for Contradiction Dimension

`DEFAULT_WEIGHTS` in `coherence.rs`:
```rust
pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    graph_quality: 0.46,
    embedding_consistency: 0.23,
    contradiction_density: 0.31,
};
```

Weight 0.31 is the second-highest Lambda weight. The ADR (crt-048 ADR-001, entry #4199)
re-normalized from the original `contradiction_density: 0.20 / 0.65 ≈ 0.31` after
removing the freshness dimension. The weight is structurally sound; the input data is not.

### Contradicts Edges — Confirmed Zero in Production

Three confirmation paths:

1. **crt-038 SCOPE.md (background research section)**: "no Contradicts edges have ever
   been written" and "0 Contradicts edges ever written" (confirmed by ASS-037 research).

2. **crt-038 agent-5 report (dead code removal)**: `run_post_store_nli` — the function
   that wrote Contradicts edges — was deleted from `nli_detection.rs`. The file was
   reduced from 1,374 lines to 120 lines. No write path for Contradicts edges remains in
   production code.

3. **Migration test (`migration_v12_to_v13.rs:617–620`)**:
   ```rust
   assert_eq!(
       count_graph_edges_by_type(&store, "Contradicts").await, 0,
       "migration must write zero Contradicts edges (AC-08)"
   );
   ```
   The migration comment explicitly documents: "No Contradicts bootstrap. All Contradicts
   edges are created at runtime by W1-2 NLI." Since NLI is disabled and the write path
   was deleted, runtime writes are also zero.

4. **nli_detection_tick.rs comment (line 155)**: `"Never writes Contradicts edges (C-13 / AC-10a)."`

### contradiction_cache — What It Stores

`ContradictionScanCacheHandle` (`Arc<RwLock<Option<ContradictionScanResult>>>`) holds the
result of `scan_contradictions()` — a heuristic-based scan using HNSW nearest-neighbor
search + negation/directive/sentiment signals (not NLI). It produces `Vec<ContradictionPair>`,
where each pair has `entry_id_a`, `entry_id_b`, `similarity`, `conflict_score`.

**The scan runs.** It is gated on `current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`
(every 4 ticks = ~60 min). It fires every tick 0, 4, 8... including on startup. After the
first scan, `ContradictionScanResult { pairs }` is written to the cache.

**But its output is not used by Lambda.** `status.rs:583–591` reads the cache and sets
`report.contradiction_count = result.pairs.len()` and `report.contradictions = result.pairs.clone()`.
These fields are in the response payload but are NOT passed to `contradiction_density_score()`.

**This is the fix:** wire `contradiction_count` (or a derived score) through to
`contradiction_density_score()` instead of `total_quarantined`.

### col-030 Collision Suppression — Dependency Analysis

`suppress_contradicts()` in `graph_suppression.rs` reads `Contradicts` edges from
`TypedRelationGraph`, which is populated from `GRAPH_EDGES`. Since there are zero
Contradicts edges in production GRAPH_EDGES, `suppress_contradicts()` silently does
nothing on every search. This is a pre-existing no-op; crt-051 does not change this.

col-030 does NOT depend on `contradiction_density_score()` and does NOT read the
contradiction cache. col-030 reads GRAPH_EDGES via `TypedRelationGraph`. crt-051 does not
touch GRAPH_EDGES or TypedRelationGraph. There is no coupling between crt-051 and col-030.

### Repair Option Analysis

**Option A: Replace quarantine count with contradiction pair count from cache**

Replace signature:
```rust
pub fn contradiction_density_score(
    contradiction_pair_count: usize,
    total_active: u64,
) -> f64
```

Score: `1.0 - (contradiction_pair_count / total_active)`. When scan hasn't run yet
(cold-start, `contradiction_count = 0`): score = 1.0 (optimistic default — healthy
assumption until evidence of contradictions).

Caller change: pass `report.contradiction_count` (set from cache read in Phase 2) instead
of `report.total_quarantined`. The cache is already read before the Lambda computation in
`compute_report()` (Phase 2 runs before Lambda at Phase 5).

**Pros:** Directly uses detected contradictions. Semantically correct. Simple change.
Cold-start behavior is reasonable (optimistic). Scan already runs; no new infrastructure.

**Cons:** `contradiction_pair_count / total_active` can exceed 1.0 if many contradictions
are detected (already handled by `.clamp(0.0, 1.0)`). Score degrades proportionally with
detected contradictions, not with pair density relative to N^2 space.

**Option B: Remove the dimension entirely**

Set `contradiction_density = 1.0` always (or zero-weight the dimension). Re-normalize
Lambda over graph_quality and embedding_consistency.

**Pros:** Honest — if we have no reliable data, report no signal.
**Cons:** Wastes the existing scan infrastructure. The scan does produce contradiction
pairs; ignoring them is a step backward. Reduces Lambda to 2 dimensions (graph, embedding)
which is a weaker health signal.

**Option C: Use scan results only when cache is warm; fall back to 1.0**

Same as Option A but explicit: if cache is None (cold-start), score = 1.0 (no penalty
from unknown state). If cache has data, compute from `pairs.len()`.

This is functionally identical to Option A since `contradiction_count` is 0 (not None)
when cold-start (Phase 2 initializes `contradiction_count: 0` in the default report, only
sets it when `result` is `Some`). Option A and C converge.

**Recommendation: Option A.** The fix is minimal, semantically correct, and uses
infrastructure already in place. The cold-start behavior (score = 1.0 when no scan has
run) is a reasonable optimistic default.

### Impact of Current Bug on Lambda Quality

With zero Contradicts edges and a typical Unimatrix deployment:
- `total_quarantined` is normally small (0–5 entries) relative to `total_active`
- Score = `1.0 - (small / large)` ≈ 1.0 in practice
- The bug likely has low numeric impact on Lambda today because the quarantine count
  is small

However the dimension is **semantically meaningless** regardless of numeric impact.
Operators reading Lambda as a coherence signal cannot trust that `contradiction_density`
reflects contradiction health. A future deployment that quarantines many entries for
non-contradiction reasons would receive an incorrectly penalized Lambda.

The fix is correctness-driven, not crisis-driven. The bug is real and confirmed; the
urgency is semantic integrity.

## Proposed Approach

**Step 1 — Change `contradiction_density_score()` signature and semantics (coherence.rs)**

Replace the `total_quarantined: u64` parameter with `contradiction_pair_count: usize`.
Score formula: `1.0 - (contradiction_pair_count as f64 / total_active as f64)`.
Zero active entries → 1.0 (existing guard).
Cold-start (pair_count = 0) → 1.0 (no penalty from absence of scan data).

**Step 2 — Update caller in compute_report() (status.rs)**

Replace `report.total_quarantined` with `report.contradiction_count` at the
`contradiction_density_score` call site. `report.contradiction_count` is set in Phase 2
from the contradiction cache — which runs before the Lambda computation in Phase 5.
Ordering is already correct; no sequencing changes needed.

**Step 3 — Update `generate_recommendations()` — no change needed**

`generate_recommendations()` already takes `total_quarantined: u64` as a separate
parameter and uses it to recommend "review quarantined entries". This is distinct from the
Lambda input and should be kept. No change.

**Step 4 — Update all tests**

- `coherence.rs` unit tests: update `contradiction_density_score` tests to use pair counts
  instead of quarantine counts. Semantics change; test names and assertions must reflect
  the new meaning.
- `status.rs` integration tests: update any assertion on `contradiction_density_score`
  behavior that passes `total_quarantined`.
- Doc comments on `contradiction_density_score()` must be updated.

No schema changes. No new tables. No new dependencies. No behavioral change to the
contradiction scan itself.

## Acceptance Criteria

- AC-01: `contradiction_density_score()` signature is
  `fn contradiction_density_score(contradiction_pair_count: usize, total_active: u64) -> f64`.
  The `total_quarantined` parameter is removed.
- AC-02: `contradiction_density_score(0, N)` returns `1.0` for any `N > 0` (cold-start
  or no contradictions detected).
- AC-03: `contradiction_density_score(0, 0)` returns `1.0` (empty database guard,
  unchanged behavior).
- AC-04: `contradiction_density_score(N, N)` returns `0.0` (all entries contradicted,
  clamped at zero).
- AC-05: For `pair_count > 0` and `total_active > pair_count`, the score is strictly
  between 0.0 and 1.0.
- AC-06: The call site in `status.rs::compute_report()` passes `report.contradiction_count`
  (not `report.total_quarantined`) to `contradiction_density_score()`. Confirmed by
  reading the updated call site.
- AC-07: `report.contradiction_count` is populated from the contradiction cache in Phase 2
  of `compute_report()` before the Lambda computation in Phase 5. The ordering invariant
  (Phase 2 before Lambda) is preserved.
- AC-08: `generate_recommendations()` still receives `total_quarantined` as its parameter
  (unchanged); the quarantine recommendation path is not altered.
- AC-09: `total_quarantined` is NOT passed to `contradiction_density_score()` anywhere in
  the codebase. Confirmed by `grep` / Grep absence.
- AC-10: All unit tests in `coherence.rs` pass. Tests for `contradiction_density_score()`
  assert the new semantics (pair-count-based, not quarantine-based).
- AC-11: `cargo test --workspace` passes with zero failures.
- AC-12: `cargo clippy --workspace -- -D warnings` passes with zero warnings.
- AC-13: The doc comment on `contradiction_density_score()` describes the new semantics:
  "fraction of contradiction-free health based on detected pair count from the contradiction
  scan cache." The old "quarantined-to-active ratio" description is removed.

## Constraints

- `contradiction_pair_count` and `total_active` are the only inputs to
  `contradiction_density_score()`. No I/O, no async, no side effects. The function must
  remain pure.
- The contradiction cache is an in-memory `Arc<RwLock<Option<ContradictionScanResult>>>`.
  The read lock in Phase 2 of `compute_report()` is already implemented and must not be
  duplicated. Pass the extracted `contradiction_count: usize` (already a `usize` field on
  `StatusReport`) to `contradiction_density_score()`.
- `contradiction_pair_count / total_active` can exceed 1.0 in degenerate cases (many
  pairs from a small active set). The existing `.clamp(0.0, 1.0)` handles this; no
  additional guard is needed.
- No schema migration. `total_quarantined` counter in `COUNTERS` table is unaffected.
- `StatusReport.contradiction_count` field type is `usize` (confirmed from `status.rs`
  initialization at line 533: `contradiction_count: 0`). The `contradiction_density_score()`
  parameter must accept `usize` directly without a cast concern.
- Potential type annotation issue: `contradiction_pair_count as f64` requires a cast from
  `usize`. This is fine — identical pattern to `stale_count as f64` in `graph_quality_score`.

## Open Questions

1. **Normalization strategy:** `1.0 - (pair_count / total_active)` treats each entry as
   independently tainted by any contradiction it participates in. An alternative is
   `1.0 - (unique_entries_in_pairs / total_active)` — penalizing by number of affected
   entries, not raw pair count. For small contradiction counts the difference is negligible;
   delivery should confirm which interpretation the human intends.

2. **Cold-start handling:** On a new deployment the contradiction scan hasn't run yet
   (first scan fires at tick 0, which is immediately on startup, but before the first
   tick `compute_report()` could be called). Returning 1.0 (optimistic) when
   `contradiction_count = 0` is the correct behavior, but should this be distinguished
   from "scan ran and found zero contradictions"? Currently the cache is `None` until the
   first scan completes; after completion it's `Some(ContradictionScanResult { pairs: [] })`.
   Both produce `contradiction_count = 0`. The score is 1.0 in both cases, which is
   correct. No action likely needed, but delivery should confirm.

3. **StatusReport JSON response shape:** `contradiction_density_score` is a field in
   `StatusReport` (populated after Lambda computation). The field name and value type
   (`f64`) do not change — only its value will change on deployments that have detected
   contradictions. This is semantically a breaking change in reporting semantics (operators
   who assumed the dimension measured quarantine density) but not a breaking JSON schema
   change. Confirm with human whether release notes or API documentation need updating.

## Tracking

https://github.com/dug-21/unimatrix/issues/540
