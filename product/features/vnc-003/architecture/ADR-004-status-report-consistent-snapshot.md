## ADR-004: Status Report Consistent Snapshot

### Context

`context_status` computes multiple aggregate metrics from different tables (COUNTERS, CATEGORY_INDEX, TOPIC_INDEX, ENTRIES). If these reads happen across separate transactions, concurrent writes could produce inconsistent results (e.g., total counts don't match distribution sums).

### Decision

All status report reads happen inside a single `store.begin_read()` transaction wrapped in `spawn_blocking`. This provides a consistent snapshot across all tables. The read transaction does not block writers (redb uses MVCC).

Full table scans are used for distribution metrics and correction chain/security metrics. This is acceptable at Unimatrix's expected scale (hundreds to low thousands of entries per project). If performance becomes an issue in the future, pre-computed counters can be added incrementally.

The `topic` and `category` filter params on `context_status` narrow the distribution output (only show distribution for the filtered dimension) but do not affect the total status counts or correction/security metrics -- those are always global.

### Consequences

**Easier:**
- Consistent metrics: totals always match distributions
- No locking overhead: MVCC read snapshot
- Simple implementation: single function with one transaction

**Harder:**
- Full ENTRIES scan for correction chain + security metrics could be slow at very large scale (deferred: pre-computed counters)
- Read transaction holds a snapshot, preventing compaction of the scanned pages until the transaction is dropped (acceptable: status report is fast)
