# Component: Report extensions — report-extensions

**Wave**: 1
**Location**: `crates/unimatrix-server/src/eval/runner/output.rs` (ProfileResult fields),
`eval/report/aggregate/mod.rs` (`find_regressions`), report rendering.
**ADR**: ADR-003 (#4896 epsilon=0.0), ADR-004 (#4898 trust). **Risks**: R-12 (OR-fold), R-17 (exit code).

## Purpose

Surface trust + cost on `ProfileResult`, render them in `report` next to P@5/MRR/latency, and
extend `find_regressions` to OR-fold trust flips + cost growth — WITHOUT changing the `eval report`
exit-code convention (body-only; exits 0 on quality regressions, R-17).

## `ProfileResult` additions (`output.rs`)

```
pub struct ProfileResult {
    pub entries: Vec<ScoredEntry>,
    pub latency_ms: ...,
    pub p_at_k: f64,
    pub mrr: f64,
    pub cc_at_k: ...,
    pub icd: ...,
    // NEW:
    pub cost_tokens: f64,        // from cost-metric
    pub trust: TrustOutcome,     // from trust-metric
}
```
Populated in `run_single_profile` (`replay.rs`/`metrics.rs`) in the SAME pass as the existing
metrics (C-03), so the sweep correlates all four families in one run (AC-04, AC-14).

## `find_regressions` extension (OR-fold — R-12)

Existing semantics (Integration Surface): OR over `mrr < baseline.mrr || p_at_k < baseline.p_at_k`.
Extend the OR with trust-flip and cost-growth, preserving the existing structure:

```
fn find_regressions(results, query_map) -> Vec<RegressionRecord> {
    // Baseline = first profile by SORTED key (R-03 lineage #2610 — never HashMap iteration order).
    let baseline = results.profile_sorted_first();

    let mut regressions = vec![];
    for candidate in results.non_baseline_profiles_sorted():
        for (scenario, cand) in candidate.scenarios:
            let base = baseline.scenario(scenario);

            // existing relevance OR:
            let mrr_reg   = cand.mrr   < base.mrr;
            let pk_reg    = cand.p_at_k < base.p_at_k;

            // NEW trust flip: baseline satisfied an assertion the candidate now violates.
            let trust_flip =
                (base.trust.absence_pass && !cand.trust.absence_pass) ||
                (base.trust.rank_pass    && !cand.trust.rank_pass);

            // NEW cost growth: epsilon = 0.0 advisory — ANY growth reported, blocks nothing.
            let cost_growth = (cand.cost_tokens - base.cost_tokens) > 0.0;

            if mrr_reg || pk_reg || trust_flip || cost_growth:
                regressions.push(RegressionRecord {
                    scenario, profile: candidate,
                    reasons: collect([mrr_reg => "mrr", pk_reg => "p@k",
                                      trust_flip => "trust", cost_growth => "cost"]),
                    trust_violations: cand.trust.violations.clone(),   // surfaced for triage
                    cost_delta: cand.cost_tokens - base.cost_tokens,
                });
    regressions
}
```

### OR-composition correctness (R-12)

- A candidate that holds trust but regresses MRR is STILL flagged (trust pass does not mask a
  relevance regression).
- A candidate that holds relevance but flips a trust assertion IS flagged.
- A candidate that holds relevance + trust but grows cost is flagged (advisory) — listed, not blocked.
- `cost_growth` uses epsilon = 0.0 strictly (`> 0.0`): any positive delta is reported.

### Exit-code invariance (R-17 — LOAD-BEARING)

Adding trust/cost to `find_regressions` changes only the BODY of the report (Section 5 list).
The `eval report` exit code is UNCHANGED: it stays the existing body-only convention (regressions
reported, exit 0). Cost is advisory (epsilon=0.0, blocks nothing, §7.1); trust flips use the SAME
fail-in-body-only semantics as the existing MRR/P@K check. No path makes the exit code non-zero
on a quality/trust/cost regression. (The ONLY non-zero-exit path in nan-018 is the shape-hash
HARD ABORT on the primary corpus — a corpus-VALIDITY precondition, not a quality verdict, and it
lives in shape-hash, not here.)

## Report rendering

- **Trust section (FR-08)**: per-profile, per-scenario pass/fail for absence + rank-below +
  redirect-to-head, with an aggregate pass-rate per profile; violations listed.
- **Cost (FR-12)**: per-profile `cost_tokens` next to P@K/MRR/latency, with a cost-delta column
  vs baseline; `k` shown as a secondary axis.
- **Correlated section (AC-04/AC-14)**: trust + P@5/MRR + cost for the SAME scenarios in ONE
  table/section so the sweep reads as "steepness X -> trust held AND relevance did not regress AND
  cost moved by delta".

## Data flow

- **Inputs**: `Vec<ProfileResult>` (carrying `cost_tokens` + `trust`), baseline selected by sorted key.
- **Output**: `Vec<RegressionRecord>` (body) + rendered report; exit code unchanged.

## Error handling

Pure aggregation; no new fallible path. Baseline selection MUST sort keys (R-03 lineage #2610) —
never depend on HashMap iteration order.

## Key test scenarios

- **Trust-flip regression (R-12.1, AC-02/03)**: baseline satisfies an assertion, candidate violates
  it -> appears in Section 5 with body-only semantics.
- **No-flip (R-12.2)**: both satisfy -> no trust regression recorded.
- **OR-composition (R-12.3)**: trust-holds-but-MRR-regresses -> flagged; relevance-holds-but-trust-flips
  -> flagged (neither masks the other).
- **Cost growth advisory (AC-09)**: cost delta > 0.0 -> reported; assert it blocks nothing.
- **Exit-code invariance (R-17)**: `eval report` exit code UNCHANGED with trust/cost regressions
  present; existing report tests pass unchanged.
- **Baseline determinism (R-03 lineage)**: baseline = first profile by sorted key, not map order.
- **Correlated section (AC-14)**: one run renders trust + P@5/MRR + cost for the same scenarios.
