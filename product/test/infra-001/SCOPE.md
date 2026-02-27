# infra-001: Dockerized Integration Test Harness

## Problem Statement

Unimatrix has 745+ unit tests that validate individual components in isolation — store operations, vector indexing, embedding generation, server validation, confidence formulas, and contradiction detection. These tests exercise internal Rust APIs directly, confirming that each function behaves correctly in isolation.

What's missing is system-level validation: testing the compiled binary through the MCP protocol — the actual interface agents use — with realistic volume, multi-step lifecycle scenarios, security probes, and edge cases. The gap between "all unit tests pass" and "I trust this system" is precisely this layer.

The product vision's core value proposition is **trustworthy, correctable, and auditable** knowledge. Trust requires evidence, and evidence requires comprehensive system-level testing that exercises:

- The MCP JSON-RPC protocol handshake and compliance
- All 9 tools through their complete parameter space (happy paths, error paths, edge cases)
- Multi-step knowledge lifecycle flows (store → search → correct → deprecate → quarantine → restore)
- The confidence system's 6-factor composite formula under real access patterns
- Contradiction detection with semantically similar but conflicting entries at scale
- Security defenses (content scanning, capability enforcement, input validation) under adversarial inputs
- Volume behavior with thousands of entries, large payloads, and concurrent operations
- Data persistence across server restart cycles

Without this test bed, every release relies on unit test coverage and manual verification — insufficient for a system where a single poisoned entry propagates across every future feature cycle.

## Goals

1. **Dockerized test environment** — A multi-stage Docker build that compiles the `unimatrix-server` binary, then runs system-level tests against it. `docker compose up` runs everything; `docker compose down` tears it down. Zero host dependencies beyond Docker.

2. **MCP protocol test client** — A Python library that spawns the server as a subprocess, completes the MCP initialize handshake, and provides typed wrappers for all 9 `context_*` tools over stdin/stdout JSON-RPC. This matches exactly how agents interact with the server.

3. **Test data generation** — Factories that produce realistic, varied test data: entries with controlled topic/category distributions, contradicting pairs, correction chains, injection payloads, PII samples, unicode edge cases, and bulk datasets with deterministic seeds for reproducibility.

4. **Eight test suites covering distinct validation concerns:**

   - **Protocol** (~15 tests) — MCP handshake, tool discovery, JSON-RPC compliance, malformed input handling, graceful shutdown.
   - **Tools** (~80 tests) — Every tool, every parameter, every error condition. Store roundtrips, search relevance, lookup filtering, get by ID, correction chains, deprecation, status reports, briefings, quarantine lifecycle.
   - **Lifecycle** (~25 tests) — Multi-step scenarios exercising knowledge management workflows end-to-end: store→search→find, correction chain integrity, confidence evolution over repeated access, agent auto-enrollment, audit log completeness, data persistence across restart.
   - **Volume** (~15 tests) — Scale testing with 1K-5K entries, search accuracy and performance at scale, 1MB payloads, 100 distinct topics, contradiction scan over large datasets.
   - **Security** (~30 tests) — Content scanning against ~50 injection patterns and PII, capability enforcement for all trust levels, input validation boundary testing (max lengths, control characters, invalid IDs).
   - **Confidence** (~20 tests) — Validate the 6-factor composite formula: base scores per status, usage factor with log transform, freshness decay, Wilson score helpfulness with min-5-vote guard, correction and trust factors, search re-ranking (0.85×similarity + 0.15×confidence).
   - **Contradiction** (~15 tests) — Detection pipeline validation: negation opposition, incompatible directives, opposing sentiment, false positive resistance, embedding consistency checks, scan at scale.
   - **Edge Cases** (~25 tests) — Unicode (CJK, RTL, emoji, ZWJ, combining chars), boundary values, empty database operations, concurrent operations, restart persistence, special characters in queries.

5. **Suite selection and markers** — Run all suites or select specific ones via environment variable. pytest markers for `smoke` (~15 critical-path tests, ~30s), `slow`, `volume`, `security`. Enables fast feedback during development and thorough validation before release.

6. **CI-ready output** — Exit code 0 = pass, non-zero = fail. JUnit XML for CI integration. JSON report for detailed analysis. Server stderr logs captured per suite. Human-readable summary.

## Non-Goals

- **Not replacing `cargo test`.** The 745+ unit tests stay where they are and continue to run via `cargo test --lib`. This harness exercises the system through the external protocol, not internal APIs.
- **Not performance benchmarking.** Volume tests validate correctness at scale and catch timeouts, but do not establish precise latency targets or regression thresholds. Performance benchmarking is a separate effort.
- **Not UI testing.** No dashboard exists yet (M6). When it does, it gets its own test infrastructure.
- **Not multi-project testing.** Project isolation is an M7 feature. This harness tests single-project behavior.
- **Not fuzz testing.** Structured property-based testing or fuzz testing (e.g., cargo-fuzz, hypothesis) is a potential future enhancement but out of scope for infra-001.
- **Not testing the test harness.** The harness itself does not need its own test suite. If the server tests pass, the harness works.

## Background Research

### MCP Protocol Transport

The server's only transport is stdio — MCP JSON-RPC over stdin/stdout. There is no HTTP endpoint. The test harness must spawn the binary as a subprocess and communicate via pipes. This is the most realistic test because it matches exactly how Claude Code (the only current client) interacts with the server.

The MCP protocol uses JSON-RPC 2.0 with specific lifecycle methods:
- `initialize` — client→server handshake with capabilities negotiation
- `initialized` — client→server notification after handshake
- `tools/list` — enumerate available tools
- `tools/call` — invoke a tool with arguments
- `shutdown` — graceful termination request

### Server Lifecycle

The server binary (`unimatrix-server`) accepts `--project-dir` to override the data directory. For testing, each test gets a fresh temp directory. Server startup is fast (~200ms) because the embedding model loads lazily in the background. This makes one-server-per-test feasible without prohibitive overhead.

Key lifecycle concerns for testing:
- Database lock (redb exclusive lock — only one process per DB file)
- PID file management
- Vector index persistence (explicit `dump()` on shutdown)
- Store compaction (explicit `compact()` on shutdown)
- Graceful shutdown via MCP `shutdown` or process termination

### Python + pytest Rationale

The harness tests the binary through its protocol, not internal APIs. This is a black-box test by design. Python + pytest provides:
- Subprocess management (`subprocess.Popen` with pipe communication)
- JSON-RPC handling (native `json` module)
- Fixture system matching server lifecycle (function-scoped = fresh server; module-scoped = shared server for volume)
- Parameterization for testing N entries × M formats × K agents
- Markers for suite selection
- JUnit XML output for CI
- Data generation libraries (`faker`, `random` with seeds)

The alternative — Rust integration tests via `cargo test --test` — would provide type safety but would couple the test harness to the Rust toolchain and make it harder to exercise the binary as a true black box.

### Existing Test Infrastructure

The project has established test infrastructure patterns:
- `TestDb` and `TestEntry` builder (unimatrix-store)
- `TestVectorIndex` and `random_normalized_embedding` (unimatrix-vector)
- `MockProvider` (unimatrix-embed)
- `test-support` feature gates on store, vector, embed, core crates
- Cumulative test infrastructure — each feature extends prior fixtures

The integration harness is additive to this. It does not replace or duplicate it — it operates at a different layer (protocol vs API).

### Docker Build Requirements

The binary requires:
- Rust 1.89+ (edition 2024)
- ONNX Runtime 1.20.x shared library (linked via ort-sys)
- The patched `anndists` crate (in `patches/anndists/`)

The test runtime requires:
- Python 3.12+
- pytest + plugins (pytest-xdist, pytest-timeout, pytest-json-report)
- The compiled `unimatrix-server` binary
- ONNX Runtime shared library (same version as build)
- The `all-MiniLM-L6-v2` ONNX model (downloaded on first embed, cached)

### Embedding Model Availability

The server lazy-loads the embedding model on first use. For tests that exercise semantic search, the model must be available. Two options:
1. **Pre-download in Docker image** — larger image, faster tests, no network dependency at runtime
2. **Download on first use** — smaller image, slower first test, requires network

Option 1 is preferred for reproducibility. The model (~90MB) is cached in the Docker image layer.

## Proposed Approach

### Architecture

```
product/test/infra-001/
├── SCOPE.md
├── Dockerfile                     # Multi-stage: builder → test-runtime
├── docker-compose.yml             # Orchestration with env var overrides
├── harness/                       # Python MCP client library
│   ├── __init__.py
│   ├── client.py                  # UnimatrixClient — subprocess + JSON-RPC
│   ├── generators.py              # Test data factories
│   ├── assertions.py              # Custom assertion helpers
│   └── conftest.py                # pytest fixtures (server lifecycle)
├── suites/                        # Test suites (pytest modules)
│   ├── conftest.py                # Suite-level fixtures
│   ├── test_protocol.py           # MCP protocol compliance
│   ├── test_tools.py              # All 9 tools coverage
│   ├── test_lifecycle.py          # Multi-step knowledge flows
│   ├── test_volume.py             # Scale & stress
│   ├── test_security.py           # Security & hardening
│   ├── test_confidence.py         # Confidence formula validation
│   ├── test_contradiction.py      # Contradiction detection
│   └── test_edge_cases.py         # Boundary conditions
├── fixtures/                      # Static test data files
│   ├── injection_patterns.json
│   ├── pii_samples.json
│   ├── unicode_corpus.json
│   └── large_entries.json
├── scripts/
│   ├── run.sh                     # Entrypoint: suite selection + reporting
│   └── report.sh                  # Collect and format results
└── pytest.ini                     # pytest configuration + markers
```

### Docker Strategy

**Stage 1 (builder):** Rust 1.89 base, copies workspace, builds release binary, runs `cargo test --lib` as baseline validation. Outputs the binary.

**Stage 2 (test-runtime):** Python 3.12-slim base, installs ONNX Runtime shared library, copies binary from builder, installs pytest + plugins, copies harness/suites/fixtures. Pre-downloads the embedding model into the image layer for offline reproducibility. Entrypoint is `run.sh`.

**docker-compose.yml:** Single `test-runner` service built from the multi-stage Dockerfile targeting `test-runtime`. Environment variables for suite selection (`TEST_SUITE`), parallelism (`TEST_WORKERS`), and pytest args (`PYTEST_ARGS`). tmpfs mount for temp databases. Volume for test results.

### MCP Client (`harness/client.py`)

Core class `UnimatrixClient`:
- Constructor spawns `unimatrix-server --project-dir <tmpdir>` subprocess with piped stdin/stdout/stderr
- `initialize()` sends MCP `initialize` request, waits for response, sends `initialized` notification
- Typed methods for all 9 tools (`context_store`, `context_search`, etc.) that build JSON-RPC requests and parse responses
- `call_tool(name, arguments)` low-level method for raw tool calls
- `shutdown()` sends MCP shutdown, waits for process exit, captures stderr log
- Timeout enforcement on every call (default 10s)
- Context manager support (`with UnimatrixClient(...) as client:`)

### Test Isolation

Default: one server per test function (pytest function-scoped fixture). Fresh temp directory, fresh database, zero state leakage.

Exception: volume suite uses module-scoped fixture to accumulate entries without re-storing 5K entries per test.

### Data Generation (`harness/generators.py`)

Deterministic factories with overridable seeds:
- `make_entry(**overrides)` — single entry with realistic defaults
- `make_entries(n, topic_distribution, category_mix)` — batch with controlled distribution
- `make_contradicting_pair(topic)` — two entries with high similarity but conflicting content
- `make_correction_chain(depth)` — chain where each corrects the previous
- `make_injection_payloads()` — content strings for injection testing
- `make_pii_content()` — (content, expected_type) pairs
- `make_unicode_edge_cases()` — CJK, RTL, emoji, ZWJ, combining chars
- `make_bulk_dataset(n, seed)` — large dataset with varied metadata

### Reporting

Test results written to `/results/` volume:
- `junit.xml` — CI integration
- `report.json` — detailed pytest results
- `summary.txt` — human-readable table (suite × passed/failed/errors)
- `logs/server-{suite}.log` — server stderr per suite

## Acceptance Criteria

- AC-01: `docker compose up --build --abort-on-container-exit` builds the server binary, runs unit tests, then runs all integration test suites, exiting with code 0 on success and non-zero on failure
- AC-02: `docker compose down -v` cleanly tears down all containers, volumes, and temp data
- AC-03: `TEST_SUITE=<name>` environment variable selects individual suites (protocol, tools, lifecycle, volume, security, confidence, contradiction, edge_cases) or comma-separated combinations
- AC-04: `UnimatrixClient` spawns the server binary, completes MCP initialize handshake, provides typed wrappers for all 9 tools, and gracefully shuts down the subprocess
- AC-05: Each test function gets a fresh server instance with its own temp database by default (no state leakage between tests)
- AC-06: Protocol suite validates MCP handshake, tool discovery (all 9 tools listed), malformed input rejection, and graceful shutdown
- AC-07: Tools suite covers every tool's required and optional parameters, valid and invalid inputs, and all three response formats (summary, markdown, json)
- AC-08: Lifecycle suite validates multi-step flows: store→search→find, correction chain integrity, confidence evolution, agent enrollment, audit trail completeness, and data persistence across restart
- AC-09: Volume suite stores 1K+ entries and validates search accuracy, lookup correctness, and status report completion at scale without timeout
- AC-10: Security suite validates content scanning detects injection patterns and PII, capability enforcement blocks unauthorized operations, and input validation rejects out-of-range values
- AC-11: Confidence suite validates the 6-factor composite formula: base scores per status, usage log transform, freshness decay, Wilson score with min-5-vote guard, and search re-ranking blend
- AC-12: Contradiction suite validates detection of negation opposition, incompatible directives, and opposing sentiment while resisting false positives on compatible related entries
- AC-13: Edge cases suite validates Unicode handling (CJK, emoji, RTL, combining chars), boundary values, empty database operations, and concurrent store+search
- AC-14: `pytest -m smoke` runs ~15 critical-path tests in under 60 seconds for quick validation
- AC-15: Test results include JUnit XML, JSON report, human-readable summary, and per-suite server logs
- AC-16: Test data generators use deterministic seeds, logging the seed on failure for reproduction
- AC-17: The Docker image pre-downloads the embedding model so tests run without network access

## Constraints

- **Python, not Rust, for the test harness.** This is a black-box test — it exercises the binary through its MCP protocol, not internal APIs. The delivery protocol's Stage 3b agents will write Python instead of Rust.
- **No modifications to the server codebase.** The harness tests the existing binary as-is. No test-only flags, no HTTP endpoints, no test modes in the server.
- **Deterministic tests.** No randomness without seeded generators. Flaky tests undermine the trust this harness is meant to build.
- **Embedding model must be available.** Tests that exercise semantic search require the ONNX model. The Docker image pre-downloads it.
- **ONNX Runtime version must match.** The test-runtime stage installs the same ONNX Runtime version the binary was compiled against (1.20.x via ort 2.0.0-rc.9).
- **No network access at test time.** All dependencies (binary, model, fixtures) baked into the image. Tests must not reach external services.
- **tmpfs for temp databases.** Test databases live in memory-backed tmpfs to avoid Docker volume I/O overhead and ensure clean teardown.
- **pytest conventions.** Standard pytest project layout, markers, fixtures. No custom test runner.

## Resolved Questions

1. **Python + pytest over Rust integration tests.** The harness tests the binary through its protocol. Python excels at subprocess management, JSON handling, and test parameterization. Rust integration tests would couple the harness to the build toolchain and make it harder to exercise as a true black box.

2. **One server per test by default.** Server startup is ~200ms (lazy embed loading). The isolation benefit (zero state leakage) outweighs the startup cost. Volume suite uses module-scoped fixture as the documented exception.

3. **Pre-download embedding model in Docker.** Reproducibility > image size. The model is ~90MB, cached in a Docker layer. Tests run offline.

4. **Test data uses deterministic seeds, not random.** Reproducibility is non-negotiable for a trust-building test harness. Seeds are logged on failure.

5. **Eight suites, not one monolith.** Each suite maps to a distinct validation concern. Enables parallel development, selective execution, and clear failure attribution.

6. **Product vision alignment.** The product vision does not explicitly list an integration test harness. However, the vision's core value proposition — "trustworthy, correctable, and auditable" — requires demonstrated trustworthiness, which requires comprehensive system-level testing. This harness is infrastructure that enables the vision, analogous to how nxs-001 (storage) enables all features above it.

## Tracking

- GH Issue: https://github.com/dug-21/unimatrix/issues/38
