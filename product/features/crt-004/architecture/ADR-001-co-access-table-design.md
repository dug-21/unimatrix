## ADR-001: CO_ACCESS Table Key Design and Partner Lookup Strategy

### Context

Co-access pairs are stored as `(min_id, max_id) -> CoAccessRecord` in a redb table. To compute co-access boost for a search result, we need to find all co-access partners of a given entry X. This requires finding all pairs where X appears as either the first or second element.

With ordered keys `(min, max)`, entry X appears as the first element in pairs `(X, Y)` where Y > X, and as the second element in pairs `(Z, X)` where Z < X. Range scanning for the first case is efficient: `(X, 0)..=(X, u64::MAX)`. But finding pairs where X is the second element requires a full table scan -- redb does not support secondary indexes.

Three options:
1. **Dual-key storage**: Store each pair twice as `(A, B)` and `(B, A)`. Doubles storage, halves read complexity.
2. **Supplementary reverse index**: A second table mapping `entry_id -> set of partner_ids`. Updated alongside CO_ACCESS.
3. **Full table scan**: Iterate all CO_ACCESS entries. Simple but O(total_pairs) per lookup.

### Decision

Use **full table scan with range-prefix optimization**. The CO_ACCESS table stores ordered pairs `(min, max)`. The `get_co_access_partners` method performs:

1. **Prefix range scan** for pairs where entry is the min: `(entry_id, 0)..=(entry_id, u64::MAX)`. O(partners_as_min).
2. **Full table scan** for pairs where entry is the max: iterate all entries, filter `key.1 == entry_id`. O(total_pairs).

At Unimatrix's scale (expected 1K-10K co-access pairs), a full table scan is ~1-5ms. This is acceptable for a query-time operation that runs at most 3 times per search (once per anchor).

If co-access pair volume grows beyond 50K, revisit with dual-key storage or a reverse index. The interface (`get_co_access_partners`) hides the implementation -- upgrading the scan strategy does not change the API.

### Consequences

- **Simpler write path**: One write per pair, not two. Half the write volume.
- **Predictable read cost**: O(total_pairs) per anchor lookup. At 10K pairs, ~5ms. At 3 anchors, ~15ms total. Within latency budget for search.
- **Migration path clear**: If performance degrades, dual-key or reverse index can be added behind the same API without breaking callers.
- **No additional table**: One table instead of two. Simpler schema, simpler cleanup.
