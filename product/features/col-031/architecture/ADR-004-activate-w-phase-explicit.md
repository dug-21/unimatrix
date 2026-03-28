## ADR-004: Activate w_phase_explicit at 0.05; AC-16 is Non-Separable from AC-12

### Context

ADR-003 (crt-026, Unimatrix #3163) deferred `w_phase_explicit` to W3-1 (GNN)
by holding it at `0.0` and hardcoding `phase_explicit_norm = 0.0` in the
scoring loop. The rationale was that phase strings were opaque to Unimatrix —
there was no signal source. col-031 provides the signal source (PhaseFreqTable),
removing the blocker.

Two sub-decisions arise:

**Sub-decision 1: What weight to use?**

`w_phase_explicit = 0.05` is the value specified in SCOPE.md (D-03). It is a
judgment-based calibration, not a research-spike-backed measurement (SR-02
risk acknowledged). Safety net: AC-12 gate (CC@5 ≥ 0.2659, ICD ≥ 0.5340).
The additive exemption from ADR-004 (crt-026, Unimatrix #3175) applies —
`w_phase_explicit` is outside the six-weight sum constraint. FusionWeights
sum with defaults becomes `0.95 + 0.02 + 0.05 = 1.02`. The `FusionWeights`
doc-comment must be updated to state this; `validate()` requires no change.

Cold-start analysis: when `use_fallback = true`, fused scoring applies
`phase_explicit_norm = 0.0` via the guard (ADR-003). The net effect of
`0.05 × 0.0 = 0.0` — no contribution. Raising the default from `0.0` to
`0.05` has no effect until the first successful tick populates the table.
The weight ships active but is inert until data exists.

**Sub-decision 2: Can AC-12 (regression gate) and AC-16 (eval harness fix) ship independently?**

`extract.rs` already reads `phase` from `query_log` rows and populates
`ScenarioContext.phase`. The SQL in `output.rs` already selects `phase`.
However, `replay.rs` passes `phase` only as metadata and does NOT forward it
to `ServiceSearchParams` (comment: "metadata passthrough only — never forwarded
to ServiceSearchParams"). Without forwarding `current_phase`, all eval replay
runs use `current_phase = None` → `phase_explicit_norm = 0.0` → AC-12 scores
match pre-col-031 trivially regardless of weight. AC-12 becomes a noise check,
not a regression gate.

Treating AC-16 and AC-12 as independent deliverables creates a structural
failure mode: AC-12 declared PASS with zero phase signal active. SR-03
identifies this as a High / High severity risk.

### Decision

1. Raise `default_w_phase_explicit()` from `0.0` to `0.05` in `infra/config.rs`.
2. Add `query_log_lookback_days: u32` to `InferenceConfig` with default `30`.
3. Update `FusionWeights` doc-comment: `0.95 + 0.02 + 0.05 = 1.02`.
4. Update `test_inference_config_default_phase_weights` to assert `0.05`.
5. AC-16 (forward `current_phase` in `replay.rs`) is a mandatory prerequisite
   for AC-12. The delivery protocol must gate AC-12 on AC-16 complete first.
   Gate 3b must reject any AC-12 PASS claim that precedes AC-16 or was measured
   with `current_phase = None` in all scenarios.

Note: AC-16 requires adding `current_phase: Option<String>` to
`ServiceSearchParams` and forwarding `record.context.phase` from `replay.rs`.
This is the complete scope of AC-16 — no change to `extract.rs` or `output.rs`
is needed (they already handle `phase`).

This decision supersedes the deferred state from ADR-003 (crt-026, Unimatrix
#3163) for the `w_phase_explicit` activation. ADR-003 crt-026 remains valid
for its other content; only the "deferred to W3-1" clause for `w_phase_explicit`
is superseded by this ADR.

### Consequences

**Easier**:
- `w_phase_explicit` is now a live, configurable signal. Operators can tune
  the weight via TOML.
- AC-12 is a real regression gate once AC-16 is present.
- Cold-start safety: `use_fallback = true` → `phase_explicit_norm = 0.0` →
  score identity with pre-col-031 preserved.

**Harder**:
- Default change affects all deployments immediately. Operators must be aware
  that scoring now includes a live phase signal after the first tick.
- `FusionWeights` doc-comment total (`1.02`) requires manual maintenance;
  not enforced by `validate()`.
- SR-02 calibration risk: `0.05` is judgment-based. If phase signal proves
  noisy for a particular deployment, the operator must tune `w_phase_explicit`
  downward. AC-12 is the only automated guard.
- W3-1 (GNN) remains the parametric successor once CC@k ≥ 0.7; when W3-1
  activates, the combined budget `w_phase_histogram + w_phase_explicit` must
  be reviewed to remain within the 0.05 additive headroom.
