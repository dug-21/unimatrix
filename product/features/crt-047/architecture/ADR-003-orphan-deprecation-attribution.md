## ADR-003: Orphan Deprecation Attribution — ENTRIES-Based at Review Time

### Context

AC-04 specifies that `orphan_deprecations` counts entries with
`status = Deprecated AND superseded_by IS NULL` where the deprecation timestamp falls
within the current cycle's window (cycle_start event timestamp → review call timestamp),
attributed via AUDIT_LOG join.

SR-01 flags that the AUDIT_LOG `operation` string must be verified across all deprecation
write paths before designing the SQL. The risk is silent under-counting: if some
deprecation paths do not write a `context_deprecate` audit row, the join will miss them.

Source code analysis reveals three deprecation write paths:

1. **`context_deprecate` (explicit)**: Calls `deprecate_with_audit()` which calls
   `log_audit_event()` with `operation: "context_deprecate"`. The entry ID is in
   `target_ids`. This path DOES write an AUDIT_LOG row.

2. **`context_correct` (chain-deprecation of original)**: The deprecation of the original
   entry is performed atomically inside `store.correct_entry()` via a direct SQL `UPDATE
   entries SET status = Deprecated`. The only AUDIT_LOG event is `"context_correct"` with
   `target_ids: [original_id, new_id]`. There is NO separate `"context_deprecate"` audit
   row for the original entry. A chain-deprecation via correction always sets
   `superseded_by = new_id` on the deprecated original — so it is NOT an orphan.

3. **Lesson-learned auto-supersede** (in `tools.rs`, the `persist_lesson_learned_if_applicable`
   helper): Calls `server.store.update_status(old_id, Status::Deprecated)` directly,
   followed by direct `store.update()` to set the bidirectional supersession links. NO
   AUDIT_LOG row is written for this deprecation. The old entry ends up with
   `superseded_by = Some(new_id)` — so it is NOT an orphan.

**Critical finding**: Every write path that produces an orphan deprecation (deprecated
without a successor, i.e., `superseded_by IS NULL`) goes through explicit
`context_deprecate`. The two chain-deprecation paths (`context_correct` and
lesson-learned auto-supersede) always set `superseded_by` to the new entry's ID. They
can never produce orphans.

Therefore: an orphan deprecation is definitionally one that originated from an explicit
`context_deprecate` call. The AUDIT_LOG join using `operation = 'context_deprecate'` is
complete and correct for orphan attribution.

However, there is a secondary complexity: the AUDIT_LOG row must be joined to the
deprecated entry's ID. The `target_ids` column in AUDIT_LOG stores a JSON array. Joining
on a JSON array in SQLite requires `json_each()` or a text match. This adds query
complexity.

An alternative approach: query ENTRIES directly, using `updated_at` as the deprecation
timestamp proxy, with no AUDIT_LOG join.

Analysis: `updated_at` on an entry is set by every write that touches the entry (status
changes, content updates, etc.). For entries that are deprecated and never subsequently
modified, `updated_at` equals the deprecation time. For entries that were deprecated and
then had their `superseded_by` field set afterward (the lesson-learned path does this in
two separate writes), `updated_at` reflects the last link update, not the deprecation
itself. But since lesson-learned entries are never orphans, this ambiguity does not affect
orphan counting.

The ENTRIES-based approach (`updated_at` within cycle window, `status = Deprecated`,
`superseded_by IS NULL`, `feature_cycle = ?`) is simpler and avoids the JSON array parse.
But `feature_cycle` records when the entry was *created*, not when it was *deprecated*.
An entry created in cycle A but deprecated in cycle B would be attributed to cycle A by
the `feature_cycle` filter.

The SCOPE.md explicitly resolves OQ-02: "Attribution must come from the AUDIT_LOG: status
changes to Deprecated with a timestamp within the current cycle's window."

### Decision

Use a hybrid approach: primary query is ENTRIES-based using `updated_at` as the
deprecation timestamp, but restrict to entries that have a corresponding AUDIT_LOG row
with `operation = 'context_deprecate'` within the cycle window. This satisfies the
OQ-02 resolution (AUDIT_LOG attribution), avoids JSON parsing on target_ids (the entry
ID appears in `detail` as well, and the filter is achievable with a subquery), and
remains correct given the finding that only explicit `context_deprecate` produces orphans.

Concretely, the query for orphan deprecations within the cycle window:

```sql
-- Cycle window bounds: cycle_start_ts and review_ts
-- Both are passed as query parameters

SELECT COUNT(*) FROM entries e
WHERE e.status = 'deprecated'
  AND e.superseded_by IS NULL
  AND e.updated_at >= ?1   -- cycle_start_ts
  AND e.updated_at <= ?2   -- review_ts (now)
  AND EXISTS (
      SELECT 1 FROM audit_log al
      WHERE al.operation = 'context_deprecate'
        AND al.timestamp >= ?1
        AND al.timestamp <= ?2
        AND al.target_ids LIKE '%' || e.id || '%'
  )
```

The `LIKE` pattern on `target_ids` is a coarse filter over the JSON array text.
Since `target_ids` is a JSON array of integers, this is safe: an integer ID will not
appear as a false-positive substring of another integer in the array for IDs above 9.
For robustness, the implementor should use `json_each()` if available and tested, or
verify that the `LIKE` match is acceptable given the corpus size.

A simpler alternative that avoids the subquery: given the finding that ALL orphan
deprecations are explicit `context_deprecate` calls, simply count entries from the
ENTRIES table where `status = Deprecated`, `superseded_by IS NULL`, and `updated_at`
falls in the cycle window. The AUDIT_LOG check adds correctness validation but is not
strictly necessary for accuracy given the write-path analysis.

**Final resolution**: Use the ENTRIES-only query (no AUDIT_LOG join) for the initial
implementation:

```sql
SELECT COUNT(*) FROM entries
WHERE status = 'deprecated'
  AND superseded_by IS NULL
  AND updated_at >= ?1   -- cycle_start_ts
  AND updated_at <= ?2   -- review_ts
```

Rationale: The write-path analysis proves that orphan deprecations can only come from
explicit `context_deprecate`. The AUDIT_LOG join would be redundant and adds JSON parsing
complexity. The ENTRIES `updated_at` field is the reliable proxy because explicit
`context_deprecate` does not subsequently modify the entry's `updated_at`.

This also resolves SR-01 (the silent failure risk): the risk was that the operation
string was inconsistent across paths. Analysis shows it IS consistent (`"context_deprecate"`
everywhere the tool is invoked), but also that the AUDIT_LOG join is not needed because
the ENTRIES data is sufficient.

`deprecations_total` (all deprecations in the cycle window, including those with a
successor) is also computed from ENTRIES using `updated_at`:

```sql
SELECT COUNT(*) FROM entries
WHERE status = 'deprecated'
  AND updated_at >= ?1
  AND updated_at <= ?2
```

Note: this counts chain-deprecations (from `context_correct`) as well. This is
intentional — `deprecations_total` is the full count of deprecated entries in the
window, regardless of whether they have a successor.

### Consequences

- **Easier**: No JSON parsing on AUDIT_LOG `target_ids`. Simpler SQL.
- **Easier**: SR-01 risk is resolved: the ENTRIES-based approach does not depend on
  AUDIT_LOG operation string consistency.
- **Harder**: `updated_at` is used as a deprecation timestamp proxy. If any future path
  modifies a deprecated-and-orphaned entry's `updated_at` after deprecation, the
  attribution window could be incorrect. This is documented as a known assumption.
- **Consequence**: The cycle window bounds (`cycle_start_ts`, `review_ts`) must be
  passed to `compute_curation_snapshot()`. The caller (`context_cycle_review`) already
  has access to `cycle_events` to derive `cycle_start_ts`.
- **Consequence**: Out-of-cycle deprecations (SR-08) — entries deprecated outside an
  active cycle — are correctly excluded from all cycle counts because their `updated_at`
  will not fall within any cycle's window. These are not separately surfaced (documented
  exclusion per ARCHITECTURE.md).
