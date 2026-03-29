# Pseudocode: search-comment

## Component: `src/services/search.rs` — FusionWeights Field Comment

### Purpose

Update the inline comment on the `w_coac` field inside the `FusionWeights` struct.
This is a doc/comment-only change. No production code logic changes.

### Invariants to Preserve

- `FusionWeights` struct definition — unchanged
- `w_coac: f64` field in `FusionWeights` — unchanged
- All `FusionWeights { w_coac: 0.10, ... }` struct literals in test code — unchanged (intentional fixtures)
- `compute_search_boost` function definition and call site — unchanged
- `compute_briefing_boost` function definition — unchanged
- `CO_ACCESS_STALENESS_SECONDS` reference in search.rs — unchanged

---

## Site 1: `FusionWeights.w_coac` field comment (approx. line 118)

### Before

```rust
pub struct FusionWeights {
    // ...
    pub w_coac: f64,    // default 0.10 — co-access affinity (lagging signal)
    // ...
}
```

### After

```rust
pub struct FusionWeights {
    // ...
    pub w_coac: f64,    // default 0.0 (zeroed in crt-032; PPR subsumes co-access signal via GRAPH_EDGES.CoAccess)
    // ...
}
```

**Invariant**: Only the comment text changes. The field type, visibility, and name are unchanged.

---

## Error Handling

No error handling needed. Comment-only change.

## Key Test Scenarios

- Grep search.rs for `default 0\.10` on the `w_coac` comment line → zero matches (R-04)
- Grep search.rs for `FusionWeights.*w_coac.*0\.10` in test fixtures → count must be unchanged from pre-delivery baseline (R-03)
- `compute_search_boost` function still present (AC-08)
- `CO_ACCESS_STALENESS_SECONDS` still referenced in search.rs (AC-07)
