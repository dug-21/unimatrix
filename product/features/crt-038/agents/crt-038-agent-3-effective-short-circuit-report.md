# Agent Report: crt-038-agent-3-effective-short-circuit

**Component**: FusionWeights::effective() short-circuit
**Wave**: 1 (parallel)
**File modified**: `crates/unimatrix-server/src/services/search.rs`
**Commit**: `8f78ee1f`

---

## Changes Made

### 1. Short-circuit guard added to `FusionWeights::effective()` (AC-02, ADR-001)

Inserted `if self.w_nli == 0.0 { return *self; }` as the FIRST branch, before the existing `if nli_available` branch. Updated the doc comment to document all four paths (short-circuit, NLI active, NLI absent, zero-denominator).

### 2. Three new unit tests (AC-02, R-01)

All three added inside the existing `#[cfg(test)]` module in `search.rs`:

- `test_effective_short_circuit_w_nli_zero_nli_available_false` — effective(false) with w_nli=0.0 returns weights unchanged (exact equality)
- `test_effective_short_circuit_w_nli_zero_nli_available_true` — effective(true) with w_nli=0.0 returns weights unchanged (short-circuit fires before nli_available branch)
- `test_effective_renormalization_still_fires_when_w_nli_positive` — w_nli=0.20, effective(false) produces re-normalized weights (guard does not suppress positive-weight path)

### 3. Updated assertion message in `test_fusion_weights_default_sum_unchanged_by_crt030`

Changed comment block and assertion message string from referencing crt-032 to crt-038 conf-boost-c defaults. Expected sum 0.92 is unchanged.

---

## Test Results

```
running 4 tests
test services::search::tests::step_6d::test_effective_renormalization_still_fires_when_w_nli_positive ... ok
test services::search::tests::step_6d::test_effective_short_circuit_w_nli_zero_nli_available_false ... ok
test services::search::tests::step_6d::test_effective_short_circuit_w_nli_zero_nli_available_true ... ok
test services::search::tests::step_6d::test_fusion_weights_default_sum_unchanged_by_crt030 ... ok

test result: ok. 4 passed; 0 failed
```

Full lib test run: 2589 passed, 1 failed. The failure is `test_inference_config_partial_toml_gets_defaults_not_error` in `config.rs` — a pre-existing failure caused by the config-defaults agent not yet updating the default weight constants. Not in scope for this component.

---

## Issues / Blockers

None. `FusionWeights` derives `Copy`, so `return *self` was valid directly (no need for `FusionWeights { ..*self }` spread form noted as a fallback in the pseudocode).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #4005 (ADR-001) and #4003 (pre-existing pattern describing the problem with open resolution options) were the most relevant results.
- Stored: Superseded entry #4003 -> #4010 via `context_correct`. Updated from "open problem with three resolution options" to resolved pattern with the fix, placement requirement, f64 safety condition, and test names.
