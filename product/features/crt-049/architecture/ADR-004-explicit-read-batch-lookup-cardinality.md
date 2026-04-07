## ADR-004: Cardinality Guard for Explicit Read `batch_entry_meta_lookup`

### Context

SR-03 from the risk assessment flags that the `batch_entry_meta_lookup` call for explicit
read IDs introduces a DB call whose cost scales with the size of the explicit read set.
No cardinality bound was stated in the scope.

The existing `batch_entry_meta_lookup` implementation (tools.rs lines 3143–3188) already
chunks input at 100 IDs per IN-clause query (pattern #883, col-026 ADR-003). However,
for a high-volume cycle with agents making hundreds of `context_get` calls, the explicit
read set could be large (e.g., 200–300 IDs → 3 chunked queries).

Relevant bounds from the system:
- A typical cycle spans a few sessions with tens of knowledge reads. High-end cycles
  (e.g., large delivery swarms with 10+ agents) might produce 50–100 explicit reads.
- `batch_entry_meta_lookup` uses the write pool server connection and targets ≤ 50ms
  per 100-entry chunk (col-026 ADR-003 latency target).
- The explicit read set is deduplicated (HashSet), so even if an entry is read 20 times,
  it contributes 1 ID to the lookup.

Three options:
- Option A: No cap. Rely on existing chunking to bound per-query cost. The total number
  of distinct entries in a Unimatrix knowledge base is bounded by the corpus size (currently
  hundreds to low thousands of entries). A cycle cannot read more entries than exist.
  Rejected as incomplete: no guard prevents a future regression.
- Option B: Cap explicit read IDs at 500 before the batch lookup. IDs beyond the cap are
  excluded from `explicit_read_by_category` (counted in `explicit_read_count` but not
  categorized). Log a warning when the cap is hit.
- Option C: Cap at 500 AND log a metric. Selected.

Expected upper bound per cycle: ≤ 150 distinct IDs for a large swarm cycle. The 500 cap
provides a 3x safety margin without restricting realistic usage.

### Decision

Before calling `batch_entry_meta_lookup` for explicit read IDs, apply a cap of 500 IDs:

```rust
const EXPLICIT_READ_META_CAP: usize = 500;

let explicit_ids_vec: Vec<u64> = explicit_ids.iter().copied().collect();
let lookup_ids = if explicit_ids_vec.len() > EXPLICIT_READ_META_CAP {
    tracing::warn!(
        "crt-049: explicit read ID set ({}) exceeds cap {}; \
         explicit_read_by_category will be partial",
        explicit_ids_vec.len(),
        EXPLICIT_READ_META_CAP
    );
    &explicit_ids_vec[..EXPLICIT_READ_META_CAP]
} else {
    &explicit_ids_vec
};
let explicit_meta_map = batch_entry_meta_lookup(store, lookup_ids).await;
```

`explicit_read_count` is still computed from the full `explicit_ids` set (uncapped) —
the cap only limits the category join. `explicit_read_by_category` will be labeled as
potentially partial in documentation (not in rendered output — the warning covers operational
monitoring).

The `EXPLICIT_READ_META_CAP` constant is placed in `tools.rs` near
`compute_knowledge_reuse_for_sessions`. It is not exposed in `FeatureKnowledgeReuse` or
the public API.

### Consequences

Easier:
- DB cost per cycle review is bounded: worst case is ceil(500/100) = 5 chunked queries
  for explicit read meta, plus the existing ceil(N_ql_inj/100) queries for query_log/injection_log IDs.
- The warning log is observable in production without requiring a new metric.

Harder:
- `explicit_read_by_category` may undercount for pathologically large cycles (>500 distinct reads).
  This is documented behavior, not a silent error.
- The cap constant must be documented in the integration brief so the spec writer can
  reference it in test plan notes.
