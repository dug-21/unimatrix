# crt-051 Implementation Brief
# Fix contradiction_density_score() — Replace Quarantine Proxy with Real Contradiction Count

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-051/SCOPE.md |
| Architecture | product/features/crt-051/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-051/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-051/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-051/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| coherence.rs (scoring function) | pseudocode/coherence.md | test-plan/coherence.md |
| status.rs (call site) | pseudocode/status.md | test-plan/status.md |
| response/mod.rs (test fixture) | pseudocode/response.md | test-plan/response.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace the broken `contradiction_density_score()` Lambda input — which incorrectly uses `total_quarantined / total_active` (a quarantine-status counter with no causal relationship to contradiction health) — with `contradiction_pair_count / total_active` drawn from the contradiction scan cache that already runs every ~60 minutes and already populates `StatusReport.contradiction_count`. The Lambda weight for the contradiction dimension (0.31, second-highest) is preserved unchanged; only the input data source is corrected.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Normalization strategy: pair count vs. unique-entry count | Raw detected pair count (`pairs.len()`), not unique entries appearing in pairs. Simpler, directly available, negligible difference at expected scale. Confirmed by human in spawn prompt. | ADR-001, SCOPE.md OQ-1 | product/features/crt-051/architecture/ADR-001-contradiction-density-score-input.md |
| Cold-start behavior | Return 1.0 (optimistic default) when `contradiction_count == 0`, regardless of whether scan has not yet run or ran and found zero pairs. Both are `contradiction_count: 0`; `contradiction_scan_performed` bool distinguishes states for operators. | ADR-001, SPEC FR-04 | product/features/crt-051/architecture/ADR-001-contradiction-density-score-input.md |
| SR-02 fixture resolution (VARIANCE-01) | Use architect's approach: set `contradiction_count: 15`, keep `contradiction_density_score: 0.7000`. `1.0 - 15/50 = 0.7000` exactly — exercises the non-trivial formula path. SPECIFICATION.md AC-15 updated accordingly. Human confirmed via spawn prompt. | RISK-TEST-STRATEGY.md R-02, ALIGNMENT-REPORT.md VARIANCE-01 | product/features/crt-051/architecture/ADR-001-contradiction-density-score-input.md |
| GRAPH_EDGES Contradicts write path | Out of scope. `run_post_store_nli` was deleted in crt-038; re-enabling requires NLI infrastructure. Scan-based cache is sufficient for this fix. | SCOPE.md Non-Goals | product/features/crt-051/architecture/ADR-001-contradiction-density-score-input.md |
| Lambda weights | Unchanged. `contradiction_density: 0.31` preserved per ADR-001 crt-048 (Unimatrix entry #4199). | SCOPE.md Goal 3 | product/features/crt-051/architecture/ADR-001-contradiction-density-score-input.md |

---

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/infra/coherence.rs` | Modify | Replace `total_quarantined: u64` param with `contradiction_pair_count: usize` in `contradiction_density_score()`; update formula, doc comment, and all three unit tests |
| `crates/unimatrix-server/src/services/status.rs` | Modify | One-line call site change at ~line 747: pass `report.contradiction_count` instead of `report.total_quarantined`; add phase-ordering comment |
| `crates/unimatrix-server/src/mcp/response/mod.rs` | Modify | Update `make_coherence_status_report()` fixture: set `contradiction_count: 15` (was `0`); `contradiction_density_score: 0.7000` unchanged |

No new files. No schema migration. No new crates.

---

## Data Structures

### contradiction_density_score() — New Signature

```rust
pub fn contradiction_density_score(
    contradiction_pair_count: usize,
    total_active: u64,
) -> f64
```

- `contradiction_pair_count`: Raw count of detected contradiction pairs from `ContradictionScanCacheHandle`. Type `usize` — matches `StatusReport.contradiction_count` directly; no cast required at the call site.
- `total_active`: Count of active entries in the knowledge base. Type `u64` — unchanged from old signature.
- Returns `f64` in `[0.0, 1.0]`.

### StatusReport fields (relevant subset, unchanged)

| Field | Type | Populated | Used By |
|-------|------|-----------|---------|
| `contradiction_count` | `usize` | Phase 2 (~line 583) from `ContradictionScanCacheHandle` | Phase 5 Lambda computation (call site, after fix) |
| `total_quarantined` | `u64` | Phase 1 from `COUNTERS` table | `generate_recommendations()` only — NOT Lambda |
| `contradiction_density_score` | `f64` | Phase 5, output of `contradiction_density_score()` | Lambda composite, JSON response |
| `contradiction_scan_performed` | `bool` | Phase 2 from cache presence | Operator-visible cold-start discriminator |
| `coherence` | `f64` | Phase 5, output of `compute_lambda()` | Lambda top-level composite |

### ContradictionScanCacheHandle

```rust
Arc<RwLock<Option<ContradictionScanResult>>>
```

Already populated by the background scan (`scan_contradictions`, every ~60 min). Already read in `compute_report()` Phase 2. Not modified by this feature.

---

## Function Signatures

### After fix — coherence.rs

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

### After fix — status.rs call site (~line 747)

```rust
// report.contradiction_count is populated in Phase 2 (contradiction cache read);
// Phase 5 must not be reordered above Phase 2. See crt-051 ADR-001.
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

### Unchanged — generate_recommendations() (coherence.rs ~line 114)

```rust
pub fn generate_recommendations(
    coherence: f64,
    lambda_threshold: f64,
    graph_stale_ratio: f64,
    embedding_inconsistencies: usize,
    total_quarantined: u64,  // <-- unchanged; quarantine recs are distinct from Lambda
) -> Vec<String>
```

---

## Constraints

1. `contradiction_density_score()` must remain a **pure function**: no I/O, no async, no side effects. This is a hard constraint.
2. Only two inputs permitted: `contradiction_pair_count: usize` and `total_active: u64`.
3. No schema migration. `COUNTERS.total_quarantined` and `StatusReport.total_quarantined` are unchanged.
4. `scan_contradictions()` and `ContradictionScanCacheHandle` must not be modified.
5. `generate_recommendations()` signature and behavior must not change.
6. `StatusReport` JSON field names and types are unchanged — only values change.
7. Lambda weights in `DEFAULT_WEIGHTS` must not change (`contradiction_density: 0.31`).
8. Floating-point test assertions must use `abs() < epsilon` tolerance: `< 1e-10` for exact calculations, `< 0.001` for approximate.
9. `as f64` cast from `usize` is safe and follows existing pattern (`stale_count as f64` in `graph_quality_score`).

---

## Dependencies

No new crates. No new imports beyond what is already present in `coherence.rs`.

| Component | Location | Role in crt-051 |
|-----------|----------|-----------------|
| `contradiction_density_score()` | `infra/coherence.rs:68` | Function being changed |
| `compute_report()` | `services/status.rs` | Call site being updated (one line) |
| `StatusReport.contradiction_count` | `services/status.rs:533` | Source of pair count — already populated in Phase 2 |
| `ContradictionScanCacheHandle` | `services/status.rs` Phase 2 ~line 583 | Already read; no new read needed |
| `generate_recommendations()` | `infra/coherence.rs:114` | Unchanged; still uses `total_quarantined` |
| `make_coherence_status_report()` | `mcp/response/mod.rs:1397` | Test fixture requiring `contradiction_count: 15` update |

---

## NOT in Scope

- Changing Lambda weights (`contradiction_density: 0.31` preserved)
- Writing Contradicts edges to `GRAPH_EDGES` (requires NLI infrastructure, future decision)
- Re-enabling `run_post_store_nli` or any NLI write path (deleted in crt-038)
- Modifying `scan_contradictions()` or `ContradictionScanCacheHandle`
- Removing `total_quarantined` from `StatusReport`, `COUNTERS`, or `generate_recommendations()`
- Changing the `context_status` output schema (field names/types unchanged)
- Implementing a new contradiction detection strategy
- Real-time (non-cached) contradiction scoring
- Pair density normalization relative to N² theoretical pairs
- Distinguishing "scan not yet run" from "scan ran, found zero pairs" — both return 1.0, which is correct

---

## Alignment Status

**Overall: PASS — no outstanding variances.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Fixes a semantic integrity defect in Lambda — directly supports "trustworthy, correctable, ever-improving" knowledge engine principle |
| Milestone Fit | PASS | Cortical-phase corrective bugfix; no future-wave capabilities pulled in |
| Scope Gaps | PASS | All 13 SCOPE.md AC (AC-01 through AC-13) addressed in specification |
| Architecture Consistency | PASS | All open questions resolved; component diagram matches spec |
| Risk Completeness | PASS | All 7 SCOPE-RISK-ASSESSMENT.md risks (SR-01–SR-07) traced to R-01–R-08 |
| VARIANCE-01 (fixture approach) | RESOLVED | SPECIFICATION.md AC-15 updated per architect's approach: `contradiction_count: 15`, `contradiction_density_score: 0.7000`. No outstanding variances. |

---

## Test Site Enumeration

Delivery must update exactly the following test locations:

### coherence.rs — 3 tests requiring full rewrite + 2 new tests

File: `crates/unimatrix-server/src/infra/coherence.rs`

| Current Name | Action | New Name | Notes |
|-------------|--------|----------|-------|
| `contradiction_density_zero_active` | Rename + update args | `contradiction_density_zero_active` (name acceptable) | Args `(0, 0)` identical numerically; update first arg type annotation to `usize`, update doc comment |
| `contradiction_density_quarantined_exceeds_active` | Full rewrite | `contradiction_density_pairs_exceed_active` | `contradiction_density_score(200, 100)` == 0.0 |
| `contradiction_density_no_quarantined` | Full rewrite | `contradiction_density_no_pairs` | `contradiction_density_score(0, 100)` == 1.0 |
| (new) | Add | `contradiction_density_cold_start` | `contradiction_density_score(0, 50)` == 1.0 (AC-17) |
| (new) | Add | `contradiction_density_partial` | `contradiction_density_score(5, 100)` approx 0.95 (AC-05) |

No test name may contain the substring "quarantined".

### response/mod.rs — 1 fixture field update

File: `crates/unimatrix-server/src/mcp/response/mod.rs`

| Line | Field | Before | After |
|------|-------|--------|-------|
| ~1422 | `contradiction_count` | `0` | `15` |
| ~1422 | `contradiction_density_score` | `0.7000` (unchanged) | `0.7000` (unchanged) |
| ~1419 | `coherence` | `0.7450` (unchanged) | `0.7450` (unchanged) |

The seven other fixtures (all `contradiction_count: 0`, `contradiction_density_score: 1.0`) require no change.

### status.rs — 1 call site update

File: `crates/unimatrix-server/src/services/status.rs`

| Lines | Change |
|-------|--------|
| ~747–748 | Pass `report.contradiction_count` instead of `report.total_quarantined` |
| ~747 | Add phase-ordering comment (see Function Signatures section above) |

---

## Changelog Entry

Exactly one line (human-confirmed):

> contradiction_density in Lambda now reflects scan-detected contradiction pairs (previously used quarantined/active as a proxy, which had no relationship to actual contradictions).

---

## Tracking

https://github.com/dug-21/unimatrix/issues/540
