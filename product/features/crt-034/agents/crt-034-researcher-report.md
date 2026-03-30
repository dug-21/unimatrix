# crt-034 Researcher Report

## Summary

SCOPE.md written to `product/features/crt-034/SCOPE.md`. Key findings below.

## Key Findings

### Tick Ordering (confirmed from background.rs lines 447–784)

```
1. maintenance_tick()              ← calls cleanup_stale_co_access() inside StatusService
2. Orphaned-edge compaction        ← DELETE graph_edges WHERE endpoint not in entries
3. TypedGraphState::rebuild()      ← reads GRAPH_EDGES; builds PPR graph  ← BEFORE THIS
4. PhaseFreqTable::rebuild()
5. Contradiction scan (N-ticks)
6. extraction_tick()
7. maybe_run_bootstrap_promotion() ← crt-023 one-shot NLI
8. run_graph_inference_tick()      ← crt-029 recurring NLI
```

The new promotion tick inserts between steps 2 and 3: after stale cleanup and
orphaned-edge compaction, before the graph rebuild that PPR consumes.

### Constants

- `CO_ACCESS_BOOTSTRAP_MIN_COUNT: i64 = 3` — private to `migration.rs`. Must be
  exposed as `pub const CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` from `unimatrix-store`.
- `EDGE_SOURCE_NLI: &str = "nli"` — in `read.rs:1630`. A parallel
  `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` should be added alongside it.

### Bootstrap SQL Pattern (migration.rs lines 422–446)

INSERT OR IGNORE with window function `MAX(count) OVER ()` for normalized weight.
`bootstrap_only = 0`, `source = 'co_access'`.

### No Existing UPDATE Path

There is no `UPDATE graph_edges SET weight = ?` anywhere in the codebase. The NLI
bootstrap promotion (crt-023) uses DELETE + INSERT OR IGNORE (full replacement, not
weight patch). The co_access promotion needs a true UPDATE for existing edges with
drifted weights — this must use `write_pool_server()` directly, not the analytics drain
(`AnalyticsWrite::GraphEdge` is INSERT OR IGNORE only).

### Write Path Constraint

The analytics drain `AnalyticsWrite::GraphEdge` docstring explicitly states:
"W1-2 NLI confirmed edge writes MUST NOT use this variant — use direct write_pool path
instead." This constraint applies equally to the co_access promotion tick.

### InferenceConfig Pattern

New field follows `max_graph_inference_per_tick` exactly:
- Field name: `max_co_access_promotion_per_tick: usize`
- Serde default fn
- validate() range check [1, 10000]
- Project-level config merge
- Default: 200 (no ML inference cost, can process more pairs per tick)

### Dependency Risk

GH #409 (intelligence-driven retention) prunes co_access rows. If it ships before
crt-034, signal crossing the threshold is lost permanently. This is a hard blocking
dependency — crt-034 must ship first.

## Open Questions Surfaced

1. Weight delta (0.1): named constant or config field? Recommend constant for v1.
2. Promotion cap default: 200 vs matching NLI's 100?
3. MAX(count) normalization scope: two-query SQL approach vs in-memory over capped batch?
4. Stale-edge GC for below-threshold pairs: defer to #409 or include in v1?
5. Idempotency contract: no COUNTERS marker needed (recurring, not one-shot) — confirm.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 19 relevant entries; most
  relevant: #2704 (crt-023 bootstrap idempotency), #3785 (crt-032 w_coac zeroed,
  PPR is sole co-access signal carrier), #3739 (PPR dense CoAccess graph from bootstrap),
  #2417 (crt-021 typed edge weight model).
- Stored: entry #3821 "GRAPH_EDGES tick writes: use write_pool_server() directly; insert between orphaned-edge compaction and TypedGraphState rebuild" via /uni-store-pattern
