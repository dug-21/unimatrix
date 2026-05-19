# Gate 3b Report (Iter 2): vnc-018

> Gate: 3b (Code Review — Iteration 2)
> Date: 2026-05-19
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Iter-1 fix: test_protocol.py (14 tools, context_graph) | PASS | Function renamed, 14 expected, context_graph in list |
| Iter-1 fix: graph_read_neighbors.rs ≤500 lines | PASS | 356 lines; tests extracted to graph_read_neighbors_tests.rs |
| Iter-1 fix: graph_queries.rs ≤500 lines | PASS | 450 lines; tests extracted to graph_queries_tests.rs |
| Code quality — graph_read.rs ≤500 lines | FAIL | 600 lines; #[cfg(test)] block (lines 304–600) should be extracted |
| Build clean | PASS | Finished dev profile, no errors |
| Critical check: schema_version == 26 absent | PASS | Zero matches |
| All other critical checks (1–16, 18) | PASS | Carried forward from iter-1 (all verified clean) |

## Detailed Findings

### Iter-1 Fix A: test_protocol.py (AC-16, FR-14)

**Status**: PASS

`test_list_tools_returns_fourteen` (line 36) asserts exactly 14 tools. Expected list (lines 43–58) includes `"context_graph"`. Previously-failing function `test_list_tools_returns_thirteen` is gone.

### Iter-1 Fix B: graph_read_neighbors.rs line count

**Status**: PASS

File is 356 lines. Tests extracted to `graph_read_neighbors_tests.rs` (5262 bytes). Module uses `#[path = "graph_read_neighbors_tests.rs"] mod tests;` pattern consistently with other split modules in this codebase.

### Iter-1 Fix C: graph_queries.rs line count

**Status**: PASS

File is 450 lines. Tests extracted to `graph_queries_tests.rs` (18332 bytes). Split follows the `query_log_tests.rs` pattern (pre-existing codebase convention).

### New Finding: graph_read.rs exceeds 500-line limit

**Status**: FAIL

`crates/unimatrix-server/src/mcp/graph_read.rs` is 600 lines. Production code ends at line ~298; `#[cfg(test)] mod tests` runs from line 304 to line 600 (297 lines of tests). The file was already 600 lines in the initial delivery — this was present in iter 1 but not flagged in the iter-1 report.

The project's 500-line rule applies to all source files without exception. The fix is the same extraction pattern applied to `graph_read_neighbors.rs` in this rework: extract the `tests` block to `graph_read_tests.rs` and reference it via `#[cfg(test)] #[path = "graph_read_tests.rs"] mod tests;`.

**Evidence**: `wc -l crates/unimatrix-server/src/mcp/graph_read.rs` → 600.

**Fix**: Extract lines 304–600 (the `mod tests` block) to a new file `crates/unimatrix-server/src/mcp/graph_read_tests.rs`. Replace the inline `mod tests { ... }` block with `#[cfg(test)] #[path = "graph_read_tests.rs"] mod tests;`. Result: `graph_read.rs` at ~304 lines, under the limit.

### Build

**Status**: PASS

`cargo build --workspace` completes with `Finished dev profile [unoptimized + debuginfo]` and no errors. 20 pre-existing warnings in unimatrix-server (unrelated to vnc-018); no new errors.

### Critical Check: schema_version == 26

**Status**: PASS

`grep -r 'schema_version.*== 26' crates/` returns zero matches. ADR-007 compliance confirmed.

### All Other Critical Checks (carried from iter-1)

**Status**: PASS (all 16 from iter-1 confirmed; no regressions introduced by the extraction rework)

Spot-checked to confirm the extraction rework did not break any critical constraint:
- Critical check 1 (SQL CTE — no find_terminal_active): PASS — graph_queries.rs extraction did not affect the CTE implementations
- Critical check 2 (validate_no_unsupported_params inside handle_graph): PASS — graph_read.rs line 138
- Critical check 3 (require_cap in tools.rs before handle_graph): PASS — tools.rs line 3381
- Critical check 4 (current mode AND status filter): PASS — in graph_read_supersession.rs
- Critical check 17 (test_protocol.py asserts 14 tools, context_graph in list): PASS — verified above
- Critical check 18 (all module files ≤500 lines): FAIL — graph_read.rs is 600 lines

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| graph_read.rs is 600 lines (100 over 500-line limit) | rust-dev | Extract lines 304–600 (the `#[cfg(test)] mod tests { ... }` block) to a new file `crates/unimatrix-server/src/mcp/graph_read_tests.rs`. Replace with `#[cfg(test)] #[path = "graph_read_tests.rs"] mod tests;`. This is identical to the extraction already done for graph_read_neighbors_tests.rs. |

---

## Knowledge Stewardship

- Stored: nothing novel to store — the graph_read.rs 500-line violation follows the same pattern already documented for graph_read_neighbors.rs (test-in-source-file exceeding the limit). The iter-1 validator missed it because it did not enumerate all new files against the limit; this is a process gap in validation methodology, not a new architectural lesson. No new pattern to store.
