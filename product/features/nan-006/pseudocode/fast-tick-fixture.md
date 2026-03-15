# Pseudocode: C2 — fast_tick_server fixture + UnimatrixClient extra_env

## Files Modified
- `product/test/infra-001/harness/client.py` — add `extra_env` parameter
- `product/test/infra-001/harness/conftest.py` — add `fast_tick_server` fixture
- `product/test/infra-001/suites/conftest.py` — re-export `fast_tick_server`

---

## 1. client.py — UnimatrixClient extra_env parameter

### Current signature
```
def __init__(
    self,
    binary_path: str | Path,
    project_dir: str | Path | None = None,
    timeout: float = DEFAULT_TIMEOUT,
):
```

### New signature
```
def __init__(
    self,
    binary_path: str | Path,
    project_dir: str | Path | None = None,
    timeout: float = DEFAULT_TIMEOUT,
    extra_env: dict[str, str] | None = None,
):
```

### Change in body (after existing env setup)
Current:
```python
env = os.environ.copy()
env.setdefault("RUST_LOG", "info")
```

New:
```python
env = os.environ.copy()
env.setdefault("RUST_LOG", "info")
if extra_env:
    env.update(extra_env)
```

Everything else in __init__ remains unchanged.

---

## 2. conftest.py — fast_tick_server fixture

### Location: product/test/infra-001/harness/conftest.py
### Add after existing `server` fixture (line ~77):

```python
@pytest.fixture(scope="function")
def fast_tick_server(tmp_path):
    """Fresh server per test with 30-second tick interval.

    Identical to the `server` fixture except UNIMATRIX_TICK_INTERVAL_SECS=30
    is passed to the subprocess, enabling time-extended availability tests
    without waiting 15 minutes for the production tick.
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
```

---

## 3. suites/conftest.py — re-export fast_tick_server

Add `fast_tick_server` to the re-export line:

Current:
```python
from harness.conftest import server, shared_server, populated_server, admin_server  # noqa: F401
```

New:
```python
from harness.conftest import server, shared_server, populated_server, admin_server, fast_tick_server  # noqa: F401
```

---

## Data Flow

```
fast_tick_server fixture
    └─ UnimatrixClient(extra_env={"UNIMATRIX_TICK_INTERVAL_SECS": "30"})
            └─ env = os.environ.copy()
               env.update({"UNIMATRIX_TICK_INTERVAL_SECS": "30"})
               subprocess.Popen([binary, ...], env=env)
                    └─ background.rs reads env var at startup
                         └─ tick_interval_secs = 30
```

## Error Handling
- If binary not found: `RuntimeError` propagated → pytest.fail in fixture
- If server fails to initialize: shutdown called, pytest.fail with message
- Teardown: same SIGTERM/SIGKILL logic as `server` fixture

## Key Test Scenarios
- `fast_tick_server` yields a working UnimatrixClient (can call context_store, context_search)
- Server with TICK_INTERVAL=30 fires tick at ~t=30s (verified by test_tick_liveness)
