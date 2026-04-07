## ADR-003: Outcome Weighting — Inline outcome_weight() in phase_freq_table.rs

### Context

The rebuild path needs to map `cycle_phase_end.outcome` strings to a numeric
weight (`f32`). The existing `infer_gate_result()` in `tools.rs` performs
a similar classification but has two incompatibilities:

1. **Signature mismatch:** `infer_gate_result(outcome: Option<&str>, pass_count: u32)`
   requires `pass_count` — a stateful count of prior passes for the cycle, not
   available in the per-phase weighting context without an additional query.

2. **Layering violation:** `tools.rs` is in `unimatrix-server/src/mcp/` (the MCP
   request handler layer). `phase_freq_table.rs` is in `unimatrix-server/src/services/`
   (the services layer). Importing from `mcp/` into `services/` would create an
   inward dependency — `services/` should not depend on `mcp/`.

3. **Semantic mismatch:** `infer_gate_result` returns a four-valued
   `GateResult` enum (Pass, Fail, Rework, Unknown). The weighting function
   needs only a two-valued `f32`: pass=`1.0`, non-pass=`0.5`. Wrapping
   `GateResult` to produce `f32` would require importing `unimatrix_observe`
   types into a context that currently has no such dependency.

4. **DRY risk is acceptable here:** Outcome vocabulary drift (e.g., new outcome
   string variants) would need to be updated in both places. This risk is low
   because the vocabulary is stable and the weighting function applies a simpler
   policy than `infer_gate_result`. A future unification under a shared module
   is appropriate only if a third consumer emerges.

### Decision

Define a private free function `outcome_weight(outcome: &str) -> f32` in
`unimatrix-server/src/services/phase_freq_table.rs`:

```rust
/// Map a cycle_phase_end outcome string to a frequency weight.
///
/// "pass" (case-insensitive contains) → 1.0 (full signal weight)
/// "rework" or "fail" (case-insensitive contains) → 0.5 (half signal weight)
/// All other strings (including "unknown", empty, unrecognized) → 1.0
///   (graceful degradation — missing/unknown outcome treated as unweighted)
///
/// Priority: rework check runs before fail check (consistent with infer_gate_result
/// priority order in tools.rs, col-026 R-03, to prevent "rework" matching "fail"
/// substring in hypothetical future strings).
fn outcome_weight(outcome: &str) -> f32 {
    let lower = outcome.to_lowercase();
    if lower.contains("rework") {
        return 0.5;
    }
    if lower.contains("fail") {
        return 0.5;
    }
    if lower.contains("pass") {
        return 1.0;
    }
    // Unknown / empty / unrecognized: degrade to unweighted (AC-05)
    1.0
}
```

The `apply_outcome_weights` step in `PhaseFreqTable::rebuild()`:

1. Build `HashMap<String, Vec<f32>>` keyed by `phase`, values are per-cycle
   weights collected from all `PhaseOutcomeRow`s for that phase.
2. Collapse to `HashMap<String, f32>` by taking the mean weight per phase.
3. For each `PhaseFreqRow`, multiply `row.freq` by the per-phase weight
   (default `1.0f32` when phase absent from map). Cast weighted freq to `i64`
   via `(row.freq as f32 * weight).round() as i64`. Replace `row.freq`.
4. Return the modified `Vec<PhaseFreqRow>` for grouping and rank normalization.

### Consequences

- No cross-layer dependency between `services/` and `mcp/`.
- Function is unit-testable in isolation within `phase_freq_table.rs` tests.
- If the outcome vocabulary evolves, `outcome_weight` and `infer_gate_result`
  are two independent update sites. This is acceptable given the current
  vocabulary stability.
- The `1.0` default for unrecognized outcomes (including empty strings) ensures
  AC-05: phases with no outcome history are unweighted, not suppressed.
