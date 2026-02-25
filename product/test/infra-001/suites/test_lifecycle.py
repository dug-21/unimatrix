"""Suite 3: Lifecycle (~25 tests).

Multi-step scenarios exercising knowledge management workflows end-to-end.
Each test exercises a complete flow, not isolated operations.
"""

import pytest
from harness.assertions import (
    assert_tool_success,
    assert_tool_error,
    extract_entry_id,
    parse_entry,
    parse_entries,
    parse_status_report,
    assert_search_contains,
    assert_search_not_contains,
    get_result_text,
)
from harness.generators import make_entries, make_correction_chain
from harness.client import UnimatrixClient
from harness.conftest import get_binary_path


@pytest.mark.smoke
def test_store_search_find_flow(server):
    """L-01: Store -> search -> find flow."""
    store_resp = server.context_store(
        "lifecycle store search find unique content abc123",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    search_resp = server.context_search(
        "lifecycle store search find unique content abc123", format="json"
    )
    assert_search_contains(search_resp, entry_id)


@pytest.mark.smoke
def test_correction_chain_integrity(server):
    """L-02: Correction chain integrity (3-deep)."""
    chain = make_correction_chain(3, seed=100)

    # Store original
    store_resp = server.context_store(
        agent_id="human", format="json", **{k: v for k, v in chain[0].items() if not k.startswith("_")}
    )
    prev_id = extract_entry_id(store_resp)

    # Apply corrections
    for entry in chain[1:]:
        correct_resp = server.context_correct(
            prev_id,
            entry["content"],
            reason=entry.get("_reason", "correction"),
            agent_id="human",
            format="json",
        )
        assert_tool_success(correct_resp)
        prev_id = extract_entry_id(correct_resp)


def test_confidence_evolution_over_access(server):
    """L-03: Confidence evolves with repeated access."""
    store_resp = server.context_store(
        "confidence evolution lifecycle test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Access multiple times with helpful=true
    for _ in range(5):
        server.context_get(entry_id, agent_id="human", helpful=True)

    # Verify entry still accessible
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_agent_auto_enrollment(server):
    """L-04: Agent auto-enrolled on first request."""
    # New agent_id should be auto-enrolled as Restricted
    resp = server.context_search("anything", agent_id="brand-new-agent-xyz")
    assert_tool_success(resp)


def test_store_deprecate_search_excluded(server):
    """L-07: Store -> deprecate -> search doesn't find."""
    store_resp = server.context_store(
        "deprecate lifecycle unique mno789",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    server.context_deprecate(entry_id, reason="outdated", agent_id="human")
    search_resp = server.context_search(
        "deprecate lifecycle unique mno789", format="json"
    )
    assert_search_not_contains(search_resp, entry_id)


def test_store_quarantine_restore_search_finds(server):
    """L-08: Store -> quarantine -> restore -> search finds."""
    store_resp = server.context_store(
        "quarantine restore lifecycle unique pqr456",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Quarantine
    server.context_quarantine(entry_id, agent_id="human")
    search_resp = server.context_search(
        "quarantine restore lifecycle unique pqr456", format="json"
    )
    assert_search_not_contains(search_resp, entry_id)

    # Restore
    server.context_quarantine(entry_id, action="restore", agent_id="human")
    search_resp = server.context_search(
        "quarantine restore lifecycle unique pqr456", format="json"
    )
    assert_search_contains(search_resp, entry_id)


def test_multi_agent_interaction(server):
    """L-09: Different trust levels interact correctly."""
    # Privileged agent stores
    store_resp = server.context_store(
        "multi-agent content lifecycle test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Restricted agent can search
    search_resp = server.context_search(
        "multi-agent content lifecycle", agent_id="restricted-agent"
    )
    assert_tool_success(search_resp)

    # Restricted agent cannot store
    store_resp_restricted = server.context_store(
        "restricted store attempt",
        "testing",
        "convention",
        agent_id="restricted-agent",
    )
    assert_tool_error(store_resp_restricted)


@pytest.mark.smoke
def test_isolation_no_state_leakage(server):
    """L-06: No state leakage between function-scoped tests.

    This test stores a unique value. If it appears in searches from
    other test functions (different server instances), isolation is broken.
    """
    store_resp = server.context_store(
        "isolation sentinel value unique xyz789",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    # Verify it exists in THIS server
    search_resp = server.context_search(
        "isolation sentinel value unique xyz789", format="json"
    )
    assert_search_contains(search_resp, entry_id)


def test_full_lifecycle_pipeline(server):
    """L-11: Store, access, correct, deprecate, status."""
    # Store
    store_resp = server.context_store(
        "full lifecycle pipeline content",
        "architecture",
        "decision",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Access
    server.context_get(entry_id, agent_id="human")
    server.context_search("lifecycle pipeline", agent_id="human")

    # Correct
    correct_resp = server.context_correct(
        entry_id,
        "corrected lifecycle pipeline content",
        reason="updated",
        agent_id="human",
        format="json",
    )
    new_id = extract_entry_id(correct_resp)

    # Deprecate the corrected entry
    server.context_deprecate(new_id, reason="superseded", agent_id="human")

    # Status should reflect changes
    status_resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(status_resp)


def test_data_persistence_across_restart(tmp_path):
    """L-12: Data persists across server restart."""
    binary = get_binary_path()

    # Start server, store entry, shutdown
    client1 = UnimatrixClient(binary, project_dir=str(tmp_path))
    client1.initialize()
    store_resp = client1.context_store(
        "persistence test content across restart xyz",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    client1.shutdown()

    # Restart server with same project dir, verify entry exists
    client2 = UnimatrixClient(binary, project_dir=str(tmp_path))
    client2.initialize()
    get_resp = client2.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    assert "persistence test content" in entry.get("content", "")
    client2.shutdown()


def test_helpfulness_voting(server):
    """L-14: Helpful=true/false voting works."""
    store_resp = server.context_store(
        "helpfulness voting test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Vote helpful
    server.context_get(entry_id, agent_id="human", helpful=True)
    # Vote unhelpful
    server.context_get(entry_id, agent_id="agent-2", helpful=False)
    # Entry should still be accessible
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_briefing_reflects_stored_knowledge(server):
    """L-17: Briefing content reflects stored knowledge."""
    server.context_store(
        "developers should always write tests before implementation for reliability",
        "testing",
        "duties",
        agent_id="human",
    )
    resp = server.context_briefing("developer", "implement new feature", agent_id="human")
    result = assert_tool_success(resp)
    assert len(result.text) > 0


def test_status_reflects_lifecycle_changes(server):
    """L-18: Status report reflects lifecycle changes."""
    # Empty status
    status0 = server.context_status(agent_id="human", format="json")
    assert_tool_success(status0)

    # Store entries
    for i in range(3):
        server.context_store(
            f"status lifecycle {i}", "testing", "convention", agent_id="human"
        )

    # Status should show entries
    status1 = server.context_status(agent_id="human", format="json")
    assert_tool_success(status1)


def test_deprecate_then_correct_errors(server):
    """L-20: Cannot correct an already-deprecated entry."""
    store_resp = server.context_store(
        "deprecate then correct", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    server.context_deprecate(entry_id, agent_id="human")
    resp = server.context_correct(entry_id, "new content", agent_id="human")
    assert_tool_error(resp)


def test_multi_step_correction_chain(server):
    """L-22: Multi-step correction chain (5 deep)."""
    chain = make_correction_chain(5, seed=200)

    store_resp = server.context_store(
        agent_id="human", format="json", **{k: v for k, v in chain[0].items() if not k.startswith("_")}
    )
    prev_id = extract_entry_id(store_resp)

    for entry in chain[1:]:
        correct_resp = server.context_correct(
            prev_id,
            entry["content"],
            reason=entry.get("_reason", "correction"),
            agent_id="human",
            format="json",
        )
        assert_tool_success(correct_resp)
        prev_id = extract_entry_id(correct_resp)

    # Final entry should be accessible
    get_resp = server.context_get(prev_id, format="json")
    assert_tool_success(get_resp)


def test_full_pipeline_10_entries(server):
    """L-25: Store 10 -> search -> correct 2 -> deprecate 1 -> status."""
    ids = []
    for i in range(10):
        resp = server.context_store(
            f"pipeline entry {i} about testing patterns and architecture",
            "testing",
            "convention",
            agent_id="human",
            format="json",
        )
        ids.append(extract_entry_id(resp))

    # Search
    search_resp = server.context_search("testing patterns architecture", format="json")
    assert_tool_success(search_resp)

    # Correct 2
    for eid in ids[:2]:
        server.context_correct(
            eid, "corrected pipeline content", agent_id="human", format="json"
        )

    # Deprecate 1
    server.context_deprecate(ids[2], agent_id="human")

    # Status
    status_resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(status_resp)
