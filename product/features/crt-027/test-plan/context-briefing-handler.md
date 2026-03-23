# Test Plan: context-briefing-handler (mcp/tools.rs)

## Component

`crates/unimatrix-server/src/mcp/tools.rs`

Changes (inside `#[cfg(feature = "mcp-briefing")]` block):
- Replace `BriefingService::assemble()` with `IndexBriefingService::index()`
- Three-step query derivation via `derive_briefing_query`
- Return flat indexed table via `format_index_table()`
- `BriefingParams.role` is ignored; `task` feeds step 1 of derivation

## Risks Covered

R-06 (query derivation on MCP path), R-08 (feature flag), R-10 (cold-state fallback),
IR-01 (WA-2 histogram boost from parent session)

## ACs Covered

AC-06, AC-07, AC-08 (MCP path), AC-09 (MCP call site), AC-11, AC-13 (no BriefingService in tools.rs)

---

## Unit Test Expectations

Tests in `mcp/tools.rs` `#[cfg(test)]` block.

### Existing Tests to Update

The following existing tests in `tools.rs` reference `BriefingParams` struct:
- `test_briefing_params_required_fields` — currently asserts `role` and `task` are required
- `test_briefing_params_missing_role` — currently asserts error on missing `role`
- `test_briefing_params_missing_task` — currently asserts error on missing `task`

**After crt-027**: `role` may remain a required field for backward compatibility OR
become optional (the spec says `role` is still declared but ignored). If `BriefingParams`
gains a `topic` field as required (replacing `role`), the deserialization tests must be
updated. If `BriefingParams` is backward-compatible (`role` still required, `topic`
optional or derived), existing tests may compile as-is.

**Action**: Confirm the `BriefingParams` struct definition in the updated handler and
update the three existing tests accordingly. Do not delete them.

### New Tests: `context_briefing` Handler Behavior

#### `test_briefing_returns_flat_indexed_table` (AC-08)
**Arrange**: Test database with 2+ active entries
**Act**: Call `context_briefing` handler with a `BriefingParams { topic: "test", role: "architect", task: None, ... }`
**Assert**:
- Response text contains the flat table header (column names: `#`, `id`, `topic`, `cat`, `conf`, `snippet`)
- Response text does NOT contain `"## Decisions"`
- Response text does NOT contain `"## Injections"`
- Response text does NOT contain `"## Conventions"`

#### `test_briefing_active_entries_only` (AC-06)
**Arrange**: Store one `Active` entry and one `Deprecated` entry with the same topic
**Act**: `context_briefing` with `topic` matching both entries
**Assert**: Response contains only the Active entry's ID. Deprecated entry ID absent.

#### `test_briefing_default_k_twenty` (AC-07)
**Arrange**: 25 active entries in store
**Act**: `context_briefing` with no `k` param (use default)
**Assert**: Response contains up to 20 entries; NOT capped at 3 (the old UNIMATRIX_BRIEFING_K default)

#### `test_briefing_k_override` (AC-07 — k param)
**Arrange**: 25 active entries
**Act**: `context_briefing` with `k = 5`
**Assert**: Response contains at most 5 entries

#### `test_briefing_query_derivation_task_param` (AC-09 MCP step 1)
**Arrange**: Store entries tagged with topic "explicit-task-keyword"
**Act**: `context_briefing` with `task = "explicit-task-keyword"`
**Assert**: Response contains entries matching "explicit-task-keyword" (step 1 used task)

#### `test_briefing_query_derivation_topic_fallback` (R-10 scenario 2)
**Arrange**: Empty session state (no topic_signals), `topic = "crt-027"`
**Act**: `context_briefing` with `topic = "crt-027"` and no `task`, no `session_id`
**Assert**: Response is either non-empty (entries matched) or an empty table (no matches) —
no error, no panic. `format_index_table(&[])` returns empty string: verify the MCP result
is a success response (not an error), even for zero entries.

#### `test_briefing_role_field_ignored` (FR-14 backward compat)
**Arrange**: `BriefingParams { role: "architect", task: None, topic: "test" }`
**Act**: `context_briefing` handler processes the request
**Assert**: No error. The `role` field does not cause a dispatch failure. Result is based
on `topic` or `task`, not `role`.

### Integration Tests (infra-001 suites/test_tools.py)

The following scenarios must be added to or updated in the `tools` suite:

#### `test_briefing_no_section_headers` (AC-08)
Using `populated_server` fixture:
- Call `context_briefing(topic="test", role="architect")`
- Assert response does not contain `"## Decisions"`, `"## Injections"`, `"## Conventions"`
- Assert response contains at least one digit (row number in flat table)

#### `test_briefing_returns_active_entries_only` (AC-06)
Using `server` fixture:
- Store active entry with topic "test-active-topic"
- Store deprecated entry with same topic (via `context_deprecate`)
- Call `context_briefing(topic="test-active-topic")`
- Assert deprecated entry ID absent from response

#### `test_briefing_default_k_higher_than_three` (AC-07)
Using `populated_server` fixture (50 pre-loaded entries):
- Call `context_briefing` with no `k` param
- Assert response contains more than 3 entries (validates k=20 default > old k=3 default)

#### `test_briefing_session_wa2_boost` (AC-11)
Using `server` fixture:
- Register a session
- Store entries in category "decision" (5 entries) and "pattern" (1 entry)
- Trigger several searches with `session_id` to build up "decision" histogram
- Call `context_briefing` with that `session_id`
- Assert "decision" category entries rank higher than they would without session_id
- This test requires careful setup; if histogram accumulation is non-trivial, document
  as partially manual and rely on unit-level AC-11 coverage.

---

## Deleted Content Verification (AC-13)

After migration, `mcp/tools.rs` must NOT contain:
- `format_briefing` function call
- `Briefing` struct construction
- `BriefingService` reference (excluding comments)
- `crate::services::briefing::BriefingParams` usage (unless BriefingParams is retained
  as the MCP schema struct — confirm per implementation; see FR-14)

**Gate check**: `grep "format_briefing\|BriefingService\|BriefingResult" crates/unimatrix-server/src/mcp/tools.rs`
returns no results.

---

## Edge Cases

| Scenario | Test | Expected |
|----------|------|----------|
| `topic` param is feature ID with no KB entries | `test_briefing_query_derivation_topic_fallback` | `Ok(empty_table)` |
| `role` field present but ignored | `test_briefing_role_field_ignored` | No error |
| `max_tokens: 1000` cap | Verify `IndexBriefingParams.max_tokens` is passed through | Result respects budget |
| `session_id` present but session expired | FM-04 from RISK-TEST-STRATEGY | Graceful fallback to topic |
