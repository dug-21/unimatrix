# Test Plan — Reload overlap engine (context_reload + compaction_reread)

**Component**: `unimatrix-observe/src/session_metrics.rs` — one overlap primitive, two callers
**Risks**: R-07 (dual reload collapsed — High), R-09 (basis-points encoding for `context_reload_pct`)
**ACs**: AC-13 (dual reload not collapsed), AC-20 (basis-points)

> ADR-005: two columns, two gates, ONE overlap engine, never collapsed. The shared primitive is the temptation surface — a refactor that derives one window from the other destroys the distinct semantics.

## Unit tests

### Shared primitive, two distinct callers (R-07, AC-13)
- `test_overlap_primitive_pure_window_input` — the shared overlap primitive takes its window as INPUT and returns overlap; it does not embed either caller's gate. Two callers pass distinct windows.
- `test_context_reload_uses_cross_session_window` — `context_reload` caller uses the cross-session window (continuity/handoff); assert it is NOT gated on `compacted_at`.
- `test_compaction_reread_uses_within_cycle_compaction_gate` — `compaction_reread` caller uses the post-compaction within-cycle window gated on `compacted_at` (delegates the gate to compaction_reckoning.md).
- `test_neither_window_derived_from_other` (R-07) — assert the two outputs are independent: changing the compaction window does not change `context_reload_pct`, and vice versa.

### context_reload_pct basis-points encoding (R-09, AC-20)
- `test_context_reload_pct_basis_points_encode` — `compute_context_reload_pct` returns a FRACTION `f64` in [0.0, 1.0] (live `session_metrics.rs`); the engine/persist boundary converts via `round(fraction × 10000)` to an `i64`. 0.375 → 3750; 0.0 → 0; 1.0 → 10000.
- `test_context_reload_pct_rounding_to_nearest` — fraction 0.00005 → 1; 0.99995 → 10000 (round to nearest, not floor/truncate).
- `test_context_reload_pct_no_float_column` — structural: the persisted column is `i64`; no `f64` reaches the bind (footgun #4529 designed out).

## Integration tests

- `test_cycle_review_dual_reload_independent` (harness, AC-13) — two scenarios through the full review:
  1. Cross-session reload present, ZERO compactions → `context_reload_pct` non-zero, `compaction_reread_count` "unavailable".
  2. Compaction-rereads present, single session (no cross-session reload) → `compaction_reread_count` non-zero, `context_reload` near-zero/unavailable.
  Assert the two columns persist independently — neither is derived from the other's window.

## Edge cases
- Single-session cycle → no cross-session overlap → `context_reload` reflects only cross-session (near-zero / unavailable), distinct from a measured zero.
- Cycle with reload overlap exactly at window boundary → consistent inclusion/exclusion rule (pinned window, ADR-005).
- `compute_context_reload_pct` returns a value producing >10000 basis points → clamp (delegates clamp assertion to store_cycle_review.md AC-14).

## Expected behaviors / assertions summary
- One shared primitive, two windows, two columns, two gates — never collapsed.
- `context_reload_pct` = `round(fraction × 10000)` basis-points integer (fraction in [0.0,1.0] from the live `-> f64`); round-to-nearest; no float column.
- Neither reload metric's window is derived from the other's.
