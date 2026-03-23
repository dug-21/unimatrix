# crt-028 Test Plan Overview — WA-5 PreCompact Transcript Restoration

## Overall Test Strategy

crt-028 spans three files with distinct test needs:

| Layer | Approach |
|-------|----------|
| `hook.rs` new functions | Unit tests — all pure functions; no tokio runtime; use `std::fs::write` for fixture files |
| `listener.rs` allowlist fix | Unit tests for `sanitize_observation_source`; integration test for end-to-end UDS write |
| `index_briefing.rs` regression | `#[tokio::test]` with real in-memory store (mirrors existing service test pattern) |

Primary testing surface is **unit tests in the modified files**. Integration harness supplements with
one new security test and one lifecycle regression check.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Location | Test Function(s) |
|---------|----------|--------------|---------------|-----------------|
| R-01 | Critical | hook.rs | hook.rs unit | `extract_transcript_block_missing_file_returns_none`, `prepend_transcript_none_block_writes_briefing`, `extract_transcript_block_all_malformed_lines_returns_none`, `extract_transcript_block_empty_path_returns_none` |
| R-02 | Critical | hook.rs | hook.rs unit | `extract_transcript_block_thinking_heavy_session_returns_some_or_none`, `extract_transcript_block_file_just_over_window_seeks` |
| R-03 | Critical | hook.rs | hook.rs unit | `extract_transcript_block_zero_byte_file_returns_none`, `extract_transcript_block_file_equals_window_reads_from_start`, `extract_transcript_block_file_one_byte_over_window_seeks`, `extract_transcript_block_window_minus_one_reads_from_start` |
| R-04 | High | hook.rs | hook.rs unit | `build_exchange_pairs_system_record_between_tool_use_and_result`, `build_exchange_pairs_back_to_back_assistant_no_result`, `build_exchange_pairs_orphaned_tool_result_skipped` |
| R-05 | High | hook.rs | hook.rs unit | `build_exchange_pairs_three_exchanges_most_recent_first`, `build_exchange_pairs_single_exchange_no_reversal_artifact`, `extract_transcript_block_budget_exhausted_most_recent_kept` |
| R-06 | High | hook.rs | hook.rs unit | `extract_key_param_snippet_truncated_at_utf8_boundary`, `build_exchange_pairs_user_text_budget_fill_utf8_boundary`, `extract_key_param_key_param_truncated_at_utf8_boundary` |
| R-07 | High | listener.rs | listener.rs unit + infra-001 security | `sanitize_observation_source_all_six_cases`, `test_context_search_source_injection_sanitized` (new infra test) |
| R-08 | High | index_briefing.rs | index_briefing.rs unit | `index_briefing_excludes_quarantined_entry` |
| R-09 | Med | hook.rs | hook.rs unit | `extract_key_param_unknown_tool_first_string_field_fallback`, `extract_key_param_unknown_tool_long_first_string_truncated` |
| R-10 | Med | hook.rs | hook.rs unit | `build_exchange_pairs_tool_only_assistant_turn_emits_pairs`, `build_exchange_pairs_thinking_only_turn_suppressed`, `build_exchange_pairs_all_tool_call_session_emits_pairs` |
| R-11 | Med | hook.rs | hook.rs unit | `extract_transcript_block_non_jsonl_path_returns_none` |
| R-12 | Low | hook.rs | hook.rs unit | `prepend_transcript_transcript_only_has_headers`, `prepend_transcript_both_none_empty_string`, `prepend_transcript_both_present_correct_order`, `prepend_transcript_briefing_only_verbatim` |
| R-13 | Low | compile-time | CI | `cargo check` gate after crt-027 merge |

---

## AC-to-Test Location Map

| AC-ID | Description | Test File | Test Function |
|-------|-------------|-----------|---------------|
| AC-01 | PreCompact stdout begins with transcript block before briefing | hook.rs unit | `prepend_transcript_both_present_transcript_precedes_briefing` |
| AC-02 | Exchange order: most-recent first | hook.rs unit | `build_exchange_pairs_three_exchanges_most_recent_first` |
| AC-03 | Tool pairs formatted as `[tool: name(key_param) → snippet]`, truncated to 300 bytes | hook.rs unit | `build_exchange_pairs_tool_pair_format_and_truncation` |
| AC-04 | `type:"tool_result"` in user turns skipped as user text | hook.rs unit | `build_exchange_pairs_user_tool_result_skipped` |
| AC-05 | Transcript portion byte length ≤ `MAX_PRECOMPACT_BYTES` | hook.rs unit | `extract_transcript_block_respects_byte_budget` |
| AC-06 | `transcript_path = None` → briefing written unchanged | hook.rs unit | `prepend_transcript_none_block_writes_briefing` |
| AC-07 | Missing file → silent skip, briefing written, exit 0 | hook.rs unit | `extract_transcript_block_missing_file_returns_none` |
| AC-08 | Malformed JSONL lines skipped silently; parseable lines used | hook.rs unit | `build_exchange_pairs_malformed_lines_skipped`, `extract_transcript_block_mixed_valid_invalid_jsonl` |
| AC-09 | No user/assistant pairs → transcript block omitted | hook.rs unit | `extract_transcript_block_system_only_returns_none` |
| AC-10 | `MAX_PRECOMPACT_BYTES` constant defined, distinct from `MAX_INJECTION_BYTES` | grep | `grep "MAX_PRECOMPACT_BYTES.*3000" hook.rs` |
| AC-11 | `sanitize_observation_source` all 6 allowlist cases | listener.rs unit | `sanitize_observation_source_all_six_cases` |
| AC-12 | Quarantine exclusion regression in `IndexBriefingService::index()` | index_briefing.rs unit | `index_briefing_excludes_quarantined_entry` |
| AC-13 | Doc comment on `index()` contains "delegated to" + "validate_search_query" | grep | `grep -A5 "pub.*async fn index" index_briefing.rs` |
| AC-14 | Existing hook.rs tests pass; no regressions | cargo test | `cargo test -p unimatrix-server hook` |
| AC-15 | Hook exits 0 even on transcript failure (structural: `Option<String>` return type) | hook.rs unit | `extract_transcript_block_missing_file_returns_none` (None = no panic/exit) |

---

## Cross-Component Test Dependencies

- AC-01 requires `prepend_transcript` + a fixture JSONL file with at least one user/assistant exchange.
- AC-03 requires `build_exchange_pairs` AND `extract_key_param` together (via fixture JSONL).
- AC-05 requires `extract_transcript_block` with a many-exchange JSONL fixture.
- AC-08 covers two distinct paths: `build_exchange_pairs` (parse-level skip) and `extract_transcript_block` (file-level integration of both skip + parse).

---

## Non-Negotiable Gate Tests

The following tests must pass before Stage 3c gate approval (from RISK-TEST-STRATEGY.md):

1. **R-01 gate**: `extract_transcript_block_missing_file_returns_none` — non-existent path returns `None`.
2. **R-03 gate**: `extract_transcript_block_zero_byte_file_returns_none` AND `extract_transcript_block_file_equals_window_reads_from_start` — seek boundary correctness.
3. **R-07 gate**: `sanitize_observation_source_all_six_cases` — all 6 allowlist cases correct.
4. **R-08 gate**: `index_briefing_excludes_quarantined_entry` — quarantine post-filter exercised.
5. **R-10 gate**: `build_exchange_pairs_tool_only_assistant_turn_emits_pairs` AND `build_exchange_pairs_thinking_only_turn_suppressed` — OQ-SPEC-1 behavior verified.

---

## Integration Harness Plan

### Existing Suites Applicable to crt-028

| Suite | Relevance |
|-------|-----------|
| `smoke` | Mandatory gate — must pass to confirm no regression in existing tool handlers |
| `tools` | Covers `context_search` (53 tests include tool dispatch path changed by GH #354 fix) |
| `security` | Covers input validation and field content scanning; R-07 source-injection path |
| `lifecycle` | Covers store→search→briefing flows; does not cover PreCompact hook path directly |

**Run order for Stage 3c**: `smoke` → `security` → `tools` → `lifecycle`

The PreCompact hook path is **not exercised** by any existing infra-001 suite — the integration harness
calls the server via MCP JSON-RPC, not via the hook UDS binary. Transcript extraction, `build_exchange_pairs`,
and `prepend_transcript` are hook-process-only logic and are not reachable through infra-001.

### New Integration Test Needed

**Suite**: `suites/test_security.py`
**Test**: `test_context_search_source_field_sanitized`
**Fixture**: `server` (fresh DB, no state leakage)

Scenario:
1. Send a `context_search` tool call (via MCP) with a session and a `source` value of `"Injected\nEvil"` (or another non-allowlist string).
2. Query the observations table (via `context_status` or a direct DB check) to verify the `hook` column value is `"UserPromptSubmit"`, not the injected value.

This test exercises the R-07 / AC-11 security fix end-to-end through the UDS wire — verifying that
`sanitize_observation_source` is actually called, not bypassed.

**Note**: The `source` field is set by the hook process, not directly exposed in the MCP tool call.
The infra-001 test must use the `server` fixture's underlying hook UDS path or verify via observable
side-effects (observation row content). If the hook source field is not directly settable via the MCP
interface, mark this test as `@pytest.mark.xfail(reason="source field not settable via MCP — verify via unit test only")` and rely on the unit test for coverage. Document the decision in Stage 3c.

### Suites NOT Applicable

- `confidence` — no confidence formula changes in crt-028
- `contradiction` — no contradiction detection changes
- `volume` — no schema changes; no new storage paths
- `edge_cases` — hook-side edge cases (Unicode, boundary) are unit-tested; no MCP surface

### Verdict

**Minimum integration gate**: `smoke` (mandatory) + `security` (R-07 new test).
The bulk of crt-028 test coverage lives in unit tests. Integration tests supplement with the
GH #354 security regression path.
