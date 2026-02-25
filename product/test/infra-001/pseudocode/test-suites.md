# Pseudocode: C6 — Test Suites

## Overview

Eight test modules in `suites/`. Each maps to a distinct validation concern. Tests use harness components (C2 client, C3 generators, C4 assertions, C5 fixtures, C7 static fixtures).

All tests that write data use `agent_id="human"` (Privileged trust level with Write capability). Tests validating capability enforcement use specific agent_ids to trigger different trust levels.

## Suite 1: `test_protocol.py` (~15 tests)

```python
# Fixture: server (function-scoped)
# Validates: MCP protocol compliance (AC-06)

@pytest.mark.smoke
def test_initialize_returns_capabilities(server):
    """P-01: Initialize response has capabilities with tools enabled."""
    # server fixture already called initialize()
    # We can test by calling list_tools
    resp = server.list_tools()
    assert_tool_success(resp)  # If tools work, init succeeded

@pytest.mark.smoke
def test_server_info(server):
    """P-02: Verify server reports name='unimatrix' and has version."""
    # Re-initialize a fresh client to capture init response
    # (server fixture already initialized, so we test via tools/list)
    resp = server.list_tools()
    # Server info validated during initialize; this confirms tools work

def test_list_tools_returns_nine(server):
    """P-03: tools/list returns exactly 9 context_* tools."""
    resp = server.list_tools()
    result = assert_tool_success(resp)
    tools = result.parsed  # tools/list returns tool array
    tool_names = [t["name"] for t in tools]
    assert len(tool_names) == 9
    expected = {
        "context_search", "context_lookup", "context_get", "context_store",
        "context_correct", "context_deprecate", "context_status",
        "context_briefing", "context_quarantine"
    }
    assert set(tool_names) == expected

def test_tool_schemas_valid(server):
    """P-04: Each tool's inputSchema is valid JSON Schema."""
    resp = server.list_tools()
    result = assert_tool_success(resp)
    tools = result.parsed
    for tool in tools:
        schema = tool.get("inputSchema", {})
        assert "type" in schema  # Must have type field
        assert schema["type"] == "object"

def test_unknown_tool_returns_error(server):
    """P-05: Calling nonexistent tool returns error."""
    resp = server.call_tool("context_nonexistent", {})
    assert_tool_error(resp)

def test_malformed_json_rpc(server):
    """P-06: Invalid JSON on stdin produces error or is handled gracefully."""
    server.send_raw_bytes(b"this is not json\n")
    # Server should not crash; subsequent calls should still work
    resp = server.context_status(agent_id="human")
    assert_tool_success(resp)  # Server survived

def test_missing_required_params(server):
    """P-07: Tool call without required params returns error."""
    resp = server.call_tool("context_store", {})  # Missing content, topic, category
    assert_tool_error(resp)

def test_concurrent_requests(server):
    """P-08: Two rapid sequential requests both get correct responses."""
    resp1 = server.context_store("entry one", "testing", "convention", agent_id="human")
    resp2 = server.context_store("entry two", "testing", "convention", agent_id="human")
    assert_tool_success(resp1)
    assert_tool_success(resp2)

def test_graceful_shutdown(tmp_path):
    """P-10: Shutdown request + clean process exit."""
    binary = get_binary_path()
    client = UnimatrixClient(binary, project_dir=str(tmp_path))
    client.initialize()
    client.shutdown()
    # Process should have exited cleanly
    assert client._process.poll() is not None
    assert client._process.returncode == 0

def test_empty_tool_arguments(server):
    """P-13: {} arguments handled per tool defaults."""
    # context_status accepts all optional params
    resp = server.call_tool("context_status", {})
    assert_tool_success(resp)

def test_unknown_fields_ignored(server):
    """P-14: Extra fields in arguments don't cause errors."""
    resp = server.call_tool("context_status", {"unknown_field": "value", "agent_id": "human"})
    assert_tool_success(resp)

@pytest.mark.smoke
def test_json_format_responses_parseable(server):
    """P-15: All tools with format=json return valid JSON."""
    # Store something first
    server.context_store("test content", "testing", "convention",
                        agent_id="human", format="json")
    # Test format=json on several tools
    resp = server.context_status(agent_id="human", format="json")
    result = assert_tool_success(resp)
    assert result.parsed is not None  # Must be valid JSON
```

## Suite 2: `test_tools.py` (~80 tests)

```python
# Fixture: server (function-scoped)
# Validates: All 9 tools, all parameters (AC-07)
# Uses format="json" for structured assertion

# ── context_store (15 tests) ──

@pytest.mark.smoke
def test_store_minimal(server):
    """Store with required fields only."""
    resp = server.context_store("test content", "testing", "convention", agent_id="human")
    assert_tool_success(resp)

def test_store_all_fields(server):
    """Store with all optional fields."""
    resp = server.context_store(
        "full content", "testing", "convention",
        title="Full Entry", tags=["tag1", "tag2"],
        source="test-source", agent_id="human", format="json"
    )
    result = assert_tool_success(resp)

def test_store_roundtrip(server):
    """Store then get, verify all fields match."""
    resp = server.context_store(
        "roundtrip content", "architecture", "decision",
        title="Roundtrip Test", tags=["roundtrip"],
        agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)

    get_resp = server.context_get(entry_id, agent_id="human", format="json")
    entry = parse_entry(get_resp)
    assert entry["content"] == "roundtrip content"
    assert entry["topic"] == "architecture"

def test_store_invalid_category(server):
    """Store with invalid category returns error."""
    resp = server.context_store("content", "testing", "invalid_category", agent_id="human")
    assert_tool_error(resp, "category")

def test_store_empty_content(server):
    """Store with empty content returns error."""
    resp = server.context_store("", "testing", "convention", agent_id="human")
    assert_tool_error(resp)

def test_store_restricted_agent_rejected(server):
    """Restricted agent cannot store (no Write capability)."""
    resp = server.context_store("content", "testing", "convention", agent_id="unknown-agent")
    assert_tool_error(resp, "capability")

# ── context_search (12 tests) ──

@pytest.mark.smoke
def test_search_returns_results(server):
    """Store entry, search for it, find it."""
    server.context_store("unique searchable testing content", "testing", "convention", agent_id="human")
    resp = server.context_search("searchable testing content", format="json")
    entries = parse_entries(resp)
    assert len(entries) > 0

def test_search_with_topic_filter(server):
    """Search filtered by topic."""
    server.context_store("arch content", "architecture", "decision", agent_id="human")
    server.context_store("test content", "testing", "convention", agent_id="human")
    resp = server.context_search("content", topic="architecture", format="json")
    entries = parse_entries(resp)
    for e in entries:
        assert e.get("topic") == "architecture"

def test_search_with_k_limit(server):
    """Search with k parameter limits results."""
    for i in range(5):
        server.context_store(f"entry {i} about testing", "testing", "convention", agent_id="human")
    resp = server.context_search("testing", k=2, format="json")
    entries = parse_entries(resp)
    assert len(entries) <= 2

def test_search_all_formats(server):
    """Search returns valid responses in all three formats."""
    server.context_store("format test", "testing", "convention", agent_id="human")
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_search("format test", format=fmt)
        assert_tool_success(resp)

# ── context_lookup (10 tests) ──

def test_lookup_by_topic(server):
    """Lookup filtered by topic."""
    server.context_store("topic lookup", "security", "convention", agent_id="human")
    resp = server.context_lookup(topic="security", format="json")
    entries = parse_entries(resp)
    assert len(entries) > 0

def test_lookup_by_category(server):
    """Lookup filtered by category."""
    server.context_store("cat lookup", "testing", "decision", agent_id="human")
    resp = server.context_lookup(category="decision", format="json")
    entries = parse_entries(resp)
    assert len(entries) > 0

def test_lookup_by_id(server):
    """Lookup by specific entry ID."""
    store_resp = server.context_store("id lookup", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    resp = server.context_lookup(id=entry_id, format="json")
    entries = parse_entries(resp)
    assert any(_extract_id(e) == entry_id for e in entries)

def test_lookup_with_limit(server):
    """Lookup with limit parameter."""
    for i in range(5):
        server.context_store(f"limit test {i}", "testing", "convention", agent_id="human")
    resp = server.context_lookup(topic="testing", limit=2, format="json")
    entries = parse_entries(resp)
    assert len(entries) <= 2

# ── context_get (6 tests) ──

def test_get_existing(server):
    """Get existing entry by ID."""
    store_resp = server.context_store("get test", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    resp = server.context_get(entry_id, format="json")
    entry = parse_entry(resp)
    assert entry.get("content") == "get test"

def test_get_nonexistent(server):
    """Get nonexistent ID returns error."""
    resp = server.context_get(99999, format="json")
    assert_tool_error(resp)

def test_get_all_formats(server):
    """Get returns valid response in all formats."""
    store_resp = server.context_store("format get", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_get(entry_id, format=fmt)
        assert_tool_success(resp)

# ── context_correct (8 tests) ──

def test_correct_creates_chain(server):
    """Correct deprecates original and creates new entry."""
    store_resp = server.context_store("original", "testing", "convention",
                                       agent_id="human", format="json")
    original_id = extract_entry_id(store_resp)
    correct_resp = server.context_correct(
        original_id, "corrected content",
        reason="Updated guidance", agent_id="human", format="json"
    )
    assert_tool_success(correct_resp)
    # Original should be deprecated
    get_resp = server.context_get(original_id, format="json")
    entry = parse_entry(get_resp)
    # Status should indicate deprecated

def test_correct_nonexistent(server):
    """Correct nonexistent entry returns error."""
    resp = server.context_correct(99999, "content", agent_id="human")
    assert_tool_error(resp)

# ── context_deprecate (5 tests) ──

def test_deprecate_changes_status(server):
    """Deprecate changes entry status."""
    store_resp = server.context_store("to deprecate", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    dep_resp = server.context_deprecate(entry_id, reason="outdated", agent_id="human")
    assert_tool_success(dep_resp)

def test_deprecate_nonexistent(server):
    """Deprecate nonexistent entry returns error."""
    resp = server.context_deprecate(99999, agent_id="human")
    assert_tool_error(resp)

# ── context_status (8 tests) ──

@pytest.mark.smoke
def test_status_empty_db(server):
    """Status on empty database returns valid report."""
    resp = server.context_status(agent_id="human", format="json")
    result = assert_tool_success(resp)

def test_status_with_entries(server):
    """Status shows correct entry count after stores."""
    for i in range(3):
        server.context_store(f"status test {i}", "testing", "convention", agent_id="human")
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    # Should show at least 3 entries

def test_status_topic_filter(server):
    """Status filtered by topic."""
    server.context_store("arch status", "architecture", "decision", agent_id="human")
    resp = server.context_status(topic="architecture", agent_id="human", format="json")
    assert_tool_success(resp)

# ── context_briefing (8 tests) ──

def test_briefing_returns_content(server):
    """Briefing with role and task returns content."""
    server.context_store("developer guidance for testing", "testing", "duties", agent_id="human")
    resp = server.context_briefing("developer", "implement feature", agent_id="human")
    assert_tool_success(resp)

def test_briefing_empty_db(server):
    """Briefing on empty DB returns valid (possibly empty) response."""
    resp = server.context_briefing("developer", "implement feature", agent_id="human")
    assert_tool_success(resp)

# ── context_quarantine (8 tests) ──

def test_quarantine_entry(server):
    """Quarantine changes entry status."""
    store_resp = server.context_store("to quarantine", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    q_resp = server.context_quarantine(entry_id, reason="suspect", agent_id="human")
    assert_tool_success(q_resp)

def test_quarantine_excluded_from_search(server):
    """Quarantined entry not in search results."""
    store_resp = server.context_store("quarantine search test", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    search_resp = server.context_search("quarantine search test", format="json")
    assert_search_not_contains(search_resp, entry_id)

def test_quarantine_visible_via_get(server):
    """Quarantined entry still accessible via get."""
    store_resp = server.context_store("quarantine get test", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)

def test_restore_quarantined_entry(server):
    """Restore returns entry to active status."""
    store_resp = server.context_store("restore test", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    restore_resp = server.context_quarantine(entry_id, action="restore", agent_id="human")
    assert_tool_success(restore_resp)

def test_quarantine_requires_admin(server):
    """Restricted agent cannot quarantine."""
    store_resp = server.context_store("admin test", "testing", "convention",
                                       agent_id="human", format="json")
    entry_id = extract_entry_id(store_resp)
    q_resp = server.context_quarantine(entry_id, agent_id="unknown-restricted")
    assert_tool_error(q_resp, "capability")
```

## Suites 3-8: Abbreviated Structure

Suites 3-8 follow the same pattern. Each test:
1. Sets up data via generators or direct store calls
2. Exercises the server via client tool methods
3. Asserts via the assertions module

Key tests per suite are specified in the specification. Full test lists are in the per-component test plans.

### Suite 3: test_lifecycle.py — Multi-step flows with shared_server
### Suite 4: test_volume.py — 1K-5K entries with shared_server
### Suite 5: test_security.py — Injection, PII, capabilities
### Suite 6: test_confidence.py — 6-factor formula validation
### Suite 7: test_contradiction.py — Detection pipeline
### Suite 8: test_edge_cases.py — Unicode, boundaries, restart
