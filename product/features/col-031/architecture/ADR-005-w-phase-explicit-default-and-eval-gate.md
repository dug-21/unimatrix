## ADR-005: `w_phase_explicit` Default Raised to 0.05; Eval Harness Fix as Non-Separable Deliverable

### Context

**Weight default change:**
`w_phase_explicit` has been `0.0` since crt-026 (ADR-003, Unimatrix #3163), held
as a W3-1 placeholder. col-031 provides the signal source (the frequency table),
so the weight must be raised to activate the term. The question is: what default
value and should it ship active?

Options considered:
1. Keep `0.0`, require operators to explicitly configure the weight. This makes
   AC-12 a vacuous gate — scoring is never activated, PPR's prerequisite check is
   meaningless.
2. Raise to `0.05` and ship active. Cold-start degrades gracefully: empty table →
   all `phase_affinity_score = 1.0` (neutral) → `w_phase_explicit * 1.0` is a flat
   additive constant identical for all candidates → zero net ranking signal.
   Real signal only emerges when query history is meaningful, which is when the
   weight should be live.
3. Raise to a higher value (e.g., `0.10`). `w_phase_histogram = 0.02` is the
   cross-session histogram term. The frequency table is a durable cross-session signal
   (not ephemeral); `0.05 > 0.02` is directionally correct. ASS-032 provides no
   numerically derived value for the explicit phase term, so higher values add risk
   without evidence. `0.05` is conservative.

SR-02 (SCOPE-RISK-ASSESSMENT.md) notes that `0.05` is calibrated by judgment, not
a research spike. ADR-004 (crt-026, Unimatrix #3206) established that
`w_phase_explicit` is additive outside the six-weight sum constraint. With defaults:
`0.95 (six-term) + 0.02 (histogram) + 0.05 (explicit) = 1.02` total — within the
per-field `[0.0, 1.0]` range check; sum comment in `FusionWeights` must be updated.

**Eval harness fix non-separability (SR-03):**
`eval/scenarios/extract.rs` does not select `query_log.phase` in scenario extraction
SQL (Unimatrix #3555, known gap). Without this fix, AC-12 passes trivially: the
frequency table is never activated in eval replay (phase is always `None` →
`phase_explicit_norm = 0.0`). The gate becomes a noise check, not a regression gate.

If the eval harness fix and the scoring activation are treated as independently
shippable, a delivery wave could declare AC-12 PASS before the frequency table
signal was ever exercised. This would leave the PPR gate's frequency-table
prerequisite check vacuous, undermining the purpose of the eval gate.

### Decision

1. Raise `default_w_phase_explicit()` from `0.0` to `0.05` in `infra/config.rs`.
2. Update the `FusionWeights` sum-check comment to reflect the new total (1.02).
3. Update the test `test_inference_config_default_phase_weights` to assert `0.05`.
4. Add `query_log_retention_cycles: u32` to `InferenceConfig` with default `20`.
5. Treat AC-16 (eval harness fix: `extract.rs` selects `current_phase`) as a
   non-separable deliverable from AC-12 (eval regression gate). The delivery
   protocol MUST gate AC-12 on AC-16 being complete and verified first.
6. Accept the SR-02 calibration risk explicitly: `0.05` is a conservative judgment
   value; the safety net is AC-12 (eval regression gate) which must show no decrease
   in CC@5 from `0.2659` baseline and no decrease in ICD from `0.5340`.

This supersedes crt-026 ADR-003 (Unimatrix #3163) for the `w_phase_explicit`
placeholder strategy. The field is no longer hardcoded to `0.0`; the non-parametric
frequency table is the activated signal source. W3-1 (GNN) remains the parametric
successor once `CC@k ≥ 0.7`.

### Consequences

**Easier:**
- `w_phase_explicit` is now a live, configurable signal — operators can tune via
  `InferenceConfig.w_phase_explicit` in `unimatrix.toml`.
- Cold-start produces a flat `1.0` across all candidates: net ranking effect of
  `0.05 * 1.0 = 0.05` added uniformly — no entry is penalized relative to another.
- `query_log_retention_cycles = 20` aligns with GH #409 (retention framework)
  without blocking on it.

**Harder:**
- `default_w_phase_explicit()` changes affect all deployments that rely on the
  default. Operators must be aware the scoring formula now includes a live phase
  signal. The transition from `0.0` to `0.05` is a distribution-changing change
  subject to the distribution gate (#402).
- AC-12 is now a real gate (not vacuous), requiring eval replay with
  phase-populated scenarios. This is the correct and intended behavior.
- The sum comment in `FusionWeights` requires a manual update; it is not
  enforced by `validate()` (which only checks per-field range, not total sum).
  A stale comment would mislead future weight calibration work.
