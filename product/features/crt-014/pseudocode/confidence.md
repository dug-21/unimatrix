# Pseudocode: confidence.rs (MODIFIED)

**File**: `crates/unimatrix-engine/src/confidence.rs`
**Change type**: MODIFY (removals only — no new code added)

---

## Purpose

Remove the two deprecated/superseded penalty constants and their four associated unit tests. These are replaced by topology-derived constants in `graph.rs` and behavioral ordering tests in `graph.rs`'s test module.

This file documents exactly what is removed and why, so the implementer can make the change atomically alongside the `graph.rs` behavioral ordering tests.

---

## Removals: Production Constants

### Remove lines 58–65 (the two penalty constants and their doc comments)

**Current text** (lines 58–65 in the worktree):
```rust
/// Multiplicative penalty for deprecated entries in Flexible retrieval mode (crt-010).
/// Applied to the final re-ranked score, not the confidence formula.
pub const DEPRECATED_PENALTY: f64 = 0.7;

/// Multiplicative penalty for superseded entries in Flexible retrieval mode (crt-010).
/// Harsher than DEPRECATED_PENALTY since a known successor exists.
/// Applied to the final re-ranked score, not the confidence formula.
pub const SUPERSEDED_PENALTY: f64 = 0.5;
```

**Replace with**: nothing. Delete these 8 lines entirely.

### What Remains

After removal, the constant block reads (with no gap):
```rust
/// Query-time boost for `lesson-learned` category entries (col-010b).
/// Applied in search re-ranking alongside co-access affinity.
/// Does NOT modify the stored confidence formula invariant (0.92).
pub const PROVENANCE_BOOST: f64 = 0.02;

/// Cosine similarity between two f32 vectors, returned as f64 for scoring precision.
// ... (next section)
```

No other changes to `confidence.rs` production code.

---

## Removals: Unit Tests

### Remove 4 test functions from the `#[cfg(test)] mod tests` block

These are at lines 890–920 in the current worktree (within the `// -- crt-010: penalty constants tests (T-PC-01..04) --` section).

**Test 1 — remove**:
```rust
#[test]
fn deprecated_penalty_value() {
    assert_eq!(DEPRECATED_PENALTY, 0.7);
    assert!(DEPRECATED_PENALTY > 0.0 && DEPRECATED_PENALTY < 1.0);
}
```

**Test 2 — remove**:
```rust
#[test]
fn superseded_penalty_value() {
    assert_eq!(SUPERSEDED_PENALTY, 0.5);
    assert!(SUPERSEDED_PENALTY > 0.0 && SUPERSEDED_PENALTY < 1.0);
}
```

**Test 3 — remove**:
```rust
#[test]
fn superseded_penalty_harsher_than_deprecated() {
    assert!(
        SUPERSEDED_PENALTY < DEPRECATED_PENALTY,
        "superseded ({}) should be < deprecated ({})",
        SUPERSEDED_PENALTY,
        DEPRECATED_PENALTY,
    );
}
```

**Test 4 — remove**:
```rust
#[test]
fn penalties_independent_of_confidence_formula() {
    // Weight sum invariant unchanged
    let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
    assert_eq!(
        stored_sum, 0.92_f64,
        "penalty constants must not affect stored weight sum"
    );
}
```

Also remove the section header comment above them:
```rust
// -- crt-010: penalty constants tests (T-PC-01..04) --
```

### What Remains in That Region

After removal, the test block flows from the provenance boost tests (`T-PB-01..04`) directly to the cosine_similarity tests (`T-CS-01..08`):

```rust
    // -- col-010b: PROVENANCE_BOOST tests (T-PB-01..04) --
    // ... (keep these 4 tests unchanged) ...

    // -- crt-010: cosine_similarity tests (T-CS-01..08) --
    // ... (keep these 8 tests unchanged) ...
```

---

## What is NOT Changed

- The `// -- crt-010: cosine_similarity tests (T-CS-01..08) --` section and its 8 tests remain intact.
- All other constants remain: `W_BASE`, `W_USAGE`, `W_FRESH`, `W_HELP`, `W_CORR`, `W_TRUST`, `MAX_MEANINGFUL_ACCESS`, `FRESHNESS_HALF_LIFE_HOURS`, `COLD_START_ALPHA`, `COLD_START_BETA`, `PROVENANCE_BOOST`.
- All functions remain: `cosine_similarity`, `base_score`, `usage_score`, `freshness_score`, `helpfulness_score`, `correction_score`, `trust_score`, `compute_confidence`, `rerank_score`, `adaptive_confidence_weight`.
- All other unit tests (T-01 through T-11, crt-005 tests, col-010b tests, crt-010 cosine tests) remain.

---

## Replacement Coverage (R-05)

The four removed tests covered penalty constant values. Their behavioral equivalent is now in `graph.rs` test module:

| Removed test | Replaced by (graph.rs) |
|---|---|
| `deprecated_penalty_value` (DEPRECATED_PENALTY == 0.7) | `orphan_softer_than_clean_replacement` (ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY); `penalty_orphan` (graph_penalty returns ORPHAN_PENALTY for orphan entry) |
| `superseded_penalty_value` (SUPERSEDED_PENALTY == 0.5) | `penalty_clean_replacement_depth1` (graph_penalty returns CLEAN_REPLACEMENT_PENALTY for depth-1 chain) |
| `superseded_penalty_harsher_than_deprecated` | `two_hop_harsher_than_one_hop`; `orphan_softer_than_clean_replacement` |
| `penalties_independent_of_confidence_formula` | Retained semantically by `weight_sum_invariant_f64` (already in confidence.rs, unchanged) — weight sum is asserted there without referencing penalty constants |

The `penalties_independent_of_confidence_formula` test was testing two things: that constants don't affect weight sum, and implicitly that the constants exist. The weight sum assertion is already covered by the independent `weight_sum_invariant_f64` test which stays in `confidence.rs`.

---

## Implementation Note: Atomic Commit

Per R-05 (test migration window risk): the removal of these 4 tests and the addition of behavioral ordering tests in `graph.rs` MUST happen in the same commit. The implementer must not commit:

- A state where `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` are removed but behavioral ordering tests in `graph.rs` are not yet present.
- A state where `graph.rs` ordering tests exist but `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` are still present (compile error when search.rs imports are changed).

Correct order within a single commit:
1. Create `graph.rs` with all behavioral ordering tests
2. Remove penalty constants from `confidence.rs` (lines 58–65)
3. Remove 4 tests from `confidence.rs`
4. Update `search.rs` import line (remove constant imports, add graph imports)
5. Update `lib.rs` (add `pub mod graph;`)
6. Update `Cargo.toml` (add petgraph + thiserror)

---

## Error Handling

No error handling needed — this file only removes code. No new functions, no new error paths.

---

## Key Test Scenarios

- AC-14: `cargo build --workspace` with no `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` references in non-test production code. Grep passes.
- AC-15: Confidence.rs test file has no `assert_eq!(DEPRECATED_PENALTY, 0.7)` or `assert_eq!(SUPERSEDED_PENALTY, 0.5)` style assertions. `graph.rs` has ordering tests present.
- AC-18: `cargo build --workspace` produces zero errors and zero unused-import warnings related to the removed constants.
