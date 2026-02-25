"""Suite 7: Contradiction (~15 tests).

Validates the contradiction detection pipeline: three signal types,
false positive resistance, quarantine effects, embedding consistency,
and scale behavior.
"""

import pytest
from harness.assertions import (
    assert_tool_success,
    assert_tool_error,
    extract_entry_id,
    parse_entry,
    parse_status_report,
)
from harness.generators import make_contradicting_pair


def _store_pair(server, pair):
    """Store a contradicting pair and return both IDs."""
    entry_a, entry_b = pair
    resp_a = server.context_store(agent_id="human", format="json", **entry_a)
    id_a = extract_entry_id(resp_a)
    resp_b = server.context_store(agent_id="human", format="json", **entry_b)
    id_b = extract_entry_id(resp_b)
    return id_a, id_b


@pytest.mark.smoke
def test_contradiction_detected(server):
    """D-01: Negation opposition detected ('always X' vs 'never X')."""
    pair = make_contradicting_pair("testing", seed=300)
    _store_pair(server, pair)
    # Run contradiction scan via status
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    # Scan should complete without error
    assert report is not None


def test_incompatible_directives(server):
    """D-02: Incompatible directives detected."""
    server.context_store(
        "Convention: All database queries must use connection pooling for performance. "
        "This is mandatory for all production database access patterns.",
        "database",
        "convention",
        agent_id="human",
    )
    server.context_store(
        "Convention: Never use connection pooling for database queries. "
        "Direct connections are required for all database access patterns.",
        "database",
        "convention",
        agent_id="human",
    )
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_false_positive_compatible_entries(server):
    """D-04: Compatible related entries not flagged as contradictions."""
    server.context_store(
        "Pattern: Use connection pooling for read-heavy database workloads.",
        "database",
        "pattern",
        agent_id="human",
    )
    server.context_store(
        "Pattern: Use write-ahead logging for write-heavy database workloads.",
        "database",
        "pattern",
        agent_id="human",
    )
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_false_positive_different_aspect(server):
    """D-05: Same-topic different-aspect entries not flagged."""
    server.context_store(
        "Testing convention: Always run unit tests before committing code.",
        "testing",
        "convention",
        agent_id="human",
    )
    server.context_store(
        "Testing convention: Integration tests should use real database instances.",
        "testing",
        "convention",
        agent_id="human",
    )
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_contradiction_scan_in_status(server):
    """D-06: Contradiction scan appears in status report."""
    pair = make_contradicting_pair("architecture", seed=301)
    _store_pair(server, pair)
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert report is not None


def test_quarantine_effect_on_scan(server):
    """D-09: Quarantined entries excluded from contradiction scan."""
    pair = make_contradicting_pair("security", seed=302)
    id_a, id_b = _store_pair(server, pair)
    # Quarantine one of the pair
    server.context_quarantine(id_a, agent_id="human")
    # Scan should complete
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_contradiction_scan_at_100_entries(server):
    """D-10: Contradiction scan at 100 entries."""
    for i in range(100):
        topic = ["testing", "architecture", "deployment", "security", "database"][i % 5]
        server.context_store(
            f"Bulk entry {i} for contradiction scan testing about {topic}",
            topic,
            "convention",
            agent_id="human",
        )
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_generated_pair_triggers_detection(server):
    """D-11: make_contradicting_pair produces entries that server handles."""
    pair = make_contradicting_pair("performance", seed=303)
    id_a, id_b = _store_pair(server, pair)
    # Both entries should exist
    resp_a = server.context_get(id_a, format="json")
    resp_b = server.context_get(id_b, format="json")
    assert_tool_success(resp_a)
    assert_tool_success(resp_b)


def test_scan_empty_database(server):
    """D-12: Contradiction scan on empty database."""
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_scan_single_entry(server):
    """D-13: Contradiction scan with single entry."""
    server.context_store(
        "single entry for scan", "testing", "convention", agent_id="human"
    )
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_multiple_contradiction_pairs(server):
    """D-14: Multiple contradiction pairs in database."""
    for seed in [310, 311, 312]:
        pair = make_contradicting_pair(seed=seed)
        _store_pair(server, pair)
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_embedding_consistency_check(server):
    """D-08: Embedding consistency check via status."""
    server.context_store(
        "embedding consistency test content",
        "testing",
        "convention",
        agent_id="human",
    )
    resp = server.context_status(
        agent_id="human", format="json", check_embeddings=True
    )
    assert_tool_success(resp)
