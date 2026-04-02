## ADR-003: Implementation Ordering — effective() and Formula Before Eval Gate

Feature: crt-038 — conf-boost-c formula and NLI dead-code removal. Status: Accepted.

### Context

This feature has two functionally distinct groups of changes:

**Group A — Scoring correctness**:
1. `FusionWeights::effective()` short-circuit for `w_nli == 0.0` (ADR-001, AC-02)
2. Default weight constant changes in `config.rs` (AC-01)
3. Eval gate: MRR ≥ 0.2913 on `product/research/ass-039/harness/scenarios.jsonl` (AC-12)

**Group B — Dead-code removal**:
4. `run_post_store_nli` deletion (AC-03, AC-04, AC-14)
5. `maybe_run_bootstrap_promotion` deletion (AC-05, AC-06)
6. NLI auto-quarantine deletion (AC-07, AC-08)

SR-02 identifies a concrete risk: if the eval gate (AC-12) is run before the
`effective()` short-circuit (AC-02) is implemented, it will be evaluated against
the skewed formula (`w_sim≈0.588, w_conf≈0.412`) rather than the conf-boost-c
formula (`w_sim=0.50, w_conf=0.35`). The skewed formula has no empirical baseline;
the MRR gate value of 0.2913 applies to the conf-boost-c formula only. An eval
on the wrong formula would produce an uninterpretable result.

### Decision

The mandatory delivery sequence is:

**Step 1**: Implement `effective()` short-circuit (AC-02). This establishes the
correct scoring path for all subsequent steps.

**Step 2**: Change default weight constants (AC-01). Update config tests that assert
old default values.

**Step 3**: Run eval gate (AC-12). Both steps 1 and 2 must be complete. The gate
output must show MRR ≥ 0.2913 and must be attached to the PR description before
merge.

**Step 4**: Dead-code removal (Group B). Components 3, 4, 5 within Group B may be
done in any internal order — they are mutually independent. All Group B changes ship
in the same PR as Group A.

**Step 5**: `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`
pass clean (AC-10, AC-11).

Steps 1–3 are strictly ordered. Steps 4 and 5 may be interleaved in any order
relative to each other, provided all pass before the PR is opened.

The dead-code removals (Group B) do not affect the scoring pipeline and therefore
do not affect the eval gate. Running the eval before or after Group B produces the
same MRR. However, running the eval before Step 1 produces a different scoring path
than intended.

### Consequences

Easier:
- The eval gate is interpreted against the correct formula, so its result is
  actionable (pass → merge; fail → investigate scoring bug).
- Delivery has a clear checkpoint before opening the PR: eval output attached.

Harder:
- Delivery cannot parallelize Step 1 and Step 3 without risk of invalidating the
  eval result. Steps must be done sequentially in a single working branch.
