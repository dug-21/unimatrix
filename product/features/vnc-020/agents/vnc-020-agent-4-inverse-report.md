# Agent Report: vnc-020-agent-4-inverse

Component: `graph_read_inverse.rs`
Feature: vnc-020 — context_graph inverse/filter/path modes
Agent ID: vnc-020-agent-4-inverse

## Work Completed

### Files Created / Modified

- `crates/unimatrix-server/src/mcp/graph_read_inverse.rs` — full `handle_inverse` implementation (was stub); 199 lines
- `crates/unimatrix-server/src/mcp/graph_read_inverse_tests.rs` — unit/validation tests; 349 lines
- `crates/unimatrix-server/src/mcp/graph_read_inverse_integration_tests.rs` — integration tests (AC-27, AC-28, limit-default, edge cases); 247 lines
- `crates/unimatrix-server/src/mcp/graph_read_filter.rs` — fixed broken test helpers (`store.insert`, `write_pool_test`, removed `futures` dependency) to unblock test compilation

### Implementation Summary

`handle_inverse` performs a parameterized N-LEFT-JOIN antijoin SQL query:
- Validates category (required, non-empty)
- Validates `missing_edge_types` (required, non-empty), each parsed via `RelationType::from_str` (AC-02, SR-B injection prevention)
- Validates limit (default 100, range [1,500])
- Builds SQL via `sqlx::QueryBuilder`: one LEFT JOIN aliased g0, g1, ... per edge type (alias from loop counter only), WHERE category + status=0 + gN.target_id IS NULL for each join
- Uses `unimatrix_store::read::{entry_from_row, load_tags_for_entries, apply_tags}` for row hydration
- Returns `InverseResponse { entries, total_returned }`

`parse_relation_types` is exported `pub(super)` so sibling modules can import it.

## Tests

20 tests, all passing:

| Test | AC | Result |
|------|----|--------|
| test_handle_inverse_missing_category_returns_error | AC-04 | ok |
| test_handle_inverse_empty_category_returns_error | AC-04 | ok |
| test_handle_inverse_missing_edge_types_none_returns_error | AC-03 | ok |
| test_handle_inverse_missing_edge_types_empty_returns_error | AC-03 | ok |
| test_handle_inverse_unrecognized_edge_type_returns_error | AC-02 | ok |
| test_handle_inverse_sql_injection_rejected_by_type_validation | SR-B | ok |
| test_handle_inverse_limit_zero_returns_error | AC-05 | ok |
| test_handle_inverse_limit_501_returns_error | AC-05 | ok |
| test_handle_inverse_limit_500_accepted | AC-05 | ok |
| test_handle_inverse_limit_1_accepted | AC-05 | ok |
| test_handle_inverse_total_returned_matches_len | AC-06 | ok |
| test_inverse_sql_includes_status_guard_n1 | R-10 | ok |
| test_inverse_sql_includes_status_guard_n3 | R-10, IR-01 | ok |
| test_context_graph_inverse_single_type | AC-27 | ok |
| test_context_graph_inverse_and_semantics | AC-28 | ok |
| test_handle_inverse_limit_default_is_100 | AC-05 (behavioral) | ok |
| test_handle_inverse_duplicate_edge_types_not_an_error | edge case | ok |
| test_handle_inverse_10_types_does_not_error | IR-01 | ok |
| test_handle_inverse_empty_category_in_db_returns_empty_not_error | edge case | ok |
| test_inverse_response_serializes_correctly (graph_read_tests.rs) | vnc020 | ok |

Full workspace: 0 failures across all test suites.

## Issues / Deviations

None. Implementation follows pseudocode exactly. No silent deviations.

Side fix: `graph_read_filter.rs` test helpers used nonexistent `store.store()`, `NewEntry.agent_id`, and `futures::future::join_all`. Fixed to use `store.insert(NewEntry{...})` and sequential async calls. This was required to unblock test compilation — not a scope deviation from this component.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 10 entries including ADR-003 (AND semantics), ADR-002 (GraphParams lock), ADR-007 (no raw SQL), and push_bind pattern #4058. All applied.
- Stored: entry #4519 "N-LEFT-JOIN antijoin for inverse-mode missing edge queries" via `/uni-store-pattern` — covers alias-from-counter safety, EntryRecord two-step hydration gotcha, status=0 guard requirement, and write_pool_test() vs read_pool_server() for test writes.
