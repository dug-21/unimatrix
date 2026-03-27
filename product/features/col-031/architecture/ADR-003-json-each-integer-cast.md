## ADR-003: `json_each` Integer Cast Form — `CAST(json_each.value AS INTEGER)`

### Context

`query_log.result_entry_ids` is stored as a JSON array of integers produced by
`serde_json::to_string(entry_ids)` where `entry_ids: &[u64]`. A concrete example
stored value is `[42,107,308]` — no quotes around the integers.

The SQL aggregation must expand this array to individual `entry_id` values for the
`JOIN entries ON entries.id = <expanded value>` clause.

SQLite's `json_each` table-valued function expands a JSON array into rows. The
`json_each.value` column returns values typed according to the JSON element type:
- If the JSON element is a JSON number (unquoted), `json_each.value` returns a
  numeric affinity value.
- If the JSON element is a JSON string (quoted), `json_each.value` returns a text
  affinity value.

Since `serde_json` serializes `u64` as unquoted JSON numbers, `json_each.value`
will carry numeric affinity. However, `entries.id` is `INTEGER` (SQLite integer
affinity). An explicit `CAST(json_each.value AS INTEGER)` is required to:
1. Make the join condition unambiguous across SQLite versions.
2. Prevent implicit affinity coercion surprises on edge cases (empty array,
   very large u64 values near the i64 boundary).
3. Match the existing `knowledge_reuse.rs` pattern in the codebase (the precedent
   for `json_each` usage, confirmed prior to col-031).

SR-01 (SCOPE-RISK-ASSESSMENT.md) flags this as the highest implementation-surprise
risk. The choice to pin to `CAST` is a defensive measure.

### Decision

Use the following SQL form for `json_each` expansion in `query_phase_freq_table`:

```sql
SELECT
    ql.phase,
    e.category,
    CAST(je.value AS INTEGER) AS entry_id,
    COUNT(*) AS freq
FROM query_log ql
CROSS JOIN json_each(ql.result_entry_ids) je
JOIN entries e ON e.id = CAST(je.value AS INTEGER)
WHERE ql.phase IS NOT NULL
  AND ql.result_entry_ids IS NOT NULL
  AND (
      ql.feature_cycle IS NULL
      OR ql.feature_cycle IN (
          SELECT DISTINCT feature_cycle
          FROM query_log
          ORDER BY query_id DESC
          LIMIT ?1
      )
  )
GROUP BY ql.phase, e.category, e.id
ORDER BY ql.phase, e.category, freq DESC
```

Where `?1` is `retention_cycles`.

The `CAST(je.value AS INTEGER)` appears twice (in the `SELECT` projection and in
the `JOIN` condition) — both must use the cast to guarantee type consistency.

The AC-08 integration test MUST verify this form against a real `query_log` row
with a known `result_entry_ids` JSON array before the feature is gated. If the
test fails due to `json_each` expansion producing no rows, the cast form must be
revisited.

### Consequences

**Easier:**
- Explicit cast prevents affinity mismatch surprises across SQLite versions.
- The form is consistent with `knowledge_reuse.rs` precedent in the codebase.
- AC-08 integration test directly validates the SQL against live data.

**Harder:**
- The `CAST` is redundant when SQLite already returns numeric affinity, adding
  minor syntactic verbosity.
- The retention filter uses a `LIMIT` subquery rather than a date-range filter;
  this counts distinct `feature_cycle` values by recency of rows, which is
  correct for Unimatrix's cycle-oriented data model but requires a comment to
  explain to future maintainers.
- Very large u64 entry IDs near the i64 boundary (> 2^63 - 1) will silently
  overflow on cast. This is not a practical concern: entry IDs are
  auto-increment integers starting from 1.
