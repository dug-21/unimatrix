# Test Plan: infra-001/test_tools.py

## Component Summary

File `product/test/infra-001/suites/test_tools.py` is modified by adding two new test
functions. These tests exercise the full rmcp dispatch path over stdio — the exact
transport layer where the live bug fires. They are the integration complement to AC-13
(Rust in-process test). The existing 73 tests in this suite are not modified.

---

## Integration Test Expectations

### IT-01: `test_get_with_string_id`

**Fixture**: `server` (function scope, fresh DB — no state leakage)
**Marks**: `@pytest.mark.smoke`
**Placement**: In the `context_get` section of `test_tools.py`, after existing get tests

**Steps**:

```python
@pytest.mark.smoke
def test_get_with_string_id(server):
    """IT-01: context_get accepts string-encoded id over stdio transport."""
    # Arrange: store an entry and obtain its integer id
    store_resp = server.context_store(
        "coercion test content", "testing", "convention", agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)

    # Act: call context_get with id as string
    get_resp = server.context_get(str(entry_id), agent_id="human", format="json")

    # Assert: success and non-empty content
    assert_tool_success(get_resp)
    entry = parse_entry(get_resp)
    assert "coercion test content" in entry.get("content", "")
```

**Assertions**:
- `assert_tool_success(get_resp)` — response must not be an MCP error
- Content must include the stored text — confirms the correct entry was retrieved, not a
  spurious success with empty content
- Must NOT assert `"invalid type: string"` is absent — let `assert_tool_success` fail first

**What this covers**: The full chain from stdio JSON-RPC framing through rmcp dispatch
through `Parameters<GetParams>` deserialization through `deserialize_i64_or_string`. This
is the exact failure path for the reported bug (R-02, AC-13, IT-01).

---

### IT-02: `test_deprecate_with_string_id`

**Fixture**: `server` (function scope, fresh DB)
**Marks**: `@pytest.mark.smoke`
**Placement**: In the `context_deprecate` section of `test_tools.py`

**Steps**:

```python
@pytest.mark.smoke
def test_deprecate_with_string_id(server):
    """IT-02: context_deprecate accepts string-encoded id over stdio transport."""
    # Arrange: store an entry
    store_resp = server.context_store(
        "entry to deprecate via string id", "testing", "convention",
        agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)

    # Act: deprecate with string-encoded id
    dep_resp = server.context_deprecate(str(entry_id), agent_id="human", format="json")

    # Assert: success
    assert_tool_success(dep_resp)
```

**Assertions**:
- `assert_tool_success(dep_resp)` — must not return `invalid type: string` MCP error
- No content check needed — deprecation success is sufficient for this smoke test

**What this covers**: A second affected struct (`DeprecateParams.id`) over stdio transport,
confirming the fix applies to all required-integer fields and not only `GetParams` (R-02,
IT-02).

---

## Existing Suite Coverage Verification

The existing 73 tests in `test_tools.py` cover the pre-fix happy path for all tools using
integer ids. These tests must still pass after the fix (AC-11, R-10). No existing test
passes a string id — so there is no conflict with the new coercion behavior.

**Confirmation needed during Stage 3c**:
- `test_store_roundtrip` (T-03): calls `context_get(entry_id, ...)` where `entry_id` is
  an integer from `extract_entry_id`. Must still pass after annotation change.
- Any test calling `context_deprecate`, `context_quarantine`, `context_correct` with
  integer ids must still pass.

---

## Fixture Usage Notes

Both IT-01 and IT-02 use the `server` fixture (function scope). This is the correct choice
because:
- No state accumulation needed — each test creates its own entry
- No pre-loaded entries needed — tests create their own data
- Function scope ensures no state leakage between IT-01 and IT-02

Do NOT use `shared_server` for these tests. Shared state between the string-id coercion
tests and other tools tests could cause false positives if a prior test leaves an entry
with a known id.

---

## Failure Triage

If IT-01 or IT-02 fail during Stage 3c:

1. Check if the failure is `"invalid type: string"` — this means the struct annotation was
   not applied. Fix the annotation in `tools.rs` (feature-caused failure → fix now).

2. Check if the failure is a different MCP error (e.g., `"entry not found"`) — this means
   the coercion succeeded but the subsequent store lookup failed. Investigate the store
   insert step in the test; the entry may not have been created correctly.

3. If the failure is a pre-existing transport issue unrelated to string id handling:
   file a GH Issue, add `@pytest.mark.xfail(reason="Pre-existing: GH#NNN — description")`,
   and continue.

---

## Risk Coverage for This Component

| Risk | Coverage |
|------|----------|
| R-02 (rmcp dispatch path untested) | IT-01 + IT-02 both marked smoke — CI gate |
| R-07 (path string literal) | Covered implicitly — IT-01/IT-02 exercise the binary built from the workspace including the path string resolution |
| R-10 (regression) | Existing 73 tests run as part of the tools suite — all must pass |
