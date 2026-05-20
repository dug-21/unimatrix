# Agent Report: vnc-020-agent-5-filter

**Component**: `graph_read_filter.rs`
**Feature**: vnc-020 — context_graph filter mode
**Agent ID**: vnc-020-agent-5-filter

---

## Work Completed

### Files Modified / Created

- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/graph_read_filter.rs` — Full `handle_filter` implementation replacing the Wave 1 stub (246 lines)
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/graph_read_filter_tests.rs` — 20 unit tests extracted to separate file per 500-line rule (732 lines)
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/graph_read_inverse.rs` — Fixed broken `build_query_as::<EntryRecord>()` call → `build().fetch_all()` + `entry_from_row` hydration; corrected column list
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/graph_read_path.rs` — Removed dangling orphaned test content (brace mismatch from Wave 2 path agent) that was blocking compilation

### Implementation Summary

`handle_filter` implements the full correlated subquery pipeline:

1. **Parameter validation** — category required (AC-10); edge_types required when min/max_edge_count present, including empty-vec case (AC-09); limit range [1,500] default 100 (AC-11)
2. **SQL construction** via `sqlx::QueryBuilder<sqlx::Sqlite>` — base WHERE + optional min_age_days (integer epoch arithmetic per SPECIFICATION.md FR-08), min_confidence, max_confidence, two independent correlated subqueries for min_edge_count (>= N) and max_edge_count (<= N), LIMIT
3. **max_edge_count=0** uses `<= ?` binding unconditionally — never special-cased (R-02 Critical)
4. **Two separate subqueries** for both min and max bounds (R-08 — not BETWEEN)
5. **IN clause binding** via `push_bind` per element (pattern #4520, IR-04)
6. **Hydration** via `entry_from_row` + `load_tags_for_entries` + `apply_tags` (ADR-006 C-10)

### Tests: 20/20 pass

| Test | AC/Risk |
|------|---------|
| `test_handle_filter_missing_category_returns_error` | AC-10 |
| `test_handle_filter_empty_category_returns_error` | AC-10 |
| `test_handle_filter_min_edge_count_without_edge_types_returns_error` | AC-09 |
| `test_handle_filter_min_edge_count_with_empty_edge_types_returns_error` | AC-09 |
| `test_handle_filter_max_edge_count_without_edge_types_returns_error` | AC-09, R-02 |
| `test_handle_filter_limit_zero_returns_error` | AC-11 |
| `test_handle_filter_limit_501_returns_error` | AC-11 |
| `test_handle_filter_limit_default_is_100` | AC-11 |
| `test_handle_filter_category_only_no_validation_error` | R-11 |
| `test_handle_filter_total_returned_matches_len` | AC-12 |
| `test_filter_max_edge_count_zero_uses_lte_binding` | R-02 Critical, AC-29 |
| `test_filter_max_edge_count_one_returns_zero_and_one_edge_entries` | AC-29 |
| `test_filter_min_edge_count_gte2_returns_two_and_three_edge_entries` | AC-30 |
| `test_filter_both_edge_count_bounds_two_subqueries_in_sql` | R-08 |
| `test_filter_min_edge_count_only_one_gte_subquery` | R-08 |
| `test_filter_max_edge_count_only_one_lte_subquery` | R-08 |
| `test_filter_multi_type_edge_types_push_bind_pattern` | IR-04 |
| `test_filter_unrecognized_edge_type_returns_error` | SR-B |
| `test_filter_inverted_confidence_bounds_returns_empty` | edge case |
| `test_filter_deprecated_entries_excluded` | R-10 |

### Full workspace: 0 failures

---

## Issues Encountered

### 1. `build_query_as::<EntryRecord>()` — compile error (blocked inverse + filter)

The Wave 2 inverse agent used `qb.build_query_as::<EntryRecord>()` in `graph_read_inverse.rs`. `EntryRecord` does not implement `sqlx::FromRow` — it uses a custom `entry_from_row()` deserializer. This caused 3 compile errors blocking the entire crate. Fixed by switching to `.build().fetch_all(pool)` + manual `entry_from_row` hydration in both `graph_read_inverse.rs` and my `graph_read_filter.rs`. Also corrected the inverse column list (had `e.tags`, `e.agent_id` which don't exist in the schema).

### 2. `graph_read_path.rs` dangling brace (blocking compilation)

The Wave 2 path agent used the `#[path]` split pattern but left all the inline test content in `graph_read_path.rs` after the `mod tests;` declaration. This produced an "unexpected closing delimiter `}`" at line 1092. Resolved by truncating `graph_read_path.rs` to the 322 lines of valid implementation content (the test content was already correctly present in `graph_read_path_tests.rs`).

### 3. `NewEntry` API mismatches in tests

Initial test helpers used non-existent `NewEntry` fields (`agent_id`) and methods (`.store()`, `.upsert()`, `.add_edge()`). Correct API: `SqlxStore::insert(NewEntry{...})` with fields `{title, content, topic, category, tags, source, status, created_by, feature_cycle, trust_source}`. Direct edge inserts require raw SQL via `write_pool_test()`.

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — surfaced ADR-007 (no raw SQL), push_bind pattern #4058, existing antijoin hydration pattern #4519. Applied all.
- **Stored**: entry #4520 via supersession of #4519 — adds explicit `build_query_as` compile-error trap, generalizes to all QueryBuilder entry handlers, adds full hydration code example.
