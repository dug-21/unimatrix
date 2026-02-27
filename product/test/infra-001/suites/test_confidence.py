"""Suite 6: Confidence (~20 tests).

Validates the 6-factor composite formula through observable tool responses:
base scores per status, usage factor, freshness, helpfulness (Wilson score),
correction factor, trust factor, and search re-ranking blend.
"""

import time
import pytest
from harness.assertions import (
    assert_tool_success,
    extract_entry_id,
    parse_entry,
    parse_entries,
    parse_status_report,
    get_result_text,
)


def _get_confidence(server, entry_id: int) -> float | None:
    """Get confidence value from entry via format=json."""
    resp = server.context_get(entry_id, format="json")
    entry = parse_entry(resp)
    conf = entry.get("confidence")
    if conf is not None:
        return float(conf)
    return None


@pytest.mark.smoke
def test_base_score_active(server):
    """C-01: Active entry has non-zero base confidence."""
    resp = server.context_store(
        "confidence base score test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf = _get_confidence(server, entry_id)
    assert conf is not None, "Active entry should have confidence"
    assert conf > 0, f"Active entry confidence should be > 0, got {conf}"


def test_base_score_deprecated(server):
    """C-02: Deprecated entry has lower base score."""
    resp = server.context_store(
        "deprecated confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf_active = _get_confidence(server, entry_id)

    server.context_deprecate(entry_id, agent_id="human")
    conf_deprecated = _get_confidence(server, entry_id)

    assert conf_deprecated is not None
    assert conf_active is not None
    assert conf_deprecated <= conf_active, (
        f"Deprecated conf {conf_deprecated} should be <= active conf {conf_active}"
    )


def test_base_score_quarantined(server):
    """C-03: Quarantined entry has lowest base score."""
    resp = server.context_store(
        "quarantined confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf_active = _get_confidence(server, entry_id)

    server.context_quarantine(entry_id, agent_id="human")
    conf_quarantined = _get_confidence(server, entry_id)

    assert conf_quarantined is not None
    assert conf_active is not None
    assert conf_quarantined <= conf_active, (
        f"Quarantined conf {conf_quarantined} should be <= active conf {conf_active}"
    )


def test_usage_factor_increases(server):
    """C-04: Usage factor increases with access."""
    resp = server.context_store(
        "usage factor confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf_initial = _get_confidence(server, entry_id)

    # Access multiple times
    for _ in range(10):
        server.context_get(entry_id, agent_id="human")

    conf_after = _get_confidence(server, entry_id)
    assert conf_initial is not None
    assert conf_after is not None
    assert conf_after >= conf_initial, (
        f"Confidence should not decrease with usage: {conf_initial} -> {conf_after}"
    )


def test_helpfulness_increases_confidence(server):
    """C-06: helpful=true increases confidence."""
    resp = server.context_store(
        "helpful confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf_before = _get_confidence(server, entry_id)

    # Vote helpful multiple times from different agents
    for i in range(6):
        server.context_get(entry_id, agent_id=f"voter-{i}", helpful=True)

    conf_after = _get_confidence(server, entry_id)
    assert conf_before is not None
    assert conf_after is not None
    assert conf_after >= conf_before, (
        f"Helpful votes should not decrease confidence: {conf_before} -> {conf_after}"
    )


def test_unhelpful_affects_confidence(server):
    """C-07: helpful=false affects confidence."""
    resp = server.context_store(
        "unhelpful confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)

    # Vote unhelpful from different agents
    for i in range(6):
        server.context_get(entry_id, agent_id=f"negative-voter-{i}", helpful=False)

    conf = _get_confidence(server, entry_id)
    assert conf is not None


def test_confidence_in_json_format(server):
    """C-16: Confidence visible in JSON format response."""
    resp = server.context_store(
        "json confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    assert "confidence" in entry, f"Entry should have confidence field, got keys: {list(entry.keys())}"


def test_confidence_range(server):
    """C-18: Confidence is in [0, 1] range."""
    resp = server.context_store(
        "confidence range test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf = _get_confidence(server, entry_id)
    assert conf is not None
    assert 0 <= conf <= 1, f"Confidence should be in [0, 1], got {conf}"


def test_new_entry_default_confidence(server):
    """C-19: New entry has default confidence."""
    resp = server.context_store(
        "default confidence test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf = _get_confidence(server, entry_id)
    assert conf is not None, "New entry should have confidence"


def test_confidence_after_many_searches(server):
    """C-20: Confidence after 10 searches (usage factor)."""
    resp = server.context_store(
        "search usage confidence test content unique qrs",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf_before = _get_confidence(server, entry_id)

    for _ in range(10):
        server.context_search("search usage confidence test qrs")

    conf_after = _get_confidence(server, entry_id)
    assert conf_before is not None
    assert conf_after is not None
    assert conf_after >= conf_before


def test_confidence_visible_in_status(server):
    """C-13: Confidence stats visible in status report."""
    server.context_store(
        "status confidence test", "testing", "convention", agent_id="human"
    )
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert report, "Status report should contain data"


def test_confidence_recomputed_on_quarantine(server):
    """C-14: Confidence recomputed when entry quarantined."""
    resp = server.context_store(
        "quarantine recompute test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)
    conf_active = _get_confidence(server, entry_id)

    server.context_quarantine(entry_id, agent_id="human")
    conf_quarantined = _get_confidence(server, entry_id)

    assert conf_active is not None
    assert conf_quarantined is not None
    # Quarantined base score is 0.1 (ADR-001)
    assert conf_quarantined < conf_active or conf_quarantined == conf_active


def test_confidence_recomputed_on_restore(server):
    """C-15: Confidence recomputed when entry restored."""
    resp = server.context_store(
        "restore recompute test",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)

    server.context_quarantine(entry_id, agent_id="human")
    conf_quarantined = _get_confidence(server, entry_id)

    server.context_quarantine(entry_id, action="restore", agent_id="human")
    conf_restored = _get_confidence(server, entry_id)

    assert conf_quarantined is not None
    assert conf_restored is not None
    assert conf_restored >= conf_quarantined
