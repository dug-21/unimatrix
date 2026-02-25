# Pseudocode: C5 — pytest Fixtures

## File: `harness/conftest.py`

## Design

pytest fixtures manage server lifecycle. Function-scoped fixtures give each test a fresh server. Module-scoped fixtures share a server across a test module (for volume/lifecycle suites where state accumulation is the test).

## Constants

```python
import os
import logging
import pytest
from pathlib import Path
from harness.client import UnimatrixClient
from harness.generators import make_entries

logger = logging.getLogger("unimatrix.fixtures")

# Binary path resolution (FR-05.4)
def _resolve_binary() -> str:
    """Find the unimatrix-server binary."""
    # 1. Environment variable (Docker sets this)
    env_path = os.environ.get("UNIMATRIX_BINARY")
    if env_path and os.path.isfile(env_path):
        return env_path

    # 2. Fallback: search from workspace root
    # Walk up from this file to find workspace root
    workspace_root = Path(__file__).resolve().parent.parent.parent.parent
    candidates = [
        workspace_root / "target" / "release" / "unimatrix-server",
        workspace_root / "target" / "debug" / "unimatrix-server",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)

    raise RuntimeError(
        "Cannot find unimatrix-server binary. "
        "Set UNIMATRIX_BINARY env var or build with: cargo build --release"
    )


BINARY_PATH = None  # resolved lazily

def get_binary_path() -> str:
    global BINARY_PATH
    if BINARY_PATH is None:
        BINARY_PATH = _resolve_binary()
    return BINARY_PATH
```

## Fixtures

```python
@pytest.fixture(scope="function")
def server(tmp_path):
    """
    Fresh server per test (default fixture).

    Creates a unique temp directory, spawns the server, initializes MCP,
    yields the client, then shuts down and captures stderr.

    FR-05.1: function-scoped, unique temp dir, no state leakage.
    """
    binary = get_binary_path()
    client = UnimatrixClient(binary, project_dir=str(tmp_path))

    try:
        client.initialize()
    except Exception as e:
        # Cleanup on init failure
        client.shutdown()
        pytest.fail(f"Server initialization failed: {e}")

    yield client

    # Teardown (FR-05.5: log diagnostics but don't fail teardown)
    try:
        client.shutdown()
    except Exception as e:
        logger.warning(f"Server shutdown error: {e}")
    finally:
        stderr = client.get_stderr()
        if stderr:
            logger.debug(f"Server stderr for {tmp_path}:\n{stderr}")


@pytest.fixture(scope="module")
def shared_server(tmp_path_factory):
    """
    One server per test module (for volume/lifecycle suites).

    FR-05.2: module-scoped, state accumulates across tests in the module.
    """
    binary = get_binary_path()
    tmp_dir = tmp_path_factory.mktemp("shared-server")
    client = UnimatrixClient(binary, project_dir=str(tmp_dir))

    try:
        client.initialize()
    except Exception as e:
        client.shutdown()
        pytest.fail(f"Shared server initialization failed: {e}")

    yield client

    try:
        client.shutdown()
    except Exception as e:
        logger.warning(f"Shared server shutdown error: {e}")
    finally:
        stderr = client.get_stderr()
        if stderr:
            logger.debug(f"Shared server stderr:\n{stderr}")


@pytest.fixture(scope="function")
def populated_server(server):
    """
    Server pre-loaded with standard dataset.

    FR-05.3: wraps server fixture, loads 50 entries across 5 topics and 3 categories.
    Returns client with data ready for query testing.
    """
    entries = make_entries(
        50,
        seed=12345,
        topic_distribution={
            "testing": 0.3,
            "architecture": 0.25,
            "deployment": 0.2,
            "security": 0.15,
            "performance": 0.1,
        },
        category_mix=["convention", "pattern", "decision"],
    )

    stored_ids = []
    for entry in entries:
        resp = server.context_store(
            agent_id="human",  # Privileged agent for write access
            **entry,
        )
        # We don't assert here — if store fails, the test using this fixture will fail
        stored_ids.append(resp)

    # Attach metadata to client for test access
    server._test_entries = entries
    server._test_stored_ids = stored_ids
    return server


@pytest.fixture(scope="function")
def admin_server(server):
    """
    Server with admin agent context.

    Convenience fixture that stores a reference to the admin agent_id.
    Uses "human" which is bootstrapped as Privileged with all capabilities.
    """
    server._admin_agent_id = "human"
    return server
```

## File: `suites/conftest.py`

```python
# Re-export harness fixtures so pytest discovers them for suites/
# This file makes harness/conftest.py fixtures available to suites/*.py

import sys
from pathlib import Path

# Add harness parent to path so imports work
harness_dir = Path(__file__).resolve().parent.parent
if str(harness_dir) not in sys.path:
    sys.path.insert(0, str(harness_dir))

from harness.conftest import server, shared_server, populated_server, admin_server  # noqa: F401
```

## Key Design Decisions

- Binary resolution: env var first (Docker), then workspace fallback (local dev)
- Function-scoped by default: every test gets clean state
- Module-scoped only for volume/lifecycle where accumulation IS the test
- Populated server uses deterministic seed (12345) for reproducible dataset
- Teardown is defensive: log errors but never fail (FR-05.5)
- Admin agent uses "human" (bootstrapped as Privileged in server)
