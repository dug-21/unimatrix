## ADR-003: eval Module Uses test-support Feature to Access kendall_tau

### Context

`kendall_tau()` and the ranking assertion helpers (`assert_in_top_k`, `assert_tau_above`,
`assert_confidence_ordering`, `profile_to_entry_record`) live in
`crates/unimatrix-engine/src/test_scenarios.rs`. This module is gated in `lib.rs` as:

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_scenarios;
```

The eval runner (`eval/runner.rs`) is production binary code — it runs in the
`unimatrix` CLI binary and is not compiled under `#[cfg(test)]`. Without the
`test-support` feature enabled in `unimatrix-engine`, the `kendall_tau` function and
its siblings are invisible to the eval runner at compile time.

Two options were evaluated:

**Option A — Duplicate metric functions in `eval/metrics.rs`**: Copy `kendall_tau` and
friends into the eval module. Removes the feature flag dependency at the cost of
duplicate code.

**Option B — Enable `test-support` feature on `unimatrix-engine` in `unimatrix-server/Cargo.toml`
for the eval binary**: Add `features = ["test-support"]` to the `unimatrix-engine`
dependency in `unimatrix-server/Cargo.toml`. This makes the existing verified
implementations accessible to the eval runner.

A scope assumption in the SCOPE-RISK-ASSESSMENT.md flagged this exact issue: "Architect
must verify accessibility before committing to this approach."

### Decision

Enable the `test-support` feature on `unimatrix-engine` in `unimatrix-server/Cargo.toml`
(Option B).

Duplicating `kendall_tau` and its associated test infrastructure into the eval module
creates a maintenance split: if the metric is corrected in `test_scenarios.rs`, the
eval copy diverges. The `test-support` feature was designed precisely for this use case
— sharing test-grade infrastructure across crates. The eval runner's need for these
functions is legitimate production use of test-support infrastructure, not a hack.

The `test-support` feature on `unimatrix-engine` enables only `test_scenarios` — it
does not pull in any test-only state, mocking infrastructure, or unsafe code. Enabling
it in the production binary is safe.

The `unimatrix-server/Cargo.toml` dependency line becomes:
```toml
unimatrix-engine = { path = "../unimatrix-engine", features = ["test-support"] }
```

### Consequences

- `kendall_tau()` and all ranking helpers are accessible to the eval runner without
  duplication.
- The `unimatrix` binary built with this change includes the `test_scenarios` module
  from `unimatrix-engine`. This is a small code size increase (the module is ~400 lines)
  with no runtime overhead — the functions are only called when `eval run` is invoked.
- Future metric additions to `test_scenarios.rs` are automatically available to the
  eval runner without additional wiring.
- The `test-support` feature flag on `unimatrix-engine` must be documented as
  "production-safe; enables eval metric access" to prevent future engineers from
  removing it thinking it is a test-only dependency.
