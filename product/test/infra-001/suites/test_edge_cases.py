"""Suite 8: Edge Cases (~25 tests).

Unicode handling, boundary values, empty database operations,
concurrent operations, server restart persistence.
"""

import json
import pytest
from pathlib import Path
from harness.assertions import (
    assert_tool_success,
    assert_tool_error,
    extract_entry_id,
    parse_entry,
    parse_entries,
    parse_status_report,
)
from harness.generators import make_unicode_edge_cases, load_large_content
from harness.client import UnimatrixClient
from harness.conftest import get_binary_path


FIXTURES_DIR = Path(__file__).resolve().parent.parent / "fixtures"


# === Unicode Tests ====================================================

@pytest.mark.smoke
def test_unicode_cjk_roundtrip(server):
    """E-01: CJK Chinese content stored and retrieved."""
    content = "\u4f60\u597d\u4e16\u754c - Chinese greeting and technical decision"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    assert "\u4f60\u597d" in entry.get("content", "")


def test_unicode_japanese_roundtrip(server):
    """E-02: Japanese content stored and retrieved."""
    content = "\u3053\u3093\u306b\u3061\u306f\u4e16\u754c - Japanese testing convention"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    assert "\u3053\u3093\u306b\u3061\u306f" in entry.get("content", "")


def test_unicode_korean_roundtrip(server):
    """E-03: Korean content stored and retrieved."""
    content = "\uc548\ub155\ud558\uc138\uc694 \uc138\uacc4 - Korean convention"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_unicode_rtl_arabic_roundtrip(server):
    """E-04: Arabic RTL content stored and retrieved."""
    content = "\u0645\u0631\u062d\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645 - Arabic RTL convention"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_unicode_emoji_roundtrip(server):
    """E-05: Emoji content stored and retrieved."""
    content = "\U0001f600\U0001f680\U0001f4bb - Emoji convention for testing"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_unicode_zwj_roundtrip(server):
    """E-06: ZWJ sequence content stored and retrieved."""
    content = "\U0001f468\u200d\U0001f4bb\U0001f469\u200d\U0001f52c - ZWJ convention"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_unicode_combining_roundtrip(server):
    """E-07: Combining character content stored and retrieved."""
    content = "e\u0301 n\u0303 o\u0308 - Combining character convention"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


# === Empty Database Tests =============================================

@pytest.mark.smoke
def test_empty_database_operations(server):
    """E-08: All read tools return empty/zero on empty database."""
    # Search
    search_resp = server.context_search("anything", format="json")
    entries = parse_entries(search_resp)
    assert len(entries) == 0

    # Lookup
    lookup_resp = server.context_lookup(topic="testing", format="json")
    entries = parse_entries(lookup_resp)
    assert len(entries) == 0

    # Status
    status_resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(status_resp)

    # Briefing
    briefing_resp = server.context_briefing(
        "developer", "test task", agent_id="human"
    )
    assert_tool_success(briefing_resp)

    # Get nonexistent
    get_resp = server.context_get(1, format="json")
    assert_tool_error(get_resp)


# === Boundary Value Tests =============================================

def test_minimum_length_fields(server):
    """E-09: 1-char content and 1-char topic."""
    resp = server.context_store("x", "y", "convention", agent_id="human")
    assert_tool_success(resp)


def test_maximum_length_topic(server):
    """E-10: Topic at 100 characters."""
    long_topic = "t" * 100
    resp = server.context_store(
        "max topic length test", long_topic, "convention", agent_id="human"
    )
    # Server may accept or reject; key is no crash
    assert resp.result is not None or resp.error is not None


def test_ten_tags(server):
    """E-11: Entry with 10 tags."""
    tags = [f"tag{i}" for i in range(10)]
    resp = server.context_store(
        "ten tags test", "testing", "convention", tags=tags, agent_id="human"
    )
    assert_tool_success(resp)


# === Concurrent/Sequential Tests ======================================

def test_concurrent_store_operations(server):
    """E-12: Sequential store operations all succeed."""
    topics = ["testing", "architecture", "deployment", "security", "performance",
              "database", "monitoring", "caching", "networking", "logging"]
    # crt-025: "outcome" retired from CategoryAllowlist; replaced with "procedure" (ADR-005)
    categories = ["convention", "pattern", "decision", "procedure"]
    ids = []
    for i in range(20):
        category = categories[i % len(categories)]
        kwargs = {"agent_id": "human", "format": "json"}
        resp = server.context_store(
            f"Sequential store operation number {i}: This entry covers {topics[i % len(topics)]} "
            f"with a completely distinct perspective on {category} "
            f"including unique identifier {i * 1000 + 42}",
            topics[i % len(topics)],
            category,
            **kwargs,
        )
        ids.append(extract_entry_id(resp))
    assert len(ids) == 20
    assert len(set(ids)) == 20, "All IDs should be unique"


@pytest.mark.smoke
def test_restart_persistence(tmp_path):
    """E-13: Data persists across server restart."""
    binary = get_binary_path()

    # First session: store entry
    client1 = UnimatrixClient(binary, project_dir=str(tmp_path))
    client1.initialize()
    client1.wait_until_ready()
    store_resp = client1.context_store(
        "restart persistence edge case test xyz",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    client1.shutdown()

    # Second session: verify entry exists
    client2 = UnimatrixClient(binary, project_dir=str(tmp_path))
    client2.initialize()
    client2.wait_until_ready()
    get_resp = client2.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    assert "restart persistence" in entry.get("content", "")
    client2.shutdown()


def test_interleaved_store_and_search(server):
    """E-14: Interleaved store and search operations."""
    for i in range(10):
        server.context_store(
            f"interleaved entry {i} about testing",
            "testing",
            "convention",
            agent_id="human",
        )
        server.context_search(f"interleaved entry {i}", k=2)


@pytest.mark.xfail(reason="Pre-existing: GH#576 — content size cap of 8000 bytes (fix #561) now rejects 50KB content; test predates the cap")
def test_very_long_content(server):
    """E-15: Near-boundary-length content."""
    content = load_large_content(50000)  # 50KB
    resp = server.context_store(
        content, "testing", "convention", agent_id="human"
    )
    assert_tool_success(resp)


def test_special_characters_in_query(server):
    """E-16: Special characters in search query."""
    server.context_store(
        "special chars test content", "testing", "convention", agent_id="human"
    )
    resp = server.context_search("special chars 'test' \"content\" (with) [brackets]")
    assert_tool_success(resp)


def test_special_characters_in_topic(server):
    """E-17: Special characters in topic."""
    resp = server.context_store(
        "special topic content",
        "api-design",
        "convention",
        agent_id="human",
    )
    assert_tool_success(resp)


def test_special_characters_in_tags(server):
    """E-18: Special characters in tags."""
    resp = server.context_store(
        "special tags content",
        "testing",
        "convention",
        tags=["tag-with-dash", "tag_with_underscore"],
        agent_id="human",
    )
    assert_tool_success(resp)


def test_empty_tags_array(server):
    """E-19: Empty tags array."""
    resp = server.context_store(
        "empty tags content",
        "testing",
        "convention",
        tags=[],
        agent_id="human",
    )
    assert_tool_success(resp)


@pytest.mark.slow
@pytest.mark.xfail(reason="Pre-existing: GH#111 — rate limit blocks rapid sequential stores")
def test_100_rapid_sequential_stores(server):
    """E-21: 100 rapid sequential stores (R-09 stress test)."""
    for i in range(100):
        resp = server.context_store(
            f"rapid store {i} content", "testing", "convention", agent_id="human"
        )
        assert_tool_success(resp)


def test_all_formats_store_search_get(server):
    """E-22: All formats x store/search/get/lookup."""
    for fmt in ["summary", "markdown", "json"]:
        store_resp = server.context_store(
            f"format test {fmt}",
            "testing",
            "convention",
            agent_id="human",
            format=fmt,
        )
        assert_tool_success(store_resp)

    # Search in all formats
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_search("format test", format=fmt)
        assert_tool_success(resp)


def test_mixed_rtl_ltr_content(server):
    """E-23: Mixed RTL/LTR content roundtrip."""
    content = "Hello \u0645\u0631\u062d\u0628\u0627 World \u4e16\u754c mixed direction"
    resp = server.context_store(
        content, "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


@pytest.mark.smoke
def test_server_process_cleanup(tmp_path):
    """E-24: Server process cleaned up after shutdown."""
    binary = get_binary_path()
    client = UnimatrixClient(binary, project_dir=str(tmp_path))
    client.initialize()
    pid = client.pid
    assert pid is not None, "Server should have a PID while running"
    client.shutdown()
    assert client._process.poll() is not None, "Server process should have exited"


def test_store_with_source_roundtrip(server):
    """E-25: Store with source field accepted without error."""
    resp = server.context_store(
        "source roundtrip content",
        "testing",
        "convention",
        source="test-harness-v1",
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp)
    entry_id = extract_entry_id(resp)
    get_resp = server.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    # Source is accepted as a parameter but may not be exposed in the JSON response
    assert entry.get("content") == "source roundtrip content"
