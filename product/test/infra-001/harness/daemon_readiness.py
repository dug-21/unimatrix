"""Passive embed-readiness wait for the ``daemon_server`` fixture (#1060).

The daemon binds its sockets before the embedding model finishes loading, so a
first ``context_store`` over UDS can hit -32004 (EmbedNotReady). Readiness is the
same passive stderr marker ``UnimatrixClient.wait_until_ready`` uses
(``embedding model loaded``, plain and retry variants). No probe tool call: a
UDS-only call would write rows on one parity leg only (nan-021 ADR-003).
"""

from __future__ import annotations

import threading
import time
from typing import Callable

READY_MARKER = "embedding model loaded"
# The info line is emitted from this module; an inherited RUST_LOG (e.g. `warn`)
# must not filter it out, or the wait times out on a healthy daemon.
_READY_LOG_DIRECTIVE = "unimatrix_server::infra::embed_handle=info"


class DaemonNotReady(Exception):
    """Readiness wait failed (timeout or daemon exit). Message carries the stderr tail."""


def readiness_rust_log(env: dict[str, str]) -> None:
    """Set RUST_LOG so the readiness info line is always emitted (mutates ``env``)."""
    current = env.get("RUST_LOG", "").strip()
    env["RUST_LOG"] = f"{current},{_READY_LOG_DIRECTIVE}" if current else "info"


def wait_for_embed_ready(
    lines: list[str],
    lock: threading.Lock,
    poll_exit: Callable[[], int | None],
    timeout: float = 30.0,
    interval: float = 0.2,
    tail: int = 20,
) -> None:
    """Block until ``READY_MARKER`` appears in ``lines``.

    Raises ``DaemonNotReady`` if ``poll_exit()`` reports the daemon exited, or on
    timeout. A "failed to load" line is NOT terminal: the embed loader retries.
    """

    def _tail() -> str:
        with lock:
            return "\n".join(lines[-tail:])

    deadline = time.monotonic() + timeout
    while True:
        with lock:
            if any(READY_MARKER in line for line in lines):
                return
        code = poll_exit()
        if code is not None:
            raise DaemonNotReady(
                f"daemon exited (code {code}) before the embedding model loaded.\n"
                f"--- daemon stderr ---\n{_tail()}"
            )
        if time.monotonic() >= deadline:
            raise DaemonNotReady(
                f"embedding model not loaded within {timeout}s (INFRA, not a parity verdict).\n"
                f"--- daemon stderr ---\n{_tail()}"
            )
        time.sleep(interval)


def require_embed_ready(lines, lock, proc, kill: Callable[[object], None], timeout: float = 30.0) -> None:
    """Fixture wrapper: on ``DaemonNotReady`` kill the daemon and ``pytest.fail`` (fixture ERROR)."""
    import pytest

    try:
        wait_for_embed_ready(lines, lock, proc.poll, timeout=timeout)
    except DaemonNotReady as exc:
        kill(proc)
        pytest.fail(str(exc))
