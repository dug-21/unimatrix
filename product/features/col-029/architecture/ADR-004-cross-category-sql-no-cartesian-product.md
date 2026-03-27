## ADR-004: Cross-Category Edge Count SQL — No Cartesian Product

### Context

SR-04 (scope risk, medium) flagged that counting cross-category edges requires two
JOINs to the `entries` table (once for the source endpoint, once for the target
endpoint). Naively written, such a query can produce a cartesian product if the JOIN
conditions are not tightly constrained.

The naive problematic form:
```sql
SELECT COUNT(*)
FROM graph_edges ge, entries src_e, entries tgt_e
WHERE ge.source_id = src_e.id AND ge.target_id = tgt_e.id
  AND src_e.category != tgt_e.category
  AND ge.bootstrap_only = 0
```

While correct when the UNIQUE constraint on `(source_id, target_id, relation_type)`
is respected, this implicit-join style (comma-separated FROM) is error-prone and
the query planner's join order is unpredictable on large tables.

The risk is not a runtime cartesian product (the WHERE predicates prevent it) but:
1. Planner confusion: SQLite may choose a poor join order without explicit LEFT JOIN
2. Double-counting: if an entry somehow matches both `src_e` and `tgt_e` filters, the
   count inflates
3. Clarity: reviewers cannot tell at a glance that the two entries JOINs are
   independent (each keyed to a different column on `ge`)

### Decision

The cross-category edge count is computed within Query 2 using explicit LEFT JOINs
with named aliases and tight equality predicates:

```sql
LEFT JOIN entries src_e
       ON src_e.id = ge.source_id AND src_e.status = 0
LEFT JOIN entries tgt_e
       ON tgt_e.id = ge.target_id AND tgt_e.status = 0
```

The CASE expression for `cross_category_edge_count` explicitly guards on all nullable
paths introduced by LEFT JOIN:

```sql
COALESCE(SUM(
    CASE WHEN ge.id IS NOT NULL
         AND src_e.category IS NOT NULL
         AND tgt_e.category IS NOT NULL
         AND src_e.category != tgt_e.category
    THEN 1 ELSE 0 END
), 0) AS cross_category_edge_count
```

This ensures:
- `ge.id IS NOT NULL` — only rows where the LEFT JOIN matched a real edge
- `src_e.category IS NOT NULL` — src endpoint is active (status=0 join) and not NULL
- `tgt_e.category IS NOT NULL` — tgt endpoint is active and not NULL
- `src_e.category != tgt_e.category` — cross-category check

Edges where one or both endpoints are deprecated/quarantined (status != 0) produce
a NULL `src_e` or `tgt_e` row from the LEFT JOIN. The `IS NOT NULL` guard correctly
excludes them from the cross-category count — consistent with the active-only semantics
(PPR only sees active entries).

Each LEFT JOIN is keyed on an indexed column (`ge.source_id`, `ge.target_id`) so SQLite
uses the existing `idx_graph_edges_source_id` and `idx_graph_edges_target_id` indexes.

### Consequences

Easier:
- Query is unambiguous about which entries table row corresponds to which edge endpoint
- LEFT JOIN semantics make nullable handling explicit
- Index usage is predictable from the ON clause key column

Harder:
- Query 2 is longer than a minimal implementation; the guard conditions must be
  explained in a code comment
- The `ge.id IS NOT NULL` guard is idiomatic for LEFT JOIN presence checks but
  requires the implementer to understand why it is needed (documented in this ADR)
