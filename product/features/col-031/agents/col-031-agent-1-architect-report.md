# Agent Report: col-031-agent-1-architect

## Status: COMPLETE

## Outputs

### Architecture
- `/workspaces/unimatrix/product/features/col-031/architecture/ARCHITECTURE.md`

### ADR Files and Unimatrix IDs

| File | Unimatrix ID | Notes |
|------|-------------|-------|
| `ADR-001-rank-based-normalization.md` | #3685 | Supersedes pre-design #3679 (0-indexed formula corrected to 1-indexed) |
| `ADR-002-time-based-retention.md` | #3686 | New |
| `ADR-003-two-cold-start-contracts.md` | #3687 | New |
| `ADR-004-activate-w-phase-explicit.md` | #3688 | New; supersedes deferred-to-W3-1 clause of crt-026 ADR-003 (#3163) |
| `ADR-005-required-handle-threading.md` | #3689 | New |

### Superseded Pre-Design Entries
- #3679 → superseded by #3685 (formula correction: 0-indexed → 1-indexed)
- #3683 → superseded by #3690 (field name error; incomplete AC-16 scope)

## Key Decisions

1. **Rank normalization**: `score = 1.0 - ((rank-1) / N)`, 1-indexed. Single-entry bucket → 1.0. Absent entry → 1.0.
2. **Retention**: `query_log_lookback_days = 30` (time-based, no schema change). #409 owns cycle-aligned successor.
3. **Two cold-start contracts**: `phase_affinity_score` returns 1.0 (PPR neutral). Fused scoring guards on `use_fallback` before calling the method → 0.0 (score identity).
4. **Weight activation**: `w_phase_explicit = 0.05`. AC-16 is non-separable from AC-12 — both must ship in the same wave. Gap is in `replay.rs` (not `extract.rs` or `output.rs`, which already handle `phase`).
5. **Handle threading**: Required non-optional constructor param at all sites. Pre-wiring grep required per lesson #3216 / pattern #3213.

## AC-16 Scope Clarification (Critical)

`extract.rs` and `output.rs` already select and propagate `phase`. The gap is solely in `replay.rs`:
- `ServiceSearchParams` needs new field `current_phase: Option<String>`
- `replay.rs` line ~96 must add `current_phase: record.context.phase.clone()` to the struct literal

This is the complete scope of AC-16. No change to `extract.rs`.

## Open Questions

None — all scope risks resolved in architecture.
