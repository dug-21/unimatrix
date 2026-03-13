# col-022 Test Strategy Overview

## Test Approach

Three levels: unit tests (per-component), integration tests (cross-component within Rust), and MCP integration tests (infra-001 harness exercising the compiled binary).

Unit tests validate each component in isolation. Integration tests verify the cross-component data flows: hook-to-listener dispatch, session registry force-set persistence, and schema migration round-trips. MCP integration tests verify the tool is discoverable, callable, and produces correct responses through the JSON-RPC protocol.

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Level | Test Plan Section |
|---------|----------|--------------|------------|-------------------|
| R-01 | High | session.rs (set_feature_force) | Unit + Integration | mcp-tool, uds-listener |
| R-02 | High | hook.rs (build_request), validation.rs | Unit | hook-handler, shared-validation |
| R-03 | High | sessions.rs (session_from_row) | Unit + Integration | schema-migration |
| R-04 | Med | hook.rs + listener.rs (event_type constants) | Integration | hook-handler, uds-listener |
| R-05 | Med | migration.rs (v11->v12) | Unit | schema-migration |
| R-06 | Med | listener.rs (keywords JSON), sessions.rs | Unit + Integration | uds-listener, schema-migration |
| R-07 | Med | session.rs (set_feature_force concurrency) | Unit | uds-listener |
| R-08 | High | tools.rs (response format) | Unit | mcp-tool |
| R-09 | High | hook.rs (tool_name matching) | Unit | hook-handler |
| R-10 | Low | listener.rs (spawn_blocking error) | Unit | uds-listener |
| R-11 | Med | validation.rs (is_valid_feature_id) | Unit | shared-validation |
| R-12 | Med | listener.rs (cycle_stop observation) | Integration | uds-listener |

## Cross-Component Test Dependencies

1. **shared-validation <-> mcp-tool + hook-handler**: Both C1 and C2 call `validate_cycle_params()`. Validation tests live in shared-validation; callers test that they invoke it correctly.
2. **hook-handler -> uds-listener**: Hook builds RecordEvent; listener dispatches it. End-to-end event_type constant agreement tested via integration test in uds-listener.
3. **uds-listener -> schema-migration**: Listener calls `update_session_keywords()` which writes to the `keywords` column. Schema must be v12 for this to work. Round-trip tested in schema-migration.
4. **session.rs (set_feature_force) <- uds-listener**: Listener calls `set_feature_force`. Unit tests in session.rs; integration test in uds-listener verifies correct call.

## Integration Harness Plan (infra-001)

### Existing Suites to Run

| Suite | Reason |
|-------|--------|
| `smoke` (mandatory gate) | Any change at all |
| `tools` | New MCP tool `context_cycle` added -- tool discovery, parameter validation, response format |
| `protocol` | New tool registered -- tool list count changes from 11 to 12 |
| `lifecycle` | Feature attribution flow -- store/retrieve with session context |

### Gap Analysis

The existing `tools` suite covers all 9/10/11 existing tools but has no tests for `context_cycle`. The following MCP-visible behaviors are new and not covered by any existing suite:

1. `context_cycle` tool discovery (tool appears in `tools/list`)
2. `context_cycle(type: "start", topic: "col-022")` returns acknowledgment response
3. `context_cycle(type: "stop", topic: "col-022")` returns acknowledgment response
4. `context_cycle` with invalid `type` returns validation error
5. `context_cycle` with empty `topic` returns validation error
6. `context_cycle` with keywords round-trips correctly
7. `context_cycle` with >5 keywords truncates silently (response still succeeds)

### New Integration Tests to Add (Stage 3c)

Add to `suites/test_tools.py`:

```python
# Fixture: server (fresh DB, no state leakage)

def test_context_cycle_start_acknowledged(server):
    """AC-01, AC-05: context_cycle(start) returns acknowledgment."""
    result = server.call_tool("context_cycle", {"type": "start", "topic": "col-022"})
    assert result is not None
    # Response should be acknowledgment, not attribution confirmation

def test_context_cycle_stop_acknowledged(server):
    """AC-04, AC-05: context_cycle(stop) returns acknowledgment."""
    result = server.call_tool("context_cycle", {"type": "stop", "topic": "col-022"})
    assert result is not None

def test_context_cycle_invalid_type_error(server):
    """AC-07: Invalid type returns error."""
    result = server.call_tool("context_cycle", {"type": "pause", "topic": "col-022"})
    assert_is_error(result)

def test_context_cycle_empty_topic_error(server):
    """AC-06: Empty topic returns error."""
    result = server.call_tool("context_cycle", {"type": "start", "topic": ""})
    assert_is_error(result)

def test_context_cycle_with_keywords(server):
    """AC-13: Keywords accepted."""
    result = server.call_tool("context_cycle", {
        "type": "start", "topic": "col-022",
        "keywords": ["attribution", "lifecycle"]
    })
    assert result is not None

def test_context_cycle_keywords_truncated_to_five(server):
    """AC-13: >5 keywords silently truncated."""
    result = server.call_tool("context_cycle", {
        "type": "start", "topic": "col-022",
        "keywords": ["a", "b", "c", "d", "e", "f", "g"]
    })
    assert result is not None  # no error despite >5
```

Add to `suites/test_protocol.py` (or verify existing tool list count test):

```python
def test_tool_list_includes_context_cycle(server):
    """AC-01: context_cycle appears in tool list."""
    tools = server.list_tools()
    tool_names = [t["name"] for t in tools]
    assert "context_cycle" in tool_names
```

### Tests NOT Needed in Integration Harness

- Hook-side attribution (PreToolUse interception) -- not testable through MCP; tested via unit/integration tests in Rust
- `set_feature_force` behavior -- internal to session registry; unit tested
- Schema migration -- tested in Rust integration tests
- Keywords persistence to SQLite -- tested in Rust integration tests; not observable through MCP tools (no read-back path in col-022)
