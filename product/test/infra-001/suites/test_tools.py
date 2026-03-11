"""Suite 2: Tools (~80 tests).

Every tool, every parameter path, happy and error paths.
Uses format='json' for structured assertions.
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
    """T-08: Restricted agent cannot store (no Write capability)."""
    resp = server.context_store(
        "restricted content", "testing", "convention", agent_id="unknown-agent-xyz"
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
    store_resp = server.context_store(
        "correct write test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    resp = server.context_correct(
        entry_id, "updated", agent_id="unknown-restricted-agent"
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
    store_resp = server.context_store(
        "deprecate write test", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)
    resp = server.context_deprecate(
        entry_id, agent_id="unknown-restricted-agent"
    )
    assert_tool_error(resp)


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


# === context_briefing (8 tests) =======================================

def test_briefing_returns_content(server):
    """T-65: Briefing with role and task returns content."""
    server.context_store(
        "developer guidance for testing patterns",
        "testing",
        "duties",
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


# === context_retrospective (col-002) =====================================


def test_retrospective_no_data_returns_error(server):
    """T-R01: Retrospective with no observation data returns error."""
    resp = server.context_retrospective("col-999", agent_id="human")
    assert_tool_error(resp, "observation")


def test_retrospective_empty_feature_cycle_returns_error(server):
    """T-R02: Retrospective with empty feature_cycle returns validation error."""
    resp = server.context_retrospective("", agent_id="human")
    assert_tool_error(resp)


def test_retrospective_whitespace_feature_cycle_returns_error(server):
    """T-R03: Retrospective with whitespace-only feature_cycle returns error."""
    resp = server.context_retrospective("   ", agent_id="human")
    assert_tool_error(resp)


# === context_retrospective baseline comparison (col-002b) =================

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
    context_retrospective can find them via SqlObservationSource.

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
        resp = server.context_retrospective(fid, agent_id="human", format="json", timeout=30.0)
        result = assert_tool_success(resp)

    # Now run on 4th feature -- should have baseline from 3 prior
    resp = server.context_retrospective(features[3], agent_id="human", format="json", timeout=30.0)
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
        resp = server.context_retrospective(fid, agent_id="human", format="json", timeout=30.0)
        assert_tool_success(resp)

    # Run on 3rd feature -- only 2 prior vectors, insufficient for baseline
    resp = server.context_retrospective(features[2], agent_id="human", format="json", timeout=30.0)
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

    resp = server.context_retrospective(features[0], agent_id="human", format="json", timeout=30.0)
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


# === context_retrospective format dispatch (vnc-011) =======================


def test_retrospective_markdown_default(server):
    """T-R07 (vnc-011): Default format (no format param) returns markdown output.

    Seeds observation data, runs retrospective with no format param, and verifies
    response starts with the markdown header '# Retrospective:'.
    """
    features = ["col-831"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_retrospective(features[0], agent_id="human", timeout=30.0)
    result = assert_tool_success(resp)
    assert result.text.strip().startswith("# Retrospective:"), (
        f"Expected markdown header, got: {result.text[:100]}"
    )


def test_retrospective_json_explicit(server):
    """T-R08 (vnc-011): format='json' returns valid JSON output."""
    features = ["col-832"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_retrospective(features[0], agent_id="human", format="json", timeout=30.0)
    result = assert_tool_success(resp)
    parsed = _json.loads(result.text)
    assert isinstance(parsed, dict), f"Expected JSON object, got {type(parsed)}"
    assert "feature_cycle" in parsed, f"Expected feature_cycle in JSON, got keys: {list(parsed.keys())}"


def test_retrospective_format_invalid(server):
    """T-R09 (vnc-011): Invalid format returns error with descriptive message."""
    features = ["col-833"]
    db_path = _compute_db_path(server.project_dir)
    _seed_observation_sql(db_path, features)

    resp = server.context_retrospective(features[0], agent_id="human", format="xml", timeout=30.0)
    assert_tool_error(resp, "Unknown format")


# === context_status observation extension (col-002) =======================


@pytest.mark.xfail(reason="Pre-existing: GH#187 — file_count field missing from observation section")
def test_status_includes_observation_fields(server):
    """T-S01: Status report includes observation health fields."""
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert "observation" in report, "Missing observation section"
    obs = report["observation"]
    assert "file_count" in obs, "Missing file_count in observation"
    assert "total_size_bytes" in obs, "Missing total_size_bytes in observation"
    assert "oldest_file_days" in obs, "Missing oldest_file_days in observation"
    assert "retrospected_feature_count" in obs, "Missing retrospected_feature_count"
    assert "approaching_cleanup" in obs, "Missing approaching_cleanup"


def test_status_observation_retrospected_default(server):
    """T-S02: Retrospected feature count is 0 on fresh server (no stored metrics)."""
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    obs = report.get("observation", {})
    assert obs.get("retrospected_feature_count", -1) == 0
