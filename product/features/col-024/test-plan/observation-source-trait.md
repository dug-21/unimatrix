# col-024 Test Plan: ObservationSource Trait
# File: `crates/unimatrix-observe/src/source.rs`

## Component Summary

The `ObservationSource` trait gains one new method:

```rust
fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>;
```

This is a compile-time surface only — the trait carries no logic. The implementation lives in
`SqlObservationSource` (see `load-cycle-observations.md`). Test coverage here is:

1. Compile: the trait extension does not break any existing implementor.
2. Trait dispatch: the new method is callable via `&dyn ObservationSource`.
3. Existing tests in `unimatrix-observe` remain green.

---

## Risk Coverage

| Risk | From RISK-TEST-STRATEGY | How Covered Here |
|------|------------------------|-----------------|
| I-01 | Trait change breaks downstream consumers | `cargo test -p unimatrix-observe` must pass; any mock impl of `ObservationSource` outside `SqlObservationSource` must compile after adding the new method |
| AC-10 | Trait declared in `source.rs`, impl on `SqlObservationSource`, all existing tests green | `cargo test -p unimatrix-observe && cargo test -p unimatrix-server` |

---

## Unit Test Expectations

### T-TRAIT-01: Trait compiles and is callable via trait object

**AC**: AC-10
**File**: `crates/unimatrix-observe/src/source.rs` (existing test module, or a new compile-only test)
**Setup**: Use the existing `SqlObservationSource::new_default` (from `unimatrix-server`) as the
concrete implementation. No mock implementation of `ObservationSource` currently exists outside
`SqlObservationSource` — if any mock is added during Stage 3b, it must include the new method.

**Assertions**:
- `cargo test -p unimatrix-observe` exits 0 (no compilation errors from new method declaration)
- Calling `let _: &dyn ObservationSource = &source; source.load_cycle_observations("x")` compiles
  without error

**Notes**:
- No logic test at the trait level — logic is tested in `load-cycle-observations.md`
- If the spec adds a default impl to the trait (which would be a deviation — the spec says
  abstract method), flag it to the implementor: AC-10 requires it to be a required method, not
  an optional default

### T-TRAIT-02: Existing trait tests pass without modification

**AC**: AC-10, AC-12
**Assertion**: `cargo test -p unimatrix-observe` output shows same test count as before col-024;
no tests are marked as ignored, commented out, or deleted.

**Verification**: Compare `test result: ok. N tests; 0 failed` where N equals the pre-col-024
test count for the `unimatrix-observe` crate. Stage 3c tester must record this baseline count.

---

## Integration Test Expectations

No integration-level expectations specific to this component. The MCP interface to
`context_cycle_review` is unchanged; trait dispatch is an implementation detail invisible to the
JSON-RPC layer.

---

## Edge Cases

**Partial compilation check**: If a future mock or test double of `ObservationSource` is created
during Stage 3b in any test module, it must implement `load_cycle_observations`. The stage 3b
implementor must search for all `impl ObservationSource for` occurrences and add the method to
each. Stage 3c tester verifies by checking `cargo test --workspace` for compilation errors
specifically mentioning `ObservationSource`.

```bash
# Verify no implementors were missed
grep -r "impl ObservationSource for" /workspaces/unimatrix/crates/
```

Expected: only `SqlObservationSource` in `unimatrix-server/src/services/observation.rs`.
