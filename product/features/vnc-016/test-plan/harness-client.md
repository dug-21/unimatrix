# Component Test Plan: Harness Client Extension (`client.py`)

## Component

**File**: `product/test/infra-001/harness/client.py`, `context_store()` method (lines 383-414)
**Change**: Add `feature_cycle: str | None = None` keyword parameter after `edges`; add guard block

---

## AC Coverage

| AC-ID | Description |
|-------|-------------|
| AC-05 | `context_store()` accepts optional `feature_cycle` keyword argument; absent by default; forwarded only when non-None |
| AC-06 | All existing integration tests pass without regression |

## Risk Coverage

| Risk ID | How This Component's Tests Address It |
|---------|--------------------------------------|
| R-09 | Guard `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` — key is absent (not `null`) when not provided; code inspection verifies the guard is present |
| R-10 | Full pytest run confirms all existing `context_store` call sites are unaffected by the new keyword parameter |

---

## Unit Test Expectations

There are no standalone unit tests for the harness client extension. Verification is by:
1. Code inspection (AC-05)
2. Behavioral verification through the integration tests (AC-01, AC-08)
3. Full pytest run for regression (AC-06)

---

## Code Inspection Assertions (AC-05)

### Signature Assertion

The new parameter must appear after `edges` in the method signature:

```python
def context_store(
    self,
    content: str,
    topic: str,
    category: str,
    # ... existing params ...
    edges: list | None = None,
    feature_cycle: str | None = None,   # NEW — must appear after edges
) -> MCPResponse:
```

Assertions:
- `feature_cycle: str | None = None` is present in the signature
- It appears AFTER `edges` (not before any positional parameter)
- Default is `None`

### Guard Block Assertion

```python
if feature_cycle is not None:
    args["feature_cycle"] = feature_cycle
```

Assertions:
- The guard is present
- The dict key is exactly `"feature_cycle"` (matches `StoreParams` field name in `tools.rs:143`)
- The guard is `is not None` (not a truthiness check — empty string `""` is a valid cycle ID)
- The guard is inside `context_store`, not at the call site

### Backward Compatibility Assertion

All existing call sites in `test_tools.py`, `test_lifecycle.py`, and other suite files that
call `server.context_store(...)` without `feature_cycle` must be unmodified and must continue
to work. The key must be absent from `args` when `feature_cycle` is `None`.

**Critical**: If `feature_cycle` is inserted before an existing positional argument, existing
callers that pass positional args may silently break (Python resolves positional args by position).
Placing `feature_cycle` after `edges` prevents this.

---

## Integration Test Expectations (Behavioral Verification)

The harness client extension is verified by the integration tests (Component 5):

`test_dependency_on_deprecated_e2e` calls:
```python
server.context_store(
    content="...",
    topic="...",
    category="decision",
    feature_cycle=cycle_id,    # <-- exercises the new parameter
    agent_id=test_agent_id,
)
```

If the guard is missing and `feature_cycle=None` is forwarded as JSON `null`, serde may or
may not accept it (both `null` and absent key deserialize to `Option::None` for
`StoreParams.feature_cycle`). The guard is still required because it preserves the documented
contract and prevents any future serde strictness from causing a regression (R-09).

---

## Regression Test Expectations (AC-06)

After applying the client.py change, run the full infra-001 suite:

```bash
cd product/test/infra-001
python -m pytest suites/ -v --timeout=60 2>&1 | tail -30
```

All existing tests must continue to pass. Any failure in a test that does not use `feature_cycle`
in its `context_store` call indicates a signature regression (R-10).

The following suites make heavy use of `context_store` and are therefore the most relevant
regression targets:
- `test_tools.py` — direct tool parameter tests
- `test_lifecycle.py` — multi-step flows that store then retrieve
- `test_security.py` — store with capability enforcement scenarios

---

## Edge Cases from Risk Strategy

**R-09: Explicit `null` vs. absent key**: The guard `if feature_cycle is not None` means the
`"feature_cycle"` key is absent from `args` when not provided. An explicit `null` in JSON
and an absent key both deserialize to `Option::None` in Rust serde. The guard is still correct
behavior and the documented contract.

**Empty string `feature_cycle=""`**: A non-None but empty string would pass the guard and be
forwarded. The server would store an empty `feature_entries.feature_id`. This is a caller
error, not a harness error — the guard correctly forwards it. No special handling needed.

**`uds_client.py` is NOT modified (C-09)**: The `uds_client.py` is read-only. Only
`harness/client.py` is modified.
