# Pseudocode: C3 — test_availability.py

## File
`product/test/infra-001/suites/test_availability.py`

## Module Header

```python
"""Availability test suite — tick liveness, sustained operation, mutex pressure.

All tests marked @pytest.mark.availability. Run with:
    pytest -m availability

These tests are time-extended (30-110s each) and are intended as a pre-release gate,
not a per-feature or per-PR gate. See USAGE-PROTOCOL.md Pre-Release Gate section.

Wall-clock deadlines use time.time(), NOT pytest-timeout. This is intentional:
pytest-timeout kills the test process, which prevents clean server teardown and
hides hang diagnostics. Instead, we detect hangs and report them as test failures.

MCP client is NOT thread-safe. All calls in this suite are sequential.
"""

import pytest
import time

# All tests in this suite are availability tests
pytestmark = pytest.mark.availability
```

---

## Test 1: test_tick_liveness

```
Purpose: Verify the server survives a tick cycle and remains responsive afterward.

@pytest.mark.availability
def test_tick_liveness(fast_tick_server):
    # Arrange: store 3 entries to give the tick work to do
    server = fast_tick_server
    for i in range(3):
        server.context_store(
            content=f"tick liveness test entry {i}",
            topic="availability-test",
            category="convention",
            agent_id="test-agent",
        )

    # Act: wait 45s to ensure the 30s tick has fired (buffer of 15s)
    time.sleep(45)

    # Assert: server still responds to both search and status
    search_resp = server.context_search(
        query="tick liveness",
        agent_id="test-agent",
    )
    assert search_resp is not None, "search returned None after tick"
    assert search_resp.error is None, f"search error after tick: {search_resp.error}"

    status_resp = server.context_status(agent_id="test-agent")
    assert status_resp is not None, "status returned None after tick"
    assert status_resp.error is None, f"status error after tick: {status_resp.error}"
```

---

## Test 2: test_cold_start_request_race

```
Purpose: Verify the server handles immediate requests before embedding model warms up.
No crash, no unhandled panic — graceful errors or success both acceptable.

@pytest.mark.availability
def test_cold_start_request_race(tmp_path):
    # Arrange: start a fresh server (not warmed up yet)
    binary = get_binary_path()
    from harness.client import UnimatrixClient
    client = UnimatrixClient(binary, project_dir=str(tmp_path))

    try:
        client.initialize()
        # NOTE: do NOT call wait_until_ready() — we want to race

        # Act: fire search and store immediately after initialize
        # These may fail gracefully (embedding not ready) — that's OK
        # What we CANNOT tolerate is a server crash (process exit)
        try:
            search_resp = client.context_search(query="cold start", agent_id="test-agent")
            # Either success or graceful error is acceptable
        except Exception as e:
            # Client-level errors (timeout, etc.) are acceptable — no crash
            pass

        try:
            store_resp = client.context_store(
                content="cold start test",
                topic="test",
                category="convention",
                agent_id="test-agent",
            )
        except Exception as e:
            pass

        # Assert: server process is still alive (no crash)
        assert client._process.poll() is None, \
            f"Server crashed during cold-start race (returncode={client._process.returncode})"

    finally:
        client.shutdown()
```

---

## Test 3: test_concurrent_ops_during_tick (XFAIL)

```
Purpose: Verify that MCP requests don't hang indefinitely during a tick.
Each of 8 sequential calls must complete within 15s wall-clock deadline.
Currently xfail because there are no handler timeouts (GH#277).

@pytest.mark.xfail(strict=False, reason="Pre-existing: GH#277 — no handler timeouts; requests may hang during tick")
@pytest.mark.availability
def test_concurrent_ops_during_tick(fast_tick_server):
    server = fast_tick_server

    # Pre-load entries so search has work to do
    for i in range(5):
        server.context_store(
            content=f"concurrent ops test entry {i}",
            topic="concurrent-test",
            category="convention",
            agent_id="test-agent",
        )

    # Wait until we're in the tick window (t≈25-35s — tick fires at ~30s)
    time.sleep(25)

    # Fire 8 sequential MCP calls, each must complete within 15s wall-clock
    # Mix of store, search, status
    ops = [
        lambda: server.context_store(content="tick-window op", topic="test", category="convention", agent_id="test-agent"),
        lambda: server.context_search(query="concurrent", agent_id="test-agent"),
        lambda: server.context_status(agent_id="test-agent"),
        lambda: server.context_store(content="tick-window op 2", topic="test", category="pattern", agent_id="test-agent"),
        lambda: server.context_search(query="ops test", agent_id="test-agent"),
        lambda: server.context_store(content="tick-window op 3", topic="test", category="convention", agent_id="test-agent"),
        lambda: server.context_search(query="tick", agent_id="test-agent"),
        lambda: server.context_status(agent_id="test-agent"),
    ]

    for i, op in enumerate(ops):
        deadline = time.time() + 15.0
        op()  # If this hangs > 15s, the test process itself will hang (detected by pytest-timeout(150) on suite level)
        elapsed = 15.0 - (deadline - time.time())
        assert time.time() <= deadline, f"Op {i} exceeded 15s wall-clock deadline (took {elapsed:.1f}s)"
```

---

## Test 4: test_read_ops_not_blocked_by_tick (XFAIL)

```
Purpose: Verify read operations (search, get) complete within wall-clock deadline during tick window.
Currently xfail because the tick holds the SQLite mutex, blocking all ops (GH#277).

@pytest.mark.xfail(strict=False, reason="Pre-existing: GH#277 — no handler timeouts; reads may block during tick")
@pytest.mark.availability
def test_read_ops_not_blocked_by_tick(fast_tick_server):
    server = fast_tick_server

    # Pre-load 20 entries
    entry_ids = []
    for i in range(20):
        resp = server.context_store(
            content=f"read-not-blocked entry {i}: knowledge about testing availability",
            topic="availability",
            category="convention",
            agent_id="test-agent",
        )
        # Extract ID from response if possible (for get calls later)
        if resp and resp.result:
            entry_id = resp.result.get("content", [{}])[0].get("text", "")
            entry_ids.append(entry_id)

    # Wait for tick window (t≈35-40s — tick should be in progress)
    time.sleep(35)

    # Fire 5 search + 5 get calls, each must complete within 10s
    for i in range(5):
        deadline = time.time() + 10.0
        server.context_search(query="availability", agent_id="test-agent")
        assert time.time() <= deadline, f"Search {i} exceeded 10s deadline during tick window"

    # NOTE: context_get requires a valid ID. Use context_lookup with a known topic
    for i in range(5):
        deadline = time.time() + 10.0
        server.context_search(query=f"read-not-blocked entry {i}", agent_id="test-agent")
        assert time.time() <= deadline, f"Read op {i} exceeded 10s deadline during tick window"
```

---

## Test 5: test_sustained_multi_tick (XFAIL)

```
Purpose: Verify server survives 3 full tick cycles without degradation.
Currently xfail because an unwrap() in the tick task permanently kills it (GH#275).
Total duration: ~100-110s.

@pytest.mark.xfail(strict=False, reason="Pre-existing: GH#275 — unwrap() in tick task kills tick permanently after first error")
@pytest.mark.timeout(150)
@pytest.mark.availability
def test_sustained_multi_tick(fast_tick_server):
    server = fast_tick_server
    TICK_SECS = 30
    NUM_CYCLES = 3

    for cycle in range(NUM_CYCLES):
        # Store an entry each cycle
        server.context_store(
            content=f"sustained tick test cycle {cycle}",
            topic="sustained-test",
            category="convention",
            agent_id="test-agent",
        )

        # Wait for this cycle's tick to fire + buffer
        wait_secs = TICK_SECS + 5  # 35s per cycle
        time.sleep(wait_secs)

        # Assert: server still responds after this tick cycle
        search_resp = server.context_search(
            query=f"sustained tick test cycle {cycle}",
            agent_id="test-agent",
        )
        assert search_resp is not None, f"Search returned None after cycle {cycle}"
        assert search_resp.error is None, \
            f"Search error after tick cycle {cycle}: {search_resp.error}"

        status_resp = server.context_status(agent_id="test-agent")
        assert status_resp is not None, f"Status returned None after cycle {cycle}"
        assert status_resp.error is None, \
            f"Status error after tick cycle {cycle}: {status_resp.error}"

    # Total: 3 * 35s = 105s + startup time. pytest-timeout(150) guards this.
```

---

## Test 6: test_tick_panic_recovery (SKIP)

```
Purpose: Stub for future tick panic recovery test. Deferred until GH#276 is resolved.

@pytest.mark.skip(reason="Deferred: depends on GH#276 — tick supervisor restart not yet implemented")
@pytest.mark.availability
def test_tick_panic_recovery(fast_tick_server):
    """When the tick task panics, a supervisor should restart it.

    This test is a stub. Implementation blocked on GH#276.
    Expected behavior (post-fix):
    - Trigger a panic in the tick task
    - Verify the tick supervisor restarts the tick
    - Verify MCP remains responsive
    - Verify the next tick fires correctly
    """
    pass
```

---

## Imports Required

```python
import pytest
import time

from harness.client import UnimatrixClient
from harness.conftest import get_binary_path
```

## Notes on Wall-Clock vs pytest-timeout

pytest-timeout kills the test process at the timeout boundary. This is appropriate for
preventing infinite hangs in CI, but it prevents clean server teardown and hides diagnostics.

For availability tests:
- Each individual assertion uses `time.time()` to detect per-operation hangs
- The test itself can run up to pytest.mark.timeout(N) total
- For test_sustained_multi_tick, timeout(150) is required because the test legitimately
  takes ~100-110s; without it, the default 60s timeout would kill it incorrectly
- For other tests, the default 60s timeout is appropriate as a backstop
