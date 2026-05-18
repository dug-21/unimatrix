# Agent Report: vnc-016-agent-7-integration-tests

## Task

Add two integration test functions to `product/test/infra-001/suites/test_tools.py` after the vnc-015 section:
- `test_dependency_on_deprecated_e2e` (positive path, 9 steps)
- `test_dependency_on_deprecated_no_finding_without_stale_edge` (negative path)

## Files Modified

- `product/test/infra-001/suites/test_tools.py` — 218 lines appended after line 3620 (vnc-015 section end)

## Tests

Code inspection pass (Stage 3c will run actual integration tests).

Constraint verification:
- C-01: `feature_cycle=cycle_id` passed on `context_store` call for entry A — PRESENT (line 3670)
- C-01b: `agent_id=test_agent_id` (Restricted+Write) on entry A store — PRESENT (line 3671)
- C-02: `force=True` on every `context_cycle_review` call — PRESENT in both functions (lines 3721, 3810)
- C-03: Single `cycle_id` binding used consistently per test — PRESENT
- C-04: `num_records=20` in both `_seed_observation_sql` calls — PRESENT (lines 3711, 3802)
- C-05: Distinct `cycle_id` prefixes (`vnc016-` vs `vnc016neg-`) — PRESENT (lines 3640, 3772)
- C-06: `context_correct` deprecation does not pass `feature_cycle` — CORRECT (lines 3697-3706)
- C-07: No new test files created — CORRECT

Python syntax check: PASS (`ast.parse` confirmed)

Grep verification:
```
test_dependency_on_deprecated_e2e          — line 3628
test_dependency_on_deprecated_no_finding_without_stale_edge — line 3758
```

## Deviations from Pseudocode

One correction applied: pseudocode references `_resolve_db_path(server.project_dir)` but this function does not exist in `test_tools.py`. The actual helper is `_compute_db_path(server.project_dir)` (defined at line 956). Used `_compute_db_path` throughout — matches all existing call sites in the file (lines 1045, 1087, 1120, etc.).

Pseudocode also uses `import json, uuid` inside the function body; the file convention uses module-level `import json as _json` (line 949) and `import uuid` (line 953). Used `_json.loads` and `uuid.uuid4()` directly — matches existing patterns.

## Issues / Blockers

None. All dependencies (harness client `feature_cycle` param, SQL fix, usage gate fix) are confirmed present from prior agent reports.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #4452 (integration test gate fix pattern), #3800 (check_stored_review memoization), #4437 (lesson: update test_protocol.py when adding tools). Entry #4452 directly relevant — confirms this test must use the enrolled Restricted+Write agent.
- Stored: entry #4454 "Use _compute_db_path (not _resolve_db_path) and _json alias in test_tools.py" via `/uni-store-pattern` — captures the pseudocode/reality mismatch on the db path helper name and JSON import alias convention for future agents extending this suite.
