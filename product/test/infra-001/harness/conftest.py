"""pytest fixtures for server lifecycle management.

Provides function-scoped (fresh per test), module-scoped (shared),
and populated server fixtures. Binary resolution from env var or
workspace fallback, plus a session-scoped binary version preflight.
"""

import logging
import os
import re
import shutil
import signal
import subprocess
import threading
import time
import tomllib

import pytest
from pathlib import Path

from harness.client import UnimatrixClient
from harness.generators import make_entries

logger = logging.getLogger("unimatrix.fixtures")

BINARY_PATH: str | None = None

_WORKSPACE_ROOT = Path(__file__).resolve().parent.parent.parent.parent.parent

# daemon_server (nan-021 C3): how long to poll for the UDS + hook sockets to appear
# after `serve --foreground` is spawned, and how long to wait for graceful SIGTERM exit.
_DAEMON_SOCKET_DEADLINE_S = 15.0
_DAEMON_SOCKET_POLL_S = 0.25
_DAEMON_STOP_TIMEOUT_S = 15.0


def _resolve_binary() -> str:
    """Find the unimatrix binary."""
    env_path = os.environ.get("UNIMATRIX_BINARY")
    if env_path and os.path.isfile(env_path):
        return env_path

    candidates = [
        _WORKSPACE_ROOT / "target" / "release" / "unimatrix",
        _WORKSPACE_ROOT / "target" / "debug" / "unimatrix",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)

    raise RuntimeError(
        "Cannot find unimatrix binary. "
        "Set UNIMATRIX_BINARY env var or build with: cargo build --release"
    )


def get_binary_path() -> str:
    global BINARY_PATH
    if BINARY_PATH is None:
        BINARY_PATH = _resolve_binary()
    return BINARY_PATH


def _workspace_version() -> str | None:
    """Read the workspace version from the root Cargo.toml.

    Returns None when the workspace Cargo.toml is not available (e.g. inside
    the Docker test-runtime image, which only ships the harness).
    """
    cargo_toml = _WORKSPACE_ROOT / "Cargo.toml"
    if not cargo_toml.is_file():
        return None
    try:
        with open(cargo_toml, "rb") as f:
            data = tomllib.load(f)
        return data["workspace"]["package"]["version"]
    except (OSError, tomllib.TOMLDecodeError, KeyError):
        return None


@pytest.fixture(scope="session", autouse=True)
def binary_version_preflight():
    """GH#685 regression guard: fail fast on a stale or mismatched binary.

    The nan-004 bin rename (unimatrix-server -> unimatrix) left a stale
    artifact that, when picked up via UNIMATRIX_BINARY, made all 23 smoke
    tests error with an opaque 'ServerDied code 2'. This preflight runs the
    binary's `version` subcommand once per session and asserts the reported
    version matches the workspace Cargo.toml version, converting that failure
    mode into one self-explanatory session abort.
    """
    binary = get_binary_path()

    try:
        proc = subprocess.run(
            [binary, "version"],
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        pytest.exit(
            f"Binary preflight failed: could not run '{binary} version': {e}. "
            "Check UNIMATRIX_BINARY points at a current unimatrix build.",
            returncode=1,
        )

    if proc.returncode != 0:
        stderr_tail = "\n".join(proc.stderr.splitlines()[-3:])
        pytest.exit(
            f"Binary preflight failed: '{binary} version' exited with code "
            f"{proc.returncode} — the binary is likely a stale pre-rename "
            f"artifact (GH#685). stderr tail:\n{stderr_tail}",
            returncode=1,
        )

    match = re.search(r"unimatrix (\S+)", proc.stdout)
    if not match:
        pytest.exit(
            f"Binary preflight failed: '{binary} version' printed unexpected "
            f"output {proc.stdout.strip()!r} (expected 'unimatrix <version>').",
            returncode=1,
        )
    binary_version = match.group(1)

    expected = _workspace_version()
    if expected is not None and binary_version != expected:
        pytest.exit(
            f"Binary preflight failed: binary at {binary} reports version "
            f"{binary_version} but the workspace Cargo.toml version is "
            f"{expected}. Rebuild with 'cargo build --release' or point "
            "UNIMATRIX_BINARY at a current binary (GH#685).",
            returncode=1,
        )

    logger.info("Binary preflight OK: %s (version %s)", binary, binary_version)


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
        client.wait_until_ready()
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


@pytest.fixture(scope="function")
def fast_tick_server(tmp_path):
    """Fresh server per test with a 30-second tick interval (nan-006).

    Identical to the `server` fixture except UNIMATRIX_TICK_INTERVAL_SECS=30
    is passed to the subprocess, enabling time-extended availability tests
    without waiting 15 minutes for the production tick.

    Use this fixture for @pytest.mark.availability tests that exercise
    tick liveness, sustained operation, and mutex pressure.
    """
    binary = get_binary_path()
    client = UnimatrixClient(
        binary,
        project_dir=str(tmp_path),
        extra_env={"UNIMATRIX_TICK_INTERVAL_SECS": "30"},
    )

    try:
        client.initialize()
        client.wait_until_ready()
    except Exception as e:
        client.shutdown()
        pytest.fail(f"Fast-tick server initialization failed: {e}")

    yield client

    try:
        client.shutdown()
    except Exception as e:
        logger.warning("Fast-tick server shutdown error: %s", e)
    finally:
        stderr = client.get_stderr()
        if stderr:
            logger.debug("Fast-tick server stderr for %s:\n%s", tmp_path, stderr)


@pytest.fixture(scope="module")
def shared_server(tmp_path_factory):
    """One server per test module (for volume/lifecycle suites).

    State accumulates across tests in the module. Uses a higher default
    timeout (60s) since operations slow down with accumulated data.
    UNIMATRIX_WRITE_RATE_LIMIT is raised to allow volume tests to store
    more than the production cap of 60 entries (GH#111).
    """
    binary = get_binary_path()
    tmp_dir = tmp_path_factory.mktemp("shared-server")
    client = UnimatrixClient(
        binary,
        project_dir=str(tmp_dir),
        timeout=60.0,
        extra_env={"UNIMATRIX_WRITE_RATE_LIMIT": "10000"},
    )

    try:
        client.initialize()
        client.wait_until_ready()
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


# ---------------------------------------------------------------------------
# daemon_server (nan-021 C3 / entry #1928) — a LIVE UDS+hook daemon
# ---------------------------------------------------------------------------
#
# The existing `server` fixture spawns `serve --stdio` and opens NO UDS sockets,
# so it cannot back UnimatrixUdsClient / UnimatrixHookClient. The C3 UDS-leg
# baseline drives the parity workload over the real local transports, which need
# a foreground daemon binding both `unimatrix-mcp.sock` (MCP) and `unimatrix.sock`
# (hook IPC). This fixture is the documented daemon tier (#1928): spawn
# `serve --foreground`, poll for both sockets, yield their paths + the per-slug
# store DIR (the durability-barrier sampling target), then SIGTERM on teardown.
#
# EXTENDS the conftest fixture family — it does NOT fork a new spawn/transport
# path; UnimatrixUdsClient / UnimatrixHookClient are the existing clients that
# connect to the sockets this fixture surfaces (AC-07).


def _drain_stream(stream, sink: list[str], lock: threading.Lock) -> None:
    """Continuously drain a child stream into `sink` (capture-first; #5266/#5267)."""
    try:
        for line_bytes in iter(stream.readline, b""):
            try:
                line = line_bytes.decode("utf-8", errors="replace").rstrip()
            except Exception:
                line = repr(line_bytes)
            with lock:
                sink.append(line)
    except Exception:
        pass


@pytest.fixture(scope="function")
def daemon_server(tmp_path):
    """Live local daemon over UDS + hook IPC (the C3 UDS-leg substrate).

    Spawns ``unimatrix --project-dir <tmp> serve --foreground`` so the daemon
    binds the real MCP UDS socket and hook socket under an ISOLATED per-test
    data dir (the project hash derives from the git-less tmp project dir, so no
    real ``~/.unimatrix`` state is touched). Yields a dict:

        {
            "mcp_socket_path": <…/unimatrix-mcp.sock>,   # UnimatrixUdsClient target
            "socket_path":     <…/unimatrix.sock>,       # UnimatrixHookClient target
            "store_dir":       <data_dir>,               # durability-barrier sample dir
            "project_dir":     <tmp_path>,
            "pid":             <int>,
        }

    Teardown SIGTERMs the daemon, waits up to 15 s, SIGKILLs on timeout, dumps
    captured stderr on a non-clean exit, and removes the isolated data dir.
    """
    binary = get_binary_path()
    project_dir = tmp_path / "project"
    project_dir.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "info")

    stderr_lines: list[str] = []
    stderr_lock = threading.Lock()

    proc = subprocess.Popen(
        [binary, "--project-dir", str(project_dir), "serve", "--foreground"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        env=env,
    )
    drain = threading.Thread(
        target=_drain_stream, args=(proc.stderr, stderr_lines, stderr_lock), daemon=True
    )
    drain.start()

    def _stderr_tail(n: int = 20) -> str:
        with stderr_lock:
            return "\n".join(stderr_lines[-n:])

    # The daemon derives its data dir from the project hash; we discover the data
    # dir (and both socket paths) by polling for the MCP socket to appear. The
    # hook socket is a sibling in the same data dir.
    mcp_socket: Path | None = None
    hook_socket: Path | None = None
    store_dir: Path | None = None
    deadline = time.monotonic() + _DAEMON_SOCKET_DEADLINE_S

    while time.monotonic() < deadline:
        if proc.poll() is not None:
            _kill(proc)
            pytest.fail(
                f"daemon exited (code {proc.returncode}) before binding sockets.\n"
                f"--- daemon stderr ---\n{_stderr_tail()}"
            )
        found = _find_daemon_data_dir(project_dir, binary, env)
        if found is not None:
            data_dir, mcp_socket, hook_socket = found
            if mcp_socket.exists() and hook_socket.exists():
                store_dir = data_dir
                break
        time.sleep(_DAEMON_SOCKET_POLL_S)

    if mcp_socket is None or store_dir is None:
        _kill(proc)
        pytest.fail(
            f"daemon sockets did not appear within {_DAEMON_SOCKET_DEADLINE_S}s.\n"
            f"--- daemon stderr ---\n{_stderr_tail()}"
        )

    try:
        yield {
            "mcp_socket_path": str(mcp_socket),
            "socket_path": str(hook_socket),
            "store_dir": str(store_dir),
            "project_dir": str(project_dir),
            "pid": proc.pid,
        }
    finally:
        rc = _stop_daemon(proc)
        if rc not in (0, -signal.SIGTERM):
            logger.warning(
                "daemon teardown non-clean exit (rc=%s); stderr tail:\n%s",
                rc,
                _stderr_tail(),
            )
        # Remove the isolated data dir so per-test daemons never accumulate state.
        if store_dir is not None:
            shutil.rmtree(store_dir, ignore_errors=True)


def _project_hash(project_dir: Path) -> str:
    """Mirror unimatrix-engine project hashing: first 16 hex of SHA-256 over the
    canonical project-root path string (project.rs::compute_project_hash). The
    tmp project dir is git-less, so detect_project_root resolves it to itself —
    we canonicalize identically (realpath) so the hash matches the daemon's."""
    import hashlib

    canonical = os.path.realpath(str(project_dir))
    digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    return digest[:16]


def _find_daemon_data_dir(
    project_dir: Path, binary: str, env: dict
) -> tuple[Path, Path, Path] | None:
    """Resolve the daemon's data dir for `project_dir` and return
    (data_dir, mcp_socket, hook_socket), or None if not yet resolvable.

    The data dir is ``{base}/{hash16}`` (project.rs::ensure_data_directory) where
    ``hash16`` is the deterministic project hash — computed here identically to
    the Rust side, so the lookup is exact and parallel-safe (no newest-socket
    heuristic). The base is ``$HOME/.unimatrix`` (production default; no base-dir
    override exists in the shipped binary, so we honor it exactly — NFR-1).
    """
    home = Path(env.get("HOME") or os.path.expanduser("~"))
    data_dir = home / ".unimatrix" / _project_hash(project_dir)
    if not data_dir.is_dir():
        return None
    return data_dir, data_dir / "unimatrix-mcp.sock", data_dir / "unimatrix.sock"


def _kill(proc: subprocess.Popen) -> None:
    """Best-effort hard kill of a daemon process."""
    try:
        proc.kill()
        proc.wait(timeout=5)
    except Exception:
        pass


def _stop_daemon(proc: subprocess.Popen) -> int | None:
    """SIGTERM the daemon, wait up to the stop timeout, SIGKILL on overrun."""
    if proc.poll() is not None:
        return proc.returncode
    try:
        proc.send_signal(signal.SIGTERM)
    except Exception:
        pass
    try:
        return proc.wait(timeout=_DAEMON_STOP_TIMEOUT_S)
    except subprocess.TimeoutExpired:
        _kill(proc)
        return proc.returncode
