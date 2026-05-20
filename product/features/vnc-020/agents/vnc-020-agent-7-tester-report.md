# Agent Report: vnc-020-agent-7-tester

## Phase: Test Execution (Stage 3c)

## Summary

Executed all unit tests, integration smoke tests, and relevant integration suites for
vnc-020 (context_graph inverse/filter/path modes). Wrote 6 new integration tests in
infra-001 test_tools.py. All tests pass except AC-31 (path mode integration test) which
is marked xfail per OQ-1 resolution with GH#612.

---

## Test Execution Results

### Unit Tests

```
cargo test -p unimatrix-server
test result: ok. 3183 passed; 0 failed (primary crate)
+ 88 tests across 5 other test binaries
Total: 3271 passed, 0 failed
```

vnc-020 specific unit tests: 96 tests across 4 modules (all pass).

### Integration Tests

**Smoke gate (mandatory)**:
```
pytest -m smoke: 23 passed, 0 failed
```

**Tools suite** (primary — contains new vnc-020 tests):
```
pytest suites/test_tools.py: 183 passed, 4 xfailed, 0 failed
```

**Lifecycle suite** (graph/edge multi-step flows):
```
pytest suites/test_lifecycle.py: 60 passed, 5 xfailed, 2 xpassed, 0 failed
```

**Edge cases suite** (boundary values, from_id==to_id, empty DB):
```
pytest suites/test_edge_cases.py: 22 passed, 2 xfailed, 0 failed
```

### New Integration Tests Added

File: `product/test/infra-001/suites/test_tools.py` (section `# === vnc-020`)

| Test | AC | Result | Notes |
|------|----|--------|-------|
| `test_context_graph_inverse_single_type` | AC-27 | PASS | R-10 deprecated exclusion guard verified |
| `test_context_graph_inverse_and_semantics` | AC-28 | PASS | 4-state fixture; AND vs OR semantics definitively validated |
| `test_context_graph_filter_max_edge_count_zero` | AC-29 | PASS | Critical R-02 boundary; `<= 0` binding verified |
| `test_context_graph_filter_min_edge_count_gte2` | AC-30 | PASS | R-08 edge-count subquery correctness |
| `test_context_graph_path_found` | AC-31 | XFAIL GH#612 | Tick-force limitation; BFS covered by unit tests |
| `test_context_graph_path_self_loop_returns_not_found` | AC-32 | PASS | found: false for from_id == to_id |

Also updated `product/test/infra-001/harness/client.py` — added 8 new vnc-020 parameters
to `context_graph()`: `category`, `missing_edge_types`, `limit`, `min_age_days`,
`min_confidence`, `max_confidence`, `min_edge_count`, `max_edge_count`.

### GH Issues Filed

- **GH#612**: `[infra-001] test_context_graph_path_found: no tick-force mechanism for path mode BFS graph rebuild`
  — Pre-existing infrastructure limitation. AC-31 test marked xfail. BFS logic covered by unit tests.

### Constraint Checks

| Constraint | Result |
|------------|--------|
| C5 — graph_read.rs <= 500 lines | PASS (381 lines) |
| C3 — schema version = 27 | PASS |
| C7 — tool count = 14 | PASS (test_list_tools passed in tools suite) |
| AC-19 — staleness disclosure text | PASS (manual inspection confirmed exact text) |

---

## Risk Coverage

All 14 risks have coverage. No gaps.

Critical risks (R-01 through R-04): Full coverage.
High risks (R-05 through R-10): Full coverage.
Medium/Low risks (R-11 through R-14): Full coverage.

Details in `/workspaces/unimatrix/product/features/vnc-020/testing/RISK-COVERAGE-REPORT.md`.

---

## Issues Encountered and Resolved

Two issues required test fixture fixes (not code bugs):

1. **Category allowlist**: `source` and `goal` are not in INITIAL_CATEGORIES. Tests updated to
   use `convention` (inverse tests) and `decision` (filter tests) — both in allowlist.

2. **Near-duplicate deduplication**: Integration test fixture content strings with high semantic
   similarity (>0.90) were deduped to the same entry ID. Fixed by using semantically distinct
   content from different technical domains per fixture entry.

Neither issue indicates a code defect. Both are integration test fixture design requirements
specific to the Unimatrix knowledge engine's deduplication behavior.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found vnc-020 design decisions (#4502, #4503,
  #4505, #4507), vnc-018/vnc-019 ADRs confirming wire-compatibility constraints, and lesson-learned
  entries #4473 (warn+continue masks failure paths) and #4494 (visited-set keyed on resolved ID).
  These findings confirmed the test plan's risk priorities were correctly assigned.
- Stored: nothing novel to store — the relevant patterns (category allowlist fixtures, semantic
  deduplication thresholds, tick-dependent xfail pattern) are either project-specific fixture
  hygiene or already captured in existing Unimatrix entries.
