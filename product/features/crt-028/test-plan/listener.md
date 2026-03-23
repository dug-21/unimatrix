# crt-028 Test Plan: `uds/listener.rs`

Component covers: `sanitize_observation_source` (new private helper, GH #354, ADR-004).

The change is a single-line replacement at line ~813 of `listener.rs`:
- **Before**: `hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string()`
- **After**: `hook: sanitize_observation_source(source.as_deref())`

All unit tests for `sanitize_observation_source` live in the `#[cfg(test)] mod tests` block at the
bottom of `listener.rs`. The function is private (`fn`, not `pub fn`), so it is tested within the
same module.

Integration test (infra-001) supplements with an end-to-end verification of the write path.

---

## `sanitize_observation_source` — AC-11, R-07 (non-negotiable gate)

### `sanitize_observation_source_all_six_cases` (AC-11, R-07 — non-negotiable gate)

This is a single parameterized (or tabular) unit test covering all six allowlist cases from ADR-004
and ACCEPTANCE-MAP.md AC-11.

```
Input                                  | Expected Output
---------------------------------------|--------------------
Some("UserPromptSubmit")               | "UserPromptSubmit"
Some("SubagentStart")                  | "SubagentStart"
None                                   | "UserPromptSubmit"
Some("unknown")                        | "UserPromptSubmit"
Some("")                               | "UserPromptSubmit"
Some("UserPromptSubmitXXXXX")          | "UserPromptSubmit"
```

Assertions per case:
- Call `sanitize_observation_source(input)`
- Assert: returned `String` equals expected output exactly (no trailing spaces, correct case)

Implementation note: write as 6 individual `#[test]` functions (one per case) following the
`sanitize_session_id_*` pattern already present in `listener.rs` tests (lines ~3298–3324).
Example names:
- `sanitize_observation_source_known_user_prompt_submit`
- `sanitize_observation_source_known_subagent_start`
- `sanitize_observation_source_none_defaults_to_user_prompt_submit`
- `sanitize_observation_source_unknown_value_defaults_to_user_prompt_submit`
- `sanitize_observation_source_empty_string_defaults_to_user_prompt_submit`
- `sanitize_observation_source_long_known_prefix_defaults_to_user_prompt_submit`

**Rationale**: AC-11 — all six cases explicitly called out in ACCEPTANCE-MAP.md. R-07 — the allowlist
must be exhaustive; any missing case is a security gap.

---

## R-07: No Second Write Site

### `sanitize_observation_source_is_sole_write_gate` (R-07, architecture verification)

This is a **grep verification**, not a runtime test:

```bash
grep -n "hook:" crates/unimatrix-server/src/uds/listener.rs | grep -v "sanitize_observation_source"
```

Expected: only the `sanitize_observation_source` call site appears in the `hook:` field assignment
for `ObservationRow`. No other write site should bypass the helper.

Verification steps:
1. Count `ObservationRow {` constructions in `listener.rs` — assert only one.
2. Count `\.hook\s*:` field assignments in `ObservationRow` constructions — assert all route through
   `sanitize_observation_source`.

**Rationale**: R-07 — the risk is a second write site added in a future feature. The grep check
in Stage 3c verifies the current implementation is single-site only.

---

## Integration Test (infra-001 `suites/test_security.py`) — R-07 end-to-end

### `test_context_search_source_field_sanitized` (R-07, AC-11)

**Suite**: `suites/test_security.py`
**Fixture**: `server` (fresh DB)

Scenario:
1. The `source` field in `HookRequest::ContextSearch` is set by the hook process (not by MCP callers
   directly). This field is not settable via the standard MCP `context_search` tool interface.
2. **Decision**: If the infra-001 harness cannot set the `source` field directly, this test is
   marked `@pytest.mark.xfail(reason="source field only settable via hook UDS wire, not MCP interface — covered by listener.rs unit tests")`.
3. The unit tests in `listener.rs` provide the primary coverage for this risk. The integration test
   is a belt-and-suspenders check only.

**Alternative approach** (if harness cannot set source directly):
- Verify that the `context_search` tool call succeeds without error (regression smoke)
- Verify that the observation stored has `hook` column = `"UserPromptSubmit"` (default)
- This is a weaker but still valid integration check

**Rationale**: R-07 — end-to-end path confirms the fix is wired (not just unit-tested in isolation).

---

## Existing Tests (AC-14 non-regression)

No existing `listener.rs` tests must be broken by the `sanitize_observation_source` change.

Verify with: `cargo test -p unimatrix-server -- listener::tests 2>&1 | tail -30`

Key existing listener test areas unaffected:
- `sanitize_session_id_*` tests (different helper, different field)
- All `dispatch_request` integration tests in the module
- `handle_compact_payload` tests (no change in GH #354 scope)

The one-line change (`source.as_deref().unwrap_or("UserPromptSubmit")` → `sanitize_observation_source(source.as_deref())`) is semantically equivalent for all existing inputs (`None` → `"UserPromptSubmit"`, `Some("SubagentStart")` → `"SubagentStart"`). No existing behavior changes for canonical inputs.
