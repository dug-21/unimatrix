# crt-034 Architect Report

Agent: crt-034-agent-1-architect

## Outputs

- ARCHITECTURE.md: `product/features/crt-034/architecture/ARCHITECTURE.md`
- ADR-001: `product/features/crt-034/architecture/ADR-001-sql-strategy.md` (Unimatrix #3823)
- ADR-002: `product/features/crt-034/architecture/ADR-002-constants-location.md` (Unimatrix #3824)
- ADR-003: `product/features/crt-034/architecture/ADR-003-weight-delta-constant.md` (Unimatrix #3825)
- ADR-004: `product/features/crt-034/architecture/ADR-004-inference-config-field.md` (Unimatrix #3826)
- ADR-005: `product/features/crt-034/architecture/ADR-005-tick-insertion-and-sr05.md` (Unimatrix #3827)
- ADR-006: `product/features/crt-034/architecture/ADR-006-edge-directionality-v1-contract.md` (Unimatrix #3828)

## Key Decisions

| ADR | Decision | Unimatrix ID |
|-----|----------|-------------|
| ADR-001 | SQL: single-query batch fetch with scalar subquery MAX — one round-trip, Option C (UPSERT) rejected | #3823 |
| ADR-002 | Constants in unimatrix-store/src/read.rs alongside EDGE_SOURCE_NLI; CO_ACCESS_WEIGHT_UPDATE_DELTA module-private | #3824 |
| ADR-003 | Weight delta (0.1) as module-private const, not InferenceConfig field — engineering constant, not operator policy | #3825 |
| ADR-004 | max_co_access_promotion_per_tick in InferenceConfig, default 200, range [1, 10000], exact max_graph_inference_per_tick pattern | #3826 |
| ADR-005 | Anchor comment for tick insertion; current_tick: u32 added to function signature for SR-05 early-run warn | #3827 |
| ADR-006 | v1 writes one-directional edges matching bootstrap; reverse-edge follow-up contract documented | #3828 |

## Integration Surface

| Item | Value |
|------|-------|
| New module | `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` |
| Function signature | `pub(crate) async fn run_co_access_promotion_tick(store: &Store, config: &InferenceConfig, current_tick: u32)` |
| New constants (unimatrix-store) | `EDGE_SOURCE_CO_ACCESS: &str`, `CO_ACCESS_GRAPH_MIN_COUNT: i64` in `read.rs`, re-exported from `lib.rs` |
| Module-private constant | `CO_ACCESS_WEIGHT_UPDATE_DELTA: f32 = 0.1` in the new module |
| InferenceConfig new field | `max_co_access_promotion_per_tick: usize`, default 200, range [1, 10000] |
| Tick insertion | After orphaned-edge compaction (~line 547), before TypedGraphState::rebuild() (~line 549), unconditional |
| services/mod.rs | Add `pub(crate) mod co_access_promotion_tick;` |

## Open Questions

1. **GH #409 sequencing**: Has GH #409 been merged? If yes, check whether qualifying
   co_access pairs (count >= 3) still exist. SR-05 warn on first 5 ticks surfaces this
   at runtime, but confirming before implementation is safer.

2. **Reverse-edge follow-up issue**: ADR-006 documents the one-directionality v1 contract
   and the protocol for a safe follow-up. A GH issue should be filed referencing ADR-006
   so the fix has the correct implementation context.
