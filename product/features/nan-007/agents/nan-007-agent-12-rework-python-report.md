# Agent Report: nan-007-agent-12-rework-python

> Task: AC-14 rework — HookPayloadTooLargeError must be a ValueError subclass
> Date: 2026-03-20
> Result: COMPLETE

## Changes Made

### `product/test/infra-001/harness/hook_client.py`
- Line 43: changed `class HookPayloadTooLargeError(HookClientError)` to `class HookPayloadTooLargeError(HookClientError, ValueError)`
- MRO is preserved: existing code catching `HookPayloadTooLargeError` or `HookClientError` still works; code catching `ValueError` now also catches it (AC-14 requirement).

### `product/test/infra-001/tests/test_eval_hooks.py`
- `test_oversized_payload_rejected_before_send`: added `assert isinstance(exc_info.value, ValueError)` assertion with AC-14 message.
- Added new test `test_payload_too_large_raises_as_value_error`: uses `pytest.raises(ValueError)` directly on `_send(b"x" * (MAX_PAYLOAD_SIZE + 1))` — locks the AC-14 interface contract.

## Test Results

```
23 passed, 8 deselected in 50.10s
```

All 23 unit tests pass. The 8 deselected are integration tests requiring a live daemon (not part of this rework scope).

## Files Modified

- `/workspaces/unimatrix/product/test/infra-001/harness/hook_client.py`
- `/workspaces/unimatrix/product/test/infra-001/tests/test_eval_hooks.py`

## Issues / Blockers

None. The fix is a single-line inheritance change; no other code needed updating.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for Python exception multiple inheritance patterns — not run (Python-only, no Rust patterns involved; no crate to query).
- Stored: nothing novel to store — Python multiple inheritance for exception subclassing is a standard language feature, not a crate-specific trap or gotcha invisible in source code.
