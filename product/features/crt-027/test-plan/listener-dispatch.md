# Test Plan: listener-dispatch (uds/listener.rs)

## Component

`crates/unimatrix-server/src/uds/listener.rs`

Changes:
- `dispatch_request` ContextSearch arm: replace hardcoded `"UserPromptSubmit"` with
  `source.as_deref().unwrap_or("UserPromptSubmit")`
- `handle_compact_payload`: replace `BriefingService::assemble()` with `IndexBriefingService::index()`
- `format_compaction_payload`: rewritten to accept `Vec<IndexEntry>`, emit flat table
- Delete `CompactionCategories` struct

## Risks Covered

R-01 (scenario 5), R-03 (ALL 11 named tests), R-12, R-14, IR-01, IR-04

## ACs Covered

AC-05 (b, c), AC-08 (CompactPayload path), AC-10, AC-12, AC-15, AC-16, AC-17, AC-18,
AC-19, AC-20, AC-21

---

## Unit Test Expectations

### Non-Negotiable Test Names (mandatory — grep at Gate 3c)

All 11 of the following tests MUST exist in `listener.rs` `#[cfg(test)]`. They replace
the prior tests on `CompactionCategories`.

#### `format_payload_empty_entries_returns_none` (AC-18, R-03 scenario 1)
**Arrange**: `entries: &[]`, `histogram: &HashMap::new()`
**Act**: `format_compaction_payload(&[], None, None, 0, MAX_COMPACTION_BYTES, &HashMap::new())`
**Assert**: `result.is_none()`

#### `format_payload_header_present` (R-03 scenario 2)
**Arrange**: `entries: &[one_entry]`, compaction count = 1
**Act**: `format_compaction_payload(...)`
**Assert**: `result.unwrap().contains("--- Unimatrix Compaction Context ---\n")`

#### `format_payload_sorted_by_confidence` (AC-19, R-03 scenario 3)
**Arrange**: Two `IndexEntry` values with `confidence = 0.30` and `confidence = 0.90`.
Pass them in ascending order (low first).
**Act**: `format_compaction_payload(&[low_entry, high_entry], ...)`
**Assert**: The rendered string contains `"0.90"` before `"0.30"` (row 1 is the 0.90 entry).
Use `result.find("0.90").unwrap() < result.find("0.30").unwrap()`.

#### `format_payload_budget_enforcement` (AC-16, R-03 scenario 4)
**Arrange**: 20 `IndexEntry` values with large content (~200 chars each). `max_bytes = 500`.
**Act**: `format_compaction_payload(&entries, None, None, 0, 500, &HashMap::new())`
**Assert**: `result.unwrap().len() <= 500`

#### `format_payload_multibyte_utf8` (AC-17, R-03 scenario 5)
**Arrange**: `IndexEntry` with `content = "\u{4e16}\u{754c}".repeat(200)` (CJK, 3 bytes/char)
**Act**: Construct the entry so `snippet` is built during formatting
**Assert**:
- `entry.snippet.len() <= 450` (150 chars * 3 bytes max)
- `entry.snippet.is_char_boundary(entry.snippet.len())` — no split at multi-byte boundary

#### `format_payload_session_context` (R-03 scenario 6)
**Arrange**: `role = Some("architect")`, `feature = Some("crt-027")`, `compaction_count = 3`
**Act**: `format_compaction_payload(&[entry], Some("architect"), Some("crt-027"), 3, ...)`
**Assert**: Output contains `"Role: architect"`, `"Feature: crt-027"`, `"Compaction: 3"`
(or equivalent header format)

#### `format_payload_active_entries_only` (R-03 scenario 7, replaces deprecated-indicator test)
**Arrange**: `IndexBriefingService` is called with a store containing one Active and one
Deprecated entry sharing the same topic. Populate `entries` from the service result.
**Act**: `format_compaction_payload(&entries, ...)`
**Assert**: Output does NOT contain the Deprecated entry's ID or content.
Note: This test validates that `IndexBriefingService::index()` returns `status=Active` only
and that those are what reach `format_compaction_payload`.

#### `format_payload_entry_id_metadata` (R-03 scenario 8)
**Arrange**: One `IndexEntry` with `id = 42`
**Act**: `format_compaction_payload(&[entry], ...)`
**Assert**: `result.unwrap().contains("42")` — entry ID appears in the flat table `id` column

#### `format_payload_token_limit_override` (AC-20, R-03 scenario 9)
**Arrange**: `max_bytes = 400`, many entries with long content
**Act**: `format_compaction_payload(&entries, None, None, 0, 400, &HashMap::new())`
**Assert**: `result.unwrap().len() <= 400`

#### `test_compact_payload_histogram_block_present` (AC-21, R-03 scenario 10)
**Arrange**: `category_histogram = HashMap { "decision" => 5, "pattern" => 3 }`, one entry
**Act**: `format_compaction_payload(&[entry], None, None, 0, MAX_COMPACTION_BYTES, &histogram)`
**Assert**: `result.unwrap().contains("Recent session activity:")`

#### `test_compact_payload_histogram_block_absent` (AC-21, R-03 scenario 11)
**Arrange**: `category_histogram = HashMap::new()` (empty), one entry
**Act**: `format_compaction_payload(&[entry], None, None, 0, MAX_COMPACTION_BYTES, &HashMap::new())`
**Assert**: `result.unwrap()` does NOT contain `"Recent session activity:"`

### Additional Tests Required

#### `dispatch_request_source_subagentstart_tags_observation` (AC-05b, R-12 scenario 1)
**Arrange**: Submit `HookRequest::ContextSearch { source: Some("SubagentStart"), query: "test", session_id: None, ... }` to `dispatch_request` using an in-process test with a real `Store`
**Act**: Query the observations table after dispatch
**Assert**: `observation.hook == "SubagentStart"`

#### `dispatch_request_source_none_tags_observation_as_userpromptsub` (AC-05c, R-12 scenario 2)
**Arrange**: `HookRequest::ContextSearch { source: None, ... }`
**Act + Assert**: `observation.hook == "UserPromptSubmit"`

#### `dispatch_request_source_absent_in_json_tags_as_userpromptsub` (R-12 scenario 3)
**Arrange**: Deserialize a JSON ContextSearch without the `source` key; submit to dispatch_request
**Act + Assert**: `observation.hook == "UserPromptSubmit"` (backward compat path via serde default)

#### `handle_compact_payload_uses_flat_index_format` (AC-12)
**Arrange**: Register a session with topic_signals, store Active entries
**Act**: Call `handle_compact_payload` via a test UDS request
**Assert**:
- Response is `HookResponse::BriefingContent { content: ... }`
- `content` contains the flat table header (column names: `#`, `id`, `topic`, `cat`, `conf`, `snippet`)
- `content` does NOT contain `"## Decisions"`, `"## Injections"`, `"## Conventions"`

#### `handle_compact_payload_no_briefing_service_import` (AC-12 — static gate)
**Verification**: After migration, `grep -r "BriefingService" crates/unimatrix-server/src/uds/listener.rs`
returns no results. Performed at Gate 3c, not as a unit test.

#### `format_compaction_payload_histogram_only_categories_empty` (AC-18 — second case)
**Arrange**: `entries: &[]` (empty), `category_histogram: {"decision": 3}`
**Act**: `format_compaction_payload(&[], None, None, 0, MAX_COMPACTION_BYTES, &histogram)`
**Assert**: `result` is `Some(...)` containing the histogram block (not `None`)

#### `format_compaction_payload_single_row_exceeds_budget` (FM-03 from RISK-TEST-STRATEGY)
**Arrange**: Single entry with very large content, `max_bytes = 50` (smaller than one row)
**Act**: `format_compaction_payload(&[large_entry], None, None, 0, 50, &HashMap::new())`
**Assert**: Function does not panic; returns either `None` or `Some(...)` with at most 50 bytes.
Documents the behavior for the "first row already exceeds budget" edge case.

---

## Integration Test Expectations

### infra-001 suites/test_tools.py

The `context_briefing` tool is the MCP-visible surface for `IndexBriefingService`.
See `context-briefing-handler.md` for MCP-layer integration tests.

For the UDS CompactPayload path, direct infra-001 testing is limited — the harness
exercises MCP only. The `lifecycle` suite tests restart persistence, which would catch
`CompactionCategories` removal compilation failures.

### New integration test: `test_compact_payload_uses_flat_index_format`

In `suites/test_lifecycle.py`, fixture `server`:
- Store 3 active entries
- Register a session
- Call `context_briefing` to verify flat table format is returned

This is a proxy for verifying the new format reaches the MCP layer. The UDS path is
verified by unit tests in `listener.rs`.

---

## Deleted Constructs (Compile-Time Verification)

The following must be ABSENT from `listener.rs` after implementation:
- `CompactionCategories` struct definition and all constructions
- `BriefingService` import and `.assemble()` call
- `InjectionSections` usage in this file

**Gate check**: `cargo build --release` with no `dead_code` warnings for deleted types.

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|----------|
| Empty entries + empty histogram | `format_payload_empty_entries_returns_none` | `None` |
| Empty entries + non-empty histogram | `format_compaction_payload_histogram_only_categories_empty` | `Some` with histogram |
| Single-row-exceeds-budget | `format_compaction_payload_single_row_exceeds_budget` | No panic, truncated or None |
| CJK snippet | `format_payload_multibyte_utf8` | Valid UTF-8 boundary |
| Source field missing in JSON | `dispatch_request_source_absent_in_json_tags_as_userpromptsub` | Tagged "UserPromptSubmit" |
