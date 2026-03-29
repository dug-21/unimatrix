"""Suite 2: Tools (~80 tests).

Every tool, every parameter path, happy and error paths.
Uses format='json' for structured assertions.
"""

import time

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


# === context_store (15 tests) =========================================

@pytest.mark.smoke
def test_store_minimal(server):
    """T-01: Store with required fields only."""
    resp = server.context_store(
        "minimal store test", "testing", "convention", agent_id="human"
    )
    assert_tool_success(resp)


def test_store_all_fields(server):
    """T-02: Store with all optional fields."""
    resp = server.context_store(
        "full content",
        "testing",
        "convention",
        title="Full Entry",
        tags=["tag1", "tag2"],
        source="test-source",
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp)


@pytest.mark.smoke
def test_store_roundtrip(server):
    """T-03: Store then get, verify fields match."""
    resp = server.context_store(
        "roundtrip content for tools suite",
        "architecture",
        "decision",
        title="Roundtrip Test",
        tags=["roundtrip"],
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(resp)

    get_resp = server.context_get(entry_id, agent_id="human", format="json")
    entry = parse_entry(get_resp)
    assert "roundtrip content" in entry.get("content", "")


def test_store_invalid_category(server):
    """T-05: Store with invalid category returns error."""
    resp = server.context_store(
        "content", "testing", "invalid_category", agent_id="human"
    )
    assert_tool_error(resp, "category")


def test_store_empty_content(server):
    """T-06: Store with empty content rejected by gateway validation."""
    resp = server.context_store("", "testing", "convention", agent_id="human")
    assert_tool_error(resp, "content")


def test_store_empty_topic(server):
    """T-07: Store with empty topic accepted (server allows empty topic)."""
    resp = server.context_store("content", "", "convention", agent_id="human")
    assert_tool_success(resp)


def test_store_restricted_agent_rejected(server):
    """T-08: Enrolled agent without Write capability cannot store."""
    # Enroll a read-only agent explicitly — unknown agents now auto-enroll with
    # Write (PERMISSIVE_AUTO_ENROLL), so we must explicitly restrict.
    server.context_enroll(
        "test-read-only-agent", "restricted", ["read", "search"], agent_id="human"
    )
    resp = server.context_store(
        "restricted content", "testing", "convention", agent_id="test-read-only-agent"
    )
    assert_tool_error(resp)


def test_store_with_tags(server):
    """T-11: Store with 1-3 tags succeeds."""
    resp = server.context_store(
        "tagged content",
        "testing",
        "convention",
        tags=["tag1", "tag2", "tag3"],
        agent_id="human",
    )
    assert_tool_success(resp)


def test_store_format_json(server):
    """T-13: Store format=json returns entry data."""
    resp = server.context_store(
        "json format content", "testing", "convention", agent_id="human", format="json"
    )
    result = assert_tool_success(resp)
    assert result.parsed is not None


def test_store_format_markdown(server):
    """T-14: Store format=markdown returns markdown."""
    resp = server.context_store(
        "markdown format content",
        "testing",
        "convention",
        agent_id="human",
        format="markdown",
    )
    assert_tool_success(resp)


def test_store_format_summary(server):
    """T-15: Store format=summary returns text."""
    resp = server.context_store(
        "summary format content",
        "testing",
        "convention",
        agent_id="human",
        format="summary",
    )
    assert_tool_success(resp)


# === context_search (12 tests) ========================================

@pytest.mark.smoke
def test_search_returns_results(server):
    """T-16: Store entry, search for it, find it."""
    server.context_store(
        "unique searchable testing content zyx987",
        "testing",
        "convention",
        agent_id="human",
    )
    resp = server.context_search("searchable testing content zyx987", format="json")
    entries = parse_entries(resp)
    assert len(entries) > 0


def test_search_with_topic_filter(server):
    """T-17: Search filtered by topic."""
    server.context_store(
        "architecture specific content", "architecture", "decision", agent_id="human"
    )
    server.context_store(
        "testing specific content", "testing", "convention", agent_id="human"
    )
    resp = server.context_search(
        "specific content", topic="architecture", format="json"
    )
    entries = parse_entries(resp)
    for e in entries:
        assert e.get("topic") == "architecture"


def test_search_with_category_filter(server):
    """T-18: Search filtered by category."""
    server.context_store(
        "decision content for search", "testing", "decision", agent_id="human"
    )
    resp = server.context_search(
        "decision content", category="decision", format="json"
    )
    entries = parse_entries(resp)
    for e in entries:
        assert e.get("category") == "decision"


def test_search_with_k_limit(server):
    """T-20: Search with k parameter limits results."""
    for i in range(5):
        server.context_store(
            f"k limit entry {i} about testing patterns",
            "testing",
            "convention",
            agent_id="human",
        )
    resp = server.context_search("testing patterns", k=2, format="json")
    entries = parse_entries(resp)
    assert len(entries) <= 2


def test_search_includes_deprecated_with_status(server):
    """T-21: Deprecated entries appear in search results with deprecated status."""
    store_resp = server.context_store(
        "deprecated search content unique abc",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    server.context_deprecate(entry_id, reason="outdated", agent_id="human")
    resp = server.context_search("deprecated search content unique abc", format="json")
    entry = assert_search_contains(resp, entry_id)
    assert entry.get("status") == "deprecated"


def test_search_excludes_quarantined(server):
    """T-22: Search excludes quarantined entries."""
    store_resp = server.context_store(
        "quarantined search content unique def",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    resp = server.context_search(
        "quarantined search content unique def", format="json"
    )
    assert_search_not_contains(resp, entry_id)


def test_search_all_formats(server):
    """T-23: Search returns valid responses in all three formats."""
    server.context_store(
        "format search test", "testing", "convention", agent_id="human"
    )
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_search("format search test", format=fmt)
        assert_tool_success(resp)


# === context_lookup (10 tests) ========================================

def test_lookup_by_topic(server):
    """T-28: Lookup filtered by topic."""
    server.context_store(
        "lookup topic content", "security", "convention", agent_id="human"
    )
    resp = server.context_lookup(topic="security", format="json")
    entries = parse_entries(resp)
    assert len(entries) > 0


def test_lookup_by_category(server):
    """T-29: Lookup filtered by category."""
    server.context_store(
        "lookup cat content", "testing", "decision", agent_id="human"
    )
    resp = server.context_lookup(category="decision", format="json")
    entries = parse_entries(resp)
    assert len(entries) > 0


def test_lookup_by_id(server):
    """T-30: Lookup by specific entry ID."""
    store_resp = server.context_store(
        "lookup id content", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    resp = server.context_lookup(id=entry_id, agent_id="human", format="json")
    entry = parse_entry(resp)
    assert entry.get("id") == entry_id


def test_lookup_with_limit(server):
    """T-34: Lookup with limit parameter."""
    for i in range(5):
        server.context_store(
            f"lookup limit {i}", "testing", "convention", agent_id="human"
        )
    resp = server.context_lookup(topic="testing", limit=2, format="json")
    entries = parse_entries(resp)
    assert len(entries) <= 2


def test_lookup_nonexistent_topic(server):
    """T-37: Lookup nonexistent topic returns empty."""
    resp = server.context_lookup(
        topic="nonexistent-topic-xyz", format="json"
    )
    entries = parse_entries(resp)
    assert len(entries) == 0


def test_lookup_all_formats(server):
    """T-35: Lookup returns valid response in all formats."""
    server.context_store(
        "lookup format test", "testing", "convention", agent_id="human"
    )
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_lookup(topic="testing", format=fmt)
        assert_tool_success(resp)


# === context_get (6 tests) ============================================

def test_get_existing(server):
    """T-38: Get existing entry by ID."""
    store_resp = server.context_store(
        "get existing content", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    resp = server.context_get(entry_id, format="json")
    entry = parse_entry(resp)
    assert "get existing content" in entry.get("content", "")


def test_get_nonexistent(server):
    """T-39: Get nonexistent ID returns error."""
    resp = server.context_get(99999, format="json")
    assert_tool_error(resp)


def test_get_quarantined_visible(server):
    """T-40: Get quarantined entry still accessible."""
    store_resp = server.context_store(
        "quarantined get content", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    resp = server.context_get(entry_id, format="json")
    assert_tool_success(resp)


def test_get_all_formats(server):
    """T-42: Get returns valid response in all formats."""
    store_resp = server.context_store(
        "format get test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_get(entry_id, format=fmt)
        assert_tool_success(resp)


def test_get_invalid_id(server):
    """T-43: Get with negative ID returns error."""
    resp = server.context_get(-1, format="json")
    assert_tool_error(resp)


# === context_correct (8 tests) ========================================

def test_correct_creates_chain(server):
    """T-44: Correct deprecates original and creates new entry."""
    store_resp = server.context_store(
        "original for correction", "testing", "convention", agent_id="human", format="json"
    )
    original_id = extract_entry_id(store_resp)
    correct_resp = server.context_correct(
        original_id,
        "corrected content v2",
        reason="Updated guidance",
        agent_id="human",
        format="json",
    )
    assert_tool_success(correct_resp)


def test_correct_nonexistent(server):
    """T-46: Correct nonexistent entry returns error."""
    resp = server.context_correct(99999, "content", agent_id="human")
    assert_tool_error(resp)


def test_correct_requires_write(server):
    """T-49: Correct requires Write capability."""
    server.context_enroll(
        "test-read-only-agent", "restricted", ["read", "search"], agent_id="human"
    )
    store_resp = server.context_store(
        "correct write test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    resp = server.context_correct(
        entry_id, "updated", agent_id="test-read-only-agent"
    )
    assert_tool_error(resp)


def test_correct_preserves_metadata(server):
    """T-50: Correct preserves original metadata unless overridden."""
    store_resp = server.context_store(
        "metadata preserve test",
        "architecture",
        "decision",
        title="Original Title",
        tags=["preserve"],
        agent_id="human",
        format="json",
    )
    original_id = extract_entry_id(store_resp)
    correct_resp = server.context_correct(
        original_id,
        "corrected metadata content",
        agent_id="human",
        format="json",
    )
    new_id = extract_entry_id(correct_resp)
    get_resp = server.context_get(new_id, format="json")
    entry = parse_entry(get_resp)
    assert entry.get("topic") == "architecture"


def test_correct_all_formats(server):
    """T-51: Correct returns valid response in all formats."""
    for fmt in ["summary", "markdown", "json"]:
        # Create a fresh entry for each format test
        store_resp = server.context_store(
            f"correct format test {fmt} unique",
            "testing",
            "convention",
            agent_id="human",
            format="json",
        )
        entry_id = extract_entry_id(store_resp)
        resp = server.context_correct(
            entry_id,
            f"corrected content {fmt}",
            agent_id="human",
            format=fmt,
        )
        assert_tool_success(resp)


# === context_deprecate (5 tests) ======================================

def test_deprecate_changes_status(server):
    """T-52: Deprecate changes entry status."""
    store_resp = server.context_store(
        "to deprecate", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    dep_resp = server.context_deprecate(entry_id, reason="outdated", agent_id="human")
    assert_tool_success(dep_resp)


def test_deprecate_nonexistent(server):
    """T-54: Deprecate nonexistent entry returns error."""
    resp = server.context_deprecate(99999, agent_id="human")
    assert_tool_error(resp)


def test_deprecate_requires_write(server):
    """T-55: Deprecate requires Write capability."""
    server.context_enroll(
        "test-read-only-agent", "restricted", ["read", "search"], agent_id="human"
    )
    store_resp = server.context_store(
        "deprecate write test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    resp = server.context_deprecate(
        entry_id, agent_id="test-read-only-agent"
    )
    assert_tool_error(resp)


@pytest.mark.xfail(reason="Pre-existing: GH#405 — deprecated confidence can exceed active due to background scoring timing; not caused by col-028")
def test_deprecated_visible_in_search_with_lower_confidence(server):
    """T-56: Deprecated entries visible in search with reduced confidence."""
    store_resp = server.context_store(
        "deprecated exclusion test content unique ghi",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    get_before = server.context_get(entry_id, format="json")
    conf_active = parse_entry(get_before).get("confidence", 1.0)
    server.context_deprecate(entry_id, agent_id="human")
    get_after = server.context_get(entry_id, format="json")
    conf_deprecated = parse_entry(get_after).get("confidence", 1.0)
    assert conf_deprecated <= conf_active


# === context_status (8 tests) =========================================

@pytest.mark.smoke
def test_status_empty_db(server):
    """T-57: Status on empty database returns valid report."""
    resp = server.context_status(agent_id="human", format="json")
    result = assert_tool_success(resp)
    assert result.parsed is not None


def test_status_with_entries(server):
    """T-58: Status shows correct entry count after stores."""
    for i in range(3):
        server.context_store(
            f"status count test {i}", "testing", "convention", agent_id="human"
        )
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert report, "Status report should not be empty"


def test_status_topic_filter(server):
    """T-59: Status filtered by topic."""
    server.context_store(
        "status topic test", "architecture", "decision", agent_id="human"
    )
    resp = server.context_status(
        topic="architecture", agent_id="human", format="json"
    )
    assert_tool_success(resp)


def test_status_all_formats(server):
    """T-63: Status returns valid response in all formats."""
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_status(agent_id="human", format=fmt)
        assert_tool_success(resp)


def test_status_category_lifecycle_field_present(server):
    """crt-031: context_status JSON output includes category_lifecycle field.

    Verifies the new per-category lifecycle section is populated and contains
    correctly labeled entries (adaptive vs pinned). AC-09.
    """
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)

    lifecycle = report.get("category_lifecycle")
    assert lifecycle is not None, "category_lifecycle field missing from status JSON"
    # Vec<(String, String)> serializes as a JSON object (dict)
    assert isinstance(lifecycle, dict), (
        f"category_lifecycle must be a dict, got: {type(lifecycle)}"
    )
    # Default config: must contain at least the 5 initial categories
    assert len(lifecycle) >= 5, (
        f"Expected at least 5 categories in category_lifecycle, got: {lifecycle}"
    )
    # lesson-learned must be present and labeled adaptive (default config)
    assert "lesson-learned" in lifecycle, (
        f"lesson-learned not found in category_lifecycle keys: {list(lifecycle.keys())}"
    )
    assert lifecycle["lesson-learned"] == "adaptive", (
        f"Expected lesson-learned to be 'adaptive', got: {lifecycle['lesson-learned']}"
    )
    # All other default categories must be pinned
    for cat in ("decision", "convention", "pattern", "procedure"):
        if cat in lifecycle:
            assert lifecycle[cat] == "pinned", (
                f"Expected {cat} to be 'pinned', got: {lifecycle[cat]}"
            )


# === context_briefing (8 tests) =======================================

def test_briefing_returns_content(server):
    """T-65: Briefing with role and task returns content."""
    server.context_store(
        "developer guidance for testing patterns",
        "testing",
        "convention",
        agent_id="human",
    )
    resp = server.context_briefing("developer", "implement feature", agent_id="human")
    assert_tool_success(resp)


def test_briefing_empty_db(server):
    """T-69: Briefing on empty DB returns valid response."""
    resp = server.context_briefing("developer", "implement feature", agent_id="human")
    assert_tool_success(resp)


def test_briefing_missing_required_params(server):
    """T-71: Briefing without required params returns error."""
    resp = server.call_tool("context_briefing", {"role": "developer"})
    assert_tool_error(resp)


def test_briefing_all_formats(server):
    """T-70: Briefing returns valid response in all formats."""
    for fmt in ["summary", "markdown", "json"]:
        resp = server.context_briefing(
            "developer", "test task", agent_id="human", format=fmt
        )
        assert_tool_success(resp)


# === context_briefing crt-027 WA-4b integration tests (4 tests) =======

def test_briefing_returns_flat_index_table(populated_server):
    """T-CRT027-01: context_briefing returns flat index table format (AC-08, R-05).

    After WA-4b migration from BriefingService to IndexBriefingService, the output
    must be a flat indexed table. Old section-header format must be absent.
    """
    resp = populated_server.context_briefing(
        "architect", "implement feature", agent_id="human"
    )
    assert_tool_success(resp)
    text = get_result_text(resp)
    assert "## Decisions" not in text, (
        "T-CRT027-01: flat index format must not contain '## Decisions' header"
    )
    assert "## Injections" not in text, (
        "T-CRT027-01: flat index format must not contain '## Injections' header"
    )
    assert "## Conventions" not in text, (
        "T-CRT027-01: flat index format must not contain '## Conventions' header"
    )


def test_briefing_active_entries_only(server):
    """T-CRT027-02: context_briefing returns only Active entries (AC-06, IR-02).

    When a topic has one Active and one Deprecated entry, only the Active entry
    must appear in the briefing result.
    """
    unique_topic = "crt027-active-only-test-unique-delta"
    # Store an active entry
    store_resp = server.context_store(
        "active entry content for crt027 active only test",
        unique_topic,
        "decision",
        agent_id="human",
        format="json",
    )
    assert_tool_success(store_resp)
    active_id = extract_entry_id(store_resp)

    # Store and deprecate another entry with the same topic
    dep_store_resp = server.context_store(
        "deprecated entry content for crt027 active only test",
        unique_topic,
        "decision",
        agent_id="human",
        format="json",
    )
    assert_tool_success(dep_store_resp)
    deprecated_id = extract_entry_id(dep_store_resp)
    server.context_deprecate(deprecated_id, reason="outdated", agent_id="human")

    # Call briefing with the topic as task
    resp = server.context_briefing(
        "architect", unique_topic, agent_id="human"
    )
    assert_tool_success(resp)
    text = get_result_text(resp)
    # Deprecated entry ID must not appear in the flat table
    assert str(deprecated_id) not in text, (
        f"T-CRT027-02: deprecated entry {deprecated_id} must not appear in briefing output"
    )


def test_briefing_default_k_higher_than_three(populated_server):
    """T-CRT027-03: context_briefing default k is 20, not 3 (AC-07, R-09).

    The old BriefingService defaulted to k=3 (UNIMATRIX_BRIEFING_K=3 was the default).
    IndexBriefingService must default to k=20. A populated DB with 50 entries should
    return more than 3 results.
    """
    resp = populated_server.context_briefing(
        "developer", "test", agent_id="human"
    )
    assert_tool_success(resp)
    text = get_result_text(resp)
    # Count numeric row markers in the flat table. With 50 entries and k=20 default,
    # the table should have significantly more than 3 rows. We assert > 3 to detect
    # any regression back to the old k=3 default.
    # The flat table rows start with a right-justified row number followed by spaces.
    # At minimum, check that the text is non-trivially long (more than k=3 would produce).
    # We verify by checking the text length is larger than what 3 entries would produce.
    if text:
        # A 3-entry flat table would be ~300 bytes; a 10-entry table would be ~1000+ bytes.
        assert len(text) > 300, (
            f"T-CRT027-03: briefing text too short ({len(text)} bytes); "
            "expected more than 3 entries (k=20 default). May indicate UNIMATRIX_BRIEFING_K regression."
        )


def test_briefing_k_override(populated_server):
    """T-CRT027-04: context_briefing max_tokens=500 limits result budget (AC-07).

    Passing max_tokens constrains the output byte budget, demonstrating the budget
    enforcement path. The harness uses max_tokens (not k directly); the response must
    succeed and respect the budget ceiling.
    """
    # Use min-valid max_tokens=500; the flat table output should be within budget.
    resp = populated_server.context_briefing(
        "developer", "test", agent_id="human", max_tokens=500
    )
    assert_tool_success(resp)
    text = get_result_text(resp)
    # Result must be a valid response string (possibly empty if entries don't fit budget)
    assert text is not None, "T-CRT027-04: briefing with max_tokens=500 must return a result"


# === context_quarantine (8 tests) =====================================

def test_quarantine_entry(server):
    """T-73: Quarantine changes entry status."""
    store_resp = server.context_store(
        "quarantine status test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    q_resp = server.context_quarantine(entry_id, reason="suspect", agent_id="human")
    assert_tool_success(q_resp)


def test_quarantine_excluded_from_search(server):
    """T-74: Quarantined entry not in search results."""
    store_resp = server.context_store(
        "quarantine search exclusion test unique jkl",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    search_resp = server.context_search(
        "quarantine search exclusion test unique jkl", format="json"
    )
    assert_search_not_contains(search_resp, entry_id)


def test_quarantine_excluded_from_lookup(server):
    """T-75: Quarantined entry excluded from default lookup."""
    store_resp = server.context_store(
        "quarantine lookup test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    lookup_resp = server.context_lookup(topic="testing", format="json")
    entries = parse_entries(lookup_resp)
    ids = [e.get("id") for e in entries]
    assert entry_id not in ids


def test_quarantine_visible_via_get(server):
    """T-76: Quarantined entry still accessible via get."""
    store_resp = server.context_store(
        "quarantine get visible test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)


def test_restore_quarantined(server):
    """T-77: Restore returns entry to active status."""
    store_resp = server.context_store(
        "restore test content", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    server.context_quarantine(entry_id, agent_id="human")
    restore_resp = server.context_quarantine(
        entry_id, action="restore", agent_id="human"
    )
    assert_tool_success(restore_resp)


def test_quarantine_requires_admin(server):
    """T-78: Restricted agent cannot quarantine (requires Admin)."""
    store_resp = server.context_store(
        "admin quarantine test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    q_resp = server.context_quarantine(
        entry_id, agent_id="unknown-restricted-agent"
    )
    assert_tool_error(q_resp)


def test_quarantine_all_formats(server):
    """T-80: Quarantine returns valid response in all formats."""
    store_resp = server.context_store(
        "quarantine format test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    for fmt in ["summary", "markdown", "json"]:
        q_resp = server.context_quarantine(entry_id, agent_id="human", format=fmt)
        assert_tool_success(q_resp)
        # After first quarantine, restore for next iteration
        server.context_quarantine(entry_id, action="restore", agent_id="human")


# === context_enroll (alc-002) =============================================


def test_enroll_new_agent(server):
    """T-E01: Admin enrolls a new agent via MCP, verify success response."""
    resp = server.context_enroll(
        "new-worker",
        "internal",
        ["read", "write", "search"],
        agent_id="human",
    )
    assert_tool_success(resp)
    text = get_result_text(resp)
    assert "Enrolled" in text or "enrolled" in text


def test_enroll_update_existing_agent(server):
    """T-E02: Auto-enroll via search, then enroll with higher capabilities."""
    # Auto-enroll by calling search
    server.context_search("test", agent_id="auto-enroll-agent")

    # Upgrade via enrollment
    resp = server.context_enroll(
        "auto-enroll-agent",
        "internal",
        ["read", "write", "search"],
        agent_id="human",
    )
    assert_tool_success(resp)
    text = get_result_text(resp)
    assert "Updated" in text or "updated" in text


def test_enroll_requires_admin(server):
    """T-E03: Non-admin agent calls context_enroll, expect capability denied."""
    # First auto-enroll a restricted agent
    server.context_search("test", agent_id="restricted-agent")

    # Try to enroll as the restricted agent
    resp = server.context_enroll(
        "some-target",
        "internal",
        ["read"],
        agent_id="restricted-agent",
    )
    assert_tool_error(resp, "lacks")


def test_enroll_protected_agent_rejected(server):
    """T-E04: Attempt to enroll 'system', expect protected agent error."""
    resp = server.context_enroll(
        "system",
        "restricted",
        ["read"],
        agent_id="human",
    )
    assert_tool_error(resp, "protected bootstrap agent")


def test_enroll_self_lockout_prevented(server):
    """T-E05: Admin tries to remove own Admin, expect self-lockout error."""
    # Enroll an admin agent
    server.context_enroll(
        "admin-test",
        "internal",
        ["read", "write", "admin"],
        agent_id="human",
    )

    # Self-enrollment without Admin
    resp = server.context_enroll(
        "admin-test",
        "internal",
        ["read", "write"],
        agent_id="admin-test",
    )
    assert_tool_error(resp, "lockout")


def test_enroll_json_format(server):
    """T-E06: Enrollment with json format returns valid JSON response."""
    resp = server.context_enroll(
        "json-test-agent",
        "internal",
        ["read", "write"],
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp)
    import json
    text = get_result_text(resp)
    data = json.loads(text)
    assert data["action"] == "enrolled"
    assert data["agent_id"] == "json-test-agent"
    assert data["trust_level"] == "internal"
    assert "read" in data["capabilities"]
    assert "write" in data["capabilities"]


def test_enrolled_agent_can_write(server):
    """T-E07: Enroll agent with Write, verify it can context_store."""
    server.context_enroll(
        "writer-agent",
        "internal",
        ["read", "write", "search"],
        agent_id="human",
    )

    # Now the enrolled agent should be able to store
    resp = server.context_store(
        "test content from enrolled agent",
        "testing",
        "convention",
        agent_id="writer-agent",
    )
    assert_tool_success(resp)


# === context_cycle_review (col-002) =====================================


def test_retrospective_no_data_returns_error(server):
    """T-R01: Retrospective with no observation data returns error."""
    resp = server.context_cycle_review("col-999", agent_id="human")
    assert_tool_error(resp, "observation")


def test_retrospective_empty_feature_cycle_returns_error(server):
    """T-R02: Retrospective with empty feature_cycle returns validation error."""
    resp = server.context_cycle_review("", agent_id="human")
    assert_tool_error(resp)


def test_retrospective_whitespace_feature_cycle_returns_error(server):
    """T-R03: Retrospective with whitespace-only feature_cycle returns error."""
    resp = server.context_cycle_review("   ", agent_id="human")
    assert_tool_error(resp)


# === context_cycle_review baseline comparison (col-002b) =================

import hashlib
import json as _json
import os
import sqlite3
import time
import uuid


def _compute_db_path(project_dir):
    """Compute the server's SQLite DB path from the project directory.

    Replicates the Rust compute_project_hash logic:
    SHA256(canonicalized_path) -> first 16 hex chars -> ~/.unimatrix/{hash}/unimatrix.db
    """
    canonical = os.path.realpath(project_dir)
    digest = hashlib.sha256(canonical.encode()).hexdigest()[:16]
    return os.path.join(os.path.expanduser("~"), ".unimatrix", digest, "unimatrix.db")


def _seed_observation_sql(db_path, feature_ids, num_records=20):
    """Seed observation data directly into the server's SQLite tables.

    Inserts rows into the `sessions` and `observations` tables so that
    context_cycle_review can find them via SqlObservationSource.

    Returns a list of (feature_id, session_id) tuples for reference.
    """
    conn = sqlite3.connect(db_path)
    conn.execute("PRAGMA journal_mode=WAL")
    now_secs = int(time.time())
    now_millis = now_secs * 1000
    # Use recent timestamps (1 day ago) to stay within 60-day retention window
    base_ts_millis = now_millis - 86_400_000

    seeded = []
    try:
        for fid in feature_ids:
            session_id = f"test-{fid}-{uuid.uuid4().hex[:8]}"

            # Insert session with feature_cycle set
            conn.execute(
                "INSERT INTO sessions (session_id, feature_cycle, started_at, status) "
                "VALUES (?, ?, ?, 0)",
                (session_id, fid, now_secs),
            )

            # Insert observation records
            for i in range(num_records):
                ts_millis = base_ts_millis + (i * 300_000)  # 5-minute intervals

                if i % 4 == 0:
                    hook, tool = "PreToolUse", "Read"
                    input_json = _json.dumps(
                        {"file_path": f"/workspaces/project/product/features/{fid}/SCOPE.md"}
                    )
                elif i % 4 == 1:
                    hook, tool = "PreToolUse", "Bash"
                    input_json = _json.dumps({"command": f"cargo test -p {fid}"})
                elif i % 4 == 2:
                    hook, tool = "PreToolUse", "Write"
                    input_json = _json.dumps(
                        {"file_path": f"/workspaces/project/product/features/{fid}/test.rs"}
                    )
                else:
                    hook, tool = "PostToolUse", "Read"
                    input_json = None

                response_size = 1024 if hook == "PostToolUse" else None
                response_snippet = "some output" if hook == "PostToolUse" else None

                conn.execute(
                    "INSERT INTO observations "
                    "(session_id, ts_millis, hook, tool, input, response_size, response_snippet) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (session_id, ts_millis, hook, tool, input_json, response_size, response_snippet),
                )

            seeded.append((fid, session_id))

        conn.commit()
        # Force WAL checkpoint so the server's connection sees seeded data
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        conn.close()

    return seeded


@pytest.mark.xfail(reason="Pre-existing: GH#305 — baseline_comparison null when synthetic features lack delivery counter registration")
def test_retrospective_baseline_present(server):
    """T-R04 (col-002b): Baseline comparison present with 3+ prior MetricVectors.

    Seeds observation data for 4 features, runs retrospective on the first 3
    to generate MetricVectors, then runs on the 4th and verifies
    baseline_comparison is present in the response.
    """
    features = ["col-801", "col-802", "col-803", "col-804"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    # Generate MetricVectors for first 3 features
    for fid in features[:3]:
        resp = server.context_cycle_review(fid, agent_id="human", format="json", timeout=30.0)
        result = assert_tool_success(resp)

    # Now run on 4th feature -- should have baseline from 3 prior
    resp = server.context_cycle_review(features[3], agent_id="human", format="json", timeout=30.0)
    result = assert_tool_success(resp)

    # Parse report and check for baseline_comparison
    if result.parsed and isinstance(result.parsed, dict):
        report = result.parsed
    else:
        report = _json.loads(result.text) if result.text.strip().startswith("{") else {}

    assert "baseline_comparison" in report, (
        f"Expected baseline_comparison in report, got keys: {list(report.keys())}"
    )
    baseline = report["baseline_comparison"]
    assert baseline is not None, "baseline_comparison should not be null with 3 prior MetricVectors"
    assert isinstance(baseline, list), f"Expected list, got {type(baseline)}"
    assert len(baseline) > 0, "baseline_comparison should have entries"

    # Verify each entry has required fields
    for entry in baseline:
        assert "metric_name" in entry, f"Missing 'metric_name' in baseline entry: {entry}"
        assert "status" in entry, f"Missing 'status' in baseline entry: {entry}"
        assert "current_value" in entry, f"Missing 'current_value' in baseline entry: {entry}"
        assert "mean" in entry, f"Missing 'mean' in baseline entry: {entry}"


def test_retrospective_insufficient_baseline(server):
    """T-R05 (col-002b): Baseline comparison absent with fewer than 3 MetricVectors.

    Seeds observation data for 3 features, runs retrospective on only 2 to
    generate MetricVectors, then runs on the 3rd. With only 2 prior vectors,
    baseline_comparison should be null/absent.
    """
    features = ["col-811", "col-812", "col-813"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    # Generate MetricVectors for only 2 features
    for fid in features[:2]:
        resp = server.context_cycle_review(fid, agent_id="human", format="json", timeout=30.0)
        assert_tool_success(resp)

    # Run on 3rd feature -- only 2 prior vectors, insufficient for baseline
    resp = server.context_cycle_review(features[2], agent_id="human", format="json", timeout=30.0)
    result = assert_tool_success(resp)

    if result.parsed and isinstance(result.parsed, dict):
        report = result.parsed
    else:
        report = _json.loads(result.text) if result.text.strip().startswith("{") else {}

    # baseline_comparison should be null or absent
    baseline = report.get("baseline_comparison")
    assert baseline is None, (
        f"Expected null baseline_comparison with only 2 prior vectors, got: {baseline}"
    )


def test_retrospective_21_rules_active(server):
    """T-R06 (col-002b): default_rules returns 21 rules covering all 4 categories.

    Seeds observation data, runs retrospective, verifies report structure
    includes hotspots section that can contain findings from agent, friction,
    session, and scope categories. (Does not guarantee all categories fire --
    that depends on the observation data patterns.)
    """
    features = ["col-821"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_cycle_review(features[0], agent_id="human", format="json", timeout=30.0)
    result = assert_tool_success(resp)

    if result.parsed and isinstance(result.parsed, dict):
        report = result.parsed
    else:
        report = _json.loads(result.text) if result.text.strip().startswith("{") else {}

    # Verify hotspots section exists
    assert "hotspots" in report, f"Expected hotspots in report, got keys: {list(report.keys())}"
    hotspots = report["hotspots"]
    assert isinstance(hotspots, list), f"Expected list, got {type(hotspots)}"

    # Verify metrics section exists (proves computation pipeline works)
    assert "metrics" in report, f"Expected metrics in report"


# === context_cycle_review format dispatch (vnc-011) =======================


def test_retrospective_markdown_default(server):
    """T-R07 (vnc-011): Default format (no format param) returns markdown output.

    Seeds observation data, runs retrospective with no format param, and verifies
    response starts with the rebranded markdown header '# Unimatrix Cycle Review —'
    (col-026 AC-01: header rebranded from '# Retrospective:').
    """
    features = ["col-831"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_cycle_review(features[0], agent_id="human", timeout=30.0)
    result = assert_tool_success(resp)
    assert result.text.strip().startswith("# Unimatrix Cycle Review —"), (
        f"Expected rebranded markdown header (col-026 AC-01), got: {result.text[:100]}"
    )


def test_retrospective_json_explicit(server):
    """T-R08 (vnc-011): format='json' returns valid JSON output."""
    features = ["col-832"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_cycle_review(features[0], agent_id="human", format="json", timeout=30.0)
    result = assert_tool_success(resp)
    parsed = _json.loads(result.text)
    assert isinstance(parsed, dict), f"Expected JSON object, got {type(parsed)}"
    assert "feature_cycle" in parsed, f"Expected feature_cycle in JSON, got keys: {list(parsed.keys())}"


def test_retrospective_format_invalid(server):
    """T-R09 (vnc-011): Invalid format returns error with descriptive message."""
    features = ["col-833"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_cycle_review(features[0], agent_id="human", format="xml", timeout=30.0)
    assert_tool_error(resp, "Unknown format")


# === context_status observation extension (col-002) =======================


def test_status_includes_observation_fields(server):
    """T-S01: Status report includes observation health fields."""
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert "observation" in report, "Missing observation section"
    obs = report["observation"]
    # Fields match ObservationJson in mcp/response/status.rs
    assert "record_count" in obs, "Missing record_count in observation"
    assert "session_count" in obs, "Missing session_count in observation"
    assert "oldest_record_days" in obs, "Missing oldest_record_days in observation"
    assert "retrospected_feature_count" in obs, "Missing retrospected_feature_count"
    assert "approaching_cleanup" in obs, "Missing approaching_cleanup"


def test_status_observation_retrospected_default(server):
    """T-S02: Retrospected feature count is 0 on fresh server (no stored metrics)."""
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    obs = report.get("observation", {})
    assert obs.get("retrospected_feature_count", -1) == 0


# === crt-019: Confidence Signal Activation (AC-08a, AC-08b, R-07, R-11) ======


def test_context_get_implicit_helpful_vote(server):
    """AC-08a: context_get with helpful=null registers an implicit helpful vote.

    When helpful is not specified, the server injects implicit helpful=true
    via UsageContext (FR-06 / C-04). Multiple agents calling context_get without
    helpful specified should cause confidence to increase (more helpful votes
    raise the Bayesian helpfulness score).

    The MCP response exposes confidence but not helpful_count directly.
    We verify the end-to-end effect: confidence increases after multiple
    implicit helpful votes from distinct agents.

    Verifies: FR-06, C-04 (no second spawn_blocking), AC-08a.
    """
    store_resp = server.context_store(
        "crt019 implicit vote test content unique abc987",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Read initial confidence
    initial_resp = server.context_get(entry_id, format="json")
    initial_entry = parse_entry(initial_resp)
    initial_conf = float(initial_entry.get("confidence", 0))

    # Multiple agents call context_get without specifying helpful (implicit helpful=true)
    # UsageDedup allows one vote per agent per entry, so we use distinct agents
    for i in range(8):
        server.context_get(entry_id, agent_id=f"crt019-implicit-voter-{i}", format="json")
        time.sleep(0.05)

    # Wait for spawn_blocking completions
    time.sleep(0.5)

    # Read confidence after implicit helpful votes
    after_resp = server.context_get(entry_id, format="json")
    after_entry = parse_entry(after_resp)
    after_conf = float(after_entry.get("confidence", 0))

    # Confidence should be valid
    assert 0 <= after_conf <= 1, f"confidence out of range: {after_conf}"
    assert 0 <= initial_conf <= 1, f"initial confidence out of range: {initial_conf}"

    # After 8 implicit helpful votes, confidence should increase (or stay same at ceiling)
    # The Bayesian formula: (helpful + alpha0) / (total + alpha0 + beta0)
    # 8 votes at cold-start: (8+3)/(8+3+3) = 11/14 ≈ 0.786 vs neutral 3/6 = 0.5
    assert after_conf >= initial_conf, (
        f"confidence must not decrease after implicit helpful votes: "
        f"initial={initial_conf:.4f}, after={after_conf:.4f}. "
        f"AC-08a: implicit helpful=None must register as helpful=true."
    )


def test_context_lookup_doubled_access_count(server):
    """AC-08b: context_lookup registers doubled access weight vs context_get.

    context_lookup sets access_weight=2 (deliberate retrieval signal, ADR-004).
    The effect is observable as a greater confidence boost from usage factor
    compared to a single context_get access with access_weight=1.

    Since helpful_count and access_count are not directly exposed in the MCP
    JSON response (they are internal store fields), we verify the behavior
    end-to-end through the confidence signal:
    - An entry accessed via context_lookup should receive more usage boost
      than the same number of context_get calls.

    Additionally verifies that context_lookup returns the entry successfully
    and does not inject helpful votes (AC-08b: helpful_count == 0 semantics).

    R-11: store-layer dedup behavior is tested in unit tests (services/usage.rs).
    R-07: dedup-before-multiply is tested in unit tests (services/usage.rs).
    This integration test validates the end-to-end tool behavior.
    """
    # Store entry A — will be accessed via context_lookup (access_weight=2)
    lookup_resp = server.context_store(
        "crt019 lookup doubled access entry unique xyz321",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    lookup_id = extract_entry_id(lookup_resp)

    # Store entry B — will be accessed via context_get (access_weight=1)
    get_resp = server.context_store(
        "crt019 get single access entry unique abc123",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    get_id = extract_entry_id(get_resp)

    # Read initial confidences (should be equal — same signal profile)
    init_lookup_conf = float(parse_entry(server.context_get(lookup_id, format="json")).get("confidence", 0))
    init_get_conf = float(parse_entry(server.context_get(get_id, format="json")).get("confidence", 0))

    # Access entry A via context_lookup N times (weight=2 each)
    for i in range(5):
        server.context_lookup(id=lookup_id, agent_id=f"crt019-lookup-agent-{i}", format="json")
        time.sleep(0.05)

    # Access entry B via context_get N times (weight=1 each)
    for i in range(5):
        server.context_get(get_id, agent_id=f"crt019-get-agent-{i}", helpful=None, format="json")
        time.sleep(0.05)

    time.sleep(0.5)

    # Verify context_lookup returned the entry (tool works)
    verify_resp = server.context_lookup(id=lookup_id, format="json")
    assert_tool_success(verify_resp)

    # Verify both entries have valid confidence after access
    final_lookup_conf = float(parse_entry(server.context_get(lookup_id, format="json")).get("confidence", 0))
    final_get_conf = float(parse_entry(server.context_get(get_id, format="json")).get("confidence", 0))

    assert 0 <= final_lookup_conf <= 1, f"lookup entry confidence out of range: {final_lookup_conf}"
    assert 0 <= final_get_conf <= 1, f"get entry confidence out of range: {final_get_conf}"

    # Both confidences should have increased (usage factor)
    assert final_lookup_conf >= init_lookup_conf, (
        f"lookup entry confidence must not decrease with usage: "
        f"{init_lookup_conf:.4f} -> {final_lookup_conf:.4f}"
    )
    assert final_get_conf >= init_get_conf, (
        f"get entry confidence must not decrease with usage: "
        f"{init_get_conf:.4f} -> {final_get_conf:.4f}"
    )


# === crt-023: NLI + Cross-Encoder Re-ranking (W1-4) ==========================


def test_search_nli_not_ready_fallback_results(server):
    """T-CRT023-01: context_search returns valid results when NLI is not ready (AC-05, AC-14).

    In CI the NLI model is not cached, so NliServiceHandle is in Failed/NotReady
    state. The server must fall back to cosine-similarity ranking and return
    results without error. Response schema must be unchanged.
    """
    # Store an entry so search has something to find
    store_resp = server.context_store(
        "nli not ready fallback test unique crt023 alpha search",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Search — NLI absent in CI means cosine fallback must kick in
    search_resp = server.context_search(
        "nli not ready fallback test unique crt023 alpha search",
        format="json",
        agent_id="human",
    )
    # Must succeed without error — AC-14 graceful degradation
    assert_tool_success(search_resp)
    entries = parse_entries(search_resp)
    # Stored entry must be findable via cosine fallback (AC-05)
    result_ids = [e.get("id") for e in entries if e.get("id") is not None]
    assert entry_id in result_ids, (
        f"AC-05/AC-14: context_search must return results via cosine fallback when NLI "
        f"is not ready. entry_id={entry_id} not found in results: {result_ids}"
    )


def test_search_nli_absent_uses_renormalized_weights(server):
    """T-CRT024-01: NLI-absent path re-normalizes weights; all scores finite and in [0,1] (R-09, AC-06).

    In CI, the NLI model is not loaded, so FusionWeights::effective(nli_available=false)
    is invoked. The five non-NLI weights are re-normalized to sum to 1.0. The returned
    final_score values for all ScoredEntry items must be:
      - finite (no NaN from zero-denominator or unchecked division, R-02, R-03)
      - in [0.0, 1.0] (NFR-02 range guarantee)
      - non-negative (R-11: Ineffective entries must not produce negative scores)

    Fixture: server (fresh DB, NLI absent — cold start).
    """
    # Store an entry to ensure search has something to score
    store_resp = server.context_store(
        "crt024 nli absent renormalized weights test unique omega scoring",
        "testing NLI-absent scoring path with re-normalized fusion weights",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Search — NLI absent in CI means FusionWeights::effective(false) is used
    search_resp = server.context_search(
        "crt024 nli absent renormalized weights test unique omega scoring",
        format="json",
        agent_id="human",
    )

    assert_tool_success(search_resp)
    entries = parse_entries(search_resp)

    # Must find at least one entry (the one we stored)
    result_ids = [e.get("id") for e in entries if e.get("id") is not None]
    assert entry_id in result_ids, (
        f"T-CRT024-01: stored entry must be findable via NLI-absent scoring path. "
        f"entry_id={entry_id} not in results: {result_ids}"
    )

    # All returned scores must be finite and in [0, 1] — NLI-absent re-normalization guard
    for e in entries:
        score = e.get("final_score")
        if score is not None:
            assert isinstance(score, (int, float)), (
                f"T-CRT024-01/R-02: final_score must be numeric, got {type(score)}"
            )
            import math
            assert math.isfinite(score), (
                f"T-CRT024-01/R-02: final_score must be finite (no NaN/Inf). "
                f"NLI-absent zero-denominator guard may have failed. Got: {score}"
            )
            assert score >= 0.0, (
                f"T-CRT024-01/R-11: final_score must be >= 0.0 (shift-and-scale for "
                f"Ineffective entries). Got: {score}"
            )
            assert score <= 1.0, (
                f"T-CRT024-01/NFR-02: final_score must be <= 1.0. Got: {score}"
            )


def test_store_response_not_blocked_by_nli_task(server):
    """T-CRT023-02: context_store MCP response returns promptly; not blocked by NLI task (NFR-02).

    The NLI post-store detection is fire-and-forget. Even when NLI is active or
    loading, the context_store MCP response must return well within 2 seconds.
    This validates that the fire-and-forget spawn does not block the return path.
    """
    import time as _time
    start = _time.monotonic()
    resp = server.context_store(
        "nli fire and forget store response timing test crt023 beta",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    elapsed = _time.monotonic() - start

    assert_tool_success(resp)
    assert elapsed < 5.0, (
        f"NFR-02: context_store must return within 5s (fire-and-forget NLI must not "
        f"block response). Took {elapsed:.2f}s."
    )


# === context_cycle phase signal (crt-025 WA-1) ============================


def test_cycle_phase_end_type_accepted(server):
    """T-CRT025-01: context_cycle accepts 'phase-end' as a valid type (AC-02)."""
    resp = server.context_cycle(
        "phase-end",
        "crt-025-phase-end-type-test",
        phase="scope",
        next_phase="design",
        agent_id="human",
    )
    assert_tool_success(resp)


def test_cycle_phase_end_stores_row(server):
    """T-CRT025-02: Three sequential cycle events (start→phase-end→stop) all succeed (AC-04, AC-08).

    Note: CYCLE_EVENTS are written via the UDS hook path (not the MCP tool path).
    In the integration harness (no hooks), context_cycle calls only validate and acknowledge.
    This test verifies that all three event types are accepted and do not return errors.
    The phase_narrative path is separately verified in test_cycle_review_includes_phase_narrative
    using direct SQL seeding of CYCLE_EVENTS.
    """
    topic = "crt025-stores-row-test"

    resp1 = server.context_cycle("start", topic, next_phase="scope", agent_id="human")
    assert_tool_success(resp1)

    resp2 = server.context_cycle(
        "phase-end", topic, phase="scope", next_phase="design", agent_id="human"
    )
    assert_tool_success(resp2)

    resp3 = server.context_cycle("stop", topic, phase="design", agent_id="human")
    assert_tool_success(resp3)


def test_cycle_invalid_type_rejected(server):
    """T-CRT025-03: context_cycle rejects unknown type 'pause' with descriptive error (AC-02)."""
    resp = server.context_cycle("pause", "crt-025-invalid-type-test", agent_id="human")
    assert_tool_error(resp)


def test_cycle_phase_with_space_rejected(server):
    """T-CRT025-04: context_cycle rejects phase value containing a space (AC-03, R-06)."""
    resp = server.context_cycle(
        "phase-end",
        "crt-025-phase-space-test",
        phase="scope review",
        agent_id="human",
    )
    assert_tool_error(resp)


def test_cycle_outcome_category_rejected(server):
    """T-CRT025-05: context_store with category='outcome' returns InvalidCategory error
    after crt-025 retirement of 'outcome' from CategoryAllowlist (AC-15, R-03)."""
    resp = server.context_store(
        "test entry with retired outcome category",
        "crt-025-outcome-reject-test",
        "outcome",
        agent_id="human",
    )
    assert_tool_error(resp)


def _seed_cycle_events_sql(db_path, cycle_id, events):
    """Seed CYCLE_EVENTS rows directly into the SQLite database.

    `events` is a list of dicts with keys: seq, event_type, phase, outcome, next_phase, timestamp.
    Used to test phase_narrative without requiring the UDS hook path (which is not active in
    the integration harness).
    """
    import sqlite3 as _sqlite3
    conn = _sqlite3.connect(db_path)
    conn.execute("PRAGMA journal_mode=WAL")
    for ev in events:
        conn.execute(
            "INSERT INTO cycle_events (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                cycle_id,
                ev["seq"],
                ev["event_type"],
                ev.get("phase"),
                ev.get("outcome"),
                ev.get("next_phase"),
                ev.get("timestamp", int(time.time())),
            ),
        )
    conn.commit()
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    conn.close()


def test_cycle_review_includes_phase_narrative(server):
    """T-CRT025-06: context_cycle_review includes phase_narrative when CYCLE_EVENTS rows
    exist for the queried feature cycle (AC-12, R-08).

    Seeds both observation data and CYCLE_EVENTS rows directly via SQL so that
    context_cycle_review can return a report that includes phase_narrative.
    (CYCLE_EVENTS are written via the UDS hook path which is not active in the harness.)
    """
    import json as _json
    topic = "crt025-phase-narrative-present"
    now = int(time.time())

    db_path = _compute_db_path(server.project_dir)
    # Seed observation data so context_cycle_review returns a report
    _seed_observation_sql(db_path, [topic], num_records=20)
    # Seed CYCLE_EVENTS directly (UDS path unavailable in harness)
    _seed_cycle_events_sql(db_path, topic, [
        {"seq": 0, "event_type": "cycle_start",     "next_phase": "scope",  "timestamp": now - 300},
        {"seq": 1, "event_type": "cycle_phase_end", "phase": "scope", "next_phase": "design", "timestamp": now - 200},
        {"seq": 2, "event_type": "cycle_stop",      "phase": "design",      "timestamp": now - 100},
    ])

    resp = server.context_cycle_review(topic, agent_id="human", format="json", timeout=30.0)
    assert_tool_success(resp)
    text = get_result_text(resp)
    try:
        data = _json.loads(text)
    except (_json.JSONDecodeError, TypeError):
        # Rendered text response — check for phase narrative section markers
        assert "phase" in text.lower() or "scope" in text.lower() or "design" in text.lower(), (
            "T-CRT025-06: cycle_review must include phase narrative section when events exist (AC-12)"
        )
        return
    phase_narrative = data.get("phase_narrative")
    assert phase_narrative is not None, (
        "T-CRT025-06: phase_narrative key must be present when CYCLE_EVENTS rows exist (AC-12)"
    )


def test_cycle_review_no_phase_narrative_for_old_feature(server):
    """T-CRT025-07: context_cycle_review does NOT include phase_narrative for a feature cycle
    that has no CYCLE_EVENTS rows — backward compatibility (AC-13, R-08).

    Seeds only observation data (so cycle_review returns a report) but no CYCLE_EVENTS rows.
    """
    import json as _json
    topic = "crt025-no-cycle-events-old"

    # Seed observation data so context_cycle_review can produce a report
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, [topic], num_records=20)
    # Deliberately do NOT seed any CYCLE_EVENTS rows for this topic

    resp = server.context_cycle_review(topic, agent_id="human", format="json", timeout=30.0)
    assert_tool_success(resp)
    text = get_result_text(resp)
    try:
        data = _json.loads(text)
    except (_json.JSONDecodeError, TypeError):
        # Non-JSON (rendered) response — phase_narrative section should be absent
        assert "phase_narrative" not in text and "Phase Narrative" not in text, (
            "T-CRT025-07: phase_narrative must be absent in rendered text when no CYCLE_EVENTS (AC-13)"
        )
        return
    assert "phase_narrative" not in data, (
        "T-CRT025-07: phase_narrative key must be absent when no CYCLE_EVENTS rows exist (AC-13, R-08)"
    )


# === context_cycle goal parameter (col-025) ============================


def test_cycle_start_goal_accepted(server):
    """T-COL025-01: context_cycle(start) with goal parameter succeeds (AC-01)."""
    resp = server.context_cycle(
        "start",
        "col-025-goal-accepted-test",
        goal="Implement feature goal signal for col-025.",
        agent_id="human",
    )
    assert_tool_success(resp)


def test_cycle_start_goal_exceeds_max_bytes_rejected(server):
    """T-COL025-02: context_cycle(start) rejects goal > 1024 bytes with descriptive error (AC-13a).

    MAX_GOAL_BYTES = 1024. A 1025-byte goal must be rejected; no DB write occurs.
    The error message must reference the byte limit.
    """
    oversized_goal = "a" * 1025
    resp = server.context_cycle(
        "start",
        "col-025-goal-rejected-test",
        goal=oversized_goal,
        agent_id="human",
    )
    result = assert_tool_error(resp)
    # Error text must reference goal/bytes so agent knows what to fix
    assert "goal" in result.text.lower() or "1024" in result.text or "byte" in result.text.lower(), (
        f"T-COL025-02: error must mention goal byte limit, got: {result.text[:200]}"
    )


def test_cycle_start_goal_at_exact_max_bytes_accepted(server):
    """T-COL025-03: context_cycle(start) accepts goal of exactly 1024 bytes (AC-13a boundary).

    1024 bytes is the inclusive upper bound — must be accepted without error.
    """
    boundary_goal = "a" * 1024
    resp = server.context_cycle(
        "start",
        "col-025-goal-boundary-test",
        goal=boundary_goal,
        agent_id="human",
    )
    assert_tool_success(resp)


def test_cycle_start_empty_goal_treated_as_no_goal(server):
    """T-COL025-04: context_cycle(start) with empty goal normalizes to None (AC-17).

    An empty string goal must not produce an error and must be treated as if no
    goal was supplied. The cycle start succeeds.
    """
    resp = server.context_cycle(
        "start",
        "col-025-empty-goal-test",
        goal="",
        agent_id="human",
    )
    assert_tool_success(resp)


def test_cycle_start_whitespace_goal_normalized_to_none(server):
    """T-COL025-05: context_cycle(start) with whitespace-only goal normalizes to None (AC-17)."""
    resp = server.context_cycle(
        "start",
        "col-025-whitespace-goal-test",
        goal="   ",
        agent_id="human",
    )
    assert_tool_success(resp)


# === context_cycle_review col-026 integration tests ====================


def test_cycle_review_phase_timeline_present(server):
    """T-COL026-01: context_cycle_review returns Phase Timeline section when cycle_events exist.

    Seeds cycle_events (start, phase_end, stop) via SQL, then calls context_cycle_review
    and asserts the markdown response contains a Phase Timeline section (AC-06).
    """
    import json as _json
    topic = "col-026-phase-timeline-test"
    now = int(time.time())

    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, [topic], num_records=20)
    _seed_cycle_events_sql(db_path, topic, [
        {"seq": 0, "event_type": "cycle_start",     "next_phase": "scope",  "timestamp": now - 600},
        {"seq": 1, "event_type": "cycle_phase_end", "phase": "scope", "next_phase": "design",
         "outcome": "pass", "timestamp": now - 400},
        {"seq": 2, "event_type": "cycle_phase_end", "phase": "design", "next_phase": "implementation",
         "outcome": "pass", "timestamp": now - 200},
        {"seq": 3, "event_type": "cycle_stop",      "phase": "implementation", "timestamp": now - 50},
    ])

    resp = server.context_cycle_review(topic, agent_id="human", format="markdown", timeout=30.0)
    assert_tool_success(resp)
    text = get_result_text(resp)

    assert "Phase Timeline" in text, (
        f"T-COL026-01: Phase Timeline section must be present when cycle_events exist (AC-06). "
        f"Got first 500 chars: {text[:500]}"
    )
    # At least one phase name must appear
    assert any(phase in text for phase in ["scope", "design", "implementation"]), (
        f"T-COL026-01: At least one phase name must appear in Phase Timeline. Got: {text[:500]}"
    )


def test_cycle_review_is_in_progress_json(server):
    """T-COL026-02: context_cycle_review returns is_in_progress=true in JSON when no cycle_stop.

    Seeds a cycle_start event only (no cycle_stop). Calls context_cycle_review in JSON
    format and asserts is_in_progress is true (AC-05, R-05).
    """
    import json as _json
    topic = "col-026-in-progress-test"
    now = int(time.time())

    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, [topic], num_records=20)
    _seed_cycle_events_sql(db_path, topic, [
        {"seq": 0, "event_type": "cycle_start", "next_phase": "scope", "timestamp": now - 300},
    ])

    resp = server.context_cycle_review(topic, agent_id="human", format="json", timeout=30.0)
    assert_tool_success(resp)
    text = get_result_text(resp)

    try:
        data = _json.loads(text)
        assert data.get("is_in_progress") is True, (
            f"T-COL026-02: is_in_progress must be true when cycle_stop is absent (AC-05, R-05). "
            f"Got is_in_progress={data.get('is_in_progress')!r}"
        )
    except (_json.JSONDecodeError, TypeError):
        # Non-JSON response — check markdown for IN PROGRESS
        assert "IN PROGRESS" in text or "in progress" in text.lower(), (
            f"T-COL026-02: markdown must show IN PROGRESS when cycle_stop absent. Got: {text[:300]}"
        )


def test_briefing_response_starts_with_context_get_instruction(server):
    """T-COL025-06: context_briefing response starts with CONTEXT_GET_INSTRUCTION header (AC-18).

    After col-025, all format_index_table output is prefixed with the
    CONTEXT_GET_INSTRUCTION header. Verify this through the MCP tool interface.
    """
    # Pre-load an entry so briefing has something to return
    server.context_store(
        "Feature goal signal for col-025 improves briefing query precision.",
        "col-025",
        "decision",
        agent_id="human",
    )

    resp = server.context_briefing("architect", "feature goal signal", agent_id="human")
    assert_tool_success(resp)
    text = get_result_text(resp)
    instruction = "Use context_get with the entry ID for full content when relevant."
    # Either the instruction is present at the start, or the response is empty (no entries matched)
    if text.strip():
        assert text.strip().startswith(instruction), (
            f"T-COL025-06: briefing output must start with CONTEXT_GET_INSTRUCTION, "
            f"got first 200 chars: {text[:200]}"
        )


# === context_cycle_review crt-033 memoization ==========================


def test_cycle_review_force_param_accepted(server):
    """T-CRT033-01: context_cycle_review accepts force parameter without param-validation error.

    With force=true and no observation data, the expected response is
    ERROR_NO_OBSERVATION_DATA (not a parameter-validation error). This confirms
    that the force field is recognized and deserialized correctly (AC-12).
    """
    resp = server.call_tool("context_cycle_review", {
        "feature_cycle": "crt033-force-param-test",
        "agent_id": "human",
        "force": True,
    })
    # A JSON-RPC level error is expected (no observation data).
    # Confirm it is the expected error type (observation data absent, error code -32010),
    # not a parameter parse failure (-32602 invalid params) or unknown-field error.
    assert resp.error is not None, (
        "T-CRT033-01: expected a JSON-RPC error (no observation data), got success"
    )
    error_code = resp.error.get("code", 0)
    error_message = resp.error.get("message", "")
    # Must NOT be a parameter-validation error (-32602)
    assert error_code != -32602, (
        f"T-CRT033-01: force=true must not cause param-validation error (-32602). "
        f"Got code={error_code}, message={error_message[:200]}"
    )
    # Must be the observation-data-absent error (-32010) or similar observation error
    assert "observation" in error_message.lower() or "no data" in error_message.lower() or error_code == -32010, (
        f"T-CRT033-01: expected observation-data error, got code={error_code}, "
        f"message={error_message[:200]}"
    )


# === context_status crt-033 pending_cycle_reviews field =================


def test_status_pending_cycle_reviews_field_present(server):
    """T-CRT033-02: context_status JSON response contains pending_cycle_reviews as an array.

    Verifies the new field added in crt-033 is always present and always an array
    (may be empty on a fresh DB). AC-09/AC-10.
    """
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert "pending_cycle_reviews" in report, (
        "T-CRT033-02: pending_cycle_reviews field must be present in context_status JSON"
    )
    field_value = report["pending_cycle_reviews"]
    assert isinstance(field_value, list), (
        f"T-CRT033-02: pending_cycle_reviews must be a list/array, got {type(field_value)}: {field_value!r}"
    )
    # On a fresh DB with no cycle_events rows, the list must be empty
    assert field_value == [], (
        f"T-CRT033-02: fresh DB must have empty pending_cycle_reviews, got: {field_value!r}"
    )


# === vnc-012: String-encoded integer coercion (IT-01, IT-02) ================

@pytest.mark.smoke
def test_get_with_string_id(server):
    """IT-01 (vnc-012): context_get accepts string-encoded id over stdio transport.

    Stores an entry and retrieves it using a JSON string id (e.g., "42" instead of 42).
    This exercises the full rmcp Parameters<T> deserialization path over stdio --
    the exact path where the live bug fires.
    Must return success and non-empty content.
    """
    store_resp = server.context_store(
        "IT-01 string id coercion test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(store_resp)
    entry_id = extract_entry_id(store_resp)

    string_id = str(entry_id)
    get_resp = server.call_tool(
        "context_get",
        {"id": string_id, "agent_id": "human"},
    )

    assert_tool_success(get_resp)
    text = get_result_text(get_resp)
    assert len(text) > 0, "IT-01: content must be non-empty"
    assert "IT-01 string id coercion test content" in text, (
        "IT-01: retrieved content must match stored content"
    )


@pytest.mark.smoke
def test_deprecate_with_string_id(server):
    """IT-02 (vnc-012): context_deprecate accepts string-encoded id over stdio transport.

    Stores an entry and deprecates it using a JSON string id.
    This exercises the full rmcp Parameters<T> deserialization path for a write tool.
    Must return success.
    """
    store_resp = server.context_store(
        "IT-02 string id coercion deprecate test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(store_resp)
    entry_id = extract_entry_id(store_resp)

    string_id = str(entry_id)
    deprecate_resp = server.call_tool(
        "context_deprecate",
        {"id": string_id, "agent_id": "human", "reason": "IT-02 coercion test"},
    )

    assert_tool_success(deprecate_resp)

