## ADR-001: FusionWeights::effective() Short-Circuit When w_nli == 0.0

Feature: crt-038 — conf-boost-c formula and NLI dead-code removal. Status: Accepted.

### Context

`FusionWeights::effective(nli_available: bool)` (search.rs) has two paths:
- `nli_available=true`: return weights unchanged.
- `nli_available=false`: zero `w_nli`, then re-normalize the remaining five core
  weights by dividing each by their sum.

The re-normalization was designed for the case where `w_nli > 0.0` (NLI was expected
to contribute) but the model is temporarily unavailable. Re-distributing a positive
NLI weight budget across the other signals is semantically correct in that case.

With the conf-boost-c defaults (`w_nli=0.00`, `w_sim=0.50`, `w_conf=0.35`, others
0.00) and `nli_enabled=false`, the execution path is:
1. `nli_available=false` (because `nli_enabled=false` sets this).
2. Re-normalization fires with denominator = 0.50 + 0.35 = 0.85.
3. Effective weights become `w_sim≈0.588, w_conf≈0.412`.

This diverges from the formula evaluated in ASS-039 (which produced MRR=0.2913).
Re-distributing `w_nli=0.00` is a no-op in intent but produces a scaling artifact
in execution. The result is a scoring formula that was never evaluated and has no
empirical basis. This is a correctness error, not an optimization opportunity.

The ASS-039 eval was run with `nli_enabled=true` and `w_nli=0.0`, which takes the
`nli_available=true` path in `effective()` and returns weights unchanged. To reach
the same effective formula post-ship, `effective()` must return unchanged weights
whenever `w_nli == 0.0`, regardless of the `nli_available` argument.

### Decision

Add a short-circuit to `FusionWeights::effective()` as the first branch:

```rust
if self.w_nli == 0.0 {
    return *self;
}
```

This guard precedes the `if nli_available { ... }` check. When `w_nli == 0.0`:
- No weight budget exists to redistribute.
- Re-normalization would silently inflate sim and conf beyond their configured values.
- Returning `self` unchanged produces the exact formula specified in the config.

The existing `nli_available=true` fast path and the re-normalization path for
`w_nli > 0.0` are both preserved unchanged. The zero-denominator guard for the
all-zero pathological case is also preserved (it cannot be reached when `w_nli == 0.0`
with any non-zero remaining weight, but remains for safety).

This change MUST be implemented before any eval run for AC-12 (ADR-003 ordering
constraint). The MRR gate value of 0.2913 was produced by the `effective(true)`
path (weights unchanged); the short-circuit reproduces that behavior on the
`effective(false)` path when `w_nli == 0.0`.

This is a correctness fix, not an optimization. The two behaviors differ in
observable output when `w_nli == 0.0` and `nli_available == false`.

### Consequences

Easier:
- Conf-boost-c formula operates exactly as evaluated in ASS-039.
- Future operators setting `w_nli=0.0` via config will not get silently re-normalized
  weights when NLI is disabled.
- The short-circuit is a 3-line guard with no branching complexity added to
  `effective()` tests.

Harder:
- Tests that assert `effective(false)` behavior with `w_nli=0.0` will need updating
  to expect unchanged weights rather than re-normalized weights. Any such tests
  were asserting the incorrect behavior.
- The doc comment on `effective()` must be updated to document the new short-circuit
  branch.
