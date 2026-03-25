## ADR-003: Batch IN-Clause for Cross-Feature Entry Metadata Lookup

### Context

SR-02 identifies an N+1 DB read risk in the knowledge reuse cross-feature split. The fix
requires fetching `(title, feature_cycle, category)` for each distinct entry ID that was
delivered during the cycle. A cycle with 40+ served entries needs these fields for all IDs.

The existing `compute_knowledge_reuse` function accepts an `entry_category_lookup: Fn(u64) ->
Option<String>` closure. If this closure is implemented by calling `store.get_entry(id)` once
per ID, the result is N individual DB reads — a measurable latency regression on a tool that
already touches several tables.

Three approaches were considered:

**Option A**: Extend the existing single-entry closure to return all metadata. The closure
is called N times (N+1 pattern). Rejected: SR-02 explicitly prohibits this.

**Option B**: Add a second closure `entry_meta_lookup: Fn(&[u64]) -> HashMap<u64, EntryMeta>`
that takes the full set of IDs and returns metadata for all of them in one call. The function
accumulates all IDs first (steps 1–4), then calls the closure once. The caller implements it
as a single SQL `IN` clause. Accepted.

**Option C**: Move `compute_knowledge_reuse` into a `Store` method that can issue its own
queries. Rejected: this violates col-020b ADR-004 (#921) which explicitly keeps computation
server-side in `knowledge_reuse.rs` as a pure function with store coupling only in the caller.

The Chunked Batch Scan pattern (#883) is referenced for the chunking strategy when the ID set
exceeds SQLite's parameter limit (~999 bind parameters).

### Decision

Extend `compute_knowledge_reuse` with a second closure parameter:

```rust
entry_meta_lookup: impl Fn(&[u64]) -> HashMap<u64, EntryMeta>
```

Where `EntryMeta` is a new `pub struct EntryMeta { title: String, feature_cycle: Option<String>, category: String }` defined in `knowledge_reuse.rs` (not exposed in the public `unimatrix-observe` API).

The function calls `entry_meta_lookup` exactly once per invocation, passing all distinct entry
IDs collected in steps 1–4. If the set is empty, the call is skipped.

The caller in `tools.rs` implements the closure with this SQL:
```sql
SELECT id, title, category, feature_cycle
FROM entries
WHERE id IN (?, ...) AND status != 'quarantined'
```

Chunking: if IDs exceed 100, split into chunks of 100, issue one query per chunk, union
the results into one `HashMap`. This keeps individual query bind-parameter counts well below
SQLite's limit. Max total latency target for the batch: ≤ 50ms for ≤ 100 entries on a warm DB
(same budget as existing knowledge reuse queries).

The existing `entry_category_lookup` closure is superseded by the new closure (the metadata
contains category). For backward compatibility in tests, the old closure can be synthesized
from the new one.

`top_cross_feature_entries` is populated by filtering `entry_meta_lookup` results where
`feature_cycle != current_feature_cycle`, then sorting by serve count descending, taking top 5.

### Consequences

Easier:
- N+1 query pattern is structurally impossible: closure takes a slice, not a single ID.
- `EntryMeta` is a typed struct, not a tuple — fields are named at the call site.
- Pure function signature preserved; unit tests supply a synthetic HashMap.
- Chunking follows established pattern (#883), no new infrastructure.

Harder:
- Existing test fixtures for `compute_knowledge_reuse` must supply the new closure parameter.
- `EntryMeta` is a new type that the spec must define and tests must construct.
- The caller in `tools.rs` must collect all IDs before calling `compute_knowledge_reuse`
  (minor restructuring of the call site).
