# Pseudocode: graph_read_inverse.rs (Wave 2)
# New file: crates/unimatrix-server/src/mcp/graph_read_inverse.rs

## Purpose

`handle_inverse` executes a SQL LEFT JOIN antijoin query to find entries of a given
category that have no incoming edges of ALL the specified `missing_edge_types`
(AND semantics per ADR-003). This module has no in-memory graph access — it is
pure SQL, queries the live database, and has no staleness concern.

---

## Imports

```
use rmcp::model::ErrorData;
use sqlx::QueryBuilder;
use unimatrix_core::{EntryRecord, Store};
use unimatrix_engine::graph::RelationType;

use crate::error::ERROR_INVALID_PARAMS;
use super::{GraphParams, InverseResponse};
```

---

## Constants

```
const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;
```

---

## Entry Point

```
pub(super) async fn handle_inverse(
    store: &Store,
    params: &GraphParams,
) -> Result<InverseResponse, ErrorData>
```

### Step 1: Validate category (required)

```
let category: &str = match params.category.as_deref() {
    Some(c) if !c.is_empty() => c,
    _ => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        "inverse mode requires category",
        None,
    )),
};
```

### Step 2: Validate missing_edge_types (required, non-empty)

```
let raw_types: &[String] = match &params.missing_edge_types {
    Some(v) if !v.is_empty() => v.as_slice(),
    _ => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        "inverse mode requires at least one edge type in missing_edge_types",
        None,
    )),
};
```

Parse each element via `RelationType::from_str`. Collect into `Vec<RelationType>`.
On any unrecognized value, return a validation error naming the unrecognized string
and listing all 16 recognized types (AC-02):

```
let edge_types: Vec<RelationType> = parse_relation_types(raw_types)?;
// parse_relation_types: see Helper Functions section below.
```

### Step 3: Validate limit (default 100, range [1, 500])

```
let limit: u32 = match params.limit {
    None => DEFAULT_LIMIT,
    Some(n) if n >= 1 && n <= MAX_LIMIT => n,
    Some(n) => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        format!("limit must be in range 1..=500, got {n}"),
        None,
    )),
};
```

### Step 4: Build parameterized antijoin SQL

Use `sqlx::QueryBuilder` with SQLite dialect. Do NOT interpolate any caller values
as string fragments — all values are bound via `push_bind` (pattern #4058).

SQL structure for N types (one LEFT JOIN per type):

```sql
SELECT e.id, e.title, e.topic, e.category, e.content, e.confidence,
       e.status, e.tags, e.created_at, e.updated_at, e.supersedes,
       e.superseded_by, e.agent_id, e.feature_cycle, e.helpful_count,
       e.unhelpful_count
FROM entries e
LEFT JOIN graph_edges g0
    ON e.id = g0.target_id AND g0.relation_type = <type_0>
LEFT JOIN graph_edges g1
    ON e.id = g1.target_id AND g1.relation_type = <type_1>
...
WHERE e.category = <category>
  AND e.status = 0
  AND g0.target_id IS NULL
  AND g1.target_id IS NULL
  ...
LIMIT <limit>
```

Pseudocode for construction:

```
let mut qb = QueryBuilder::new(
    "SELECT e.id, e.title, e.topic, e.category, e.content, e.confidence, \
     e.status, e.tags, e.created_at, e.updated_at, e.supersedes, \
     e.superseded_by, e.agent_id, e.feature_cycle, e.helpful_count, \
     e.unhelpful_count \
     FROM entries e"
);

// Add one LEFT JOIN per missing edge type.
// Alias names g0, g1, ... are loop-counter-derived — NEVER from caller input (SR-B).
for (i, rel_type) in edge_types.iter().enumerate() {
    qb.push(format!(" LEFT JOIN graph_edges g{i} ON e.id = g{i}.target_id AND g{i}.relation_type = "));
    qb.push_bind(rel_type.as_str());  // bind the relation type string safely
}

// WHERE clause.
qb.push(" WHERE e.category = ");
qb.push_bind(category);
qb.push(" AND e.status = 0");

// NULL checks for each join alias.
for i in 0..edge_types.len() {
    qb.push(format!(" AND g{i}.target_id IS NULL"));
}

// LIMIT.
qb.push(" LIMIT ");
qb.push_bind(limit as i64);
```

CRITICAL: The alias names (`g0`, `g1`, ...) are ONLY ever generated from the loop
counter `i` — they are structural SQL identifiers, not parameterizable, and no
caller-supplied value ever appears in their construction.

CRITICAL: `AND e.status = 0` is a required WHERE clause that filters out deprecated
entries (R-10). It MUST be present regardless of the number of LEFT JOINs.

### Step 5: Execute query

```
let pool = store.read_pool_server();
let entries: Vec<EntryRecord> = qb
    .build_query_as::<EntryRecord>()
    .fetch_all(pool)
    .await
    .map_err(|e| ErrorData::new(
        ERROR_INTERNAL,  // from crate::error
        format!("inverse mode SQL error: {e}"),
        None,
    ))?;
```

Note: `store.read_pool_server()` returns a `&sqlx::Pool<Sqlite>`. The `build_query_as`
approach requires `EntryRecord` to implement `sqlx::FromRow`. This is already the case
(used by `graph_read_subgraph.rs`). Import `ERROR_INTERNAL` from `crate::error`.

### Step 6: Return response

```
let total_returned = entries.len();
Ok(InverseResponse { entries, total_returned })
```

---

## Helper Functions

### parse_relation_types (module-level private)

Used by `handle_inverse`. Produces exact same error format for consistency.
This function is also independently needed by `graph_read_filter.rs` and
`graph_read_path.rs` — however, since sibling modules cannot call each other directly,
each module defines its own copy OR the function is elevated to `pub(super)` in one
of the modules and imported by the others. The implementation agent should:

Option A (simplest): define it as a private function in each module independently
(3 copies, ~10 lines each, no cross-module coupling needed).

Option B: define it as `pub(super)` in `graph_read_inverse.rs` and import from the
other two via `use super::graph_read_inverse::parse_relation_types`.

Either is acceptable; Option A avoids coupling. The tester agent's tests can verify
each module's validation independently.

```
fn parse_relation_types(raw: &[String]) -> Result<Vec<RelationType>, ErrorData> {
    let mut out = Vec::with_capacity(raw.len());
    for t in raw {
        match RelationType::from_str(t) {
            Some(rt) => out.push(rt),
            None => return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!(
                    "unrecognized edge type '{}' — recognized types: \
                     About, Advances, Asserts, Cites, CoAccess, \
                     Contradicts, DerivedFrom, Informs, Mentions, \
                     Motivates, Prerequisite, Refutes, RelatedTo, \
                     Supersedes, Supports, Tests",
                    t
                ),
                None,
            )),
        }
    }
    Ok(out)
}
```

---

## State Machines / Lifecycle

No state machine. This is a stateless request-response handler.

---

## Error Handling

| Error Condition | Error Type | Message |
|-----------------|-----------|---------|
| `category` absent or empty | `ERROR_INVALID_PARAMS` | "inverse mode requires category" |
| `missing_edge_types` absent or empty | `ERROR_INVALID_PARAMS` | "inverse mode requires at least one edge type in missing_edge_types" |
| Unrecognized element in `missing_edge_types` | `ERROR_INVALID_PARAMS` | "unrecognized edge type '{x}' — recognized types: ..." (lists all 16) |
| `limit` out of range [1, 500] | `ERROR_INVALID_PARAMS` | "limit must be in range 1..=500, got {n}" |
| SQL execution error | `ERROR_INTERNAL` | "inverse mode SQL error: {e}" |

All errors propagate to `handle_graph` via `?` and are returned as `ErrorData` to the caller.

---

## Key Test Scenarios

- AC-01: Write entries with and without incoming Cites edges; assert only entries without
  incoming Cites are returned.
- AC-02: `missing_edge_types=["NotAType"]` → validation error names "NotAType" and lists
  all 16 types.
- AC-03: `missing_edge_types=None` and `missing_edge_types=[]` → exact error text
  "inverse mode requires at least one edge type in missing_edge_types".
- AC-03a: `edge_types=["Cites"]` with `mode="inverse"` → validation error caught by
  `validate_no_unsupported_params` (tests this at the graph_read.rs level).
- AC-04: `category` absent → exact error "inverse mode requires category".
- AC-05: `limit` default is 100; `limit=0` and `limit=501` produce validation errors.
- AC-06: `total_returned == entries.len()` in every response.
- AC-27 (infra-001): Integration test — write entries with/without incoming edges,
  assert only active, no-edge entries returned.
- AC-28 (infra-001): 4-state fixture (missing both / missing first only / missing second
  only / has both); assert AND semantics — only entries missing ALL types are returned.
- R-10: Write one active and one deprecated entry, both without incoming Cites edges;
  assert only the active entry is returned.
- IR-01: N=3 missing_edge_types — SQL is valid and executes without parameter offset error.
- SR-B: `missing_edge_types=["Cites'; DROP TABLE entries; --"]` → validation error from
  `parse_relation_types`, no SQL executed.
