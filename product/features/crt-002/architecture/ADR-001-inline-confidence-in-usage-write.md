## ADR-001: Inline Confidence Computation in Usage Write Transaction

### Context

crt-002 computes confidence from EntryRecord fields (access_count, helpful_count, etc.) that are updated by crt-001's `record_usage()`. The naive approach -- a separate read-modify-write after usage recording -- adds a second write transaction per retrieval. Scope risk SR-03 identified this as a contention concern, and SR-08 identified the overhead of using `Store::update()` (which performs full index diffs) for a non-indexed field.

The existing `record_usage()` already reads each entry, modifies counter fields, and writes it back within a single write transaction. The confidence computation only needs the updated counter values, which are available in-scope at that point.

### Decision

Extend `record_usage()` into `record_usage_with_confidence()` that accepts an optional confidence computation function `Option<&dyn Fn(&EntryRecord, u64) -> f32>`. After applying all counter updates to the deserialized record, if the function is provided, call it with the updated record and current timestamp, set `record.confidence = result`, and serialize. The confidence update happens inside the same write transaction as the counter updates -- zero additional transactions on the retrieval path.

The existing `record_usage()` is preserved (calls `record_usage_with_confidence` with `None`) for backward compatibility with the `EntryStore::record_access` trait method.

The function pointer approach keeps the store crate free of confidence formula dependencies. The server passes `confidence::compute_confidence` as the function; the store crate has no knowledge of weights, Wilson scores, or component functions.

### Consequences

**Easier:**
- No additional write transactions on the retrieval path (resolves SR-03)
- No index diffs for confidence updates (resolves SR-08)
- Confidence is always consistent with the latest counter values (no stale-read between transactions)
- The store crate remains a pure data layer -- it applies a function, it doesn't know the formula

**Harder:**
- The `record_usage_with_confidence` signature is wider (7 parameters + function pointer)
- Testing requires either a real confidence function or a test double
- If the confidence function panics, it can abort the entire usage write transaction (mitigated: the function is pure and cannot panic on valid EntryRecord inputs)
