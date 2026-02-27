# Implementation Brief: infra-001 — Dockerized Integration Test Harness

## Purpose

This brief hands off from design (Session 1) to delivery (Session 2). It defines what to build, in what order, and how components map to implementation tasks.

## Feature Context

- **Feature ID**: infra-001
- **Location**: `product/test/infra-001/`
- **Language**: Python 3.12 + pytest (NOT Rust)
- **Scope**: Black-box integration tests against the `unimatrix-server` binary via MCP protocol over stdio
- **Binary under test**: `crates/unimatrix-server/` (compiled in Docker build stage)

## Important Delivery Notes

This feature deviates from standard delivery protocol in several ways:

1. **Python, not Rust.** Stage 3b agents write Python code, not Rust. The Rust workspace rules do not apply.
2. **pytest, not cargo test.** Stage 3c runs `pytest` not `cargo test`. Test validation uses pytest exit codes and JUnit XML.
3. **No crate modifications.** The harness lives entirely in `product/test/infra-001/`. No changes to any crate under `crates/`.
4. **Docker build is the integration test.** The "does it work?" validation is `docker compose up --build --abort-on-container-exit`.
5. **Feature location.** `product/test/infra-001/` not `product/features/infra-001/`.

## Component Map

Components from architecture (C1-C8), ordered by implementation dependency.

| Component | Description | Dependencies | Pseudocode | Test Plan |
|-----------|------------|-------------|-----------|-----------|
| C2: MCP Client | UnimatrixClient class — subprocess management, JSON-RPC, tool wrappers | None | pseudocode/mcp-client.md | test-plan/mcp-client.md |
| C4: Assertions | Response parsing and assertion helpers | C2 | pseudocode/assertions.md | test-plan/assertions.md |
| C3: Generators | Test data factories with deterministic seeds | None | pseudocode/generators.md | test-plan/generators.md |
| C5: Fixtures | pytest fixtures for server lifecycle management | C2 | pseudocode/fixtures.md | test-plan/fixtures.md |
| C7: Static Fixtures | JSON fixture files (injection patterns, PII, unicode, large entries) | None | pseudocode/static-fixtures.md | test-plan/static-fixtures.md |
| C1: Docker Pipeline | Dockerfile + docker-compose.yml | C2, C3, C4, C5, C7 | pseudocode/docker-pipeline.md | test-plan/docker-pipeline.md |
| C8: Runner Scripts | run.sh + report.sh + pytest.ini | C1 | pseudocode/runner-scripts.md | test-plan/runner-scripts.md |
| C6: Test Suites | 8 test suite modules | C2, C3, C4, C5, C7 | pseudocode/test-suites.md | test-plan/test-suites.md |

## Implementation Waves

### Wave 1: Foundation (C2, C4, C3, C7)

Build the harness infrastructure. These components have no dependencies on Docker or the test suites.

- **C2 (MCP Client)**: `harness/client.py` — the critical path component. Subprocess management, JSON-RPC framing, MCP handshake, tool wrappers, timeout enforcement, stderr drain.
- **C4 (Assertions)**: `harness/assertions.py` — response parsing abstraction per ADR-001. Depends on C2's response format.
- **C3 (Generators)**: `harness/generators.py` — deterministic data factories. Independent of C2.
- **C7 (Static Fixtures)**: `fixtures/*.json` — JSON files with injection patterns, PII samples, unicode corpus, large entries. Independent.

### Wave 2: Test Infrastructure (C5, C1, C8)

Build the pytest and Docker infrastructure that ties everything together.

- **C5 (Fixtures)**: `harness/conftest.py` — server lifecycle fixtures (function-scoped, module-scoped, populated). Depends on C2.
- **C1 (Docker Pipeline)**: `Dockerfile` + `docker-compose.yml` — multi-stage build. Depends on all harness files existing.
- **C8 (Runner Scripts)**: `scripts/run.sh` + `scripts/report.sh` + `pytest.ini` — entrypoint and reporting.

### Wave 3: Test Suites (C6)

Write the 8 test suite modules. These depend on all harness infrastructure from Waves 1-2.

Priority order (by risk value):
1. `test_protocol.py` — validates MCP handshake works (foundation for all other tests)
2. `test_tools.py` — validates all 9 tools work (foundation for lifecycle/security tests)
3. `test_security.py` — validates trust defenses
4. `test_lifecycle.py` — validates multi-step flows
5. `test_confidence.py` — validates learning mechanism
6. `test_contradiction.py` — validates drift detection
7. `test_edge_cases.py` — validates boundary conditions
8. `test_volume.py` — validates scale behavior

## File Manifest

```
product/test/infra-001/
├── Dockerfile
├── docker-compose.yml
├── pytest.ini
├── harness/
│   ├── __init__.py
│   ├── client.py              # C2: UnimatrixClient
│   ├── generators.py          # C3: Test data factories
│   ├── assertions.py          # C4: Response abstraction
│   └── conftest.py            # C5: pytest fixtures
├── suites/
│   ├── conftest.py            # Suite-level shared fixtures
│   ├── test_protocol.py       # Suite 1: MCP protocol
│   ├── test_tools.py          # Suite 2: Tool coverage
│   ├── test_lifecycle.py      # Suite 3: Knowledge lifecycle
│   ├── test_volume.py         # Suite 4: Volume/stress
│   ├── test_security.py       # Suite 5: Security
│   ├── test_confidence.py     # Suite 6: Confidence
│   ├── test_contradiction.py  # Suite 7: Contradiction
│   └── test_edge_cases.py     # Suite 8: Edge cases
├── fixtures/
│   ├── injection_patterns.json
│   ├── pii_samples.json
│   ├── unicode_corpus.json
│   └── large_entries.json
└── scripts/
    ├── run.sh
    └── report.sh
```

## Key Technical Decisions

| Decision | Reference |
|----------|-----------|
| Python + pytest for black-box testing | SCOPE.md "Python + pytest Rationale" |
| Response abstraction layer | ADR-001 |
| One server per test by default | Architecture C5, SCOPE.md "Resolved Questions" |
| Deterministic seeded generators | SCOPE.md Constraints |
| Pre-downloaded embedding model | SCOPE.md "Embedding Model Availability" |
| No server modifications | SCOPE.md Constraints |

## Risk Mitigations for Delivery

| Risk | Mitigation in Code |
|------|-------------------|
| R-01 (JSON-RPC framing) | Client reads line-by-line, validates JSON before parsing |
| R-02 (Subprocess orphaning) | Fixture teardown: shutdown() -> SIGTERM -> 5s wait -> SIGKILL |
| R-03 (Response format changes) | All assertions go through assertions.py (ADR-001) |
| R-09 (stderr deadlock) | Dedicated thread drains stderr continuously |
| R-07 (Timeout calibration) | 10s default, configurable per-call, diagnostic on timeout |

## Source Documents

- SCOPE.md: `product/test/infra-001/SCOPE.md`
- Architecture: `product/test/infra-001/architecture/ARCHITECTURE.md`
- ADR-001: `product/test/infra-001/architecture/ADR-001-response-abstraction-pattern.md`
- Specification: `product/test/infra-001/specification/SPECIFICATION.md`
- Risk Strategy: `product/test/infra-001/RISK-TEST-STRATEGY.md`
- Scope Risk Assessment: `product/test/infra-001/SCOPE-RISK-ASSESSMENT.md`
- Alignment Report: `product/test/infra-001/ALIGNMENT-REPORT.md`
- Design Notes: `product/test/infra-001/DESIGN.md`
