# crt-048: Drop Freshness from Lambda — 3-Dimension Coherence Gate

## Problem Statement

Lambda, the composite coherence metric (crt-005), carries four dimensions: confidence
freshness (0.35), graph quality (0.30), contradiction density (0.20), and embedding
consistency (0.15). The freshness dimension scores entries based on wall-clock time since
last access: an entry is "stale" if `max(updated_at, last_accessed_at)` is older than
`DEFAULT_STALENESS_THRESHOLD_SECS` (24 hours). This made freshness a proxy for
maintenance health — if entries go unaccessed, assume they're drifting.

crt-036 (Intelligence-Driven Retention, GH #409) replaced time-based retention with
cycle-based retention. Entries now survive or deprecate based on feature cycle history:
a cycle-reviewed entry outside K retained cycles is pruned; entries that survive K cycles
are considered maintained by definition. This invalidates the wall-clock age proxy.

The observed consequence: `stale_confidence_count` grows monotonically as long-lived,
high-value entries (ADRs, conventions) age past the 24h staleness threshold even while
graph quality improves and cycle-based maintenance correctly keeps them alive. Lambda
trends toward zero for structural reasons disconnected from actual knowledge quality:

```
confidence_freshness: 0.2914 → 0.2414 → 0.1791  (dropping ~0.06/day)
stale_entries:        873 → 943 → 1,027           (growing ~84/day)
graph_quality:        0.9426 → 0.9591             (improving — maintenance working)
```

The heaviest single weight in Lambda (0.35) is actively penalizing durable, proven
entries in favor of newly written, unvalidated entries. This is backwards.

## Decision Taken (GH #520 comment, 2026-04-06)

Option 1 — Drop freshness from Lambda entirely. Lambda becomes a 3-dimension structural
integrity metric: graph quality, contradiction density, embedding consistency. Weights
are re-normalized. The `confidence_freshness` dimension, `CoherenceWeights.confidence_freshness`
field, and `confidence_freshness_score()` call sites are removed from the maintenance
path.

Rationale from GH #520: Lambda is a structural integrity metric for the maintenance gate.
The three surviving dimensions are structural and domain-neutral. Freshness measured
activity (has anything been accessed recently?), not structure — a different question
that belongs elsewhere if at all. Time-based freshness also embeds a domain-specific
cadence assumption (daily cycle) that fails for any non-daily-cadence platform.

Also resolves GH #425 (activity-relative freshness decay research): the research premise
was "how do we fix time-based decay for idle platforms?" The answer is to drop the time
dimension rather than calibrate it.

## Goals

1. Remove `confidence_freshness` as a Lambda input dimension by deleting the
   `CoherenceWeights.confidence_freshness` field and its weight constant.
2. Re-normalize default Lambda weights across the three surviving dimensions:
   graph quality ≈ 0.43, contradiction density ≈ 0.29, embedding consistency ≈ 0.21
   (subject to ADR, must sum to 1.0 with at most 3 decimal places each).
3. Remove the `confidence_freshness_score()` function from `infra/coherence.rs` and its
   call sites in `services/status.rs` (main path and `coherence_by_source` loop).
4. Remove the `stale_confidence_count` and `confidence_freshness_score` fields from
   `StatusReport` and all response serialization/formatting paths.
5. Remove `oldest_stale_age()` from `infra/coherence.rs` and its call site in
   `services/status.rs` (used only for the staleness-based recommendation).
6. Update `generate_recommendations()` to remove the stale-confidence recommendation
   branch that referenced `stale_confidence_count`.
7. Remove the `DEFAULT_STALENESS_THRESHOLD_SECS` constant (24h) from `infra/coherence.rs`
   if no other caller references it.
8. Ensure all existing unit tests in `infra/coherence.rs` pass; delete tests that were
   specific to the freshness dimension; update `lambda_weight_sum_invariant` and related
   tests for 3-dimension weights.
9. Ensure `context_status` output (text/markdown/JSON) no longer reports
   `confidence_freshness` or stale confidence count.

## Non-Goals

- Replacing freshness with a different dimension in this feature. Option 2 (cycle-aware
  dimension) and Option 3 (cycle-relative redefinition) are out of scope. If a new 4th
  dimension is warranted in the future, it is a separate feature.
- Changing the underlying `updated_at` / `last_accessed_at` timestamp fields on entries.
  These timestamps remain for other uses (audit, history); only their use as a Lambda
  input is removed.
- Addressing Type 2 failure (entries retrieved but consistently unhelpful). That is a
  curation signal problem, not Lambda's problem, and is a separate future scope.
- Touching `feature_entries`, `cycle_events`, or `cycle_review_index` tables. crt-036
  already governs cycle-based retention; this feature only fixes Lambda's weights.
- Changing `coherence_by_source` computation logic beyond removing the freshness
  dimension from the per-source lambda calculation.
- Changing the `[inference] freshness_half_life_hours` config field. That governs the
  confidence scoring pipeline's freshness factor (separate from Lambda's staleness
  threshold). Do not touch it.
- Closing GH #425 manually — the issue is already closed; no action needed.
- Changing the Lambda threshold (`DEFAULT_LAMBDA_THRESHOLD = 0.8`) or the maintenance
  recommendation trigger logic beyond removing the stale-confidence branch.

## Background Research

### Lambda Implementation (infra/coherence.rs)

`compute_lambda()` takes four f64 scores plus a `&CoherenceWeights` struct:

```rust
pub struct CoherenceWeights {
    pub confidence_freshness: f64,  // ← remove
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}

pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,   // ← remove; renormalize remaining
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};
```

`confidence_freshness_score()` scans all active entries and returns `(score, stale_count)`
by comparing `max(updated_at, last_accessed_at)` to a 24h staleness threshold. This is
a pure function with no I/O.

`oldest_stale_age()` also scans all active entries; used only for the staleness
recommendation string in `generate_recommendations()`.

### Call Sites (services/status.rs, Phase 5, ~line 695)

```
Phase 5 — Coherence dimensions:
  line 695: confidence_freshness_score(&active_entries, ...) → (freshness_dim, stale_conf_count)
  line 700: report.confidence_freshness_score = freshness_dim
  line 701: report.stale_confidence_count = stale_conf_count
  line 766: oldest_stale_age(&active_entries, ...) → oldest_stale
  line 771: compute_lambda(report.confidence_freshness_score, ...)
  lines 793–804: coherence_by_source loop calls confidence_freshness_score per source
```

### Response Paths

`StatusReport` in `mcp/response/status.rs` exposes:
- `confidence_freshness_score: f64` — public field in struct and JSON serialization
- `stale_confidence_count: u64` — public field in struct and JSON serialization
- Text format: "Coherence: ... (confidence_freshness: ..., ...)" on one line
- Markdown format: "- **Confidence Freshness**: ..." in ### Coherence section
- JSON struct: `confidence_freshness_score`, `stale_confidence_count` fields

All three output paths must be updated. Existing integration tests reference these
fields and must be updated or removed.

### crt-036 Data Now Available

After crt-036, the following cycle-aware data is available in the store:
- `cycle_review_index`: one row per reviewed feature cycle (feature_cycle PK,
  computed_at, raw_signals_available)
- `cycle_events`: lifecycle events keyed by cycle_id
- `sessions.feature_cycle`: attribution of sessions to cycles

This data makes a future cycle-relative freshness dimension _possible_, but it is
explicitly out of scope for this feature per the decision in GH #520.

### ADR-003 (entry #179) — Current Weight Rationale

ADR-003 justified weights as "unequal weights reflecting impact on search quality."
Freshness carried 0.35 as the heaviest dimension because confidence directly affects
every search query. Post-crt-036, that justification no longer applies — cycle-based
retention already ensures entries with no learning value are deprecated. The three
surviving dimensions are all structural.

### Prior Research (GH #425, closed)

GH #425 researched activity-relative freshness decay for idle platforms. It identified
three candidate approaches (cycle-anchored freshness, freeze-aware dampening, two-speed
decay) and deferred until crt-036 shipped. The GH #520 owner decision closes #425 by
choosing "drop the time dimension entirely" over any of the three candidates.

### Lesson Learned (entry #3704)

FRESHNESS_HALF_LIFE_HOURS was previously set to 168h (1 week), causing ADRs and
conventions to score near zero after 26 days. Fixed in bugfix-426 to 8760h (1 year).
This is a separate constant from `DEFAULT_STALENESS_THRESHOLD_SECS` (24h) used in
Lambda. Do not confuse the two: the half-life governs the confidence scoring pipeline;
the staleness threshold governs Lambda's freshness dimension (being removed here).

### Test Infrastructure

`infra/coherence.rs` contains ~30 unit tests. Tests specific to the freshness dimension
(`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`) are deleted. Tests referencing 4-dimension lambda
(`lambda_all_ones`, `lambda_all_zeros`, `lambda_weighted_sum`, `lambda_specific_four_dimensions`,
`lambda_single_dimension_deviation`, `lambda_weight_sum_invariant`) are updated for
3-dimension weights. `lambda_renormalization_without_embedding` and similar tests
remain relevant and may need value updates only.

## Proposed Approach

1. **Update `CoherenceWeights` and `DEFAULT_WEIGHTS`** in `infra/coherence.rs`: remove
   `confidence_freshness` field; update remaining weights to sum to 1.0. Candidate
   re-normalized weights (from 0.30 : 0.20 : 0.15 → normalize to 1.0):
   - graph_quality: 0.30 / 0.65 ≈ 0.4615 → round to 0.46
   - contradiction_density: 0.20 / 0.65 ≈ 0.3077 → round to 0.31
   - embedding_consistency: 0.15 / 0.65 ≈ 0.2308 → round to 0.23
   Exact values are an ADR decision; they must sum to 1.0.

2. **Remove freshness functions** from `infra/coherence.rs`:
   - Delete `confidence_freshness_score()`
   - Delete `oldest_stale_age()`
   - Delete `DEFAULT_STALENESS_THRESHOLD_SECS` constant (if unused elsewhere)

3. **Update `compute_lambda()`** signature: remove `freshness: f64` parameter; remove
   `confidence_freshness` from the weighted sum in both the `Some(embed)` and `None`
   branches.

4. **Update `generate_recommendations()`**: remove `stale_confidence_count` and
   `oldest_stale_age_secs` parameters; remove the stale-confidence recommendation
   branch.

5. **Update `services/status.rs` Phase 5**: remove `confidence_freshness_score()` call
   and variable; remove `oldest_stale_age()` call; update `compute_lambda()` call;
   remove `report.confidence_freshness_score` and `report.stale_confidence_count`
   assignments; remove `confidence_freshness_score()` from the `coherence_by_source`
   loop; update `generate_recommendations()` call.

6. **Update `StatusReport`** in `mcp/response/status.rs` (and any response types):
   remove `confidence_freshness_score` and `stale_confidence_count` fields from struct,
   JSON serialization, text format, and markdown format.

7. **Update all tests** in `infra/coherence.rs` and any integration tests that reference
   the removed fields or functions.

## Acceptance Criteria

- AC-01: `CoherenceWeights` struct contains no `confidence_freshness` field. The struct
  has exactly three fields: `graph_quality`, `embedding_consistency`, `contradiction_density`.
- AC-02: `DEFAULT_WEIGHTS` values sum to 1.0 (within f64 epsilon). No `confidence_freshness`
  field is present.
- AC-03: `confidence_freshness_score()` function does not exist in `infra/coherence.rs`.
  Verified by: `cargo build --workspace` with no "dead code" warning on any freshness
  function.
- AC-04: `oldest_stale_age()` function does not exist in `infra/coherence.rs`.
- AC-05: `compute_lambda()` signature does not include a `freshness: f64` parameter.
- AC-06: `StatusReport` contains no `confidence_freshness_score` field and no
  `stale_confidence_count` field. All three output modes (text, markdown, JSON) omit
  these values. Verified by: existing integration tests updated/passing; grep for
  `confidence_freshness` in `mcp/response/` returns zero matches.
- AC-07: `context_status` called with `maintain=false` returns a Lambda value computed
  from exactly 3 dimensions (graph, contradiction, embedding). Verified by: unit test
  asserting `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS) == 1.0`.
- AC-08: `context_status` called with `maintain=false` (no embedding check) returns
  Lambda computed from 2 dimensions (graph, contradiction), re-normalized. Verified by:
  unit test asserting `compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS) == 1.0`.
- AC-09: `generate_recommendations()` does not accept or reference stale confidence count
  or oldest stale age parameters. The function signature shrinks accordingly.
- AC-10: All existing tests in `infra/coherence.rs` and integration tests pass after
  the change. No new test failures unrelated to this feature.
- AC-11: `DEFAULT_STALENESS_THRESHOLD_SECS` constant is removed if it has no remaining
  callers outside the freshness dimension. (If another caller exists, it may remain with
  a comment noting the Lambda use was removed.)
- AC-12: The ADR recording the new 3-dimension Lambda weights is stored in Unimatrix
  (superseding entry #179).

## Constraints

1. **`compute_lambda()` is called in two places in `services/status.rs`**: the main
   lambda computation and the per-source `coherence_by_source` loop. Both call sites
   must be updated identically — removing the freshness argument from both.

2. **`StatusReport` is serialized in three formats** (text, markdown, JSON). All three
   must be updated; the JSON format is consumed by callers who may parse field names
   directly. Removing a JSON field is a breaking change for any downstream caller
   relying on `confidence_freshness_score` or `stale_confidence_count` in the JSON
   output. This is accepted as correct behavior per the design decision.

3. **`generate_recommendations()` test coverage**: the existing tests
   `recommendations_below_threshold_stale_confidence` will be deleted as the branch it
   tests no longer exists. The remaining recommendation branches (graph stale ratio,
   embedding inconsistencies, quarantined entries) are unchanged.

4. **`active_entries` allocation**: `active_entries` is still needed in Phase 5 for
   `coherence_by_source` grouping by `trust_source`. The variable is not removed — only
   the freshness scan over it is removed.

5. **`load_active_entries_with_tags()` call**: this store read at Phase 5 loads all
   active entries into memory for both the freshness scan and the coherence-by-source
   grouping. After this feature, only coherence-by-source grouping uses the result.
   The store call remains; only the freshness use of the data is removed.

6. **Test count**: the project has 2169+ unit tests. Deleting freshness-specific tests
   is permitted; no test count floor applies to this feature. The overall test suite must
   not regress on unrelated functionality.

7. **No config migration**: `[inference] freshness_half_life_hours` in config is not
   touched by this feature. Operators do not need to change their config files.

## Resolved Decisions (from owner review 2026-04-06)

**OQ-1 — Weights locked: graph=0.46, contradiction=0.31, embedding=0.23 (sum=1.00).**
The GH #520 comment (0.43/0.29/0.21) was a typo; normalizing those values ÷0.93 gives
0.462/0.312/0.226 → rounds to the same answer. The ADR must record the original
0.30:0.20:0.15 ratio (2:1.33:1) as justification — these weights preserve that
structural relationship.

**OQ-2 — Clean removal, no migration window.**
Integration tests in `product/test/` have zero matches for `confidence_freshness` or
`stale_confidence_count`. All callers are Rust unit tests (deleted/updated in this
feature) and historical feature artifacts (crt-005, vnc-008, col-029 pseudocode — inert).
The `ass-038` harness JSONL files contain captured responses with these fields but are
research artifacts, not live callers. Release notes / GH issue should mention the JSON
field removal so operators with custom scripts are not surprised.

**OQ-3 — Retain `coherence_by_source` unchanged.**
Per-source structural Lambda becomes *more* meaningful after freshness removal: currently
a trust_source with many recent writes gets a freshness boost regardless of structural
health. After removal, each source is scored purely on graph quality, embedding
consistency, and contradiction density — a genuinely diagnostic signal. No change beyond
removing freshness from the per-source lambda computation.

## Implementation Notes

**`DEFAULT_STALENESS_THRESHOLD_SECS` survives — do not remove it.**
After the three Lambda-related call sites are removed (lines 698, 769, 796 of
`services/status.rs`), one call site at line 1242 remains. This is inside
`run_maintenance()` — the background tick's confidence refresh path. It identifies
stale entries for confidence score re-computation; it has nothing to do with Lambda.
`StatusParams` no longer carries a `maintain` field (removed in bugfix/252); the
`run_maintenance()` function is called exclusively from `background.rs:961` via the
background tick. The MCP path cannot activate it. The constant and its surviving call
site must not be removed. Add a comment on the constant making the surviving use
explicit: "Used by run_maintenance() confidence refresh targeting — not a Lambda input."

**Test count is larger than the ~10 estimate in Background Research.**
`mcp/response/mod.rs` has six `StatusReport` default-construct instances (approximately
lines 614, 710, 973, 1054, 1137, 1212, 1291) that each explicitly set
`confidence_freshness_score: 1.0` and `stale_confidence_count: 0`. These all fail to
compile when the struct fields are removed — expect approximately 12 field removals in
`mod.rs` test fixtures on top of the ~10 `coherence.rs` test deletions.

## Tracking

https://github.com/dug-21/unimatrix/issues/520
