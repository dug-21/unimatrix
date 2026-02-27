## ADR-002: Maintenance Opt-In on context_status

### Context

Prior to crt-005, `context_status` was primarily a read-only diagnostic tool. The one exception is co-access stale pair cleanup (crt-004), which performs lightweight write operations piggybacked on the status call. crt-005 extends this pattern significantly by adding confidence refresh (batch writes to ENTRIES) and HNSW graph compaction (HNSW rebuild + VECTOR_MAP writes).

SR-07 identified that mixing reads and writes on a diagnostic call violates command-query separation. A `status` call should report state, not change it. The behavioral contract changes from "safe to call repeatedly with no side effects" to "calling this tool modifies the knowledge base."

Three design options were considered:
1. **Default-on maintenance with opt-out**: `maintenance` defaults to true. Simple self-healing, but a read operation silently writes — violates principle of least surprise.
2. **Default-off maintenance with opt-in**: `maintenance` defaults to false. Status is read-only by default. Callers explicitly request maintenance when they want writes. Clean CQS separation.
3. **Separate tool**: A `context_maintain` tool for writes. Cleanest separation, but adds API surface for a tool that is only called occasionally.

Key insight: `context_status` is an admin/diagnostic tool called infrequently (not on any agent hot path). It is inherently a maintenance action — when someone checks KB health, they are performing maintenance. But the default should be safe (read-only reporting), and writes should require explicit intent.

### Decision

Add `maintain: Option<bool>` to `StatusParams` with a default of `false`.

When `maintain` is absent or false (default — read-only diagnostics):
- All coherence dimension scores are computed (read-only)
- Lambda and maintenance recommendations are generated
- `stale_confidence_count` reports the number of stale entries
- `graph_stale_ratio` reports the current stale ratio
- No confidence refresh writes occur
- No HNSW compaction occurs
- Co-access stale pair cleanup still runs (existing lightweight behavior, preserves backward compatibility)
- `confidence_refreshed_count = 0` and `graph_compacted = false`

When `maintain` is true (explicit opt-in — maintenance mode):
- All read-only behavior above, plus:
- Confidence refresh runs for stale entries (capped at 100 per call)
- HNSW graph compaction triggers when stale ratio exceeds threshold
- StatusReport includes `confidence_refreshed_count` and `graph_compacted` reflecting actual actions

The coherence metric (lambda) is always computed regardless of the `maintain` parameter. Dimension scores reflect the current state of the knowledge base, not the post-maintenance state. Recommendations tell the caller what `maintain: true` would fix.

### Consequences

**Easier:**
- `context_status` remains a safe read-only diagnostic by default — no surprise writes
- The behavioral contract is explicit: callers opt in to writes with clear intent
- Backward compatible: existing callers without the parameter get the same read-only behavior they expect
- Testing is simpler: the default path has no write side effects to verify
- Recommendations bridge the gap: "42 entries have stale confidence" tells the caller why they should call again with `maintain: true`

**Harder:**
- Maintenance does not happen automatically — callers must explicitly request it
- An agent that never passes `maintain: true` will see lambda degrade over time without self-healing
- Two-call pattern for maintenance: first call diagnoses, second call (with `maintain: true`) fixes
- Coordinators/scrum-masters should be taught to call `context_status(maintain: true)` at session boundaries
