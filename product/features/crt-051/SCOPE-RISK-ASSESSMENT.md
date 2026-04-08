# Scope Risk Assessment: crt-051

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `contradiction_density_score` has 3 unit tests in `coherence.rs` that pass `total_quarantined`-semantics values — all three must be rewritten, not just updated | Med | High | Architect should treat test rewrite as a first-class deliverable; test names encode old semantics ("quarantined") and will mislead if renamed only partially |
| SR-02 | `response/mod.rs` contains 8 test fixtures with hardcoded `contradiction_density_score` values (seven at 1.0, one at 0.7000) — the 0.7000 fixture has `total_quarantined: 3` and `contradiction_count: 0`, meaning after the fix its score changes from 0.7000 to 1.0 | Med | High | Spec writer must enumerate all 8 fixture sites; the 0.7000 fixture is a semantic divergence that will break the test unless the hardcoded value is updated |
| SR-03 | Phase ordering in `compute_report()` is confirmed safe (Phase 2 sets `contradiction_count` at line 583–593; Phase 5 uses it at line 747) — but the ordering is implicit in sequential code, not enforced by types | Low | Low | Spec writer should add an AC that asserts ordering by inspection (grep/comment) — no code change needed, just documentation guard |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | SCOPE Open Question 1 (pair count vs. unique entry count) is unresolved — the two interpretations diverge significantly on a db with N pairs sharing entries | Med | Med | Spec writer must resolve this before delivery starts; recommend pair count (simpler, already available, divergence negligible at expected scale) |
| SR-05 | Cold-start window: between server start and first tick completion, `contradiction_cache` is `None` → `contradiction_count = 0` → score = 1.0 (optimistic) — Lambda is transiently inflated during this window | Low | High | Accept as intended; confirm in spec that `contradiction_scan_performed: false` distinguishes this state for operators who inspect the JSON |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `generate_recommendations()` is called with `total_quarantined` on a separate code path (SCOPE Step 3 confirms no change needed) — a missed search for `total_quarantined` at delivery time could accidentally alter the recommendations path | Low | Med | AC-08 and AC-09 together cover this; spec should require both a positive (recommendations path untouched) and a negative (grep absence of `total_quarantined` at scoring call site) check |
| SR-07 | Stale contradiction cache: `ContradictionScanCacheHandle` is rebuilt every ~60 min; Lambda now directly reflects cache age — a cache that is hours old on a low-traffic instance could misrepresent current contradiction state | Low | Low | Out of scope for this fix; document as known limitation in doc comment |

## Assumptions

- **SCOPE §Background / Phase ordering**: Assumes `compute_report()` Phase 2 always executes before Phase 5. Confirmed true in current code (lines 583 and 747 of `status.rs`), but the assumption is not type-enforced — a future refactor could silently reorder. If wrong, `contradiction_count` would be 0 at scoring time, producing an always-optimistic score even on a warm cache.
- **SCOPE §Proposed Approach, Option A**: Assumes `StatusReport.contradiction_count` is `usize` and maps to `contradiction_pair_count: usize` without a cast concern. Confirmed at `status.rs:533`. If the field type ever changes to `u32` or `u64`, the signature must follow.
- **SCOPE §Non-Goals**: Assumes the contradiction scan itself is correct and produces valid pair counts. No validation of scan quality is in scope.

## Design Recommendations

- **SR-02 is the highest-surprise risk**: The 0.7000 fixture in `response/mod.rs:1422` will silently produce a wrong assertion value after the fix. The spec writer should call this out explicitly in AC-10 or add a dedicated AC.
- **SR-01 + SR-04**: Resolve Open Question 1 (pair count vs. unique entry count) before writing test assertions — the unit test values for `contradiction_density_score(N_pairs, M_active)` depend on this choice.
- **SR-05**: Confirm in spec that the `contradiction_scan_performed` boolean is the operator-visible signal for cold-start state. The score of 1.0 is correct behavior; the boolean is the distinguishing signal.
