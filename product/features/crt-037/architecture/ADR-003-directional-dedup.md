## ADR-003: Directional Dedup for query_existing_informs_pairs

### Context

SCOPE.md OQ-3 resolved that `query_existing_informs_pairs` uses directional
`(source_id, target_id)` dedup rather than symmetric `(min(source, target), max(source, target))`.

The existing `query_existing_supports_pairs` uses symmetric normalization. The SQL stores
edges in the order they were written, and the pre-filter check in Phase 4 normalizes both
sides to `(min, max)` before set lookup. This is correct for `Supports` because the
relationship is symmetric: "A supports B" and "B supports A" are treated as the same edge
for dedup purposes.

`Informs` is not symmetric. The temporal ordering guard in Phase 4b ensures:
- `source.created_at < target.created_at` always holds when an edge is written.
- The reverse pair (target_id, source_id) would fail the temporal guard during detection.

This means a `(source_id=376, target_id=2060)` edge in `GRAPH_EDGES` can never coexist with
a `(source_id=2060, target_id=376)` Informs edge — the detection logic prevents it. Symmetric
dedup would therefore never suppress a legitimate reverse edge. But it introduces an
unnecessary normalization step and, more importantly, obscures the directional contract:
reading `existing_informs_pairs.contains(&(source, target))` is explicit about direction;
reading `existing_informs_pairs.contains(&(min(a,b), max(a,b)))` conceals it.

There is a theoretical risk: if data ever had ordering anomalies (two entries with the same
`created_at` timestamp at millisecond resolution), symmetric dedup could suppress a valid
pair (SCOPE.md OQ-3 language: "could suppress valid edges if data had ordering anomalies").
Directional dedup is safe in this case — it will attempt to score the pair again, and the
temporal guard will drop it at Phase 4b. The `INSERT OR IGNORE` backstop still prevents
a double-write at the DB level.

### Decision

`query_existing_informs_pairs` returns `HashSet<(u64, u64)>` where each tuple is the
directional `(source_id, target_id)` pair as stored in `GRAPH_EDGES` — no normalization:

```sql
SELECT source_id, target_id
FROM graph_edges
WHERE relation_type = 'Informs' AND bootstrap_only = 0
```

The Phase 4b dedup check uses:

```rust
if existing_informs_pairs.contains(&(source_id, neighbor_id)) {
    continue; // directional: this exact directed edge was already written
}
```

The `INSERT OR IGNORE` on the `UNIQUE(source_id, target_id, relation_type)` index is the
secondary backstop. The pre-filter avoids redundant NLI scoring for already-written pairs;
the backstop prevents duplicate rows if the pre-filter was empty (degraded path).

### Consequences

- The dedup semantic matches the Informs edge semantic: directional, empirical-to-normative,
  temporally ordered. A developer reading the Phase 4b dedup check understands it immediately
  without needing to know about the normalization convention from `Supports`.
- The SQL is simpler than `query_existing_supports_pairs` (no `MIN`/`MAX` normalization in
  the query or in the Rust mapping).
- The `HashSet<(u64, u64)>` type is identical to the Supports pre-filter type. The Phase 2
  fetch and the Phase 4b check follow the same pattern as the Supports pre-filter, with the
  only difference being the `contains` key construction (no `min`/`max` swap).
- Tests must cover: empty table, single row returned as-is, row stored in reverse order is
  NOT found by directional lookup (verifying non-normalization), and bootstrap_only=1 rows
  excluded. This last test is important because if normalization were accidentally added, the
  reverse-order test would silently pass.
