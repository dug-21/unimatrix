# Pseudocode: graph_read_filter.rs (Wave 2)
# New file: crates/unimatrix-server/src/mcp/graph_read_filter.rs

## Purpose

`handle_filter` executes a parameterized correlated subquery to find entries matching
a category and optional property + edge-count constraints. All SQL is constructed from
typed `GraphParams` fields bound as sqlx parameters — no raw SQL from callers (ADR-007,
C9). This module has no in-memory graph access — it is pure SQL, queries the live
database, and has no staleness concern.

---

## Imports

```
use rmcp::model::ErrorData;
use sqlx::QueryBuilder;
use unimatrix_core::{EntryRecord, Store};
use unimatrix_engine::graph::RelationType;

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};
use super::{GraphParams, FilterResponse};
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
pub(super) async fn handle_filter(
    store: &Store,
    params: &GraphParams,
) -> Result<FilterResponse, ErrorData>
```

### Step 1: Validate category (required)

```
let category: &str = match params.category.as_deref() {
    Some(c) if !c.is_empty() => c,
    _ => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        "filter mode requires category",
        None,
    )),
};
```

### Step 2: Validate edge_types (required when edge-count constraints are present)

```
let has_edge_count = params.min_edge_count.is_some() || params.max_edge_count.is_some();

let edge_types: Option<Vec<RelationType>> = match &params.edge_types {
    None | Some(v) if v.is_empty() => {
        if has_edge_count {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "filter mode requires edge_types when edge_count constraints are specified",
                None,
            ));
        }
        None  // no edge type filter
    }
    Some(types) => {
        // parse_relation_types validates each type via RelationType::from_str
        Some(parse_relation_types(types)?)
    }
};
```

Note: When `has_edge_count=false`, `edge_types` being absent is fine — the filter
is a pure property filter (FR-07, R-11 category-only is valid). When `has_edge_count=true`,
edge_types MUST be non-empty (FR-06, AC-09).

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

### Step 4: Build parameterized correlated subquery SQL

Use `sqlx::QueryBuilder`. All caller values bound via `push_bind`. No string
interpolation of caller input (ADR-007, NFR-06, SR-A).

#### 4a. Base query

```
let mut qb = QueryBuilder::new(
    "SELECT e.id, e.title, e.topic, e.category, e.content, e.confidence, \
     e.status, e.tags, e.created_at, e.updated_at, e.supersedes, \
     e.superseded_by, e.agent_id, e.feature_cycle, e.helpful_count, \
     e.unhelpful_count \
     FROM entries e \
     WHERE e.category = "
);
qb.push_bind(category);
qb.push(" AND e.status = 0");
```

#### 4b. Optional: min_age_days

`created_at` is `INTEGER NOT NULL` (Unix epoch seconds). Use integer epoch arithmetic,
NOT `datetime()` text comparison (FR-07).

```
if let Some(days) = params.min_age_days {
    // entries where created_at is at least `days` days old.
    // strftime('%s','now') returns the current Unix epoch as text, cast to INTEGER.
    // days * 86400 = seconds in N days (u32 * 86400 fits in i64).
    qb.push(" AND e.created_at < (CAST(strftime('%s','now') AS INTEGER) - ");
    qb.push_bind(days as i64 * 86400_i64);
    qb.push(")");
}
```

CRITICAL: Use integer epoch arithmetic (`CAST(strftime('%s','now') AS INTEGER) - N`),
not `datetime()` text comparison. `created_at` is stored as an integer epoch.

#### 4c. Optional: min_confidence

```
if let Some(min_c) = params.min_confidence {
    qb.push(" AND e.confidence >= ");
    qb.push_bind(min_c);
}
```

#### 4d. Optional: max_confidence

```
if let Some(max_c) = params.max_confidence {
    qb.push(" AND e.confidence <= ");
    qb.push_bind(max_c);
}
```

#### 4e. Optional: min_edge_count (>= N outgoing edges of edge_types)

When `min_edge_count` is present, `edge_types` is guaranteed non-empty (Step 2 guard).
Add a correlated subquery for the lower bound.

```
if let Some(min_n) = params.min_edge_count {
    // edge_types is guaranteed Some and non-empty here.
    let et = edge_types.as_ref().unwrap();
    qb.push(" AND (SELECT COUNT(*) FROM graph_edges g WHERE g.source_id = e.id AND g.relation_type IN (");
    push_relation_type_list(&mut qb, et);  // bind each type — see Helper Functions
    qb.push(")) >= ");
    qb.push_bind(min_n as i64);
}
```

#### 4f. Optional: max_edge_count (<= N outgoing edges of edge_types)

When `max_edge_count` is present, `edge_types` is guaranteed non-empty (Step 2 guard).
Add a SEPARATE correlated subquery for the upper bound (R-08 — two independent
subqueries, NOT a single BETWEEN).

CRITICAL: max_edge_count=0 is valid and must work correctly (R-02). The `<= ?` binding
with value 0 returns entries where COUNT(*) = 0. Do NOT special-case 0.

```
if let Some(max_n) = params.max_edge_count {
    // edge_types is guaranteed Some and non-empty here.
    let et = edge_types.as_ref().unwrap();
    qb.push(" AND (SELECT COUNT(*) FROM graph_edges g WHERE g.source_id = e.id AND g.relation_type IN (");
    push_relation_type_list(&mut qb, et);  // bind each type — see Helper Functions
    qb.push(")) <= ");
    qb.push_bind(max_n as i64);
}
```

Note: When both `min_edge_count` and `max_edge_count` are present, this produces TWO
separate `(SELECT COUNT(*) ...)` subquery clauses (R-08). This is intentional.

#### 4g. LIMIT

```
qb.push(" LIMIT ");
qb.push_bind(limit as i64);
```

### Step 5: Execute query

```
let pool = store.read_pool_server();
let entries: Vec<EntryRecord> = qb
    .build_query_as::<EntryRecord>()
    .fetch_all(pool)
    .await
    .map_err(|e| ErrorData::new(
        ERROR_INTERNAL,
        format!("filter mode SQL error: {e}"),
        None,
    ))?;
```

### Step 6: Return response

```
let total_returned = entries.len();
Ok(FilterResponse { entries, total_returned })
```

---

## Helper Functions

### parse_relation_types (module-level private)

Same implementation as documented in `graph_read_inverse.md`. Validates each element
via `RelationType::from_str`, returns `Err(ErrorData)` on unrecognized value listing
all 16 types. Returns `Ok(Vec<RelationType>)` on success.

### push_relation_type_list (module-level private)

Appends a parameterized IN-clause argument list to a `QueryBuilder`. The list is
bound element-by-element using `push_bind`, with commas added as literals.
No string interpolation of type values.

```
fn push_relation_type_list(qb: &mut QueryBuilder<sqlx::Sqlite>, types: &[RelationType]) {
    let mut sep = qb.separated(", ");
    for rt in types {
        sep.push_bind(rt.as_str());
    }
}
```

Alternative (if `QueryBuilder::separated` is not available in sqlx 0.8):

```
fn push_relation_type_list(qb: &mut QueryBuilder<sqlx::Sqlite>, types: &[RelationType]) {
    for (i, rt) in types.iter().enumerate() {
        if i > 0 {
            qb.push(", ");
        }
        qb.push_bind(rt.as_str());
    }
}
```

The implementation agent should use whichever form the project already uses for
multi-element IN-clause binding (see pattern #4058 entry #3442).

---

## State Machines / Lifecycle

No state machine. This is a stateless request-response handler.

---

## Error Handling

| Error Condition | Error Type | Message |
|-----------------|-----------|---------|
| `category` absent or empty | `ERROR_INVALID_PARAMS` | "filter mode requires category" |
| `min_edge_count` or `max_edge_count` present but `edge_types` absent/empty | `ERROR_INVALID_PARAMS` | "filter mode requires edge_types when edge_count constraints are specified" |
| Unrecognized element in `edge_types` | `ERROR_INVALID_PARAMS` | "unrecognized edge type '{x}' — recognized types: ..." |
| `limit` out of range [1, 500] | `ERROR_INVALID_PARAMS` | "limit must be in range 1..=500, got {n}" |
| SQL execution error | `ERROR_INTERNAL` | "filter mode SQL error: {e}" |

---

## Key Test Scenarios

- AC-07 (infra-001): category="goal", min_age_days=30, max_edge_count=0, edge_types=["Advances"]
  → returns only old goals with zero outgoing Advances edges.
- AC-08 (infra-001): category="decision", min_edge_count=2, edge_types=["Advances"] →
  returns only entries with 2+ outgoing Advances edges.
- AC-09: min_edge_count=1 with edge_types=None → exact error text
  "filter mode requires edge_types when edge_count constraints are specified".
- AC-10: category absent → exact error "filter mode requires category".
- AC-11: limit default 100; limit=0 and limit=501 → validation errors.
- AC-12: total_returned == entries.len() in every response.
- AC-29 (infra-001): max_edge_count=0 boundary — entries with 0, 1, 2, 3 outgoing
  Advances edges; assert only the 0-edge entry is returned (critical R-02 test).
- AC-30 (infra-001): min_edge_count=2 with entries having 0/1/2/3 outgoing edges;
  assert only 2 and 3 are returned.
- R-02: SQL constructed for max_edge_count=0 uses `<= ?` binding with value 0 —
  no special-casing.
- R-08: Both min_edge_count and max_edge_count present → SQL contains TWO separate
  `(SELECT COUNT(*) ...)` clauses; verify with combined-bounds integration test
  (entries 0,1,2,3,4 edges; filter min=2,max=3 → only 2 and 3 returned).
- R-10 (filter): deprecated entries excluded — write deprecated entry, assert absent.
- R-11: category-only query (no other params) → valid, returns all active entries in
  category up to limit=100 (no validation error).
- SR-A: extreme typed values (min_age_days=u32::MAX, min_confidence=f64::INFINITY) →
  no panic, SQL executes cleanly.
- IR-04: edge_types=["Advances","Supports"] with multi-type IN clause → verify correct
  COUNT(*) semantics for entries with mixed outgoing edge types.
