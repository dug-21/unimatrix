"""pytest fixtures for server lifecycle management.

Provides function-scoped (fresh per test), module-scoped (shared),
and populated server fixtures. Binary resolution from env var or
workspace fallback.
"""

import logging
import os

import pytest
from pathlib import Path

from harness.client import UnimatrixClient
from harness.generators import make_entries

logger = logging.getLogger("unimatrix.fixtures")

BINARY_PATH: str | None = None


def _resolve_binary() -> str:
    """Find the unimatrix-server binary."""
    env_path = os.environ.get("UNIMATRIX_BINARY")
    if env_path and os.path.isfile(env_path):
        return env_path

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


def get_binary_path() -> str:
    global BINARY_PATH
    if BINARY_PATH is None:
        BINARY_PATH = _resolve_binary()
    return BINARY_PATH


@pytest.fixture(scope="function")
def server(tmp_path):
    """Fresh server per test (default fixture).

    Creates a unique temp directory, spawns the server, initializes MCP,
    yields the client, then shuts down and captures stderr.
    """
    binary = get_binary_path()
    client = UnimatrixClient(binary, project_dir=str(tmp_path))

    try:
        client.initialize()
    except Exception as e:
        client.shutdown()
        pytest.fail(f"Server initialization failed: {e}")

    yield client

    try:
        client.shutdown()
    except Exception as e:
        logger.warning("Server shutdown error: %s", e)
    finally:
        stderr = client.get_stderr()
        if stderr:
            logger.debug("Server stderr for %s:\n%s", tmp_path, stderr)


@pytest.fixture(scope="module")
def shared_server(tmp_path_factory):
    """One server per test module (for volume/lifecycle suites).

    State accumulates across tests in the module.
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
        logger.warning("Shared server shutdown error: %s", e)
    finally:
        stderr = client.get_stderr()
        if stderr:
            logger.debug("Shared server stderr:\n%s", stderr)


@pytest.fixture(scope="function")
def populated_server(server):
    """Server pre-loaded with standard dataset.

    Loads 50 entries across 5 topics and 3 categories.
    Attaches _test_entry_ids to client for test access.
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
        resp = server.context_store(agent_id="human", **entry)
        stored_ids.append(resp)

    server._test_entries = entries
    server._test_stored_responses = stored_ids
    return server


@pytest.fixture(scope="function")
def admin_server(server):
    """Server with admin agent context reference.

    Uses 'human' which is bootstrapped as Privileged with all capabilities.
    """
    server._admin_agent_id = "human"
    return server
