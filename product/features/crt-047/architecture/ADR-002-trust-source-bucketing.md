## ADR-002: trust_source Bucketing Strategy for Correction Attribution

### Context

Corrections are entries where `supersedes IS NOT NULL`. The `trust_source` field on the
correcting entry records which actor class created it. The SCOPE.md Background Research
section documents observed values: `"agent"`, `"human"`, `"system"`, `"direct"`.

`context_correct` hard-codes `trust_source: "agent"` for all agent-called corrections
(confirmed in `tools.rs` line ~901 and `store_correct.rs`). Human-stored corrections
(via `context_store` with a human caller) would carry `"human"`. The `"system"` value
is used by cortical implant lesson-learned writes. The `"direct"` value is used by
`embed_reconstruct`.

The feature's purpose is to distinguish in-flow agent curation (agents catching and
fixing errors during work) from post-hoc human curation (humans reviewing and correcting
after the fact). `"system"` and `"direct"` writes are automated infrastructure behavior,
not curation decisions.

AC-03 in SCOPE.md specifies:
- `corrections_agent`: entries where `trust_source = "agent"` and `supersedes IS NOT NULL`
- `corrections_human`: entries where `trust_source IN ("human", "privileged")` and
  `supersedes IS NOT NULL`
- `"system"` and `"direct"` excluded from both buckets

An open question in the SCOPE.md (OQ-01, resolved) asked whether to surface excluded
counts. The Constraints section states: "An optional `corrections_system` informational
field may surface the excluded count — ADR-gated by the architect."

### Decision

Add `corrections_system` as an informational field on `CurationSnapshot` and as a stored
column in `cycle_review_index`.

The mapping is:

| trust_source value | Bucket |
|-------------------|--------|
| `"agent"` | `corrections_agent` |
| `"human"` | `corrections_human` |
| `"privileged"` | `corrections_human` (treated as trusted human operator) |
| `"system"` | `corrections_system` (informational only) |
| `"direct"` | `corrections_system` (informational only) |
| any other unknown value | `corrections_system` (fail-safe: unknown = not agent/human) |

`corrections_system` is excluded from `corrections_total`, which equals
`corrections_agent + corrections_human`. This makes `corrections_total` a measure of
intentional curation decisions only, which is the signal the feature exists to measure.

Rationale for including `corrections_system`:
1. Operators debugging unexpected curation counts need visibility into where corrections
   are coming from. Without it, a burst of lesson-learned auto-supersedes would cause
   `corrections_total` to appear unchanged while the corpus is being modified.
2. The field costs one INTEGER column and one `CASE WHEN` clause in SQL. The overhead is
   negligible.
3. Surfacing it as informational-only (excluded from σ baseline) avoids polluting the
   behavioral signal with infrastructure noise.

The `corrections_system` column is added to `cycle_review_index` as part of the v24
migration. Six total new snapshot columns (plus `first_computed_at` per ADR-001): seven
new columns in the v24 migration block.

The SQL to compute `corrections_agent` and `corrections_human`:

```sql
SELECT
    COUNT(*) FILTER (WHERE trust_source = 'agent')      AS corrections_agent,
    COUNT(*) FILTER (WHERE trust_source IN ('human', 'privileged')) AS corrections_human,
    COUNT(*) FILTER (WHERE trust_source NOT IN ('agent', 'human', 'privileged')) AS corrections_system
FROM entries
WHERE feature_cycle = ?1
  AND supersedes IS NOT NULL
```

`corrections_total = corrections_agent + corrections_human` (computed, not stored).

Note: SQLite supports `FILTER (WHERE ...)` on aggregate functions since version 3.30.0
(2019-10-04). The Unimatrix `sqlx` dependency targets a modern SQLite; this syntax is
safe. If compatibility is a concern, use `SUM(CASE WHEN ... THEN 1 ELSE 0 END)` instead.

### Consequences

- **Easier**: Operators see the full correction attribution picture, not just the
  behavioral signal. Debugging unexpected counts is tractable.
- **Easier**: Unknown future `trust_source` values are safely bucketed into
  `corrections_system` rather than silently dropped from all buckets.
- **Harder**: One additional stored column in `cycle_review_index` (manageable: it is
  purely additive).
- **Consequence**: `corrections_total` does not equal the count of all entries with
  `supersedes IS NOT NULL`. This must be documented in the implementation spec to prevent
  implementors from assuming the equality.
