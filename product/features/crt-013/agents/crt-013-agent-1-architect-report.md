# Agent Report: crt-013-agent-1-architect

## Deliverables

### ARCHITECTURE.md
`product/features/crt-013/architecture/ARCHITECTURE.md`

### ADRs (Unimatrix only, per CLAUDE.md)

| ADR | Unimatrix ID | Title |
|-----|-------------|-------|
| ADR-001 | #701 | W_COAC Disposition — Delete Dead Weight Constant |
| ADR-002 | #702 | Two-Mechanism Co-Access Architecture |
| ADR-003 | #703 | Behavior-Based Status Penalty Tests |
| ADR-004 | #704 | Single StatusAggregates Store Method |

## Integration Surface Summary

| Integration Point | Change | Crate |
|-------------------|--------|-------|
| `co_access_affinity()` | Remove | unimatrix-engine |
| `W_COAC` constant | Remove | unimatrix-engine |
| `EpisodicAugmenter` + module | Remove | unimatrix-adapt |
| `AdaptationService::episodic_adjustments()` | Remove | unimatrix-adapt |
| `BriefingService.semantic_k` | Add field | unimatrix-server |
| `Store::compute_status_aggregates()` | New method | unimatrix-store |
| `Store::load_active_entries_with_tags()` | New method | unimatrix-store |
| `StatusAggregates` struct | New type | unimatrix-store |

## Key Decisions

1. **W_COAC: Option A (delete)** — Dead code, zero behavioral change, stored weights stay at 0.92
2. **Two-mechanism co-access is transitional** — Graph Enablement may replace scalar boost; ADR-002 documents this explicitly per human framing
3. **Behavior-based penalty tests** — Assert ranking not scores; survive Graph Enablement constant replacement per human framing
4. **Single StatusAggregates method** — One SQL round-trip, comparison test proves equivalence

## Risk Mitigations Applied

- SR-01: Exhaustive grep confirms W_COAC/co_access_affinity only in confidence.rs
- SR-02: Tests use deterministic confidence, isolated from crt-011
- SR-03: Comparison test with field-by-field equality, explicit NULL handling contract
- SR-04: Grep confirms episodic referenced from 3 files only, no external callers
- SR-05: Minimal config — field + env var, no config struct
- SR-06: Pre-computed embeddings, ranking assertions not score assertions
- SR-07: No new indexes needed at current scale
