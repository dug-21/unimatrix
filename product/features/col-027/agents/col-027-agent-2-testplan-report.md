# Agent Report: col-027-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Agent ID: col-027-agent-2-testplan

---

## Files Produced

- `/workspaces/unimatrix/product/features/col-027/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/col-027/test-plan/core-constants.md`
- `/workspaces/unimatrix/product/features/col-027/test-plan/hook-registration.md`
- `/workspaces/unimatrix/product/features/col-027/test-plan/hook-dispatcher.md`
- `/workspaces/unimatrix/product/features/col-027/test-plan/observation-storage.md`
- `/workspaces/unimatrix/product/features/col-027/test-plan/friction-metrics.md`

---

## Risk Coverage Mapping Summary

| Risk ID | Priority | Component | Test File | Test Count |
|---------|----------|-----------|-----------|-----------|
| R-01 | Critical | observation-storage | observation-storage.md | T-OS-01 through T-OS-11 (11 tests, 4 for `extract_error_field`, 4 for `extract_observation_fields`, 1 negative/guard test, 2 edge cases) |
| R-02 | Critical | friction-metrics | friction-metrics.md | T-FM-08 through T-FM-10 (3 coupled tests in single functions) |
| R-03 | High | observation-storage | observation-storage.md | T-OS-08 (compound), T-OS-10 |
| R-04 | High | friction-metrics | friction-metrics.md | T-FM-01 through T-FM-04 |
| R-05 | Med | hook-dispatcher | hook-dispatcher.md | T-HD-01, T-HD-05 |
| R-06 | Med | friction-metrics | friction-metrics.md | T-FM-11 through T-FM-15, T-FM-18 |
| R-07 | Med | friction-metrics | friction-metrics.md | T-FM-16, T-FM-17 |
| R-08 | Med | hook-dispatcher | hook-dispatcher.md | T-HD-02 through T-HD-04, + binary integration |
| R-09 | Low | hook-dispatcher | hook-dispatcher.md | T-HD-06 |
| R-10 | Low | observation-storage | observation-storage.md | assertion in T-OS-08 |
| R-11 | Med | core-constants | core-constants.md | T-CC-01, T-CC-02, T-CC-03 |
| R-12 | Low | friction-metrics | friction-metrics.md | T-FM-07 |
| R-13 | Med | friction-metrics | friction-metrics.md | T-FM-19, T-FM-20 |
| R-14 | High | hook-registration | hook-registration.md | T-HR-01 through T-HR-04 (shell inspection) |

All 14 risks have test coverage. No gaps.

---

## Integration Harness Plan Summary

- **Mandatory gate**: `pytest suites/ -v -m smoke --timeout=60`
- **No new infra-001 suite tests needed**: The `PostToolUseFailure` hook fires via stdin/stdout
  to the hook binary, not through the MCP JSON-RPC interface. All observable behaviour is in
  unit-testable code paths.
- **One new binary integration test** (AC-12): shell-level exit-code assertions run against the
  compiled binary during Stage 3c. Not added to infra-001 pytest suite.
- **Suites to run**: smoke (required), tools (optional regression check).

---

## Critical Design Decisions Embedded in Test Plans

1. **R-02 coupling requirement** (ADR-004): T-FM-08 through T-FM-10 in `friction-metrics.md`
   must be placed in a single test function calling both `compute_universal()` and
   `PermissionRetriesRule::detect()` on the same record slice. The implementer must resolve
   the import path — either in-module `use crate::metrics::compute_universal` in `friction.rs`,
   or a new test in `crates/unimatrix-observe/tests/`. Both are acceptable.

2. **R-01 compound assertion** (AC-03/AC-04): T-OS-08 must assert all four conditions
   (`obs.hook`, `obs.tool.is_some()`, `obs.response_snippet`, `obs.response_size`) in one
   block. Splitting risks one masking another.

3. **String constant discipline** (NFR-06/R-11): All test comparisons against
   `"PostToolUseFailure"` must use `hook_type::POSTTOOLUSEFAILURE`, not inline string literals.
   T-CC-03 is a code-inspection constraint, not a test function.

4. **`make_failure` helper scope**: The `friction.rs` helper uses `make_failure(ts, tool)`.
   The `metrics.rs` helper needs a different signature to match the existing `make_post` pattern
   there. These are independent helpers; do not share across crate boundaries.

---

## Open Questions

None. All decisions resolved in ADRs and IMPLEMENTATION-BRIEF.md. The only ambiguity is
`extract_error_field` behaviour for `error: ""` (empty string) — the test plan (T-OS-07) requires
the implementer to choose `None` or `Some("")` and document the choice. `None` is recommended.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "col-027 architectural decisions" (category: decision,
  topic: col-027) — found all 5 col-027 ADRs (#3473–#3477). Full content read from disk.
- Queried: `/uni-knowledge-search` for "hook observation detection rule testing patterns" —
  found entries #763 (server-side observation intercept pattern), #2907 (mandatory source_domain
  guards), #2928 (string-refactor test patterns), #2843 (blast-radius pattern).
- Queried: `/uni-knowledge-search` for "testing procedures gate verification integration test"
  (category: procedure) — found #487 (workspace tests without hanging), #750 (pipeline validation
  tests), #553 (smoke-test validation). None were blocking; proceeded without.
- Queried: `/uni-knowledge-search` for "two-site atomicity coupled test same function pattern"
  — found entry #3472 (atomic update requirement, architecture-level). The test-enforcement
  pattern (single-function cross-module assertion) was not yet stored.
- Stored: entry #3479 "Two-Site Atomicity Enforcement: Cross-Module Coupled Test Pattern" via
  `mcp__unimatrix__context_store` (pattern, topic: col-027). Novel because #3472 captures the
  architecture requirement but not the test implementation pattern for enforcing it.
