# Pseudocode Overview: infra-001

## Component Interaction

```
Docker Build (C1)
  |-- builds unimatrix-server binary (Rust, release mode)
  |-- copies into Python test-runtime image
  |-- entrypoint: run.sh (C8)
       |
       v
pytest (C8 invokes)
  |-- conftest.py (C5) creates fixtures
  |      |-- spawns UnimatrixClient (C2)
  |      |-- client.initialize() completes MCP handshake
  |      |-- yields client to test
  |      |-- teardown: client.shutdown()
  |
  |-- test_*.py (C6) uses fixtures
  |      |-- calls client.context_store(...) etc.
  |      |-- uses generators (C3) for test data
  |      |-- uses assertions (C4) for response validation
  |      |-- uses static fixtures (C7) for injection/PII/unicode data
  |
  |-- results written to /results/ (C8)
```

## Shared Types

All components use these shared data structures:

### MCPResponse (used by C2, consumed by C4)
```python
@dataclass
class MCPResponse:
    id: int                    # JSON-RPC request ID
    result: dict | None        # Successful result (tools/call result envelope)
    error: dict | None         # Error envelope if request failed
    raw: dict                  # Full raw JSON-RPC response
```

### ToolResult (parsed from MCPResponse by C4)
```python
@dataclass
class ToolResult:
    content: list[dict]        # MCP content array [{type: "text", text: "..."}]
    is_error: bool             # Whether the tool reported an error
    text: str                  # Extracted text from content[0].text
    parsed: dict | None        # JSON-parsed text (if format=json), else None
```

### Entry dict (produced by C3, asserted by C4)
```python
entry = {
    "content": str,            # required
    "topic": str,              # required
    "category": str,           # required, from allowlist
    "title": str | None,       # optional
    "tags": list[str] | None,  # optional, max 10
    "source": str | None,      # optional
}
```

## Data Flow

1. Test function requests data from generator (C3) or static fixture (C7)
2. Test calls `client.context_store(...)` which returns MCPResponse
3. Test passes response to assertion helper (C4): `assert_tool_success(response)`
4. Assertion helper parses response into ToolResult, extracts entry data
5. Test makes further calls (search, lookup, get) and asserts on results

## Component Dependencies

```
C3 (generators) -----> independent
C7 (static fixtures) -> independent
C2 (client) ----------> independent (core subprocess/JSON-RPC)
C4 (assertions) ------> depends on C2 response format (MCPResponse)
C5 (fixtures) --------> depends on C2 (UnimatrixClient)
C1 (docker) ----------> depends on all harness files existing
C8 (runner) ----------> depends on C1 (runs inside container)
C6 (test suites) -----> depends on C2, C3, C4, C5, C7
```

## Module Layout

```
product/test/infra-001/
    harness/__init__.py        # exports UnimatrixClient, generators, assertions
    harness/client.py          # C2
    harness/generators.py      # C3
    harness/assertions.py      # C4
    harness/conftest.py        # C5 (auto-loaded by pytest)
    suites/conftest.py         # re-exports harness fixtures for suites/
    suites/test_protocol.py    # C6: Suite 1
    suites/test_tools.py       # C6: Suite 2
    suites/test_lifecycle.py   # C6: Suite 3
    suites/test_volume.py      # C6: Suite 4
    suites/test_security.py    # C6: Suite 5
    suites/test_confidence.py  # C6: Suite 6
    suites/test_contradiction.py # C6: Suite 7
    suites/test_edge_cases.py  # C6: Suite 8
    fixtures/*.json            # C7
    Dockerfile                 # C1
    docker-compose.yml         # C1
    pytest.ini                 # C8
    scripts/run.sh             # C8
    scripts/report.sh          # C8
```

## Key Protocol Details (from server source)

- Server name: "unimatrix"
- Server uses rmcp library, stdio transport
- Server logs to stderr (tracing_subscriber), MCP protocol on stdout
- CLI: `unimatrix-server --project-dir <path>` (optional `--verbose`)
- 9 tools: context_{search, lookup, get, store, correct, deprecate, status, briefing, quarantine}
- Categories: outcome, lesson-learned, decision, convention, pattern, procedure, duties, reference
- Trust levels: System > Privileged > Internal > Restricted
- Default agents: "system" (System), "human" (Privileged)
- Unknown agents auto-enroll as Restricted (Read + Search only)
- Content scanning: injection patterns (5 categories) + PII patterns (4 categories)
