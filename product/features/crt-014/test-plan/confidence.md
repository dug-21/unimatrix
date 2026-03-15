# Test Plan: confidence.rs — crt-014

## Component

`crates/unimatrix-engine/src/confidence.rs` — MODIFIED (removal only).

Changes under test:
1. Remove `pub const DEPRECATED_PENALTY: f64 = 0.7;` (line 60).
2. Remove `pub const SUPERSEDED_PENALTY: f64 = 0.5;` (line 65).
3. Remove 4 test functions from the `#[cfg(test)]` block (lines 888–920):
   - `deprecated_penalty_value`
   - `superseded_penalty_value`
   - `superseded_penalty_harsher_than_deprecated`
   - `penalties_independent_of_confidence_formula`

There are **no new functions** to add to `confidence.rs`. All behavioral coverage for penalties migrates to `graph.rs`. The only test in this list that requires attention is `penalties_independent_of_confidence_formula` — see note below.

---

## AC-14: Constants Absent from Production Code (R-11)

### Shell Verification

After removal, the following must return no output from production code paths:

```bash
grep -n "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" \
    /workspaces/unimatrix-crt-014/crates/unimatrix-engine/src/confidence.rs
```

Expected: no output (constants and tests removed).

```bash
grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" \
    /workspaces/unimatrix-crt-014/crates/ --include="*.rs"
```

Expected: zero matches (constants absent from the entire workspace after all changes to `confidence.rs` and `search.rs`).

---

## AC-15: Behavioral Ordering Tests Replace Constant-Value Tests

The 4 removed tests map to behavioral replacements as follows:

| Removed Test | Replaced By | Location |
|-------------|-------------|----------|
| `deprecated_penalty_value` | `orphan_softer_than_clean_replacement` (constant ordering assertion) | `graph.rs` |
| `superseded_penalty_value` | `two_hop_harsher_than_one_hop` (constant ordering assertion) | `graph.rs` |
| `superseded_penalty_harsher_than_deprecated` | `partial_supersession_softer_than_clean` (ordering assertion) | `graph.rs` |
| `penalties_independent_of_confidence_formula` | `weight_sum_invariant_unchanged` (weight sum check) | `confidence.rs` (retained) |

### `penalties_independent_of_confidence_formula` — Retention Decision

The body of this test is:

```rust
let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
assert_eq!(stored_sum, 0.92_f64, "penalty constants must not affect stored weight sum");
```

This test does NOT reference `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY`. It only verifies the weight sum invariant. After removing the two constants, this test continues to compile and pass as-is.

Decision: **Retain this test in `confidence.rs`** but rename it to remove the misleading reference to "penalty constants":

```rust
#[test]
fn weight_sum_invariant_is_0_92() {
    let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
    assert_eq!(stored_sum, 0.92_f64, "stored weight components must sum to 0.92");
}
```

This counts as the "replacement" for `penalties_independent_of_confidence_formula`. The behavioral assertion (sum == 0.92) is preserved; the misleading test name is corrected.

---

## Remaining Tests: No-Break Verification

After the 4 tests are removed (3 deleted, 1 renamed), the remaining tests in `confidence.rs` must continue to pass without modification. The following test groups must be verified:

### crt-010 Tests (Preserved)

All non-penalty tests from `crt-010` remain:
- `cosine_similarity_*` test block (lines ~922+)
- `rerank_score_*` test block
- `Wilson score` tests
- `confidence_compute_*` tests
- `provenance_boost_*` test

None of these tests reference `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY`. They are unaffected by the constant removal.

### Verification

```bash
cargo test --package unimatrix-engine -- confidence 2>&1 | tail -20
```

Expected: all remaining confidence tests pass. Zero failures. The test count will decrease by exactly 3 (4 tests removed, 1 renamed — same test count from the renamed test).

---

## Compile-Clean Verification (AC-18)

After removing the constants and their tests, verify the module compiles clean:

```bash
cargo build --package unimatrix-engine 2>&1 | grep "^error"
```

Expected: no output.

If `search.rs` still imports `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` from `confidence` at this point, the build will fail with:

```
error[E0432]: unresolved import `crate::confidence::DEPRECATED_PENALTY`
```

This is R-11 (dead import). The `search.rs` import line must be updated in the same PR.

---

## Atomic Commit Requirement (R-05)

The following changes must land in a single commit to avoid a window where CI passes with no penalty coverage:

1. Remove 3 test functions from `confidence.rs` (`deprecated_penalty_value`, `superseded_penalty_value`, `superseded_penalty_harsher_than_deprecated`).
2. Rename `penalties_independent_of_confidence_formula` → `weight_sum_invariant_is_0_92` in `confidence.rs`.
3. Remove `pub const DEPRECATED_PENALTY` and `pub const SUPERSEDED_PENALTY` from `confidence.rs`.
4. Add behavioral ordering tests to `graph.rs`: `orphan_softer_than_clean_replacement`, `two_hop_harsher_than_one_hop`, `partial_supersession_softer_than_clean`.
5. Update `search.rs` import to remove `DEPRECATED_PENALTY, SUPERSEDED_PENALTY`.

If these ship in separate commits, CI can pass between commits with no penalty coverage — which is R-05.

### Commit Verification Checklist

Before the commit lands, run:

```bash
# Zero constant references in production code
grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" crates/ --include="*.rs" | grep -v "#\[test\]" | grep -v "// "

# All graph ordering tests present
cargo test --package unimatrix-engine -- graph 2>&1 | grep "test.*ordering\|orphan.*clean\|two_hop\|partial.*super"

# confidence tests still pass (minus the 4 removed)
cargo test --package unimatrix-engine -- confidence 2>&1 | tail -10
```

---

## No Net Coverage Loss — Mapping Table

| Old Test | Coverage | New Test | Coverage |
|----------|----------|----------|----------|
| `deprecated_penalty_value` | DEPRECATED_PENALTY == 0.7 (exact value) | `orphan_softer_than_clean_replacement` | ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY (ordering) |
| `superseded_penalty_value` | SUPERSEDED_PENALTY == 0.5 (exact value) | `two_hop_harsher_than_one_hop` | graph_penalty(depth-2) < graph_penalty(depth-1) (behavioral) |
| `superseded_penalty_harsher_than_deprecated` | SUPERSEDED_PENALTY < DEPRECATED_PENALTY | `partial_supersession_softer_than_clean` | PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY |
| `penalties_independent_of_confidence_formula` | Weight sum == 0.92 | `weight_sum_invariant_is_0_92` | Weight sum == 0.92 (same assertion, renamed) |

The new tests cover **more** than the old tests: the old tests asserted exact constant values. The new tests assert behavioral ordering properties that remain meaningful as the constants evolve. Coverage is equal or better in all four cases.
