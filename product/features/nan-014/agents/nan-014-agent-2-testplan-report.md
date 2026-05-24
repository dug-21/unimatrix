# Agent Report: nan-014-agent-2-testplan

## Task
Test Plan Design (Stage 3a) for nan-014 Container Packaging.

## Output Files

| File | Lines | Content |
|------|-------|---------|
| `test-plan/OVERVIEW.md` | ~95 | Test strategy, risk-to-test mapping, integration harness plan |
| `test-plan/pidguard-self-pid.md` | ~70 | R-02 self-PID guard tests (3 unit tests + edge cases) |
| `test-plan/config-env-override.md` | ~75 | R-13 UNIMATRIX_CONFIG env var tests (4 unit tests + edge cases) |
| `test-plan/serve-foreground.md` | ~80 | R-01/R-05/R-11 foreground mode tests (5 unit + 2 shell + edge cases) |
| `test-plan/health-subcommand.md` | ~80 | R-03 health check tests (2 CLI + 4 unit + 1 shell + edge cases) |
| `test-plan/dockerfile.md` | ~80 | R-04/R-07/R-08/R-12/R-14 build validation (7 shell tests + checklist) |
| `test-plan/docker-compose.md` | ~50 | R-01/R-05 compose tests (3 shell tests + checklist) |
| `test-plan/dockerignore.md` | ~60 | R-09 context exclusion tests (4 grep/shell tests + checklist) |
| `test-plan/ci-container-jobs.md` | ~60 | R-10 CI independence tests (4 static analysis tests + checklist) |

## Risk Coverage Summary

All 14 risks covered. All 30 scenarios mapped to specific tests.

| Priority | Risks | Tests Planned |
|----------|-------|---------------|
| High | R-01, R-02, R-03, R-04, R-05 | 14 unit + 5 shell |
| Medium | R-06, R-07, R-08, R-09, R-10, R-13 | 6 unit + 7 shell/static |
| Low | R-11, R-12, R-14 | 4 unit + 1 shell |

## Integration Harness Plan

- MANDATORY: `pytest -m smoke`
- Run: `protocol`, `tools`, `lifecycle` suites
- Skip: `confidence`, `contradiction`, `security`, `volume`, `edge_cases`, `adaptation`
- New integration tests: NONE needed (rationale documented in OVERVIEW.md)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 16 entries returned; relevant: #4575 (ADR-007 PidGuard self-PID), #4571 (ADR-003 health check UDS), #4570 (ADR-002 ORT SHA-256), #4572 (ADR-004 CI independence)
- Stored: nothing novel to store -- test plan design follows established patterns (Arrange/Act/Assert, risk-driven coverage mapping). No new testing infrastructure or patterns discovered.

## Open Questions

1. **Thread safety of env var tests**: The `config-env-override` tests use `std::env::set_var` which is not thread-safe. Implementation should use `#[serial_test::serial]` or run in isolated test binaries. The tester agent should verify this at Stage 3c.

2. **Container test execution environment**: Shell-based tests (dockerfile, docker-compose) require Docker available in the test environment. If Stage 3c runs in a CI container without Docker-in-Docker, these tests must be deferred to manual verification or a separate CI job. The tester agent should document which tests could and could not be executed.

3. **Health check live socket test**: `test_health_run_success_on_live_socket` requires spawning a UnixListener in the test. Verify the test does not leak sockets or file descriptors. The listener thread should be properly joined or use a scoped thread.
