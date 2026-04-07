# crt-048 Implementation Brief — Drop Freshness from Lambda

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-048/SCOPE.md |
| Architecture | product/features/crt-048/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-048/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-048/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-048/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| infra/coherence.rs | pseudocode/coherence.md | test-plan/coherence.md |
| services/status.rs | pseudocode/status.md | test-plan/status.md |
| mcp/response/status.rs | pseudocode/response-status.md | test-plan/response-status.md |
| mcp/response/mod.rs | pseudocode/response-mod.md | test-plan/response-mod.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Remove the `confidence_freshness` dimension from Lambda, the composite coherence health
metric in `context_status`, leaving it as a three-dimension structural integrity metric
(graph quality, contradiction density, embedding consistency) with re-normalized weights
(0.46 / 0.31 / 0.23). crt-036's cycle-based retention invalidated the wall-clock recency
proxy; this feature deletes all freshness computation, removes the associated struct fields
and JSON output, and retains `DEFAULT_STALENESS_THRESHOLD_SECS` exclusively for
`run_maintenance()` confidence refresh.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| New Lambda weights after freshness removal | graph=0.46, contradiction=0.31, embedding=0.23 (sum=1.00); proportional re-normalization of original 0.30:0.20:0.15 ratio preserving 2:1.33:1 structural relationship | GH #520 owner review 2026-04-06, OQ-1 | architecture/ADR-001-three-dimension-lambda-weights.md — stored in Unimatrix as #4192, supersedes #179 |
| `DEFAULT_STALENESS_THRESHOLD_SECS` retention | Retain with comment; sole surviving caller is `run_maintenance()` confidence refresh in background tick — not a Lambda input. Goal 7 "remove if no other caller" does not apply. | SCOPE.md §Implementation Notes; SR-03 High risk | architecture/ADR-002-staleness-constant-retention.md — stored in Unimatrix as #4193 |
| `compute_lambda()` named-struct refactor | Decline. Four remaining parameters have distinct types (`f64`, `Option<f64>`, `f64`, `&CoherenceWeights`); mis-ordering risk is low. Named struct adds indirection without safety gain for a 4-param pure function. | ARCHITECTURE.md §Technology Decisions | (ADR-001 context) |
| Clean removal, no migration window | Zero live callers outside Rust test code (OQ-2 grep of `product/test/` confirmed zero matches for `confidence_freshness` and `stale_confidence_count`). PR release notes must document removed JSON keys. | SCOPE.md §Resolved Decisions OQ-2 | (NFR-06, C-07 in SPECIFICATION.md) |
| `coherence_by_source` unchanged in logic | Retained; per-source Lambda becomes more diagnostic after freshness removal (structural purity per source). Only the `compute_lambda()` call signature update is required. | SCOPE.md §Resolved Decisions OQ-3 | (FR-12, AC-13) |

---

## Files to Create/Modify

All changes are in `crates/unimatrix-server/`. No changes to any other crate.

| File | Action | Summary |
|------|--------|---------|
| `src/infra/coherence.rs` | Modify | Remove `CoherenceWeights.confidence_freshness` field; update `DEFAULT_WEIGHTS` to 3-dimension values (0.46/0.31/0.23); delete `confidence_freshness_score()` and `oldest_stale_age()` functions; update `compute_lambda()` signature and body; update `generate_recommendations()` signature and body; retain `DEFAULT_STALENESS_THRESHOLD_SECS` with updated doc comment; delete ~11 freshness tests; update ~11 lambda/re-normalization tests |
| `src/services/status.rs` | Modify | Remove `confidence_freshness_score()` call (line ~695), two field assignments, and `oldest_stale_age()` call (line ~766) from Phase 5 main path; update both `compute_lambda()` call sites (line ~771 and the `coherence_by_source` loop ~793–804) to 3-dimension signature; update `generate_recommendations()` call (line ~811–818) to remove 2 arguments; retain `active_entries` allocation and `load_active_entries_with_tags()` call |
| `src/mcp/response/status.rs` | Modify | Remove `confidence_freshness_score: f64` and `stale_confidence_count: u64` fields from `StatusReport` and `StatusReportJson`; remove from `Default` impl; remove from `From<&StatusReport>` impl; remove from text format (coherence line), markdown format (`**Confidence Freshness**` bullet), and JSON format (automatic once struct fields gone) |
| `src/mcp/response/mod.rs` | Modify | Remove field assignments from all 8 fixture sites (16 field references): `make_status_report()` helper (lines 614/618), 6 inline test fixtures (lines 710/714, 973/977, 1054/1058, 1137/1141, 1212/1216, 1291/1295), and `make_coherence_status_report()` helper (lines 1434/1438 — non-default values 0.8200/15, not caught by default-value search); remove 4 tests entirely (`test_coherence_json_all_fields`, `test_coherence_json_f64_precision`, `test_coherence_stale_count_rendering`, `test_coherence_default_values`); fix lines 1731 and 1794/1798 |

No schema migrations. No Cargo.toml changes. No new files created.

---

## Data Structures

### CoherenceWeights (post-crt-048)

```rust
pub struct CoherenceWeights {
    pub graph_quality: f64,          // weight 0.46
    pub contradiction_density: f64,  // weight 0.31
    pub embedding_consistency: f64,  // weight 0.23 — optional dimension
}

pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    graph_quality: 0.46,
    contradiction_density: 0.31,
    embedding_consistency: 0.23,
};
```

Invariant: `graph_quality + contradiction_density + embedding_consistency == 1.0` within
f64 epsilon. `lambda_weight_sum_invariant` test enforces this via
`(sum - 1.0_f64).abs() < f64::EPSILON` (NFR-04; exact `==` is forbidden for this sum).

### StatusReport (field removals)

```
REMOVED: confidence_freshness_score: f64
REMOVED: stale_confidence_count: u64
```

Both fields removed from struct definition, `Default` impl, `From<&StatusReport>` impl,
and all three output formats (text/markdown/JSON). This is a breaking JSON change per
OQ-2 / C-07.

### DEFAULT_STALENESS_THRESHOLD_SECS (retained)

```rust
/// Staleness threshold for confidence refresh: 24 hours in seconds.
///
/// Used by run_maintenance() in services/status.rs to identify entries eligible
/// for confidence score re-computation. NOT a Lambda input — the Lambda freshness
/// dimension was removed in crt-048.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;
```

---

## Function Signatures

### compute_lambda (updated)

```rust
pub fn compute_lambda(
    graph_quality: f64,
    embedding_consistency: Option<f64>,
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64
```

When `embedding_consistency` is `None`, re-normalization over the 2 remaining dimensions:
- graph effective weight: `0.46 / 0.77 ≈ 0.5974`
- contradiction effective weight: `0.31 / 0.77 ≈ 0.4026`

### generate_recommendations (updated)

```rust
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>
```

Removed parameters: `stale_confidence_count: u64`, `oldest_stale_age_secs: u64`.
Stale-confidence recommendation branch deleted.

### Deleted functions

```rust
// DELETED — no replacement
fn confidence_freshness_score(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> (f64, u64)

// DELETED — no replacement
fn oldest_stale_age(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> u64
```

---

## Constraints

1. **Both `compute_lambda()` call sites in `services/status.rs` must be updated
   identically.** Main path (~line 771) and `coherence_by_source` loop (~lines 793–804).
   An asymmetric update is the highest-likelihood silent failure mode (R-06, Critical).

2. **`StatusReport` struct field removal must be atomic.** All 8 fixture sites in
   `mcp/response/mod.rs` (16 field references) must be found and removed before a build
   is attempted. The `make_coherence_status_report()` helper at line 1434 sets non-default
   values (0.8200/15) — it is not found by a search for `1.0` or `0`. It must be removed
   explicitly (R-02, Critical/High likelihood).

3. **Weight literals 0.46, 0.31, 0.23 are locked** per OQ-1 resolution. Do not re-derive.
   These preserve the 2:1.33:1 structural ratio of the original 0.30:0.20:0.15 weights.

4. **`DEFAULT_STALENESS_THRESHOLD_SECS` must not be removed.** Surviving call site at
   `services/status.rs` line ~1242 (`run_maintenance()`). SCOPE.md Goal 7 "remove if no
   other caller" is superseded by Implementation Notes. ADR-002 encodes this as a hard
   constraint (R-03, Critical).

5. **`compute_lambda()` positional argument ordering.** All four remaining parameters are
   either `f64` or `Option<f64>` — a mis-ordered call compiles silently. Implementation
   must grep all `compute_lambda(` invocations in `crates/` and verify each semantically,
   not just syntactically (R-01, Critical).

6. **`[inference] freshness_half_life_hours` in operator config is not touched.** This is
   a separate subsystem (confidence scoring pipeline, not Lambda). No config migration.

7. **Breaking JSON change is intentional.** PR description must list `confidence_freshness_score`
   and `stale_confidence_count` as removed JSON keys (NFR-06, C-07).

8. **No schema migration.** Zero database changes.

9. **`load_active_entries_with_tags()` call in Phase 5 is retained.** It serves the
   `coherence_by_source` grouping; only the freshness scan over its output is removed (FR-11).

10. **`lambda_weight_sum_invariant` test must use epsilon comparison**, not exact `==`.
    Even though 0.46+0.31+0.23 = 1.00 is exactly representable in IEEE 754, NFR-04
    mandates `(sum - 1.0_f64).abs() < f64::EPSILON` as a robustness guard (R-04).

---

## Dependencies

### Crates Affected

| Crate | Nature |
|-------|--------|
| `unimatrix-server` | All changes are in this crate only |

No changes to `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`, or `unimatrix-core`.

### External Dependencies

None. No new crates. No Cargo.toml changes.

### Unimatrix Knowledge

| Entry | Role |
|-------|------|
| #4192 (ADR-001, supersedes #179) | New 3-dimension weight ADR — must be stored via `context_correct` as a delivery step (AC-12) |
| #4193 (ADR-002) | Retention of `DEFAULT_STALENESS_THRESHOLD_SECS` — already stored |
| #179 (ADR-003, deprecated) | Original 4-dimension weight ADR — must show status "deprecated" with superseded-by link post-delivery |
| #4189 | Pattern: structural dimensions belong in Lambda; time-based dimensions do not |
| #3704 | Lesson: `FRESHNESS_HALF_LIFE_HOURS` miscalibration history — confirms these are two distinct constants; do not confuse them |

---

## NOT in Scope

1. Replacing freshness with a new 4th dimension. A future cycle-relative dimension (Options 2
   and 3 from GH #520) is a separate feature.
2. Using `cycle_review_index`, `cycle_events`, or `sessions.feature_cycle` data.
3. Removing `updated_at` / `last_accessed_at` timestamp fields from entries.
4. Touching `feature_entries`, `cycle_events`, or `cycle_review_index` tables.
5. Changing `coherence_by_source` computation logic (only call signature updated).
6. Touching `[inference] freshness_half_life_hours` config (separate subsystem).
7. Changing `DEFAULT_LAMBDA_THRESHOLD` (remains 0.8).
8. Changing maintenance recommendation trigger logic beyond removing the stale-confidence branch.
9. Type 2 failure handling (entries retrieved but consistently unhelpful).
10. Any database schema migration.
11. GH #425 manual close (already closed).

---

## Alignment Status

**Overall: PASS with one WARN.** No variances require human approval.

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Resolves "Time-based freshness in Lambda" Critical gap in PRODUCT-VISION.md §Domain Coupling; Lambda is now a 3-dimension structural health metric (graph, contradiction, embedding) |
| Milestone Fit | PASS | Cortical phase cleanup; no Wave 2 capabilities, no W3-1 dependencies, no GNN references |
| Scope Gaps | PASS | All 9 SCOPE.md goals covered in FR-01 through FR-18 and architecture component breakdown |
| Scope Additions | WARN | ARCHITECTURE.md enumerates 8 exact fixture sites vs. SCOPE.md estimate of 6. Authoritative count for delivery: **8 sites / 16 field references** (ARCHITECTURE.md §Component D table). Risk already mitigated by R-02 in RISK-TEST-STRATEGY.md. |
| Architecture Consistency | PASS | Architecture, specification, and scope internally consistent; no contradictions |
| Risk Completeness | PASS | All 7 scope risks (SR-01 through SR-07) mapped to architecture decisions and test scenarios |

### W-01 Detail (informational, no approval required)

SCOPE.md §Implementation Notes estimates "~6 sites / ~12 removals" in `mcp/response/mod.rs`.
The architect's code audit found 8 sites / 16 references. The extra sites are line 1291
(seventh inline fixture) and line 1434 (`make_coherence_status_report()` helper with non-default
values 0.8200/15). Implementers must use the ARCHITECTURE.md table as the authoritative
checklist, not the SCOPE.md estimate. R-02 in RISK-TEST-STRATEGY.md specifically flags the
`make_coherence_status_report()` helper as the highest-risk missed site.

---

## Deleted Code Inventory (Pre-flight Reference)

### Functions deleted from `infra/coherence.rs`
- `confidence_freshness_score(entries, now, staleness_threshold_secs) -> (f64, u64)`
- `oldest_stale_age(entries, now, staleness_threshold_secs) -> u64`

### Tests deleted from `infra/coherence.rs` (~11 tests)
`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`, `recommendations_below_threshold_stale_confidence`

### Tests updated in `infra/coherence.rs` (~11 tests)
`lambda_all_ones`, `lambda_all_zeros`, `lambda_weighted_sum`,
`lambda_specific_four_dimensions` (rename to `lambda_specific_three_dimensions`),
`lambda_single_dimension_deviation`, `lambda_weight_sum_invariant`,
`lambda_renormalization_without_embedding`, `lambda_renormalization_partial`,
`lambda_renormalized_weights_sum_to_one`, `lambda_embedding_excluded_specific`,
`lambda_custom_weights_zero_embedding`

### Tests deleted from `mcp/response/mod.rs` (4 tests)
`test_coherence_json_all_fields`, `test_coherence_json_f64_precision`,
`test_coherence_stale_count_rendering`, `test_coherence_default_values`

### Fixture sites updated in `mcp/response/mod.rs` (8 sites, 16 field references)

| Site | Lines (freshness_score / stale_count) | Note |
|------|--------------------------------------|------|
| `make_status_report()` helper | 614 / 618 | Default values |
| Inline fixture 1 | 710 / 714 | Default values |
| Inline fixture 2 | 973 / 977 | Default values |
| Inline fixture 3 | 1054 / 1058 | Default values |
| Inline fixture 4 | 1137 / 1141 | Default values |
| Inline fixture 5 | 1212 / 1216 | Default values |
| Inline fixture 6 | 1291 / 1295 | Default values |
| `make_coherence_status_report()` | 1434 / 1438 | **Non-default: 0.8200 / 15** |

Additional: line 1731 (`report2.stale_confidence_count = 0`); lines 1794/1798 (default assertions).

---

## Delivery Pre-flight Checklist

Before opening PR:
- [ ] `cargo build --workspace` — zero errors, zero freshness-related warnings
- [ ] `grep -r "confidence_freshness" crates/` — zero matches
- [ ] `grep -r "stale_confidence_count" crates/` — zero matches (except `run_maintenance` area — that is `DEFAULT_STALENESS_THRESHOLD_SECS`, not this field)
- [ ] `grep -rn "compute_lambda(" crates/unimatrix-server/src/services/status.rs` — exactly 2 matches, both with 4 arguments (not 5)
- [ ] `grep -n "DEFAULT_STALENESS_THRESHOLD_SECS" crates/unimatrix-server/src/infra/coherence.rs` — exactly 1 definition with updated doc comment
- [ ] `context_correct` executed for entry #179 → new ADR entry created (AC-12)
- [ ] PR description lists removed JSON keys: `confidence_freshness_score`, `stale_confidence_count`
