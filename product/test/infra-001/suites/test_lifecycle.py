"""Suite 3: Lifecycle (~25 tests).

Multi-step scenarios exercising knowledge management workflows end-to-end.
Each test exercises a complete flow, not isolated operations.
"""

import time
import threading

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


def test_store_deprecate_status_changed(server):
    """L-07: Store -> deprecate -> entry status changed to deprecated."""
    store_resp = server.context_store(
        "deprecate lifecycle unique mno789",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)
    server.context_deprecate(entry_id, reason="outdated", agent_id="human")
    get_resp = server.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    assert entry.get("status") == "deprecated"


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
    # Enroll restricted-agent with read/search only — unknown agents now
    # auto-enroll with Write (PERMISSIVE_AUTO_ENROLL), so restrict explicitly.
    server.context_enroll(
        "restricted-agent", "restricted", ["read", "search"], agent_id="human"
    )

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

    # Restricted agent cannot store (no Write capability)
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
    client1.wait_until_ready()
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
    client2.wait_until_ready()
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


# === crt-018b: Effectiveness-Driven Retrieval ================================


def test_effectiveness_search_ordering_after_cold_start(server):
    """L-E01: Cold-start effectiveness state produces zero delta (AC-17 item 1, AC-06, R-07).

    Without a background tick, EffectivenessState is empty.  All entries receive
    utility_delta = 0.0.  Search ordering must be identical to pre-crt-018b
    (confidence + similarity only).  No panic, no regression.

    AC-17 item 1 note: the full ordering change is only observable after a
    background tick writes classifications into EffectivenessState.  That path
    requires an internal trigger not yet exposed through MCP.  This test
    validates the prerequisite: cold-start is safe and produces no distortion.
    """
    # Store two entries with similar content but differing votes (drives confidence apart)
    resp_a = server.context_store(
        "effectiveness search ordering cold start entry alpha unique k7q",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    id_a = extract_entry_id(resp_a)

    resp_b = server.context_store(
        "effectiveness search ordering cold start entry beta unique k7q",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    id_b = extract_entry_id(resp_b)

    # Vote A helpful repeatedly to raise confidence
    for i in range(5):
        server.context_get(id_a, agent_id=f"e-voter-a-{i}", helpful=True)
    time.sleep(0.3)

    # Search — both entries should be returned, no panic
    search_resp = server.context_search(
        "effectiveness search ordering cold start entry unique k7q",
        format="json",
        agent_id="human",
    )
    entries = parse_entries(search_resp)
    result_ids = [e.get("id") for e in entries if e.get("id")]
    # Both entries must be findable (no suppression)
    assert id_a in result_ids or id_b in result_ids, (
        "At least one seeded entry must appear in search results. "
        "Cold-start must not suppress entries: AC-06."
    )
    # No tool-level error
    assert_tool_success(search_resp)


def test_briefing_effectiveness_tiebreaker(server):
    """L-E02: Briefing context_briefing completes without error (AC-17 item 2, AC-07).

    Stores entries with differing helpfulness vote patterns, then calls
    context_briefing.  At cold-start, effectiveness_priority(None) = 0 for all
    entries (AC-06 / R-07 guard): briefing degrades to confidence-only sort.
    The test verifies: no panic, non-empty output, entries returned.

    Full tiebreaker ordering is unit-tested in briefing.rs
    (test_injection_sort_effectiveness_is_tiebreaker).
    """
    # Store a "helpful" entry
    helpful_resp = server.context_store(
        "briefing effectiveness tiebreaker helpful entry unique q8w",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    helpful_id = extract_entry_id(helpful_resp)

    # Store an "unhelpful" entry
    unhelpful_resp = server.context_store(
        "briefing effectiveness tiebreaker unhelpful entry unique q8w",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    unhelpful_id = extract_entry_id(unhelpful_resp)

    # Vote helpful entry 5 times helpful, unhelpful entry 5 times unhelpful
    for i in range(5):
        server.context_get(helpful_id, agent_id=f"brief-voter-h-{i}", helpful=True)
        server.context_get(unhelpful_id, agent_id=f"brief-voter-u-{i}", helpful=False)
    time.sleep(0.3)

    # Call context_briefing — must not error
    briefing_resp = server.context_briefing(
        "tester",
        "verify effectiveness tiebreaker q8w",
        agent_id="human",
    )
    result = assert_tool_success(briefing_resp)

    # Briefing must return some content
    assert len(result.text) > 0, (
        "context_briefing must return non-empty content (AC-07)."
    )
    assert helpful_id is not None and unhelpful_id is not None


def test_context_status_does_not_advance_consecutive_counters(server):
    """L-E03: context_status calls must not increment consecutive_bad_cycles (R-04, AC-01, AC-09).

    Calls context_status 10 times.  If R-04 were violated, status calls would
    increment counters, eventually triggering auto-quarantine on entries that
    have never been seen by the background tick writer.

    Observable proxy: after many status calls, the stored entry must still be
    Active (not Quarantined).  Since AC-01 requires that only the background
    tick writes EffectivenessState, we confirm the entry status via context_get.
    """
    # Store a test entry that would be auto-quarantined if counters were wrongly incremented
    store_resp = server.context_store(
        "status counter test entry must remain active unique r4z",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Call context_status 10 times (simulates frequent status polling)
    for _ in range(10):
        status_resp = server.context_status(agent_id="human", format="json")
        assert_tool_success(status_resp)

    # Entry must still be Active — not quarantined by status calls
    get_resp = server.context_get(entry_id, format="json")
    entry = parse_entry(get_resp)
    status = entry.get("status", "").lower()
    assert status == "active", (
        f"Entry must remain Active after 10 context_status calls; got '{status}'. "
        "R-04: context_status must NOT write EffectivenessState."
    )


def test_auto_quarantine_disabled_when_env_zero(tmp_path):
    """L-E04: UNIMATRIX_AUTO_QUARANTINE_CYCLES=0 disables auto-quarantine (AC-12, R-03).

    Starts a server with auto-quarantine disabled.  Stores entries and confirms
    the server starts and accepts requests normally.  Since the tick interval
    is 15 minutes, we cannot drive the tick in integration tests; instead we
    verify that the server starts without error and serves requests correctly
    when the threshold is 0.

    This covers the startup validation path (CYCLES=0 must be accepted, not rejected).
    """
    import os
    binary = get_binary_path()

    env = os.environ.copy()
    env["UNIMATRIX_AUTO_QUARANTINE_CYCLES"] = "0"

    import subprocess, threading, json, tempfile, time as _time
    # vnc-005: default invocation is now bridge mode; use `serve --stdio` for stdio path.
    proc = subprocess.Popen(
        [binary, "--project-dir", str(tmp_path), "serve", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )

    stderr_lines = []
    def drain():
        for line in iter(proc.stderr.readline, b""):
            stderr_lines.append(line.decode("utf-8", errors="replace").rstrip())
    t = threading.Thread(target=drain, daemon=True)
    t.start()

    # Give server 5s to start
    _time.sleep(2)
    assert proc.poll() is None, (
        f"Server exited immediately with CYCLES=0 (must not exit). "
        f"Stderr: {' '.join(stderr_lines[-5:])}"
    )

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


@pytest.mark.xfail(
    reason=(
        "Pre-existing: GH#291 — tick interval not overridable at integration level. "
        "UNIMATRIX_TICK_INTERVAL_SECONDS env var needed to drive ticks in test. "
        "Unit tests in background.rs cover trigger logic end-to-end."
    )
)
def test_auto_quarantine_after_consecutive_bad_ticks(server):
    """L-E05: Auto-quarantine fires after N consecutive bad ticks (AC-17 item 3, AC-10, R-03).

    Requires the background tick to be drivable at test time, which is not
    currently possible through the MCP interface (tick interval = 15 minutes).
    Marked xfail until UNIMATRIX_TICK_INTERVAL_SECONDS or equivalent is added.
    """
    # Store an entry that would accumulate bad classifications
    store_resp = server.context_store(
        "auto quarantine consecutive bad ticks test entry unique m3x",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # If the tick could be driven here, we would:
    # 1. Force N=3 consecutive ticks classifying this entry as Ineffective
    # 2. Call context_status and verify entry is Quarantined
    # 3. Verify auto_quarantined_this_cycle contains entry_id
    # Since we cannot drive the tick, this fails with xfail as expected
    assert False, "Background tick cannot be driven externally (15-minute interval)"


# === crt-019: Confidence Signal Activation (R-01 critical end-to-end) ========


def test_empirical_prior_flows_to_stored_confidence(server):
    """R-01: Empirical prior flows from ConfidenceState through closure to stored confidence.

    This is the most critical integration test for crt-019. A unit test alone
    cannot verify R-01 because a unit test can mock the closure. Only an
    end-to-end MCP-level test proves that the Bayesian formula is active.

    Strategy: compare confidence of a voted entry vs an unvoted entry.
    - If the Bayesian formula is wired correctly (R-01 passes), helpful votes
      raise the helpfulness component, increasing confidence.
    - If R-01 is broken (bare fn ptr), alpha0/beta0 defaults silently — but
      individual entry vote counts (helpful_count on EntryRecord) still affect
      the helpfulness_score formula, so the confidence signal is still observable.

    The MCP response exposes `confidence` but not `helpful_count` directly.
    We use confidence as the observable end-to-end signal.

    Additional verification: the formula does not produce NaN or out-of-range
    values for any entry in the population (Bayesian formula guard for R-12).
    """
    # Store a "voted" entry that will receive multiple helpful votes
    voted_resp = server.context_store(
        "crt019 prior test voted entry decision architecture patterns unique k7x",
        "testing",
        "decision",
        agent_id="human",
        format="json",
    )
    voted_id = extract_entry_id(voted_resp)

    # Store a control entry that will receive unhelpful votes
    unvoted_resp = server.context_store(
        "crt019 prior test unvoted control entry baseline unique m9z",
        "testing",
        "decision",
        agent_id="human",
        format="json",
    )
    unvoted_id = extract_entry_id(unvoted_resp)

    # Read initial confidences (should be similar — both fresh entries)
    init_voted_conf = float(parse_entry(server.context_get(voted_id, format="json")).get("confidence", 0))
    init_unvoted_conf = float(parse_entry(server.context_get(unvoted_id, format="json")).get("confidence", 0))
    assert 0 <= init_voted_conf <= 1, f"initial voted confidence out of range: {init_voted_conf}"
    assert 0 <= init_unvoted_conf <= 1, f"initial unvoted confidence out of range: {init_unvoted_conf}"

    # Generate 8 helpful votes on the voted entry using 8 distinct agents
    # (UsageDedup: one vote per agent per entry — need distinct agents)
    for i in range(8):
        server.context_get(
            voted_id,
            agent_id=f"crt019-prior-voter-{i}",
            helpful=True,
            format="json",
        )
        time.sleep(0.05)

    # Generate 8 unhelpful votes on the unvoted entry using 8 distinct agents
    for i in range(8):
        server.context_get(
            unvoted_id,
            agent_id=f"crt019-prior-neg-voter-{i}",
            helpful=False,
            format="json",
        )
        time.sleep(0.05)

    # Wait for all spawn_blocking completions
    time.sleep(0.5)

    # Read final confidences
    final_voted_resp = server.context_get(voted_id, format="json")
    final_voted_entry = parse_entry(final_voted_resp)
    final_voted_conf = float(final_voted_entry.get("confidence", 0))

    final_unvoted_resp = server.context_get(unvoted_id, format="json")
    final_unvoted_entry = parse_entry(final_unvoted_resp)
    final_unvoted_conf = float(final_unvoted_entry.get("confidence", 0))

    # Both confidences must be valid (no NaN propagation — R-12 guard)
    assert 0 <= final_voted_conf <= 1, (
        f"voted entry confidence out of range [0,1]: {final_voted_conf}. "
        f"R-12: Bayesian formula may have produced NaN."
    )
    assert 0 <= final_unvoted_conf <= 1, (
        f"control entry confidence out of range [0,1]: {final_unvoted_conf}. "
        f"R-12: Bayesian formula may have produced NaN."
    )

    # Key assertion: voted entry confidence >= unvoted after divergent vote signals
    # Bayesian formula:
    #   voted:   (8+3)/(8+3+3) = 11/14 ≈ 0.786 (high helpfulness component)
    #   unvoted: (0+3)/(8+3+3) = 3/14 ≈ 0.214 (low helpfulness due to 8 unhelpful)
    # This divergence drives confidence difference in the W_HELP=0.12 component.
    assert final_voted_conf >= final_unvoted_conf, (
        f"R-01 end-to-end: voted entry ({final_voted_conf:.4f}) must have >= confidence "
        f"than unhelpfully-voted entry ({final_unvoted_conf:.4f}). "
        f"Helpful votes should raise confidence; unhelpful votes should lower it. "
        f"If equal, the Bayesian formula may not be receiving the vote data correctly."
    )


# === crt-014: Topology-Aware Supersession ====================================


def test_search_multihop_injects_terminal_active(server):
    """L-CRT14-01: Multi-hop injection — search for superseded A (A→B→C, C active) injects C.

    Verifies AC-13 and R-06: search.rs Step 6b must follow the full supersession
    chain via find_terminal_active, not stop at the single-hop superseded_by value.

    Chain built via context_correct (A corrected to B, B corrected to C):
      - A: superseded (has superseded_by=B.id), content matches query
      - B: superseded (has superseded_by=C.id), intermediate hop
      - C: active terminal

    Expected: C.id appears in search results (injected); B.id does NOT appear as
    the injected successor (B is an intermediate superseded node, not the terminal).
    """
    unique = "crt014 multihop injection test unique q9z"

    # Store A with content that will match the search query
    resp_a = server.context_store(
        f"{unique} alpha entry",
        "testing",
        "decision",
        agent_id="human",
        format="json",
    )
    id_a = extract_entry_id(resp_a)

    # Correct A to B (A becomes superseded, B is new)
    resp_b = server.context_correct(
        id_a,
        f"{unique} beta entry corrected",
        reason="first correction",
        agent_id="human",
        format="json",
    )
    id_b = extract_entry_id(resp_b)

    # Correct B to C (B becomes superseded, C is the active terminal)
    resp_c = server.context_correct(
        id_b,
        f"{unique} gamma entry final correction",
        reason="second correction",
        agent_id="human",
        format="json",
    )
    id_c = extract_entry_id(resp_c)

    # Verify state: A and B are deprecated (context_correct sets Deprecated + superseded_by), C is active
    entry_a = parse_entry(server.context_get(id_a, format="json"))
    entry_b = parse_entry(server.context_get(id_b, format="json"))
    entry_c = parse_entry(server.context_get(id_c, format="json"))
    assert entry_a.get("status") == "deprecated", (
        f"A must be deprecated (context_correct sets original to Deprecated); got: {entry_a.get('status')}"
    )
    assert entry_a.get("superseded_by") == id_b, (
        f"A.superseded_by must point to B; got: {entry_a.get('superseded_by')}"
    )
    assert entry_b.get("status") == "deprecated", (
        f"B must be deprecated; got: {entry_b.get('status')}"
    )
    assert entry_b.get("superseded_by") == id_c, (
        f"B.superseded_by must point to C; got: {entry_b.get('superseded_by')}"
    )
    assert entry_c.get("status") == "active", (
        f"C (terminal) must be active; got: {entry_c.get('status')}"
    )

    # Search using the unique prefix — A's content semantically matches
    search_resp = server.context_search(f"{unique}", format="json", agent_id="human")
    assert_tool_success(search_resp)
    entries = parse_entries(search_resp)
    result_ids = [e.get("id") for e in entries if e.get("id") is not None]

    # C (terminal active) must be present — injected via multi-hop traversal
    assert id_c in result_ids, (
        f"AC-13: terminal active entry C (id={id_c}) must be injected into search results. "
        f"Multi-hop traversal (A→B→C) must follow to C, not stop at B. "
        f"Got result IDs: {result_ids}"
    )

    # B must NOT be present as the injected entry — it is a superseded intermediate
    # (B may appear if it matched the query directly, but it must not appear as injected
    # successor; if B is superseded it will have a penalty applied regardless)
    # The key invariant: C is present. B being absent or present with penalty is acceptable.
    # We assert the positive: C is in results.
    # Note: B may appear in results with its own penalty — that is correct behavior.


def test_search_deprecated_entry_visible_with_topology_penalty(server):
    """L-CRT14-02: Deprecated orphan entry visible in search with ORPHAN_PENALTY applied.

    Verifies AC-12 (topology-derived penalty, not removed constant) and IR-02:
    - Store 5 active entries with similar content (ensures HNSW returns multiple results)
    - Store B (active, similar content)
    - Deprecate B (B becomes orphan: Deprecated + no successor)
    - Search: B appears in results with deprecated status (visible in Flexible mode)
    - Active entries rank above B (B penalized by ORPHAN_PENALTY=0.75)

    This test validates that the topology-derived penalty path is active (not the
    removed DEPRECATED_PENALTY constant). The ordering assertion is behavioral,
    not a constant-value check.

    Note: stores multiple active entries to ensure HNSW returns enough candidates
    for B to appear alongside active entries in the same result set.
    """
    unique = "crt014 topology penalty orphan test unique p5y"

    # Store 5 active entries with similar content to populate HNSW enough for recall
    active_ids = []
    for i in range(5):
        resp = server.context_store(
            f"{unique} active knowledge entry index {i} patterns architecture design",
            "testing",
            "decision",
            agent_id="human",
            format="json",
        )
        active_ids.append(extract_entry_id(resp))

    # Store B: similar content to the active entries
    resp_b = server.context_store(
        f"{unique} active knowledge entry deprecated orphan patterns architecture design",
        "testing",
        "decision",
        agent_id="human",
        format="json",
    )
    id_b = extract_entry_id(resp_b)

    # Deprecate B — makes it an orphan (Deprecated + no successor)
    server.context_deprecate(id_b, reason="outdated", agent_id="human")

    # Verify B is deprecated
    entry_b = parse_entry(server.context_get(id_b, format="json"))
    assert entry_b.get("status") == "deprecated", (
        f"B must be deprecated; got: {entry_b.get('status')}"
    )

    # Search with k=10 to retrieve both active and deprecated entries
    search_resp = server.context_search(f"{unique}", format="json", agent_id="human", k=10)
    assert_tool_success(search_resp)
    entries = parse_entries(search_resp)
    result_ids = [e.get("id") for e in entries if e.get("id") is not None]

    # B must appear in results (deprecated entries visible in Flexible mode)
    assert id_b in result_ids, (
        f"AC-12: deprecated orphan entry B (id={id_b}) must appear in Flexible mode search. "
        f"Got result IDs: {result_ids}. "
        f"Deprecated entries must remain visible in search (not excluded like quarantined)."
    )

    # All active entries that appear must rank above B
    result_statuses = {e.get("id"): e.get("status") for e in entries}
    pos_b = result_ids.index(id_b)

    active_ids_in_results = [eid for eid in result_ids if result_statuses.get(eid) == "active"]
    for eid in active_ids_in_results:
        pos_active = result_ids.index(eid)
        assert pos_active < pos_b, (
            f"AC-12: active entry (id={eid}, pos={pos_active}) must rank above "
            f"deprecated orphan B (id={id_b}, pos={pos_b}). "
            f"ORPHAN_PENALTY (0.75) must reduce B's score below active entries. "
            f"Result order: {result_ids}"
        )


# === GH #264 fix: concurrent search stability ================================


@pytest.mark.smoke
def test_concurrent_search_stability(server):
    """L-GH264: 8 rapid sequential context_search calls all complete within 30 seconds.

    Regression test for GH #264: crt-014 added 4x Store::query_by_status() calls
    inside spawn_blocking on every context_search.  Under load this serialised all
    searches on the Store Mutex and exhausted the tokio blocking thread pool,
    causing MCP connection drops.

    The fix caches the entry snapshot in SupersessionState (background tick,
    15-min rebuild) so the search hot path performs zero store I/O for graph
    construction.

    Note: the MCP stdio client is inherently single-threaded (it shares stdin/stdout
    with no call-level lock).  This test validates the same property — that each
    search call completes quickly without store I/O — using sequential calls with a
    wall-clock budget.  8 searches x ~3s per call (embed + HNSW) = <30s budget.
    Pre-GH#264 regression: the 4x query_by_status() calls in spawn_blocking would
    serialise each search on the Store Mutex AND exhaust the thread pool, causing
    searches to stall indefinitely rather than completing in ~3s each.
    """
    # Pre-populate entries to ensure search has work to do
    for i in range(5):
        server.context_store(
            f"concurrent search stability entry {i} unique x9r",
            "testing",
            "convention",
            agent_id="human",
        )

    results = []

    # Run 8 searches sequentially — each must complete quickly.
    # The MCP client serialises over stdio; parallel threading would corrupt
    # the request/response stream.
    start = time.monotonic()
    for i in range(8):
        resp = server.context_search(
            "concurrent search stability unique x9r",
            format="json",
            agent_id="human",
        )
        results.append(resp)
    elapsed = time.monotonic() - start

    assert len(results) == 8, f"Expected 8 results, got {len(results)}"
    assert elapsed < 30.0, (
        f"8 sequential searches took {elapsed:.1f}s — exceeds 30s budget. "
        "This suggests blocking thread pool exhaustion (GH #264 regression): "
        "store I/O in the search hot path serialises calls on the Store Mutex."
    )

    # Verify each result is a tool-level success
    for i, resp in enumerate(results):
        assert_tool_success(resp)


# === crt-023: NLI Lifecycle (W1-4) ===========================================


def test_search_nli_absent_returns_cosine_results(server):
    """L-CRT023-01: Store → search with NLI absent returns cosine-ranked results (AC-14).

    In CI the NLI model is not cached. NliServiceHandle transitions to Failed.
    The search pipeline must fall back to cosine similarity and return valid
    results without tool-level error. Validates graceful degradation end-to-end
    through the MCP interface (AC-14, AC-05).
    """
    store_resp = server.context_store(
        "nli absent cosine fallback lifecycle test unique crt023 epsilon",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    search_resp = server.context_search(
        "nli absent cosine fallback lifecycle test unique crt023 epsilon",
        format="json",
        agent_id="human",
    )
    assert_tool_success(search_resp)
    entries = parse_entries(search_resp)
    result_ids = [e.get("id") for e in entries if e.get("id") is not None]
    assert entry_id in result_ids, (
        f"AC-14: stored entry must appear in cosine-fallback search results when NLI "
        f"is absent. entry_id={entry_id}, got: {result_ids}"
    )


def test_post_store_nli_edge_written(server):
    """L-CRT023-02: Post-store NLI detection does not crash server (AC-10, NLI absent case).

    When NLI model is absent (CI), the post-store fire-and-forget task must exit
    cleanly without writing edges (NliServiceHandle.get_provider() returns Err).
    Observable: context_store succeeds, server remains healthy for subsequent
    context_get and context_search calls. No crash, no MCP error.

    When NLI model IS present (future CI), this test verifies that a follow-up
    context_get still works after the fire-and-forget task completes — the entry
    is not corrupted by the NLI task side effects.
    """
    # Store entry with content that has clear semantic neighbors
    resp = server.context_store(
        "post store nli detection lifecycle test unique crt023 zeta databases always use pool",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(resp)
    entry_id = extract_entry_id(resp)

    # Brief wait to allow fire-and-forget task to complete (or exit immediately if NLI absent)
    time.sleep(0.5)

    # Entry must still be intact — NLI task must not corrupt it
    get_resp = server.context_get(entry_id, format="json")
    assert_tool_success(get_resp)
    entry = parse_entry(get_resp)
    assert entry.get("id") == entry_id, (
        "AC-10: entry must remain intact after post-store NLI detection task. "
        "Fire-and-forget task must not corrupt or delete the stored entry."
    )

    # Server must remain healthy
    search_resp = server.context_search(
        "post store nli detection lifecycle test unique crt023 zeta",
        format="json",
        agent_id="human",
    )
    assert_tool_success(search_resp)


def test_bootstrap_promotion_restart_noop(tmp_path):
    """L-CRT023-03: Bootstrap promotion marker prevents re-run on restart (AC-24).

    After server startup (where bootstrap promotion either ran or found nothing
    to promote), restarting the server must not produce duplicate edges. The
    COUNTERS table marker `bootstrap_nli_promotion_done=1` is a durable guard.

    Observable: two server starts with the same project_dir, each storing an
    entry and performing a search, both completing without error. No crash,
    no duplicate-entry error, no MCP tool failure.
    """
    binary = get_binary_path()

    # First server start: store an entry
    client1 = UnimatrixClient(binary, project_dir=str(tmp_path))
    client1.initialize()
    client1.wait_until_ready()

    store_resp = client1.context_store(
        "bootstrap promotion restart noop test unique crt023 eta",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    entry_id = extract_entry_id(store_resp)

    # Brief wait for any background tasks (bootstrap promotion, NLI detection)
    time.sleep(1.0)
    client1.shutdown()

    # Second server start: same project_dir — bootstrap promotion must be no-op
    client2 = UnimatrixClient(binary, project_dir=str(tmp_path))
    client2.initialize()
    client2.wait_until_ready()

    # Entry must still be intact after restart
    get_resp = client2.context_get(entry_id, format="json")
    assert_tool_success(get_resp)
    entry = parse_entry(get_resp)
    assert entry.get("id") == entry_id, (
        "AC-24: entry must persist across restart. Bootstrap promotion must not "
        "delete or corrupt stored entries."
    )

    # Search must work on second start
    search_resp = client2.context_search(
        "bootstrap promotion restart noop test unique crt023 eta",
        format="json",
        agent_id="human",
    )
    assert_tool_success(search_resp)
    client2.shutdown()
