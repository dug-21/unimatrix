# Agent Report: nan-015-agent-2-testplan

## Phase

Test Plan Design (Stage 3a)

## Deliverables

| File | Lines |
|------|-------|
| `product/features/nan-015/test-plan/OVERVIEW.md` | ~65 |
| `product/features/nan-015/test-plan/cache-path-resolution.md` | ~95 |
| `product/features/nan-015/test-plan/dockerfile.md` | ~95 |
| `product/features/nan-015/test-plan/compose-config.md` | ~75 |
| `product/features/nan-015/test-plan/documentation.md` | ~75 |

## Risk Coverage Mapping

All 15 risks from RISK-TEST-STRATEGY.md are mapped to test scenarios:

| Priority | Risks | Test Scenarios |
|----------|-------|----------------|
| Critical | R-01, R-06 | 6 (4 unit tests for R-01, 2 Dockerfile inspections for R-06) |
| High | R-02, R-03, R-04, R-05, R-10 | 10 (code inspection, doc review, Dockerfile grep, CI audit) |
| Med | R-07, R-08, R-09, R-13, R-14, R-15 | 8 (1 unit test, code inspection, compose validation, doc grep) |
| Low | R-11, R-12 | 2 (code inspection, covered by R-01 test 3) |

## Integration Suite Plan

- **Mandatory gate**: `pytest -m smoke` (confirms server starts and serves after code change)
- **No feature-specific suites needed**: nan-015 does not change MCP tool logic, protocol, confidence, contradiction, or security boundaries
- **No new integration tests needed**: `resolve_cache_dir()` is pure path resolution with no MCP-visible effect
- **Container-level tests** (AC-01 through AC-09) require Docker build, validated via shell commands in Stage 3c, outside infra-001 scope

## Self-Check

- [x] OVERVIEW.md maps risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan -- which suites to run, new tests needed
- [x] Per-component test plans match architecture component boundaries (4 components)
- [x] Every high-priority risk has at least one specific test expectation
- [x] Integration tests defined for component boundaries (env var name consistency cross-check)
- [x] All output files within `product/features/nan-015/test-plan/`
- [x] Knowledge Stewardship report block included

## Open Questions

1. **R-01 scenario 4 (dirs fallback)**: Testing the last-resort fallback (`.unimatrix/models`) requires `dirs::cache_dir()` to return `None`, which depends on `HOME` being unset. This may not be reliable in all CI environments. The developer may need a `cfg(test)` helper or conditional assertion.

2. **Env var test serialization**: The 5 unit tests that manipulate `UNIMATRIX_MODEL_CACHE` must not run in parallel. Recommend either `serial_test` crate or a shared Mutex. Note that Rust 1.89 has `std::env::set_var` marked unsafe in edition 2024 -- the implementation may need `unsafe` blocks in tests.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- 11 entries returned. Key entries: #4651 (ADR-002 cache precedence), #4650 (ADR-001 env var redirect), #4652 (ADR-003 shared volume RW), #4582 (nan-014 Docker lesson), #747 (cross-crate test infra pattern). ADR-002 test matrix directly informed R-01 test design.
- Stored: nothing novel to store -- test plan follows standard unit test + static inspection patterns documented in existing conventions. No new testing infrastructure patterns discovered.
