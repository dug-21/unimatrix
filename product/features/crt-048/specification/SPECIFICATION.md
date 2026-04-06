# Specification: crt-048 — Drop Freshness from Lambda (3-Dimension Coherence Gate)

**Feature:** crt-048
**GitHub Issue:** #520
**Status:** Approved for delivery
**Supersedes:** ADR-003 (Unimatrix entry #179)

---

## Objective

Lambda, Unimatrix's composite coherence health metric (introduced in crt-005), currently
combines four dimensions to produce a score in [0.0, 1.0]. The freshness dimension (weight
0.35) scores entries on wall-clock recency, invalidated by crt-036's cycle-based retention
which makes wall-clock age irrelevant to actual knowledge quality. This feature removes the
freshness dimension entirely, leaving Lambda as a three-dimension structural integrity
metric — graph quality, contradiction density, and embedding consistency — with re-normalized
weights that preserve the original structural weight ratios.

---

## Functional Requirements

### Removal Requirements

**FR-01** — `CoherenceWeights` struct in `infra/coherence.rs` must contain exactly three
fields: `graph_quality: f64`, `contradiction_density: f64`, `embedding_consistency: f64`.
The `confidence_freshness: f64` field must not exist.

**FR-02** — `DEFAULT_WEIGHTS` constant must assign values summing to 1.0 within f64
epsilon: `graph_quality = 0.46`, `contradiction_density = 0.31`,
`embedding_consistency = 0.23`. No `confidence_freshness` field may appear.

**FR-03** — `confidence_freshness_score()` function must be deleted from `infra/coherence.rs`.
No dead-code reference to it may remain in any crate.

**FR-04** — `oldest_stale_age()` function must be deleted from `infra/coherence.rs`. No
dead-code reference to it may remain in any crate.

**FR-05** — `compute_lambda()` must accept exactly three dimension scores plus weights:
`(graph: f64, embedding: Option<f64>, contradiction: f64, weights: &CoherenceWeights) -> f64`.
The `freshness: f64` positional parameter must not appear in the signature.

**FR-06** — `generate_recommendations()` must not accept `stale_confidence_count` or
`oldest_stale_age_secs` parameters. The stale-confidence recommendation branch must be
deleted. The remaining recommendation branches (graph stale ratio, embedding inconsistencies,
quarantined entries) are unchanged.

**FR-07** — `services/status.rs` Phase 5 must not call `confidence_freshness_score()` or
`oldest_stale_age()`. Both call sites in the main Lambda computation path must be removed.
The two `compute_lambda()` call sites (main path and `coherence_by_source` loop) must both
pass three dimensions, not four.

**FR-08** — `StatusReport` in `mcp/response/status.rs` must contain no
`confidence_freshness_score: f64` field and no `stale_confidence_count: u64` field. These
fields must be removed from the struct definition and all struct initialization sites.

**FR-09** — All three `context_status` output modes must omit freshness data:
- Text format: remove `confidence_freshness: ...` component from the coherence line.
- Markdown format: remove the `**Confidence Freshness**` bullet from the `### Coherence`
  section.
- JSON serialization: `confidence_freshness_score` and `stale_confidence_count` keys must
  not appear in the serialized output.

### Retention Requirements

**FR-10** — `DEFAULT_STALENESS_THRESHOLD_SECS` constant must be retained in
`infra/coherence.rs` with a comment making its surviving use explicit. The constant is
consumed by `run_maintenance()` in the background confidence refresh tick (not a Lambda
input). Removing it would require a hardcoded literal in `background.rs`. The constant
must not be removed even if the three Lambda call sites are the only ones a naive search
finds — the surviving call site is in `run_maintenance()`.

**FR-11** — `load_active_entries_with_tags()` call in `services/status.rs` Phase 5 must
be retained. After this feature, it serves only the `coherence_by_source` grouping by
`trust_source`. The data allocation is not removed; only the freshness scan over it is
removed.

**FR-12** — `coherence_by_source` computation must be retained unchanged in logic. Per-source
Lambda is now computed from three structural dimensions only. The `coherence_by_source`
loop call to `compute_lambda()` must pass the updated three-parameter signature, identical
to the main path update.

**FR-13** — The `[inference] freshness_half_life_hours` configuration field must not be
touched. It governs the confidence scoring pipeline's freshness factor (a separate
subsystem from Lambda). Operators do not need to change any config files.

**FR-14** — `updated_at` and `last_accessed_at` timestamp fields on entries must not be
removed or modified. These timestamps remain for audit, history, and `run_maintenance()`
use; only their role as a Lambda input is eliminated.

### Test Requirements

**FR-15** — The following tests in `infra/coherence.rs` must be deleted as they test
removed functionality:
- `freshness_empty_entries`
- `freshness_all_stale`
- `freshness_none_stale`
- `freshness_uses_max_of_timestamps`
- `freshness_recently_accessed_not_stale`
- `freshness_both_timestamps_older_than_threshold`
- `oldest_stale_no_stale`
- `oldest_stale_one_stale`
- `oldest_stale_both_timestamps_zero`
- `staleness_threshold_constant_value`
- `recommendations_below_threshold_stale_confidence` (tests the deleted recommendation branch)

**FR-16** — The following tests in `infra/coherence.rs` must be updated for three-dimension
weights and signatures:
- `lambda_all_ones`
- `lambda_all_zeros`
- `lambda_weighted_sum`
- `lambda_specific_four_dimensions` (rename to `lambda_specific_three_dimensions`)
- `lambda_single_dimension_deviation`
- `lambda_weight_sum_invariant` (must use epsilon comparison, not exact equality — see NFR-04)

**FR-17** — All `StatusReport` default-construct instances in `mcp/response/mod.rs`
(approximately six sites, each setting `confidence_freshness_score: 1.0` and
`stale_confidence_count: 0`) must have those field assignments removed. Failure to remove
all sites produces a compile error, not a test failure.

**FR-18** — `cargo build --workspace` must succeed with zero errors and zero warnings
related to removed or dead freshness symbols.

---

## Non-Functional Requirements

**NFR-01** — Lambda output range is unchanged: `compute_lambda()` must continue to return
values in [0.0, 1.0] for all valid inputs. Three-dimension and two-dimension (embedding
absent) calls must both produce values within this range.

**NFR-02** — Lambda computation performance is unchanged. The three-dimension weighted sum
is strictly simpler than the four-dimension sum. No performance regression is expected or
acceptable.

**NFR-03** — Phase 5 of `context_status` reduces in memory allocation pressure. The
`confidence_freshness_score()` function scanned all active entries; that scan is removed.
The `load_active_entries_with_tags()` allocation itself remains (for `coherence_by_source`),
but one full pass over all entries is eliminated.

**NFR-04** — Weight constant correctness: `DEFAULT_WEIGHTS.graph_quality +
DEFAULT_WEIGHTS.contradiction_density + DEFAULT_WEIGHTS.embedding_consistency` must equal
1.0 within f64 epsilon. The `lambda_weight_sum_invariant` test must use
`(sum - 1.0_f64).abs() < f64::EPSILON` comparison. Exact `==` comparison is forbidden for
this sum due to f64 representation of 0.46 + 0.31 + 0.23.

**NFR-05** — No regression in any test category unrelated to the freshness dimension.
The total test suite (`cargo test --workspace`) must pass. Freshness-specific test
deletions are permitted; no other test count floor applies.

**NFR-06** — Breaking JSON change is intentional and accepted per OQ-2 resolution. The
`confidence_freshness_score` and `stale_confidence_count` fields are removed from JSON
output with no migration window. Release notes for the PR must explicitly list these two
removed JSON keys so operators with custom parsing scripts are informed.

---

## Acceptance Criteria

**AC-01** — `CoherenceWeights` struct contains exactly three fields: `graph_quality`,
`embedding_consistency`, `contradiction_density`. No `confidence_freshness` field exists.
Verification: `grep -r "confidence_freshness" crates/` in the Rust source returns zero
matches post-delivery.

**AC-02** — `DEFAULT_WEIGHTS` values sum to 1.0 within f64 epsilon with exact literals
`graph_quality: 0.46`, `contradiction_density: 0.31`, `embedding_consistency: 0.23`.
No `confidence_freshness` field present. Verification: `lambda_weight_sum_invariant` test
passes using `(sum - 1.0_f64).abs() < f64::EPSILON`.

**AC-03** — `confidence_freshness_score()` function does not exist anywhere in
`infra/coherence.rs`. Verification: `cargo build --workspace` emits no "dead code" warning
for any freshness function; grep for `confidence_freshness_score` in `crates/` returns
zero matches.

**AC-04** — `oldest_stale_age()` function does not exist anywhere in `infra/coherence.rs`.
Verification: grep for `oldest_stale_age` in `crates/` returns zero matches.

**AC-05** — `compute_lambda()` signature contains no `freshness: f64` parameter. The
function accepts `(graph: f64, embedding: Option<f64>, contradiction: f64,
weights: &CoherenceWeights)`. Verification: function signature grep; all call sites compile
without passing a freshness argument.

**AC-06** — `StatusReport` contains no `confidence_freshness_score` field and no
`stale_confidence_count` field. Text, markdown, and JSON output modes all omit these
values. Verification: grep for `confidence_freshness` in `crates/unimatrix-server/src/mcp/`
returns zero matches; integration tests updated and passing.

**AC-07** — `context_status` with `maintain=false` and embedding enabled returns a Lambda
value computed from exactly three dimensions summing to correct weight. Verification:
unit test `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS) == 1.0` passes.

**AC-08** — `context_status` with embedding absent (None) returns Lambda computed from
two dimensions (graph and contradiction) with re-normalized weights summing to 1.0.
Verification: unit test `compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS) == 1.0` passes.

**AC-09** — `generate_recommendations()` does not accept or reference `stale_confidence_count`
or `oldest_stale_age_secs` parameters. The function signature shrinks accordingly.
Verification: function signature inspection; `cargo build --workspace` succeeds.

**AC-10** — All remaining tests in `infra/coherence.rs` pass. No new test failures
outside the explicitly deleted freshness test set. Verification: `cargo test -p unimatrix-server`
(or equivalent crate path) passes with zero unexpected failures.

**AC-11** — `DEFAULT_STALENESS_THRESHOLD_SECS` constant is retained with a comment:
"Used by run_maintenance() confidence refresh targeting — not a Lambda input." The constant
must not be removed. Verification: grep for `DEFAULT_STALENESS_THRESHOLD_SECS` in
`infra/coherence.rs` returns exactly one definition; `run_maintenance()` reference
compiles without a literal substitution.

**AC-12** — An ADR recording the new three-dimension Lambda weights is stored in Unimatrix
via `context_correct` superseding entry #179. The ADR records: exact weight literals
(0.46, 0.31, 0.23), the original structural ratio (2 : 1.33 : 1 from 0.30 : 0.20 : 0.15),
the rationale (crt-036 cycle-based retention invalidates wall-clock freshness), and a
reference to GH #520. Verification: `context_get` on the new ADR entry returns all four
data points; entry #179 shows status "deprecated" with a superseded-by link.

**AC-13** — The `coherence_by_source` per-source Lambda uses the updated three-dimension
`compute_lambda()` signature. Both the main-path call site and the per-source loop call
site are updated identically. Verification: code review confirms both call sites; unit or
integration test for `coherence_by_source` output remains passing.

**AC-14** — `mcp/response/mod.rs` compiles with all `StatusReport` default-construct
fixture sites updated (approximately six sites, approximately twelve field removals).
Verification: `cargo build --workspace` succeeds — compile errors, not test failures,
indicate incomplete removal.

---

## Domain Models

### CoherenceWeights (post-crt-048)

```
CoherenceWeights {
    graph_quality: f64,          // weight 0.46 — ratio of well-connected entries
    contradiction_density: f64,  // weight 0.31 — inverse of contradiction rate
    embedding_consistency: f64,  // weight 0.23 — embedding vector coherence (opt-in)
}
```

**Invariant:** `graph_quality + contradiction_density + embedding_consistency` equals 1.0
within f64 epsilon for DEFAULT_WEIGHTS. Custom weight structs must also satisfy this
invariant; enforcement is via the `lambda_weight_sum_invariant` test.

**Removed field:** `confidence_freshness: f64` (weight was 0.35). Governed
`confidence_freshness_score()` which scanned `max(updated_at, last_accessed_at)` against
a 24h threshold. Invalidated by crt-036: cycle-based retention makes wall-clock age
uncorrelated with knowledge quality.

### Lambda

Lambda (λ) is the composite coherence health scalar in [0.0, 1.0]. It is computed by
`compute_lambda()` as a weighted sum of the three structural dimensions. When
`embedding: Option<f64>` is `None`, the embedding weight is redistributed by re-normalizing
the remaining two weights — the same re-normalization logic introduced in crt-005 for
unavailable dimensions.

Lambda is:
- The primary input to the maintenance gate decision in `context_status`
- Compared against `DEFAULT_LAMBDA_THRESHOLD = 0.8` for maintenance recommendations
- Computed per-source in `coherence_by_source` for diagnostic per-`trust_source` breakdown
- NOT an input to confidence scoring or search re-ranking

Lambda is not:
- An activity metric (freshness removed)
- A measure of how recently entries were accessed
- Affected by entry count growth over time (structural metrics do not monotonically decay)

### StatusReport (post-crt-048)

`StatusReport` exposes Lambda and its component dimensions. The following fields are
removed:

| Removed Field               | Type  | Was Used For                          |
|-----------------------------|-------|---------------------------------------|
| `confidence_freshness_score`| f64   | Lambda freshness dimension value      |
| `stale_confidence_count`    | u64   | Count of entries past 24h threshold   |

These fields are absent from the struct, all three output modes, and all JSON serialization.
Any downstream script parsing `context_status` JSON output for these keys must be updated.

### DEFAULT_STALENESS_THRESHOLD_SECS

This constant (24 * 3600 seconds) survives this feature. It is not a Lambda input after
crt-048. Its sole remaining use is in `run_maintenance()` — the background tick's
confidence refresh pass, which targets entries whose confidence score may be stale. This
is a separate subsystem from Lambda. The constant must carry an explanatory comment to
prevent future removal.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| Lambda (λ) | Composite coherence health scalar [0,1]; 3-dimension post-crt-048 |
| Structural dimension | Lambda input derived from graph or embedding structure, domain-neutral |
| Freshness dimension | Removed: Lambda input derived from wall-clock access recency |
| `coherence_by_source` | Per-`trust_source` Lambda breakdown in `StatusReport` |
| `run_maintenance()` | Background tick function; confidence refresh; not a Lambda caller |
| `DEFAULT_LAMBDA_THRESHOLD` | 0.8 — maintenance recommendation trigger (unchanged) |
| `DEFAULT_STALENESS_THRESHOLD_SECS` | 86400s — used only by `run_maintenance()` post-crt-048 |

---

## User Workflows

### Workflow: Operator runs `context_status`

1. Operator (or agent) calls `context_status` (with `maintain=false` or `maintain=true`).
2. Server executes Phase 5 of the status computation in `services/status.rs`.
3. Phase 5 calls `load_active_entries_with_tags()` (retained for `coherence_by_source`).
4. Phase 5 does NOT call `confidence_freshness_score()` or `oldest_stale_age()`.
5. `compute_lambda()` is called with three arguments: `graph`, `embedding` (Option), `contradiction`.
6. `generate_recommendations()` is called without stale confidence parameters.
7. Output is returned in the requested format (text/markdown/JSON).
8. The coherence section reports three dimensions; `confidence_freshness` and
   `stale_confidence_count` are absent from all formats.

**Observable difference from pre-crt-048:** Lambda value is higher for long-lived entries
(ADRs, conventions) that previously incurred freshness penalty. The coherence section is
shorter by two values. `stale_confidence_count` no longer appears. Lambda no longer trends
toward zero as the knowledge base ages.

### Workflow: Background tick runs `run_maintenance()`

1. `background.rs:961` calls `run_maintenance()` on its tick interval.
2. `run_maintenance()` uses `DEFAULT_STALENESS_THRESHOLD_SECS` to identify entries whose
   confidence score has not been refreshed recently (separate from Lambda).
3. Confidence refresh runs against those entries.
4. This workflow is unaffected by crt-048 except that `DEFAULT_STALENESS_THRESHOLD_SECS`
   now carries a comment making this use explicit.

---

## Constraints

**C-01** — Both `compute_lambda()` call sites in `services/status.rs` must be updated
identically — the main Lambda path and the `coherence_by_source` per-source loop. An
asymmetric update (one site updated, one not) produces a compile error because the
signature changes.

**C-02** — `StatusReport` struct field removal must be atomic across the struct definition
and all initialization sites. `mcp/response/mod.rs` contains approximately six
default-construct instances that each set both removed fields. A partial removal causes a
compile error. All sites must be found and updated before the build is attempted.
Architect must enumerate exact line numbers of all `StatusReport` initialization sites
before pseudocode is written.

**C-03** — Weight literals `0.46`, `0.31`, `0.23` are locked per OQ-1 resolution.
These values are not subject to re-derivation during implementation. The original structural
ratio 0.30 : 0.20 : 0.15 (graph : contradiction : embedding) is preserved at the 2 : 1.33 : 1
proportion. The ADR must record this ratio as the justification.

**C-04** — `[inference] freshness_half_life_hours` in operator config is not touched.
This governs the confidence scoring pipeline (a separate subsystem). Operators do not need
to update config files for this feature.

**C-05** — The `compute_lambda()` signature change removes a positional `f64` parameter.
Because all remaining parameters are also `f64`, a call site that passes freshness as the
wrong positional argument will compile silently. The architect must search all crates for
`compute_lambda` invocations and verify each one semantically, not just syntactically.

**C-06** — No schema migration is required. This feature touches no database tables,
no `ENTRIES` columns, and no migration files.

**C-07** — The breaking JSON change (`confidence_freshness_score`, `stale_confidence_count`
removed) requires release note documentation. The PR description must list both removed
keys. No operator migration window is provided; OQ-2 confirmed zero live callers in
`product/test/`.

---

## Dependencies

### Crates Affected

| Crate | Files | Nature of Change |
|-------|-------|-----------------|
| `unimatrix-server` | `infra/coherence.rs` | Delete functions, update struct and weights |
| `unimatrix-server` | `services/status.rs` | Remove call sites, update lambda and recommendations calls |
| `unimatrix-server` | `mcp/response/status.rs` | Remove struct fields, update all three output modes |
| `unimatrix-server` | `mcp/response/mod.rs` | Remove ~12 field assignments across ~6 fixture sites |

No changes to `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`, or `unimatrix-core`.

### External Dependencies

None. This feature requires no new crates and no changes to Cargo.toml.

### Existing Components Relied Upon

- `CoherenceWeights` struct — modified in place
- `compute_lambda()` — signature updated in place
- `StatusReport` — fields removed in place
- `coherence_by_source` logic — call site updated, computation logic unchanged
- `run_maintenance()` — unchanged; retains use of `DEFAULT_STALENESS_THRESHOLD_SECS`
- `DEFAULT_LAMBDA_THRESHOLD` — unchanged at 0.8
- `lambda_renormalization_without_embedding` test (and similar) — may need value updates
  only (not structural changes) because re-normalization logic itself is unchanged

### Knowledge Dependencies

- ADR-003 (entry #179) — superseded by this feature's ADR (AC-12)
- Pattern entry #4189 — confirms the design rationale (structural dimensions belong in
  Lambda; time-based dimensions do not)

---

## NOT in Scope

The following are explicitly excluded. Any implementation touching these areas is a scope
variance requiring vision guardian review.

1. **New 4th Lambda dimension** — No replacement for freshness is introduced in this
   feature. A future cycle-relative dimension (Options 2 and 3 from GH #520) is a
   separate feature.

2. **Cycle-aware freshness** — Data from `cycle_review_index`, `cycle_events`, and
   `sessions.feature_cycle` is available post-crt-036 but not used here.

3. **`updated_at` / `last_accessed_at` removal** — These timestamps remain on entries for
   audit and `run_maintenance()` use.

4. **`feature_entries`, `cycle_events`, `cycle_review_index` tables** — Not touched.

5. **`coherence_by_source` computation logic** — Retained; only the freshness argument to
   `compute_lambda()` is removed from the per-source call.

6. **`[inference] freshness_half_life_hours` config** — Not touched. The confidence
   pipeline's freshness factor is a separate subsystem.

7. **`DEFAULT_LAMBDA_THRESHOLD` (0.8)** — Not changed.

8. **Maintenance recommendation trigger logic** — Only the stale-confidence branch is
   removed; other recommendation branches are unchanged.

9. **Type 2 failure handling** — Entries retrieved but consistently unhelpful is a curation
   signal problem, not Lambda's problem.

10. **GH #425 manual close** — Already closed; no action required.

11. **Any database schema migration** — No tables, columns, or migration files are
    modified.

---

## Open Questions

**OQ-A** — Exact line numbers of the six `StatusReport` default-construct sites in
`mcp/response/mod.rs`. SCOPE.md cites approximate lines 614, 710, 973, 1054, 1137, 1212,
1291 (seven candidates for six sites). The architect must enumerate exact sites from the
current file before writing pseudocode, because a single missed site causes a compile
error (SR-06).

**OQ-B** — `lambda_renormalization_without_embedding` and similar re-normalization tests:
do their expected values change numerically with the new three-dimension weights, or only
the four-dimension baseline? The re-normalization formula is unchanged; only the weight
values differ. The architect should verify whether expected values need updating or only
the base weight constants propagate the change automatically.

**OQ-C** — Confirm that `run_maintenance()` is the sole surviving caller of
`DEFAULT_STALENESS_THRESHOLD_SECS` after the three Lambda call sites are removed
(SCOPE.md §Implementation Notes cites line 1242 of `services/status.rs`). This is a
static analysis claim that must be re-verified at delivery start. If a second surviving
caller exists, the comment on the constant must list both.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR-003 (entry #179, the
  4-dimension weight decision being superseded), pattern entry #4189 (structural-only
  dimensions rationale), and lesson entry #3704 (freshness half-life miscalibration
  history). All three are directly relevant. Entry #4189 confirms the design rationale
  at a pattern level. Entry #179 is the ADR this feature supersedes via AC-12.
