# crt-051 Pseudocode Overview
# Fix contradiction_density_score() — Replace Quarantine Proxy with Real Pair Count

## Components Involved

| Component File | Role in This Fix |
|---|---|
| `crates/unimatrix-server/src/infra/coherence.rs` | Function signature + formula change + test rewrite |
| `crates/unimatrix-server/src/services/status.rs` | One-line call site argument change + ordering comment |
| `crates/unimatrix-server/src/mcp/response/mod.rs` | Test fixture field update |

## What Crosses Component Boundaries

One value flows between components: `report.contradiction_count: usize`, produced by
`compute_report()` Phase 2 in `status.rs` and consumed at Phase 5 by
`coherence::contradiction_density_score()` in `coherence.rs`.

No types are introduced or removed. No new imports. No schema changes.

## Shared Constraint: Phase Ordering

`status.rs::compute_report()` has numbered phases. Phase 2 populates
`report.contradiction_count` from `ContradictionScanCacheHandle`. Phase 5 calls
`contradiction_density_score()`. Phase 5 must not be reordered above Phase 2.
This is not type-enforced; it is guarded by an inline comment added at Phase 5 (AC-16).

## Shared Constraint: Type Discipline

The new first parameter of `contradiction_density_score()` is `usize` (matches
`StatusReport.contradiction_count` type exactly — no cast at call site). The second
parameter `total_active: u64` is unchanged. `as f64` casts inside the function are safe
and follow existing precedent in `graph_quality_score`.

## Sequencing: Nothing Depends on Anything

All three component changes are independent. Delivery can apply them in any order.
Compilation will fail only if `coherence.rs` is updated (new signature) but `status.rs`
is not yet updated (old argument types). Apply both together or apply `coherence.rs`
first.

## Unchanged Paths (Do Not Touch)

- `generate_recommendations()` in `coherence.rs` — still takes `total_quarantined: u64`
- `generate_recommendations()` call site in `status.rs` ~line 784 — still passes
  `report.total_quarantined`
- `DEFAULT_WEIGHTS.contradiction_density` — weight 0.31 unchanged
- `scan_contradictions()` and `ContradictionScanCacheHandle` — not touched
- `StatusReport` field names and JSON output types — unchanged; only values change
- Seven other `StatusReport` fixtures in `response/mod.rs` with `contradiction_count: 0`
  and `contradiction_density_score: 1.0` — all consistent with new semantics; no change

## Fixture Arithmetic (Reference)

`make_coherence_status_report()` fixture after fix:
- `total_active: 50` (unchanged)
- `contradiction_count: 15` (changed from 0)
- `contradiction_density_score: 0.7000` (unchanged)
- Verification: `1.0 - (15 as f64 / 50 as f64) = 1.0 - 0.30 = 0.70` exactly
