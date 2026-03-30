# Agent Report: crt-034-agent-2-spec

## Task

Write SPECIFICATION.md for crt-034 (Recurring co_access → GRAPH_EDGES Promotion Tick).

## Output

`product/features/crt-034/specification/SPECIFICATION.md`

## Acceptance Criteria Produced

15 acceptance criteria (AC-01 through AC-15), expanding the 13 from SCOPE.md:

- AC-01: New qualifying pair promoted on next tick
- AC-02: Existing edge with drift > 0.1 is updated within one tick
- AC-03: Existing edge with drift <= 0.1 produces zero DB writes
- AC-04: Cap enforced; highest-count pairs selected first
- AC-05: Tick ordering — after orphaned-edge compaction, before TypedGraphState::rebuild
- AC-06: InferenceConfig field default 200, range [1, 10000], project-level merge
- AC-07: CO_ACCESS_GRAPH_MIN_COUNT = 3 exported from unimatrix-store
- AC-08: EDGE_SOURCE_CO_ACCESS = "co_access" exported from unimatrix-store
- AC-09: Empty/sub-threshold table is a clean no-op
- AC-10: max_co_access_promotion_per_tick = 0 rejected with clear error
- AC-11: Infallible — write errors logged at warn!, tick continues
- AC-12: Inserted edge fields: bootstrap_only=0, source=EDGE_SOURCE_CO_ACCESS, created_by='tick', relation_type='CoAccess'
- AC-13: Global MAX(count) normalization — not just the capped batch
- AC-14 (new, from SR-03/entry #3822): INSERT OR IGNORE no-op on already-promoted edge produces zero writes
- AC-15 (new, from SR-03/entry #3822): Sub-threshold pair whose edge already exists is not removed by this tick

## Key Decisions Made

- Weight delta (0.1) codified as `CO_ACCESS_WEIGHT_UPDATE_DELTA` named constant (module-private), not a config field — confirmed per SCOPE.md design decisions.
- SQL shape left unconstrained (open question for architect) — spec requires correctness (global normalization, conditional UPDATE, INSERT OR IGNORE, count DESC ordering) but does not mandate two-query loop vs. CTE vs. subquery.
- GC of sub-threshold edges explicitly excluded and called out in NOT in scope.
- No COUNTERS marker — confirmed recurring tick; structural idempotency only.
- Migration constant unification (AC-07) flagged as open question for architect — whether CO_ACCESS_BOOTSTRAP_MIN_COUNT is removed or aliased.

## Open Questions for Architect

1. SQL shape: two-step per-pair loop vs. CTE/combined statement for INSERT + conditional UPDATE (SR-02)
2. MAX(count) placement: separate read-pool query vs. subquery in batch fetch (SR-01)
3. Tick anchor comment strategy in background.rs (SR-06)
4. Migration constant: remove CO_ACCESS_BOOTSTRAP_MIN_COUNT or leave as local alias of CO_ACCESS_GRAPH_MIN_COUNT (AC-07)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — entries #3822 and #3821 directly applicable; incorporated near-threshold idempotency pattern (AC-14, AC-15) and GRAPH_EDGES write path/ordering constraints.
