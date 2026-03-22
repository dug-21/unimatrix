## ADR-004: Formula Implemented as an Extractable Pure Function

### Context

The fused scoring formula is the heart of crt-024. It must be:
1. Tested in isolation without constructing a full `SearchService`
2. Callable from the single-pass loop in `SearchService.search()`
3. Extended by WA-2 with one additional term without touching the search loop

Two implementation options:

**Option A (inline closure)**: The formula is an inline closure inside `SearchService.search()`,
capturing `weights` from `self` and all signal inputs from the surrounding scope. No separate
function. Simplest to write; hard to unit test without mocking the full service.

**Option B (standalone function)**: Extract the formula as `pub(crate) fn compute_fused_score`
taking a `FusedScoreInputs` struct and a `FusionWeights` struct (or direct parameters).
Callable from `SearchService.search()` and from unit tests with controlled inputs.

The crt-023 precedent for this decision is `apply_nli_sort`: extracted from `try_nli_rerank`
specifically for testability. crt-024 maintains the same pattern with the scorer replacing
`apply_nli_sort`.

The WA-2 extension consideration: if the formula is a standalone function, WA-2 adds a
`phase_boost_norm: f64` parameter to `FusedScoreInputs` and one accumulator term — a local
change to one function. If inline, WA-2 must modify the search loop itself.

### Decision

The fused scoring formula is extracted as a `pub(crate)` pure function:

```rust
pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64
```

Where `FusedScoreInputs` holds the six normalized signal values and `FusionWeights` holds
the six weight f64 values. Both are simple structs with no behavior.

**NLI absence** is handled at the call site (in `SearchService.search()`), not inside
`compute_fused_score`. The caller applies re-normalization to weights before passing a
`FusionWeights` to the function. This keeps the function pure — it always computes a
weighted sum, never makes a decision about which weights are active.

**`status_penalty` is applied outside the function**: `compute_fused_score` returns the
fused score before penalty. The caller multiplies by `penalty_map.get(id).unwrap_or(1.0)`.
This preserves the "penalty is not a signal" invariant from ADR-001.

**WA-2 extension path**: WA-2 adds `phase_boost_norm: f64` to `FusedScoreInputs` and
`w_phase: f64` to `FusionWeights`, then adds one accumulator term to `compute_fused_score`.
No changes to `SearchService.search()` beyond populating the new fields. The `InferenceConfig`
validation gains one more field in the sum check.

### Consequences

Easier:
- Unit tests for the formula are plain Rust functions — no async, no Arc, no mock store.
- AC-11 regression test, Constraint 9 verification, and NLI-absent re-normalization are
  all directly testable by calling `compute_fused_score` with controlled inputs.
- WA-2 is a local change to one function signature and one function body — no ripple
  through the search pipeline.
- `FusedScoreInputs` serves as documentation of all formula inputs; implementers cannot
  accidentally omit a signal.

Harder:
- Two new structs (`FusedScoreInputs`, `FusionWeights`) must be defined; slight boilerplate.
- `SearchService.search()` must construct `FusedScoreInputs` per candidate inside the loop,
  including computing the normalized signals. The per-iteration construction cost is negligible
  (six f64 assignments).
- The struct's fields must be kept in sync with the formula terms in `compute_fused_score`.
  A future signal addition requires: new field in `FusedScoreInputs`, new field in
  `FusionWeights`, new term in the function, new field in `InferenceConfig`, updated validation.
  This is acceptable overhead — signal additions are deliberate architectural decisions.
