# Architecture: infra-001 — Dockerized Integration Test Harness

## Overview

A Python + pytest test harness that exercises the `unimatrix-server` binary through the MCP JSON-RPC protocol over stdio. The system is containerized via Docker with a multi-stage build: Stage 1 compiles the Rust binary, Stage 2 runs Python tests against it.

The harness is a black-box test system. It has zero access to Rust internals. Everything is validated through the 9 `context_*` MCP tools.

## Component Architecture

```
+-------------------------------------------------------------------+
|  Docker Container (test-runtime)                                  |
|                                                                   |
|  +--------------------+     stdin/stdout     +------------------+ |
|  |  pytest runner     | <--- JSON-RPC --->   | unimatrix-server | |
|  |                    |     (MCP protocol)   | (compiled binary)| |
|  |  +- harness/ ----+ |                      |                  | |
|  |  | client.py      | |     stderr -------> | (tracing logs)   | |
|  |  | generators.py  | |                      +------------------+ |
|  |  | assertions.py  | |                              |            |
|  |  | conftest.py    | |                      +-------v--------+   |
|  |  +----------------+ |                      | tmpfs /tmp     |   |
|  |                    | |                      | - redb database|   |
|  |  +- suites/ -----+ | |                      | - vector index |   |
|  |  | test_*.py (x8) | |                      | - pid file     |   |
|  |  +----------------+ |                      +----------------+   |
|  +--------------------+ |                                          |
|           |              |                                          |
|  +--------v---------+   |                                          |
|  | /results/         |   |                                          |
|  | - junit.xml       |   |                                          |
|  | - report.json     |   |                                          |
|  | - summary.txt     |   |                                          |
|  | - logs/           |   |                                          |
|  +-------------------+   |                                          |
+-------------------------------------------------------------------+
```

## Components

### C1: Docker Build Pipeline

Two-stage Dockerfile.

**Stage 1 (builder):**
- Base: `rust:1.89-bookworm`
- Copies full workspace including `patches/anndists/`
- Builds `unimatrix-server` in release mode
- Runs `cargo test --lib` as baseline gate
- Output artifact: `/app/target/release/unimatrix-server`

**Stage 2 (test-runtime):**
- Base: `python:3.12-slim-bookworm`
- Installs ONNX Runtime 1.20.1 shared library (matching ort 2.0.0-rc.9)
- Copies binary from builder
- Installs Python dependencies: pytest, pytest-timeout, pytest-json-report
- Pre-downloads `all-MiniLM-L6-v2` ONNX model into image layer
- Copies harness/, suites/, fixtures/, scripts/, pytest.ini
- Entrypoint: `scripts/run.sh`

**docker-compose.yml:**
- Single service `test-runner` targeting `test-runtime` stage
- Build context: workspace root (`../..` relative to docker-compose.yml)
- Environment: `TEST_SUITE`, `TEST_WORKERS`, `PYTEST_ARGS`, `RUST_LOG`
- Volume: `test-results:/results` for output extraction
- tmpfs: `/tmp:size=512M` for test databases

### C2: MCP Client Library (`harness/client.py`)

Core class: `UnimatrixClient`

**Responsibilities:**
- Spawn `unimatrix-server` as subprocess with piped stdin/stdout/stderr
- Complete MCP `initialize` / `initialized` handshake
- Provide typed Python methods for all 9 `context_*` tools
- Enforce per-call timeouts (default 10s)
- Capture and expose server stderr for diagnostics
- Clean shutdown: MCP shutdown request, SIGTERM, SIGKILL fallback

**Subprocess Management:**
- `subprocess.Popen` with `stdin=PIPE, stdout=PIPE, stderr=PIPE`
- Each test gets a unique `--project-dir <tmpdir>` (no database lock conflicts)
- Read loop: line-based JSON parsing from stdout (newline-delimited JSON-RPC)
- stderr: drained asynchronously via thread to prevent buffer deadlock

**JSON-RPC Protocol:**
- Request IDs: monotonically increasing integer per client instance
- Requests: `{"jsonrpc": "2.0", "id": N, "method": "...", "params": {...}}`
- Notifications: `{"jsonrpc": "2.0", "method": "...", "params": {...}}` (no id)
- Response matching: match response `id` to pending request
- Timeout: `threading.Timer` or `select` with deadline per read

**Tool Wrappers:**
Each tool method builds the correct `tools/call` envelope:
```
{"jsonrpc": "2.0", "id": N, "method": "tools/call",
 "params": {"name": "context_store", "arguments": {...}}}
```
Parse response, extract `content[0].text`, optionally JSON-parse the text.

### C3: Test Data Generation (`harness/generators.py`)

Deterministic factories using seeded `random.Random` instances.

**Key generators:**
- `make_entry(**overrides)` — single entry with realistic defaults
- `make_entries(n, ...)` — batch with topic/category distribution
- `make_contradicting_pair(topic)` — semantically similar, conflicting content
- `make_correction_chain(depth)` — chain of corrections
- `make_injection_payloads()` — prompt injection test vectors
- `make_pii_content()` — PII detection test cases
- `make_unicode_edge_cases()` — Unicode boundary cases
- `make_bulk_dataset(n, seed)` — large dataset for volume tests

**Seed management:**
- Each generator accepts optional `seed` parameter
- Default seed is deterministic per test module
- On test failure, the seed is logged for reproduction
- Volume tests use explicit seeds documented in test code

### C4: Assertion Helpers (`harness/assertions.py`)

Response abstraction layer that isolates test code from raw JSON structure (addressing SR-07).

**Key abstractions:**
- `assert_tool_success(response)` — verify no error, return parsed content
- `assert_tool_error(response, expected_code)` — verify error with code
- `assert_entry_has(response, field, value)` — field-level entry assertions
- `assert_search_contains(response, entry_id)` — entry found in search results
- `assert_search_not_contains(response, entry_id)` — entry absent from results
- `parse_entry(response)` — extract entry dict from tool response
- `parse_entries(response)` — extract entry list from search/lookup response

### C5: pytest Fixtures (`harness/conftest.py`)

**`server` fixture (function-scoped, default):**
- Creates temp directory via `tmp_path`
- Spawns `UnimatrixClient(binary_path, project_dir=tmp_path)`
- Calls `client.initialize()`
- Yields client to test
- On teardown: `client.shutdown()`, captures stderr to log file

**`shared_server` fixture (module-scoped):**
- For volume/lifecycle suites where state accumulation is the test
- One server per test module
- Uses `tmp_path_factory` for unique directory

**`populated_server` fixture (function-scoped):**
- Wraps `server` fixture
- Pre-loads standard dataset (50 entries, 5 topics, 3 categories)
- Returns client with data ready for query testing

**Binary path resolution:**
- `UNIMATRIX_BINARY` environment variable (Docker sets this)
- Fallback: search `target/release/unimatrix-server` from workspace root

### C6: Test Suites (`suites/test_*.py`)

Eight modules, each focused on one validation concern:

| Module | Fixture | Focus |
|--------|---------|-------|
| `test_protocol.py` | `server` | MCP handshake, JSON-RPC compliance |
| `test_tools.py` | `server` | All 9 tools, every parameter path |
| `test_lifecycle.py` | `shared_server` + `server` | Multi-step knowledge flows |
| `test_volume.py` | `shared_server` | Scale to 5K entries |
| `test_security.py` | `server` | Content scanning, capabilities, input validation |
| `test_confidence.py` | `server` | 6-factor formula, re-ranking |
| `test_contradiction.py` | `server` | Detection pipeline, quarantine |
| `test_edge_cases.py` | `server` | Unicode, boundaries, restart persistence |

### C7: Static Fixtures (`fixtures/`)

JSON files with pre-built test data:
- `injection_patterns.json` — ~50 prompt injection payloads
- `pii_samples.json` — PII detection test cases with expected types
- `unicode_corpus.json` — Unicode edge cases (CJK, RTL, emoji, ZWJ, combining)
- `large_entries.json` — Near-max-size payloads (100KB, 500KB, ~1MB)

### C8: Runner Scripts (`scripts/`)

**`run.sh`:**
- Parse `TEST_SUITE` env var (default: `all`)
- Map suite names to pytest paths (`protocol` -> `suites/test_protocol.py`)
- Support comma-separated suite selection
- Pass through `PYTEST_ARGS` for marker selection
- Execute pytest with JUnit XML, JSON report, and timeout options
- Capture exit code
- Call `report.sh` to generate summary

**`report.sh`:**
- Parse `report.json` for per-suite pass/fail/error counts
- Generate `summary.txt` with formatted table
- Exit with original pytest exit code

## Data Flow

### Test Execution Flow

```
1. docker compose up --build
2. Stage 1: cargo build --release && cargo test --lib
3. Stage 2: copy binary, install deps, pre-download model
4. run.sh parses TEST_SUITE, invokes pytest
5. For each test:
   a. Fixture creates temp dir
   b. Fixture spawns unimatrix-server --project-dir <tmpdir>
   c. Fixture completes MCP initialize handshake
   d. Test calls tool methods on client
   e. Client sends JSON-RPC over stdin, reads response from stdout
   f. Test asserts on parsed response
   g. Fixture shuts down server (shutdown request + SIGTERM + cleanup)
6. pytest writes junit.xml + report.json
7. report.sh writes summary.txt
8. Container exits with pytest exit code
```

### MCP Protocol Flow (per test)

```
Client                          Server
  |-- initialize request -------->|
  |<---- initialize response -----|
  |-- initialized notification -->|
  |                               |
  |-- tools/call (context_*) ---->|
  |<---- tool result -------------|
  |   (repeat for each tool call) |
  |                               |
  |-- shutdown request ---------->|
  |<---- shutdown response -------|
  |   (server process exits)      |
```

## Technology Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Test language | Python 3.12 + pytest | Black-box testing via subprocess; pytest fixtures map to server lifecycle; no Rust coupling |
| Container | Docker multi-stage | Hermetic build + test environment; CI-ready; reproducible |
| MCP transport | stdin/stdout subprocess | Only transport the server supports; matches real-world agent interaction |
| Data isolation | Unique tmpdir per test | Avoids redb exclusive lock conflicts; zero state leakage |
| Embedding model | Pre-downloaded in image | Offline reproducibility; no network at test time |
| Response parsing | Abstraction layer in assertions.py | Single point of change when response format evolves (SR-07) |
| Test selection | pytest markers + env var suite selection | Flexible: smoke (~30s), individual suite, full run |

## Constraints

- No modifications to server source code. The binary is tested as-is.
- All files live under `product/test/infra-001/`. No workspace-level changes.
- Python code only. No Rust in the harness.
- Deterministic tests. Seeded generators, no uncontrolled randomness.
- Offline test execution. All dependencies baked into Docker image.

## ADRs

- ADR-001: Response Abstraction Pattern — why tests use an assertion abstraction layer instead of raw JSON parsing
