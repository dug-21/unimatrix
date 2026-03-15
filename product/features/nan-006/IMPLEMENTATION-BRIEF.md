# Implementation Brief: nan-006 — Availability Test Suite

## Feature Context

- **Feature ID**: nan-006
- **GH Issue**: #281
- **Branch**: feature/nan-006
- **Language**: Python (test suite) + Rust (background.rs env var change)
- **Scope**: New availability test tier for time-extended reliability testing

## Component Map

| Component | Description | Pseudocode | Test Plan |
|-----------|------------|-----------|-----------|
| C1: Rust env var | `UNIMATRIX_TICK_INTERVAL_SECS` env var in background.rs | product/features/nan-006/pseudocode/rust-env-var.md | product/features/nan-006/test-plan/rust-env-var.md |
| C2: fast_tick_server fixture | New pytest fixture in harness/conftest.py + UnimatrixClient extra_env | product/features/nan-006/pseudocode/fast-tick-fixture.md | product/features/nan-006/test-plan/fast-tick-fixture.md |
| C3: test_availability.py | New test suite with 5+1 tests | product/features/nan-006/pseudocode/test-availability.md | product/features/nan-006/test-plan/test-availability.md |
| C4: USAGE-PROTOCOL.md update | Pre-Release Gate section + When to Run table | product/features/nan-006/pseudocode/docs-update.md | product/features/nan-006/test-plan/docs-update.md |
| C5: pytest mark registration | Register `availability` in pytest.ini | product/features/nan-006/pseudocode/mark-registration.md | product/features/nan-006/test-plan/mark-registration.md |

## Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/nan-006/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/nan-006/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Implementation Waves

### Wave 1: Rust change (C1)
Must land first — all Python fixtures and tests depend on the env var being available.

### Wave 2: Python infrastructure (C2, C5)
Fixture and mark registration — independent of each other, both depend on C1 being committed.

### Wave 3: Test suite + docs (C3, C4)
Tests depend on the fixture (C2). Docs update is independent but deferred to Wave 3 for clean ordering.

## Key Constraints

- MCP stdio client is NOT thread-safe — all MCP calls must be sequential
- Tests use `time.time()` wall-clock deadlines, NOT pytest-timeout (which kills the process)
- Tick interval 30s: tick fires at t≈30s, tests should wait until t≈35-40s before firing concurrent calls
- `test_sustained_multi_tick` needs `@pytest.mark.timeout(150)` to override 60s default
- xfail tests: `strict=False` — they may pass or fail, no strict enforcement
- `fast_tick_server` passes `UNIMATRIX_TICK_INTERVAL_SECS=30` as env var to subprocess

## Source Documents

- GH Issue #281
- Spawn prompt context (inline)
- `product/test/infra-001/USAGE-PROTOCOL.md`
- `product/test/infra-001/IMPLEMENTATION-BRIEF.md` (infra-001 reference)
