# Agent Report: nan-004-agent-2-testplan

## Phase: Test Plan Design (Stage 3a)

## Output Files

| File | Description |
|------|-------------|
| `product/features/nan-004/test-plan/OVERVIEW.md` | Overall test strategy, risk mapping, integration harness plan |
| `product/features/nan-004/test-plan/npm-package-structure.md` | C1 test expectations |
| `product/features/nan-004/test-plan/js-shim.md` | C2 test expectations |
| `product/features/nan-004/test-plan/binary-resolution.md` | C3 test expectations |
| `product/features/nan-004/test-plan/init-command.md` | C4 test expectations |
| `product/features/nan-004/test-plan/settings-merge.md` | C5 test expectations |
| `product/features/nan-004/test-plan/postinstall.md` | C6 test expectations |
| `product/features/nan-004/test-plan/binary-rename.md` | C7 test expectations |
| `product/features/nan-004/test-plan/model-download.md` | C8 test expectations |
| `product/features/nan-004/test-plan/version-sync.md` | C9 test expectations |
| `product/features/nan-004/test-plan/release-pipeline.md` | C10 test expectations |
| `product/features/nan-004/test-plan/release-skill.md` | C11 test expectations |

## Risk Coverage Summary

All 15 risks from RISK-TEST-STRATEGY.md are mapped to specific test expectations:

| Priority | Risks | Test Count |
|----------|-------|------------|
| Critical | R-01, R-02 | ~18 tests (merge-settings: 7 scenarios + idempotency, binary path: absolute verification + repair) |
| High | R-03, R-04, R-07, R-09, R-15 | ~14 tests (CI ldd check, dedup assertions, patches assertion, mcp.json preservation, publish order) |
| Medium | R-05, R-06, R-08, R-11, R-13, R-14 | ~19 tests (shim routing, version match, postinstall exit 0, project root agreement, env fallback, malformed JSON) |
| Low | R-10, R-12 | ~5 tests (skill preservation, binary name/subcommand preservation) |

## Integration Harness Plan

- **Suites to run**: smoke (mandatory gate), protocol, tools, lifecycle
- **Suites skipped**: confidence, contradiction, security, volume (nan-004 does not touch engine logic)
- **New integration tests**: None needed in infra-001. Feature adds JS distribution + CLI rename; MCP protocol behavior is unchanged.
- **Harness change required**: Update `get_binary_path()` in `harness/conftest.py` to search for `unimatrix` instead of `unimatrix-server`.

## Open Questions

1. **Node.js test runner availability**: The plan assumes Node.js built-in test runner (`node --test`) is available. If the project prefers a different test framework (jest, vitest, tap), the test file structure remains the same but the runner invocation changes. The built-in runner has no dependencies, which aligns with the project's minimal-dependency philosophy.

2. **Shell integration test execution**: End-to-end tests (init in a temp dir with real binary) require a built binary. These cannot run until Stage 3b completes the binary rename. The tester in Stage 3c should build first, then run JS unit tests, then shell integration tests.

3. **CI-only tests (R-03, R-07)**: The `ldd` check and clean-container smoke test are CI pipeline steps, not locally runnable tests. Their "test" is verifying the workflow YAML contains the correct steps. Actual validation happens only when the pipeline runs.

## Knowledge Stewardship

- Queried: /knowledge-search for testing procedures -- tool unavailable in agent context, proceeded without
- Stored: nothing novel to store -- test plan design phase produces plans, not reusable patterns. If novel JS test patterns emerge during Stage 3c execution, they should be stored then.
