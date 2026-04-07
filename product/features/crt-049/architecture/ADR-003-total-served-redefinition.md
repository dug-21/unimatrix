## ADR-003: `total_served` Redefinition as Explicit Reads ∪ Injections

### Context

`total_served` in `FeatureKnowledgeReuse` currently holds the same value as
`delivery_count` (see `knowledge_reuse.rs` line 235: `let total_served = delivery_count;`).
This is documented in the struct as "alias of delivery_count for display." In practice,
`render_knowledge_reuse` does not display `total_served` in the rendered output — the
renderer shows `delivery_count` directly. All production test fixtures set `total_served = 0`.

SR-01 from the risk assessment requires identifying all `total_served` consumers:
- Field definition and in-module tests: `types.rs` (lines 296, 693, 831, 1382, 1425)
- Test fixtures in `retrospective.rs`: lines 1525, 1604, 2197, 2219, 3374 — all set to `0`
- The renderer (`render_knowledge_reuse`) uses `reuse.delivery_count` for the summary line,
  NOT `reuse.total_served`. `total_served` is read at line 1036 only in a comment context.

No external consumer reads `total_served` from stored `cycle_review_index.summary_json`
rows for business logic. The field is cosmetic/semantic in the current codebase.

Three options:
- Option A: Keep `total_served = delivery_count` (now `search_exposure_count`). Preserves
  the current alias behavior but continues to misrepresent the metric.
- Option B: Redefine `total_served = |explicit_reads ∪ injections|`. Closer to the true
  meaning: entries actually consumed by or pushed to agents. Search exposures are excluded.
  This is a semantics change but has zero blast radius given the consumer analysis above.
- Option C: Remove `total_served` and surface explicit reads and injections separately.
  Deferred: would require render changes and breaks the existing field contract.

Option B is selected per SCOPE.md Goal 7 and AC-14/AC-15.

The re-review behavioral delta (SR-05): existing stored rows deserialize with
`total_served = N` where N was previously `delivery_count`. After crt-049, if a stored
row is re-reviewed, the new `total_served` would compute as `explicit_read_count +
injection_count` (deduplicated), which may differ from the stored value. The advisory
message triggered by `SUMMARY_SCHEMA_VERSION` bump to `3` surfaces this delta to callers.
The advisory message text should mention the semantic change, not only the version mismatch.

### Decision

`total_served` is computed in `compute_knowledge_reuse` as:

```rust
let injection_entry_ids: HashSet<u64> = injection_entry_ids
    .values()
    .flat_map(|s| s.iter().copied())
    .collect();

let total_served = (explicit_read_ids | &injection_entry_ids).len() as u64;
// ^ set union; both are HashSet<u64>
```

Display label in `render_knowledge_reuse`: **"Entries served to agents (reads + injections)"**.

The `SUMMARY_SCHEMA_VERSION` advisory message (in `check_stored_review`) MUST be updated
to include a note that `total_served` semantics changed: search exposures no longer contribute.

AC-15 mandates a unit test that verifies:
- A cycle with explicit reads {1, 2} and injections {2, 3} produces `total_served = 3`
  (not 4 — deduplication applies).
- A cycle with zero explicit reads and zero injections but N search exposures produces
  `total_served = 0`.

### Consequences

Easier:
- `total_served` now has a precise, non-redundant meaning distinct from `search_exposure_count`.
- ASS-040 Group 10 can use `total_served` as a reliable "actually consumed" proxy.
- The metric asymmetry (cross_session_count covers only search exposures, not explicit reads)
  is partially mitigated: `total_served` captures a broader consumption picture.

Harder:
- Any consumer that relied on `total_served ≈ delivery_count` will see different values.
  Based on the consumer inventory above, no such consumer exists in the current codebase.
- The `SUMMARY_SCHEMA_VERSION` version bump is mandatory (already planned as AC-08) to
  surface this behavioral delta to callers who hold stale cached records.
