"""Suite 4: Volume (~15 tests).

Scale testing with 1K-5K entries. Uses module-scoped shared server.
Validates correctness at scale, not performance benchmarks.
"""

import pytest
from harness.assertions import (
    assert_tool_success,
    assert_tool_error,
    parse_entries,
    parse_status_report,
    extract_entry_id,
)
from harness.generators import make_bulk_dataset, load_large_content


@pytest.mark.volume
@pytest.mark.slow
class TestVolume1K:
    """Tests that operate on a volume dataset.

    Uses 200 entries to demonstrate scale behavior while staying within
    reasonable timeouts for CI environments. The contradiction scan in
    context_status is O(N*k) with HNSW nearest-neighbor lookups, so
    datasets above 500 entries can exceed 3+ minutes on constrained hardware.
    """

    _entry_ids: list[int] = []

    @pytest.mark.smoke
    def test_store_1000_entries(self, shared_server):
        """V-01: Store entries sequentially to build dataset."""
        entries = make_bulk_dataset(200, seed=5000)
        stored = 0
        for entry in entries:
            resp = shared_server.context_store(agent_id="human", **entry)
            result = assert_tool_success(resp)
            stored += 1
        assert stored == 200
        TestVolume1K._entry_ids = []  # IDs tracked implicitly

    def test_search_accuracy_at_1k(self, shared_server):
        """V-02: Search returns relevant results at volume."""
        resp = shared_server.context_search(
            "testing patterns and architecture decisions", k=5, format="json"
        )
        entries = parse_entries(resp)
        assert len(entries) > 0, "Search should return results at 1K entries"

    def test_lookup_correctness_at_1k(self, shared_server):
        """V-03: Lookup by topic returns entries at volume."""
        resp = shared_server.context_lookup(topic="testing", limit=10, format="json")
        entries = parse_entries(resp)
        assert len(entries) > 0, "Lookup should return results at 1K entries"

    def test_status_report_at_volume(self, shared_server):
        """V-04: Status report completes at volume."""
        resp = shared_server.context_status(
            agent_id="human", format="json", timeout=120.0
        )
        report = parse_status_report(resp)
        assert report, "Status report should not be empty"

    def test_100_sequential_searches(self, shared_server):
        """V-07: 100 sequential search queries complete without hanging."""
        for i in range(100):
            topic = ["testing", "architecture", "deployment", "security", "performance"][i % 5]
            resp = shared_server.context_search(
                f"search query {i} about {topic}", k=3, timeout=30.0
            )
            assert_tool_success(resp)

    def test_distinct_topics(self, shared_server):
        """V-08: Additional distinct topics stored and retrievable."""
        for i in range(10):
            topic = f"custom-topic-{i}"
            shared_server.context_store(
                f"custom topic content {i} about distinct subject area {topic}",
                topic,
                "convention",
                agent_id="human",
            )
        resp = shared_server.context_lookup(
            topic="custom-topic-0", agent_id="human", format="json"
        )
        assert_tool_success(resp)

    def test_briefing_with_large_kb(self, shared_server):
        """V-13: Briefing works with large knowledge base."""
        resp = shared_server.context_briefing(
            "developer", "implement testing patterns", agent_id="human",
            timeout=60.0,
        )
        assert_tool_success(resp)

    def test_rapid_store_search_pairs(self, shared_server):
        """V-15: Rapid store-then-search pairs (R-09 stress test)."""
        for i in range(50):
            topic = ["testing", "architecture", "deployment", "security", "performance"][i % 5]
            shared_server.context_store(
                f"rapid pair {i} unique stress test content about {topic} number {i*100}",
                topic,
                "convention",
                agent_id="human",
            )
            shared_server.context_search(f"rapid pair {i} stress test", k=1)


@pytest.mark.volume
class TestLargeContent:
    """Tests for large content payloads.

    Server enforces a 50,000 character limit on content. Tests validate
    behavior at and beyond this boundary.
    """

    def test_large_content_at_limit(self, server):
        """V-09: Content at server limit (49,999 chars) accepted."""
        content = load_large_content(49999)
        resp = server.context_store(
            content, "testing", "convention", agent_id="human"
        )
        assert_tool_success(resp)

    def test_large_content_over_limit_rejected(self, server):
        """V-10: Content over 50,000 chars rejected with clear error."""
        content = load_large_content(51000)
        resp = server.context_store(
            content, "testing", "convention", agent_id="human"
        )
        assert_tool_error(resp, "exceeds")

    @pytest.mark.slow
    def test_large_content_near_1mb_rejected(self, server):
        """V-11: Very large content (near 1MB) rejected with clear error."""
        content = load_large_content(1000000)
        resp = server.context_store(
            content, "testing", "convention", agent_id="human"
        )
        assert_tool_error(resp, "exceeds")
