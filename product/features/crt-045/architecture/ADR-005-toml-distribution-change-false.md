## ADR-005: ppr-expander-enabled.toml Sets distribution_change=false

### Context

`product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` declares
`distribution_change = true` but omits the required `[profile.distribution_targets]`
sub-table. `parse_profile_toml()` requires all three target fields (`cc_at_k_min`, `icd_min`,
`mrr_floor`) when `distribution_change = true`, returning `EvalError::ConfigInvariant` if any
are absent. This causes `unimatrix eval run` to fail at profile parse time — before any graph
issue can be observed (AC-03).

The question is what threshold values to supply. Two approaches:

**Option A — Fix with invented thresholds:** Set `distribution_change = true` and supply
threshold values derived from the crt-042 gate or estimated from expected behaviour. Risk:
if the thresholds are wrong (too tight or too loose), the eval gate fails or passes vacuously
on first run, giving no signal about whether the fix works.

**Option B — Set distribution_change=false:** Remove the `distribution_change = true` flag
(or set it to `false`). Gate on `mrr_floor` and `p_at_5_min` — metrics whose current baseline
values are known from crt-042 — and defer CC@k and ICD until first run produces values.

This is OQ-01 resolved by the human sponsor: "CC@k and ICD are future metrics — measure on
first run, establish baselines, gate in subsequent runs. Do NOT invent floors we have never
measured."

### Decision

Set `distribution_change = false` in `ppr-expander-enabled.toml`. Add the two known metric
gates:
- `mrr_floor = 0.2651` — no regression from current baseline (crt-042 gate value)
- `p_at_5_min = 0.1083` — first run where P@5 should respond to cross-category entries

Add a comment in the TOML explaining why `distribution_change = false` is intentional (SR-04):

```toml
# distribution_change = false intentionally.
# CC@k and ICD floors cannot be set without a first-run measurement.
# Gate on mrr_floor and p_at_5_min only until baseline data is collected.
# See crt-045 ADR-005 and SCOPE.md OQ-01.
```

This satisfies AC-03 (no parse-time failure) without introducing metric gates that could fail
for reasons unrelated to the typed graph fix.

### Consequences

Easier:
- `eval run` no longer fails at parse time — AC-03 is satisfied.
- The `mrr_floor` and `p_at_5_min` gates are grounded in known baseline values, not invented.
- The TOML comment prevents future developers from re-introducing `distribution_change = true`
  without supplying the required sub-table.

Harder:
- The eval gate for ppr-expander-enabled.toml does not include CC@k or ICD on the first run.
  These must be added in a follow-up feature once the first run produces measured values.
- If `mrr_floor = 0.2651` represents a point estimate that has drifted since crt-042 shipped,
  the gate may fail on first run. The delivery agent should verify current baseline MRR before
  committing this value.
