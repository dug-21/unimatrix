## ADR-006: Behavioral Graph Edge Writes Use write_pool_server() Directly, Not Analytics Drain

### Context

The original SCOPE.md, ARCHITECTURE.md §Component 1 / §Technology Decisions, SPECIFICATION.md FR-06/FR-07, and the IMPLEMENTATION-BRIEF.md Resolved Decisions table all specified that behavioral `Informs` edges should be written via `store.enqueue_analytics(AnalyticsWrite::GraphEdge)` — the fire-and-forget analytics drain.

During pseudocode development (Stage 3b), a structural incompatibility surfaced: the `write_graph_edge` return contract required by pattern #4041 cannot be satisfied by `enqueue_analytics`.

The return contract, introduced explicitly to prevent a recurrence of the crt-040 Gate 3a regression, requires `emit_behavioral_edges` to distinguish three outcomes per write:

| Return | Meaning | Counter action |
|--------|---------|----------------|
| `Ok(true)` | New row inserted | Increment `edges_enqueued` |
| `Ok(false)` | UNIQUE conflict, silently ignored | Do not increment |
| `Err(_)` | SQL infrastructure failure | Log `warn!`, do not increment |

`enqueue_analytics` is `pub fn` (non-async), uses `try_send` semantics, and returns `()`. It cannot provide `rows_affected()` feedback. Wrapping `enqueue_analytics` to produce these three outcomes is not possible without bypassing the drain entirely. Additionally, `enqueue_analytics` sheds events when its bounded mpsc channel is full (queue-capacity-based, not `bootstrap_only`-based), so even `bootstrap_only=false` behavioral edges are subject to silent loss under queue pressure.

Two options were evaluated:

**Option A** — Use `write_pool_server()` directly for behavioral graph edge writes via a `write_graph_edge(store, source_id, target_id, weight) -> Result<bool>` helper. Returns `true` on new row, `false` on UNIQUE conflict.

**Option B** — Revert to `enqueue_analytics`, remove the `edges_enqueued` counter accuracy requirement, and accept approximate counting (counter becomes a lower bound only).

Pattern #4041 (root cause of crt-040 Gate 3a rework) is non-negotiable: it was introduced specifically to prevent counter inflation on UNIQUE conflicts and is binding on all new graph edge emission paths. This eliminates Option B.

Precedent: ADR-003 (crt-025, entry #3000) established that structural tables using direct write-pool writes are the correct pattern for data that cannot tolerate silent shedding. Entry #3883 documents the same principle for background tick graph edge writes.

### Decision

`emit_behavioral_edges` writes behavioral `Informs` edges directly via `write_pool_server()` using a `write_graph_edge` private helper that executes `INSERT OR IGNORE INTO graph_edges` and returns `Result<bool>` keyed off `rows_affected() == 1`.

The `enqueue_analytics(AnalyticsWrite::GraphEdge)` path is NOT used for behavioral edge emission. The analytics drain is bypassed for this write path only.

The `write_graph_edge` helper and `emit_behavioral_edges` live in `services/behavioral_signals.rs`. The helper takes a `write_pool_server()` connection directly from `SqlxStore`, consistent with the structural-write pattern used by `insert_goal_cluster`, `store_cycle_review`, and `insert_cycle_event`.

`INSERT OR IGNORE` semantics are preserved: a behavioral `Informs` edge that conflicts with an existing edge (e.g., one written by NLI) is silently dropped. `edges_enqueued` increments only on `Ok(true)` (new row), never on `Ok(false)` (conflict) or `Err` — satisfying pattern #4041.

The analytics drain (`enqueue_analytics`) continues to be used for other analytics writes (co-access, observation phase metrics) where eventual consistency and shed tolerance are acceptable.

**Impact on integration tests**: Since writes are synchronous (direct pool, no drain), integration tests asserting `graph_edges` rows after `context_cycle_review` do NOT require a drain flush step. This supersedes RISK-TEST-STRATEGY I-02 for behavioral edge assertions specifically: I-02's drain flush requirement applies only to the R-02 contract verification test (asserting `edges_enqueued` counter accuracy) and to any future tests that also exercise analytics-drain-written edges in the same assertion. It does not apply to behavioral edge row-count assertions.

### Consequences

**Easier**:
- `edges_enqueued` counter is accurate: it counts exactly the new rows inserted, not approximate. Satisfies pattern #4041 and prevents the crt-040 Gate 3a regression class.
- Integration tests asserting `graph_edges` rows written by step 8b do not require drain flush waits, making them faster and more deterministic.
- Behavioral edges are never silently shed under queue pressure. The 200-pair / 400-directed-edge maximum per cycle review call is bounded and safe for direct write-pool use.
- Consistent with the established structural-write pattern (entries #3000, #3883): `write_pool_server()` for data that must not be shed.

**Harder**:
- Behavioral edge writes now consume write-pool connections (max 2 connections). A 400-edge step 8b run will hold a write connection for its duration. Step 8b is already non-fatal; write-pool contention does not change the error-handling posture.
- The deviation between SPECIFICATION.md FR-06/FR-07 (specifies `enqueue_analytics`) and the implementation is now formally resolved by this ADR. The specification prose is overridden. Future spec readers must consult this ADR for the authoritative write path.
- RISK-TEST-STRATEGY I-02 requires a clarifying note (see below) to avoid confusing future test authors who read the strategy document after this decision.
