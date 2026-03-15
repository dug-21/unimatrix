"""Availability test suite — tick liveness, sustained operation, mutex pressure.

All tests marked @pytest.mark.availability. Run with:
    pytest -m availability

These tests are time-extended (30-110s each) and are intended as a pre-release
gate, not a per-feature or per-PR gate. See USAGE-PROTOCOL.md Pre-Release Gate
section.

Wall-clock deadlines use time.time(), NOT pytest-timeout. This is intentional:
pytest-timeout kills the test process, which prevents clean server teardown and
hides hang diagnostics. Instead, we detect hangs and report them as test
failures with timing context.

MCP client is NOT thread-safe. All calls in this suite are sequential.

Known failures (xfail):
  - test_concurrent_ops_during_tick: GH#277 — no handler timeouts
  - test_read_ops_not_blocked_by_tick: GH#277 — no handler timeouts
  - test_sustained_multi_tick: GH#275 — unwrap() kills tick permanently

Deferred (skip):
  - test_tick_panic_recovery: depends on GH#276
"""

import time

import pytest

from harness.client import UnimatrixClient
from harness.conftest import get_binary_path

# All tests in this module are availability tests.
pytestmark = pytest.mark.availability


def test_tick_liveness(fast_tick_server):
    """Verify the server survives a tick cycle and remains responsive afterward.

    Starts a server with a 30-second tick, stores entries, waits 45 seconds
    (tick fires at ~30s, 15s buffer), then asserts both search and status
    still succeed.
    """
    server = fast_tick_server

    # Arrange: store entries to give the tick work to do.
    for i in range(3):
        server.context_store(
            content=f"tick liveness test entry {i}: availability testing data",
            topic="availability-test",
            category="convention",
            agent_id="test-agent",
        )

    # Act: wait long enough for the 30s tick to have fired.
    time.sleep(45)

    # Assert: server still responds to both search and status.
    search_resp = server.context_search(
        query="tick liveness",
        agent_id="test-agent",
    )
    assert search_resp is not None, "search returned None after tick"
    assert search_resp.error is None, (
        f"search returned error after tick: {search_resp.error}"
    )

    status_resp = server.context_status(agent_id="test-agent")
    assert status_resp is not None, "status returned None after tick"
    assert status_resp.error is None, (
        f"status returned error after tick: {status_resp.error}"
    )


def test_cold_start_request_race(tmp_path):
    """Verify no crash when requests arrive before embedding model warms up.

    Starts a fresh server but skips wait_until_ready(), then immediately
    fires search and store requests. Graceful errors or success are both
    acceptable; what is not acceptable is a server crash.
    """
    binary = get_binary_path()
    client = UnimatrixClient(binary, project_dir=str(tmp_path))

    try:
        client.initialize()
        # Intentionally do NOT call client.wait_until_ready() — we want to race
        # requests against the embedding model initialization.

        # Fire search immediately after initialize — may fail gracefully.
        try:
            client.context_search(query="cold start race", agent_id="test-agent")
        except Exception:
            pass  # Graceful errors (timeout, not-ready) are acceptable.

        # Fire store immediately — may fail gracefully.
        try:
            client.context_store(
                content="cold start race test entry",
                topic="availability",
                category="convention",
                agent_id="test-agent",
            )
        except Exception:
            pass  # Graceful errors are acceptable.

        # Assert: server process is still alive — no crash.
        assert client._process.poll() is None, (
            f"Server process crashed during cold-start race "
            f"(returncode={client._process.returncode})"
        )

    finally:
        client.shutdown()


@pytest.mark.xfail(
    strict=False,
    reason=(
        "Pre-existing: GH#277 — no handler timeouts; "
        "requests may hang indefinitely during tick mutex hold"
    ),
)
def test_concurrent_ops_during_tick(fast_tick_server):
    """Verify MCP requests complete within wall-clock deadline during a tick.

    Waits until t≈25s (tick fires at ~30s), then fires 8 sequential MCP
    calls (mixed store/search/status), each must complete within 15 seconds.
    Currently xfail: GH#277 (no handler timeouts means requests may hang).
    """
    server = fast_tick_server

    # Pre-load entries so search has work to do.
    for i in range(5):
        server.context_store(
            content=f"concurrent ops test entry {i}: scheduling and timing data",
            topic="concurrent-test",
            category="convention",
            agent_id="test-agent",
        )

    # Wait until we're in the tick window (tick fires at ~30s).
    time.sleep(25)

    # Fire 8 sequential MCP calls, each must complete within 15s wall-clock.
    operations = [
        lambda: server.context_store(
            content="tick-window op A",
            topic="test",
            category="convention",
            agent_id="test-agent",
        ),
        lambda: server.context_search(query="concurrent", agent_id="test-agent"),
        lambda: server.context_status(agent_id="test-agent"),
        lambda: server.context_store(
            content="tick-window op B",
            topic="test",
            category="pattern",
            agent_id="test-agent",
        ),
        lambda: server.context_search(query="ops test", agent_id="test-agent"),
        lambda: server.context_store(
            content="tick-window op C",
            topic="test",
            category="convention",
            agent_id="test-agent",
        ),
        lambda: server.context_search(query="tick", agent_id="test-agent"),
        lambda: server.context_status(agent_id="test-agent"),
    ]

    for i, op in enumerate(operations):
        op_start = time.time()
        op()
        elapsed = time.time() - op_start
        assert elapsed < 15.0, (
            f"Operation {i} exceeded 15s wall-clock deadline (took {elapsed:.2f}s). "
            "Likely blocked by tick holding SQLite mutex (GH#277)."
        )


@pytest.mark.xfail(
    strict=False,
    reason=(
        "Pre-existing: GH#277 — no handler timeouts; "
        "read ops may block indefinitely during tick mutex hold"
    ),
)
def test_read_ops_not_blocked_by_tick(fast_tick_server):
    """Verify read operations complete within deadline during a tick window.

    Pre-loads 20 entries, waits until t≈35s (tick likely in progress),
    fires 5 search + 5 search-by-topic calls, each within 10s wall-clock.
    Currently xfail: GH#277 (tick holds SQLite mutex, blocking all ops).
    """
    server = fast_tick_server

    # Pre-load 20 entries.
    for i in range(20):
        server.context_store(
            content=(
                f"read-not-blocked entry {i}: knowledge about testing availability "
                "and reliability under concurrent load scenarios"
            ),
            topic="availability",
            category="convention",
            agent_id="test-agent",
        )

    # Wait for tick window (tick fires at ~30s, wait until t≈35s).
    time.sleep(35)

    # Fire 5 search calls — each must complete within 10s.
    for i in range(5):
        op_start = time.time()
        server.context_search(query="availability", agent_id="test-agent")
        elapsed = time.time() - op_start
        assert elapsed < 10.0, (
            f"Search {i} exceeded 10s wall-clock deadline during tick window "
            f"(took {elapsed:.2f}s). "
            "Likely blocked by tick holding SQLite mutex (GH#277)."
        )

    # Fire 5 more search calls with varied queries.
    for i in range(5):
        op_start = time.time()
        server.context_search(
            query=f"read-not-blocked entry {i}",
            agent_id="test-agent",
        )
        elapsed = time.time() - op_start
        assert elapsed < 10.0, (
            f"Read op {i} exceeded 10s wall-clock deadline during tick window "
            f"(took {elapsed:.2f}s). "
            "Likely blocked by tick holding SQLite mutex (GH#277)."
        )


@pytest.mark.xfail(
    strict=False,
    reason=(
        "Pre-existing: GH#275 — unwrap() in tick task kills tick permanently "
        "after first JoinError; subsequent ticks never fire"
    ),
)
@pytest.mark.timeout(150)
def test_sustained_multi_tick(fast_tick_server):
    """Verify server survives 3 full tick cycles without degradation.

    Runs 3 tick cycles at 30s each (~105s total with 5s buffer per cycle).
    After each cycle, asserts search and status both succeed.
    Currently xfail: GH#275 (unwrap() kills the tick task permanently).
    Requires @pytest.mark.timeout(150) to override the default 60s limit.
    """
    server = fast_tick_server
    tick_secs = 30
    buffer_secs = 5
    num_cycles = 3

    for cycle in range(num_cycles):
        # Store an entry each cycle to give the tick work to do.
        server.context_store(
            content=f"sustained tick test cycle {cycle}: long-running reliability data",
            topic="sustained-test",
            category="convention",
            agent_id="test-agent",
        )

        # Wait for this cycle's tick to fire plus a buffer.
        time.sleep(tick_secs + buffer_secs)

        # Assert: server still responds after this tick cycle.
        search_resp = server.context_search(
            query=f"sustained tick test cycle {cycle}",
            agent_id="test-agent",
        )
        assert search_resp is not None, (
            f"Search returned None after tick cycle {cycle}"
        )
        assert search_resp.error is None, (
            f"Search error after tick cycle {cycle}: {search_resp.error}"
        )

        status_resp = server.context_status(agent_id="test-agent")
        assert status_resp is not None, (
            f"Status returned None after tick cycle {cycle}"
        )
        assert status_resp.error is None, (
            f"Status error after tick cycle {cycle}: {status_resp.error}"
        )


@pytest.mark.skip(reason="Deferred: depends on GH#276 — tick supervisor restart not yet implemented")
def test_tick_panic_recovery(fast_tick_server):
    """Verify a tick supervisor restarts the tick task after a panic.

    This test is a stub. Implementation blocked on GH#276.

    Expected behavior (post-fix):
    - Trigger a panic in the tick task
    - Verify the tick supervisor detects the JoinError and restarts the task
    - Verify MCP remains responsive during and after the restart
    - Verify the next tick fires correctly after recovery
    """
    pass
