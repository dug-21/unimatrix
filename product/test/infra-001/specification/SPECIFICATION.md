# Specification: infra-001 — Dockerized Integration Test Harness

## Functional Requirements

### FR-01: Docker Build Pipeline

The harness builds and runs entirely within Docker.

- FR-01.1: A multi-stage Dockerfile compiles the `unimatrix-server` binary (Stage 1) and prepares a Python test runtime (Stage 2).
- FR-01.2: `docker compose up --build --abort-on-container-exit` executes the full pipeline: build, unit tests (cargo test --lib), integration tests.
- FR-01.3: `docker compose down -v` removes all containers, volumes, and temporary data.
- FR-01.4: The build stage copies the full Rust workspace including `patches/anndists/` and builds in release mode.
- FR-01.5: The test-runtime stage pre-downloads the `all-MiniLM-L6-v2` ONNX model so tests run without network access.
- FR-01.6: Environment variables control test behavior: `TEST_SUITE` (suite selection), `TEST_WORKERS` (parallelism), `PYTEST_ARGS` (additional pytest arguments).

### FR-02: MCP Client Library

A Python class that manages a `unimatrix-server` subprocess and communicates via MCP JSON-RPC over stdin/stdout.

- FR-02.1: `UnimatrixClient(binary_path, project_dir)` spawns the server subprocess with the specified project directory.
- FR-02.2: `initialize()` completes the MCP handshake: sends `initialize` request, validates response, sends `initialized` notification.
- FR-02.3: Typed methods for all 9 tools: `context_store`, `context_search`, `context_lookup`, `context_get`, `context_correct`, `context_deprecate`, `context_status`, `context_briefing`, `context_quarantine`.
- FR-02.4: Each tool method accepts the tool's parameters as keyword arguments and returns a parsed response dict.
- FR-02.5: `call_tool(name, arguments)` provides low-level access for arbitrary tool calls.
- FR-02.6: `send_raw(method, params)` sends arbitrary JSON-RPC requests for protocol-level testing.
- FR-02.7: Every call enforces a configurable timeout (default 10 seconds). Timeout raises a descriptive exception.
- FR-02.8: `shutdown()` sends MCP shutdown request, waits for clean exit, falls back to SIGTERM then SIGKILL.
- FR-02.9: Context manager support: `with UnimatrixClient(...) as client:` handles init and shutdown.
- FR-02.10: Server stderr is captured asynchronously and available for diagnostics on failure.

### FR-03: Test Data Generation

Deterministic factories that produce realistic test data.

- FR-03.1: `make_entry(**overrides)` produces a single entry dict with realistic defaults for content, topic, category, and optional fields.
- FR-03.2: `make_entries(n, topic_distribution, category_mix)` produces batches with controlled metadata distribution.
- FR-03.3: `make_contradicting_pair(topic)` produces two entries with high semantic similarity but conflicting directives (e.g., "always use X" vs "never use X").
- FR-03.4: `make_correction_chain(depth)` produces a chain where each entry corrects the previous.
- FR-03.5: `make_injection_payloads()` returns content strings for injection testing (SQL, shell, template, prompt injection).
- FR-03.6: `make_pii_content()` returns (content, expected_type) tuples for PII detection testing.
- FR-03.7: `make_unicode_edge_cases()` returns strings with CJK, RTL, emoji, ZWJ, combining characters.
- FR-03.8: `make_bulk_dataset(n, seed)` returns large datasets for volume testing.
- FR-03.9: All generators accept an optional `seed` parameter. Default seeds are deterministic per module.
- FR-03.10: On test failure, the seed value is logged for reproduction.

### FR-04: Assertion Helpers

An abstraction layer that isolates test assertions from raw JSON-RPC response structure.

- FR-04.1: `assert_tool_success(response)` verifies no error and returns parsed content.
- FR-04.2: `assert_tool_error(response, expected_code)` verifies an error response with the expected code.
- FR-04.3: `parse_entry(response)` extracts a single entry dict from a tool response.
- FR-04.4: `parse_entries(response)` extracts an entry list from search or lookup responses.
- FR-04.5: `assert_entry_has(response, field, value)` checks a specific field value on a parsed entry.
- FR-04.6: `assert_search_contains(response, entry_id)` checks that an entry ID appears in search results.
- FR-04.7: `assert_search_not_contains(response, entry_id)` checks that an entry ID does not appear in results.

### FR-05: pytest Fixtures

Server lifecycle management via pytest fixtures.

- FR-05.1: `server` fixture (function-scoped): spawns a fresh server per test in a unique temp directory, initializes, yields client, shuts down on teardown.
- FR-05.2: `shared_server` fixture (module-scoped): one server per test module for suites that accumulate state (volume, lifecycle).
- FR-05.3: `populated_server` fixture: wraps `server` with a pre-loaded standard dataset (50 entries, 5 topics, 3 categories).
- FR-05.4: Binary path resolved from `UNIMATRIX_BINARY` env var (Docker) or fallback to `target/release/unimatrix-server`.
- FR-05.5: On teardown failure, the fixture logs diagnostics (server stderr, temp directory path) but does not fail the teardown.

### FR-06: Suite Selection and Markers

- FR-06.1: `TEST_SUITE` environment variable selects suites: `all` (default), individual names (`protocol`, `tools`, `lifecycle`, `volume`, `security`, `confidence`, `contradiction`, `edge_cases`), or comma-separated combinations.
- FR-06.2: pytest markers: `smoke` (~15 critical-path tests), `slow` (tests >10s), `volume` (scale tests), `security` (security tests).
- FR-06.3: `pytest -m smoke` runs the smoke subset in under 60 seconds.

### FR-07: Reporting

- FR-07.1: JUnit XML output at `/results/junit.xml`.
- FR-07.2: JSON report at `/results/report.json` (via pytest-json-report).
- FR-07.3: Human-readable summary at `/results/summary.txt` with per-suite pass/fail/error counts.
- FR-07.4: Server stderr logs at `/results/logs/server-{suite}.log` per suite.
- FR-07.5: Container exit code matches pytest exit code (0 = pass, non-zero = fail).

## Non-Functional Requirements

### NFR-01: Isolation

Each test function runs against its own server instance with its own database in a unique temp directory. No state leaks between tests. The only exception is module-scoped fixtures for volume/lifecycle suites where state accumulation is the test.

### NFR-02: Reproducibility

All test data is generated from deterministic seeds. Static fixtures (injection patterns, PII samples, unicode corpus) are version-controlled JSON files. The Docker image pre-downloads the embedding model. No network access at test time.

### NFR-03: Performance

- Smoke tests (~15 tests): under 60 seconds total.
- Full suite (~225 tests): under 15 minutes with default parallelism.
- Server startup per test: under 2 seconds including MCP handshake.
- Individual tool call timeout: 10 seconds (configurable).
- Volume test entry storage: 1K entries in under 60 seconds, 5K entries in under 5 minutes.

### NFR-04: Diagnostics

On test failure:
- Server stderr log is available in `/results/logs/`.
- The data generator seed is logged for reproduction.
- The temp directory path is logged for post-mortem inspection (if not on tmpfs).
- pytest captures stdout/stderr per test.

### NFR-05: Maintainability

- Response parsing is centralized in `assertions.py` (ADR-001).
- Test data generation is centralized in `generators.py`.
- Server lifecycle is centralized in `conftest.py` fixtures.
- Adding a new test suite requires only a new `suites/test_*.py` file and entry in `run.sh`.

## Test Suites Specification

### Suite 1: Protocol (~15 tests)

Validates MCP protocol compliance.

| ID | Test | Acceptance |
|----|------|-----------|
| P-01 | Initialize returns capabilities | Response has `capabilities` with `tools` enabled |
| P-02 | Server info present | Response has `serverInfo` with name and version |
| P-03 | List tools returns all 9 | `tools/list` returns exactly 9 `context_*` tools |
| P-04 | Tool schemas are valid JSON Schema | Each tool's `inputSchema` validates as JSON Schema |
| P-05 | Unknown tool returns error | Calling `context_nonexistent` returns MCP error |
| P-06 | Malformed JSON-RPC rejected | Invalid JSON on stdin returns parse error |
| P-07 | Missing required params rejected | Tool call without required params returns error |
| P-08 | Concurrent requests handled | Two rapid requests both get correct responses |
| P-09 | Notifications ignored | Notification messages don't produce responses |
| P-10 | Graceful shutdown | Shutdown request + clean process exit (code 0) |
| P-11 | Invalid UTF-8 doesn't crash | Binary on stdin -> server survives or exits cleanly |
| P-12 | Large request payload | 1MB JSON-RPC request handled or rejected cleanly |
| P-13 | Empty tool arguments | `{}` arguments handled per tool's defaults |
| P-14 | Unknown fields ignored | Extra fields in arguments don't cause errors |
| P-15 | JSON format responses are parseable | All tools with format=json return valid JSON |

### Suite 2: Tools (~80 tests)

Every tool, every parameter, happy and error paths. See SCOPE.md for full test table. Tool coverage must include:

- `context_store`: 15 tests (minimal, all fields, roundtrip, near-duplicate, validation errors)
- `context_search`: 12 tests (relevance, filters, exclusions, formats, re-ranking)
- `context_lookup`: 10 tests (ID, topic, category, tags, status, combined, limit)
- `context_get`: 6 tests (existing, nonexistent, quarantined, metadata, format, validation)
- `context_correct`: 8 tests (chain, atomic, nonexistent, already-deprecated, scanning, capability)
- `context_deprecate`: 5 tests (status, idempotent, nonexistent, capability, search exclusion)
- `context_status`: 8 tests (empty DB, counts, distributions, corrections, confidence, format, embeddings)
- `context_briefing`: 8 tests (content, role, task, feature, max_tokens, quarantine exclusion, empty DB)
- `context_quarantine`: 8 tests (status, search/lookup exclusion, get visibility, restore, capability, confidence)

### Suite 3: Lifecycle (~25 tests)

Multi-step scenarios exercising knowledge management workflows end-to-end. Each test exercises a complete flow, not isolated operations.

### Suite 4: Volume (~15 tests)

Scale testing with 1K-5K entries. Uses module-scoped shared server. Validates correctness at scale, not performance benchmarks.

### Suite 5: Security (~30 tests)

Three areas: content scanning (10 tests), capability enforcement (8 tests), input validation (12 tests).

### Suite 6: Confidence (~20 tests)

Validates the 6-factor composite formula through observable tool responses: base scores, usage factor, freshness, helpfulness (Wilson score), correction factor, trust factor, and search re-ranking blend.

### Suite 7: Contradiction (~15 tests)

Validates the contradiction detection pipeline: three signal types, false positive resistance, quarantine effects, embedding consistency, and scale behavior.

### Suite 8: Edge Cases (~25 tests)

Boundary conditions: empty database, Unicode (CJK, RTL, emoji, ZWJ, combining), max-length fields, concurrent operations, server restart persistence.

## Domain Model

### Test Data

```
Entry := {
  content: str,       # required
  topic: str,         # required
  category: str,      # required, must be in allowlist
  title: str?,        # optional
  tags: [str]?,       # optional, max 10
  source: str?,       # optional
  agent_id: str?,     # optional, determines capability level
  feature: str?,      # optional, feature_cycle linkage
  format: str?        # optional: "summary" | "markdown" | "json"
}
```

### MCP Protocol

```
Request  := {"jsonrpc": "2.0", "id": int, "method": str, "params": dict}
Response := {"jsonrpc": "2.0", "id": int, "result": dict}
Error    := {"jsonrpc": "2.0", "id": int, "error": {"code": int, "message": str}}
Notification := {"jsonrpc": "2.0", "method": str, "params": dict}  # no id
```

### Trust Levels

| Level | Capabilities | Auto-enrollment |
|-------|-------------|-----------------|
| Restricted | Read (search, lookup, get, briefing) | Yes (unknown agents) |
| Internal | Read + Write (store, correct, deprecate) | No |
| Privileged | Read + Write + Admin (status, quarantine) | No |

### Server Response Formats

All tools support three response formats via `format` parameter:
- `summary` (default): compact markdown text
- `markdown`: detailed markdown
- `json`: structured JSON for machine parsing

## Constraints

- C-01: All harness code is Python. No Rust in the test harness.
- C-02: No modifications to the server binary or its source code.
- C-03: All test data is deterministic (seeded generators + static fixtures).
- C-04: Docker image requires no network access at test time.
- C-05: Server binary requires ONNX Runtime 1.20.x shared library at runtime.
- C-06: Embedding model `all-MiniLM-L6-v2` must be pre-downloaded in Docker image.
- C-07: Tests must not depend on host file system beyond Docker volumes.
- C-08: Each test function uses a unique temp directory for its server instance.
- C-09: Category allowlist for test data: outcome, lesson-learned, decision, convention, pattern, procedure, process, reference.

## Dependencies

| Dependency | Version | Purpose |
|-----------|---------|---------|
| Python | 3.12+ | Test runtime |
| pytest | latest | Test framework |
| pytest-timeout | latest | Per-test timeout enforcement |
| pytest-json-report | latest | JSON report generation |
| Docker | 24+ | Container runtime |
| Docker Compose | v2+ | Multi-service orchestration |
| ONNX Runtime | 1.20.x | Embedding model runtime (matches ort 2.0.0-rc.9) |
| Rust | 1.89+ | Binary compilation (build stage only) |

## Verification Methods

Each acceptance criterion from SCOPE.md maps to a verification approach:

| AC | Verification | Method |
|----|-------------|--------|
| AC-01 | Run `docker compose up --build --abort-on-container-exit`, verify exit 0 | Automated (CI) |
| AC-02 | Run `docker compose down -v`, verify no leftover containers/volumes | Automated (CI) |
| AC-03 | Set `TEST_SUITE=protocol`, verify only protocol tests run | Test of test infrastructure |
| AC-04 | Protocol suite P-01 through P-10 | Protocol test suite |
| AC-05 | Run two tests sequentially, verify no state leakage between them | Lifecycle test suite |
| AC-06 | Protocol suite P-01 through P-15 | Protocol test suite |
| AC-07 | Tools suite — all 80 tests | Tools test suite |
| AC-08 | Lifecycle suite — all 25 tests | Lifecycle test suite |
| AC-09 | Volume suite — 1K+ entry tests | Volume test suite |
| AC-10 | Security suite — all 30 tests | Security test suite |
| AC-11 | Confidence suite — all 20 tests | Confidence test suite |
| AC-12 | Contradiction suite — all 15 tests | Contradiction test suite |
| AC-13 | Edge cases suite — all 25 tests | Edge cases test suite |
| AC-14 | `pytest -m smoke` completes in <60s | Timed execution |
| AC-15 | Verify junit.xml, report.json, summary.txt, logs/ exist after run | File existence check |
| AC-16 | Inject failure, verify seed is logged | Manual verification |
| AC-17 | Disconnect network, run tests, verify all pass | Offline test |
