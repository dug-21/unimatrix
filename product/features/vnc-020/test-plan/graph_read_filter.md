# Test Plan: graph_read_filter.rs — Combined Property + Edge-Count Filter Handler

Component: `crates/unimatrix-server/src/mcp/graph_read_filter.rs`
Responsibility: `handle_filter` — validate filter-mode params, build parameterized
correlated subquery SQL, execute via `store.read_pool_server()`, return `FilterResponse`.

---

## Unit Test Expectations

### AC-10 — category Required

**Test**: `test_handle_filter_missing_category_returns_error`
**Arrange**: `GraphParams { mode: "filter", category: None, ... }`.
**Act**: `handle_filter(store, &params).await`.
**Assert**: `Err(ErrorData)` with message `"filter mode requires category"` (exact text).

**Risk**: R-04

---

### AC-09 — edge_types Required When Edge-Count Constraints Present

Three tests:

**Test**: `test_handle_filter_min_edge_count_without_edge_types_returns_error`
**Arrange**: `min_edge_count: Some(1), edge_types: None`.
**Assert**: `Err(ErrorData)` with message `"filter mode requires edge_types when edge_count constraints are specified"` (exact text).

**Test**: `test_handle_filter_min_edge_count_with_empty_edge_types_returns_error`
**Arrange**: `min_edge_count: Some(1), edge_types: Some(vec![])`.
**Assert**: Same exact error text.

**Test**: `test_handle_filter_max_edge_count_without_edge_types_returns_error`
**Arrange**: `max_edge_count: Some(0), edge_types: None`.
**Assert**: Same exact error text.

**Risk**: R-04, R-02

---

### AC-11 — limit Boundary Validation

Three tests:

**Test**: `test_handle_filter_limit_zero_returns_error`
**Arrange**: `limit: Some(0)`.
**Assert**: `Err(ErrorData)` with range statement [1, 500].

**Test**: `test_handle_filter_limit_501_returns_error`
**Arrange**: `limit: Some(501)`.
**Assert**: Same error with range.

**Test**: `test_handle_filter_limit_default_is_100`
**Arrange**: `limit: None`.
**Assert**: SQL contains `LIMIT 100` or result count <= 100 when 200 matching entries exist.

**Risk**: R-13

---

### R-11 — Category-Only Query (No Other Filters) Is Valid

**Test**: `test_handle_filter_category_only_no_validation_error`
**Arrange**: `GraphParams { mode: "filter", category: Some("goal".to_string()), all others None }`.
**Act**: `handle_filter` against a store with 3 goal entries.
**Assert**: Returns `Ok(FilterResponse)` with `entries.len() == 3` and `total_returned == 3`.
No validation error; SQL executes cleanly.

**Risk**: R-11

---

### R-02 — max_edge_count=0 SQL Uses <= Not Special-Cased

**Test**: `test_filter_max_edge_count_zero_uses_lte_binding`
**Arrange**: Build params with `max_edge_count: Some(0), edge_types: Some(vec!["Advances".to_string()])`.
**Act**: Inspect the SQL builder output (via unit-level SQL string helper or behavioral
verification against DB).
**Assert**: The generated SQL fragment for the `max_edge_count` subquery uses `<= ?` with
value `0`, NOT `= 0` or `IS NULL`. This verifies no special-casing of the zero boundary.
Behavioral verification: insert entries with 0 and 1 Advances edges; assert only the
0-edge entry is returned.

**Risk**: R-02 (Critical — must not be special-cased)

---

### R-08 — Both Edge-Count Bounds Generate Two Independent Subqueries

**Test**: `test_filter_both_edge_count_bounds_two_subqueries_in_sql`
**Arrange**: `min_edge_count: Some(2), max_edge_count: Some(3), edge_types: Some(vec!["Advances".to_string()])`.
**Act**: Inspect SQL builder output.
**Assert**: The SQL contains TWO separate `(SELECT COUNT(*) FROM graph_edges g WHERE ...)` 
expressions — one with `>= ?` and one with `<= ?`. NOT a single subquery with a `BETWEEN`
clause or a combined AND condition inside one SELECT COUNT block.

**Test**: `test_filter_min_edge_count_only_one_gte_subquery`
**Arrange**: `min_edge_count: Some(2), max_edge_count: None`.
**Assert**: SQL contains exactly one correlated subquery with `>= ?`. No `<= ?` clause.

**Test**: `test_filter_max_edge_count_only_one_lte_subquery`
**Arrange**: `max_edge_count: Some(0), min_edge_count: None`.
**Assert**: SQL contains exactly one correlated subquery with `<= ?`. No `>= ?` clause.

**Risk**: R-08

---

### IR-04 — edge_types Multi-Value IN Clause Uses push_bind

**Test**: `test_filter_multi_type_edge_types_push_bind_pattern`
**Arrange**: `edge_types: Some(vec!["Advances".to_string(), "Supports".to_string()])`.
**Act**: Inspect SQL parameter binding (or behavioral test: entries with only "RelatedTo"
outgoing edges are treated as having 0 edges of the specified types and thus appear in
`max_edge_count=0` results).
**Assert**: No SQL string interpolation of edge type names; each type contributes one bound
parameter in the IN clause. Behavioral verification: entries with edges of types NOT in
edge_types are counted as 0 matching edges.

**Risk**: IR-04 (push_bind pattern #4058)

---

### AC-12 — total_returned Matches entries.len()

**Test**: `test_handle_filter_total_returned_matches_len`
**Arrange**: 5 goal entries. Call `filter(category="goal")`.
**Assert**: `response.total_returned == 5` and `response.entries.len() == 5`.

---

## Integration Test Expectations (AC-29, AC-30)

### AC-29 — test_context_graph_filter_max_edge_count_zero

**Location**: `test_tools.py`.
**Fixture**: `server` (fresh DB).

**Critical boundary**: The `= 0` case must be validated as a distinct scenario from `>= 1`.

**4-entry fixture**:
| Entry | Outgoing Advances edges | Expected in result (max_edge_count=0)? |
|-------|------------------------|---------------------------------------|
| entry_0 | 0 | YES |
| entry_1 | 1 | NO |
| entry_2 | 2 | NO |
| entry_3 | 3 | NO |

**Fixture setup**: Store 4 goal entries. For entry_1, add 1 outgoing Advances edge via
`context_edge`. For entry_2, add 2. For entry_3, add 3. entry_0 has no edges.

**Action**:
```python
resp = server.context_graph(
    "filter",
    category="goal",
    max_edge_count=0,
    edge_types=["Advances"],
    agent_id="human",
    format="json",
)
```

**Assertions**:
- `entry_0_id` IS in results.
- `entry_1_id`, `entry_2_id`, `entry_3_id` are NOT in results.
- `data["total_returned"] == 1`.

**Also test** `max_edge_count=1` on the same conceptual data (separate test function
`test_context_graph_filter_max_edge_count_one`) — assert entries_0 and entry_1 are both
returned; entry_2 and entry_3 are not. This validates the general `<= N` path.

**Risks mitigated**: R-02 (Critical, AC-29), IR-04

---

### AC-30 — test_context_graph_filter_min_edge_count_gte2

**Location**: `test_tools.py`.
**Fixture**: `server` (fresh DB).

**4-entry fixture**: entries with 0, 1, 2, 3 outgoing Advances edges.

**Action**:
```python
resp = server.context_graph(
    "filter",
    category="decision",
    min_edge_count=2,
    edge_types=["Advances"],
    agent_id="human",
    format="json",
)
```

**Assertions**:
- Entry with 2 edges IS in results.
- Entry with 3 edges IS in results.
- Entry with 0 edges is NOT in results.
- Entry with 1 edge is NOT in results.
- `data["total_returned"] == 2`.

**Risks mitigated**: R-08, AC-30

---

### Combined Filter Integration Test (Q10 Pattern — AC-07)

**Test**: `test_context_graph_filter_combined_age_and_max_edge_count` (covers AC-07)
**Arrange**: Store goal entries at varying ages with varying Advances edge counts.
Two entries older than 30 days: one with 0 edges (should appear), one with 1 edge (should not).
Two entries newer than 30 days: one with 0 edges (should not appear due to age), one with 1 edge.
**Action**: `filter(category="goal", min_age_days=30, max_edge_count=0, edge_types=["Advances"])`.
**Assert**: Only the old entry with 0 edges is returned.
This is the Q10 stale Goal detection use case (W2 in the spec).

Note: Because `created_at` is set at insert time, "older than 30 days" requires inserting
entries with a manipulated timestamp. Use `sqlx` direct insert in the test helper with an
explicit `created_at` value (Unix epoch seconds, 31+ days in the past). Document this
requirement in the test; do not rely on sleep.

**Risks mitigated**: R-08 (combined bounds correctness)

---

## Edge Cases

- `min_confidence > max_confidence`: not a validation error per spec; returns empty set
  silently. Test: `test_filter_inverted_confidence_bounds_returns_empty` — assert `entries: []`,
  `total_returned: 0`, no error.
- `min_age_days=0`: logically "any age". Test: `test_filter_min_age_days_zero_returns_all_active`
  — all active category entries returned. Must not return zero results.
- Unrecognized `edge_types` value in filter mode: `test_filter_unrecognized_edge_type_returns_error`
  — assert `Err(ErrorData)` naming the unrecognized type and listing all 16 types (same
  pattern as inverse, per RelationType::from_str validation).
