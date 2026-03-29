# Pseudocode Overview: crt-032 — w_coac Reduction to 0.0

## Feature Summary

Pure default-value change. No new logic, no new functions, no schema changes.
All changes are in `crates/unimatrix-server/src/`.

## Components

| Component | File | Change Type |
|-----------|------|-------------|
| config-production | `src/infra/config.rs` | Value change (2 sites) + doc comments (3 sites) |
| config-tests | `src/infra/config.rs` (test section) | Value change in helper (1 site) + assertion update (1 site) + comment update (1 site) |
| search-comment | `src/services/search.rs` | Comment update (1 site) |

## Data Flow (Unchanged)

The scoring pipeline structure is unaffected. With `w_coac=0.0`:

```
compute_search_boost(...)  →  boost_map populated (call remains; Phase 3 removes it)
FusedScoreInputs { coac_norm, ... }  →  built as before
compute_fused_score(inputs, weights)  →  weights.w_coac * inputs.coac_norm = 0.0 * x = 0.0
```

## Shared Types (No New Types)

No new types introduced. `InferenceConfig` and `FusionWeights` are unchanged structurally.

## Fusion Weight Sum Invariant

Old: `0.25 + 0.35 + 0.15 + 0.10 + 0.05 + 0.05 = 0.95`
New: `0.25 + 0.35 + 0.15 + 0.00 + 0.05 + 0.05 = 0.85`

validate() enforces `sum <= 1.0`. 0.85 ≤ 1.0. Constraint satisfied.

## Sequencing

All three components are independent. Single wave.

- config-production: production code changes (definition sites + doc comments)
- config-tests: test code changes (helper + assertion + comment) — after production changes so tests align
- search-comment: comment-only change in search.rs

In practice: one agent handles all three in order, since all changes are in two files with no interdependencies.

## Open Questions

None. Architecture, specification, and risk strategy are complete and unambiguous.
