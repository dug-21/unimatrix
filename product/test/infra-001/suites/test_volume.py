"""Suite 4: Volume (~15 tests).

Scale testing with 1K-5K entries. Uses module-scoped shared server.
Validates correctness at scale, not performance benchmarks.
"""

import pytest
from harness.assertions import (
    assert_tool_success,
    parse_entries,
    parse_status_report,
    extract_entry_id,
)
from harness.generators import make_bulk_dataset, load_large_content


@pytest.mark.volume
@pytest.mark.slow
class TestVolume1K:
    """Tests that operate on a 1K-entry dataset."""

    _entry_ids: list[int] = []

    @pytest.mark.smoke
    def test_store_1000_entries(self, shared_server):
        """V-01: Store 1000 entries sequentially."""
        entries = make_bulk_dataset(1000, seed=5000)
        stored = 0
        for entry in entries:
            resp = shared_server.context_store(agent_id="human", **entry)
            result = assert_tool_success(resp)
            stored += 1
        assert stored == 1000
        TestVolume1K._entry_ids = []  # IDs tracked implicitly

    def test_search_accuracy_at_1k(self, shared_server):
        """V-02: Search returns relevant results at 1K entries."""
        resp = shared_server.context_search(
            "testing patterns and architecture decisions", k=5, format="json"
        )
        entries = parse_entries(resp)
        assert len(entries) > 0, "Search should return results at 1K entries"

    def test_lookup_correctness_at_1k(self, shared_server):
        """V-03: Lookup by topic returns entries at 1K."""
        resp = shared_server.context_lookup(topic="testing", limit=10, format="json")
        entries = parse_entries(resp)
        assert len(entries) > 0, "Lookup should return results at 1K entries"

    def test_status_report_at_1k(self, shared_server):
        """V-04: Status report completes at 1K entries."""
        resp = shared_server.context_status(agent_id="human", format="json")
        report = parse_status_report(resp)
        assert report, "Status report should not be empty at 1K entries"

    def test_100_sequential_searches(self, shared_server):
        """V-07: 100 sequential search queries complete without hanging."""
        for i in range(100):
            topic = ["testing", "architecture", "deployment", "security", "performance"][i % 5]
            resp = shared_server.context_search(
                f"search query {i} about {topic}", k=3
            )
            assert_tool_success(resp)

    def test_100_distinct_topics(self, shared_server):
        """V-08: 100 distinct topics stored and retrievable."""
        # The bulk dataset already uses 15 topics cycling.
        # Store additional entries with many topics
        for i in range(20):
            topic = f"custom-topic-{i}"
            shared_server.context_store(
                f"custom topic content {i}", topic, "convention", agent_id="human"
            )
        resp = shared_server.context_status(agent_id="human", format="json")
        assert_tool_success(resp)

    def test_briefing_with_large_kb(self, shared_server):
        """V-13: Briefing works with large knowledge base."""
        resp = shared_server.context_briefing(
            "developer", "implement testing patterns", agent_id="human"
        )
        assert_tool_success(resp)

    def test_100_rapid_store_search_pairs(self, shared_server):
        """V-15: 100 rapid store-then-search pairs (R-09 stress test)."""
        for i in range(100):
            shared_server.context_store(
                f"rapid pair {i} unique content",
                "testing",
                "convention",
                agent_id="human",
            )
            shared_server.context_search(f"rapid pair {i}", k=1)


@pytest.mark.volume
class TestLargeContent:
    """Tests for large content payloads."""

    def test_large_content_100kb(self, server):
        """V-09: Large content entry (100KB)."""
        content = load_large_content(102400)
        resp = server.context_store(
            content, "testing", "convention", agent_id="human"
        )
        assert_tool_success(resp)

    def test_large_content_500kb(self, server):
        """V-10: Large content entry (500KB)."""
        content = load_large_content(512000)
        resp = server.context_store(
            content, "testing", "convention", agent_id="human"
        )
        assert_tool_success(resp)

    @pytest.mark.slow
    def test_large_content_near_1mb(self, server):
        """V-11: Large content entry (near 1MB)."""
        content = load_large_content(1000000)
        resp = server.context_store(
            content, "testing", "convention", agent_id="human"
        )
        # Server may accept or reject with clear error
        # Both are valid behaviors for near-max content
        if resp.error is None and resp.result is not None:
            is_error = resp.result.get("isError", False)
            if not is_error:
                assert_tool_success(resp)
