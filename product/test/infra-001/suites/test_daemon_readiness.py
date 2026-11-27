"""#1060 unit tests — daemon_server embed-readiness wait (fake stderr source, no daemon)."""

import threading

import pytest

from harness.daemon_readiness import (
    DaemonNotReady,
    readiness_rust_log,
    require_embed_ready,
    wait_for_embed_ready,
)


def _alive():
    return None


def test_wait_returns_when_marker_appears():
    lines, lock = ["starting"], threading.Lock()
    threading.Timer(0.1, lambda: lines.append("INFO embedding model loaded successfully")).start()
    wait_for_embed_ready(lines, lock, _alive, timeout=5.0, interval=0.02)


def test_wait_accepts_retry_variant_after_failed_attempt():
    lines = ["WARN embedding model failed to load, retrying", "embedding model loaded successfully (retry)"]
    wait_for_embed_ready(lines, threading.Lock(), _alive, timeout=1.0, interval=0.02)


def test_wait_timeout_raises_with_stderr_tail():
    lines = ["WARN embedding model failed to load, retrying"]
    with pytest.raises(DaemonNotReady, match="not loaded within") as exc:
        wait_for_embed_ready(lines, threading.Lock(), _alive, timeout=0.2, interval=0.02)
    assert "failed to load" in str(exc.value)


def test_wait_early_exit_raises():
    with pytest.raises(DaemonNotReady, match=r"daemon exited \(code 3\)"):
        wait_for_embed_ready(["boom"], threading.Lock(), lambda: 3, timeout=30.0, interval=0.02)


def test_require_embed_ready_kills_and_fails():
    class _Proc:
        def poll(self):
            return 1

    killed = []
    with pytest.raises(pytest.fail.Exception, match="daemon exited"):
        require_embed_ready([], threading.Lock(), _Proc(), killed.append, timeout=1.0)
    assert len(killed) == 1


@pytest.mark.parametrize(
    "inherited,expected",
    [
        (None, "info"),
        ("warn", "warn,unimatrix_server::infra::embed_handle=info"),
    ],
)
def test_readiness_rust_log(inherited, expected):
    env = {} if inherited is None else {"RUST_LOG": inherited}
    readiness_rust_log(env)
    assert env["RUST_LOG"] == expected
