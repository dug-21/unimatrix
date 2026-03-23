# Agent Report: crt-028-agent-2-testplan (Stage 3a)

## Output Files

| File | Path |
|------|------|
| Test strategy + integration plan | `product/features/crt-028/test-plan/OVERVIEW.md` |
| hook.rs component test plan | `product/features/crt-028/test-plan/hook.md` |
| listener.rs component test plan | `product/features/crt-028/test-plan/listener.md` |
| index_briefing.rs component test plan | `product/features/crt-028/test-plan/index_briefing.md` |

## Risk Coverage Mapping Summary

| Risk ID | Priority | Covered By | Gate? |
|---------|----------|------------|-------|
| R-01 (degradation boundary) | Critical | `extract_transcript_block_missing_file_returns_none`, `prepend_transcript_none_block_writes_briefing`, `extract_transcript_block_all_malformed_lines_returns_none`, `extract_transcript_block_empty_path_returns_none` | Yes |
| R-02 (tail multiplier insufficient) | Critical | `extract_transcript_block_thinking_heavy_session_returns_some_or_none`, `extract_transcript_block_file_just_over_window_seeks` | No |
| R-03 (SeekFrom::End boundary) | Critical | `extract_transcript_block_zero_byte_file_returns_none`, `extract_transcript_block_file_equals_window_reads_from_start`, `extract_transcript_block_file_one_byte_over_window_seeks`, `extract_transcript_block_window_minus_one_reads_from_start` | Yes (zero-byte + window-equals) |
| R-04 (adjacent-record pairing) | High | `build_exchange_pairs_system_record_between_tool_use_and_result`, `build_exchange_pairs_back_to_back_assistant_no_result`, `build_exchange_pairs_orphaned_tool_result_skipped`, `build_exchange_pairs_multiple_tool_uses_in_one_turn` | No |
| R-05 (reversal order) | High | `build_exchange_pairs_three_exchanges_most_recent_first`, `build_exchange_pairs_single_exchange_no_reversal_artifact`, `extract_transcript_block_budget_exhausted_most_recent_kept` | No |
| R-06 (UTF-8 truncation) | High | `extract_key_param_snippet_truncated_at_utf8_boundary`, `build_exchange_pairs_user_text_budget_fill_utf8_boundary`, `extract_key_param_key_param_truncated_at_utf8_boundary` | No |
| R-07 (sanitize_observation_source bypass) | High | `sanitize_observation_source_all_six_cases` (unit) + infra-001 `test_context_search_source_field_sanitized` | Yes |
| R-08 (quarantine post-filter removal) | High | `index_briefing_excludes_quarantined_entry` | Yes |
| R-09 (key-param fallback sensitive field) | Med | `extract_key_param_unknown_tool_first_string_field_fallback`, `extract_key_param_unknown_tool_long_first_string_truncated`, `extract_key_param_no_string_field_returns_empty` | No |
| R-10 (OQ-SPEC-1 tool-only turns) | Med | `build_exchange_pairs_tool_only_assistant_turn_emits_pairs`, `build_exchange_pairs_thinking_only_turn_suppressed`, `build_exchange_pairs_all_tool_call_session_emits_pairs` | Yes |
| R-11 (arbitrary path read) | Med | `extract_transcript_block_non_jsonl_path_returns_none` | No |
| R-12 (prepend_transcript separator) | Low | 4 `prepend_transcript_*` tests covering all four branches | No |
| R-13 (crt-027 symbol rename) | Low | `cargo check` gate (compile-time; no runtime test needed) | CI only |

## Integration Suite Plan

- **Mandatory gate**: `python -m pytest suites/ -v -m smoke --timeout=60`
- **Applicable suites**: `security` (R-07 new test), `tools` (regression for ContextSearch path)
- **New integration test**: `test_context_search_source_field_sanitized` in `suites/test_security.py`
  — may need `xfail` if `source` field is not settable via MCP interface (decision deferred to Stage 3c)
- **Not applicable**: `confidence`, `contradiction`, `volume` — no changes to those systems

## Open Questions

1. **`tempfile` crate availability**: hook.rs tests may need `tempfile` as a `[dev-dependencies]`
   entry in `unimatrix-server/Cargo.toml`. Implementer should check whether it is already present
   before adding it.

2. **`make_jsonl_file` helper scope**: This helper can be defined as a private function within the
   `#[cfg(test)]` block. It should also be available for `extract_transcript_block_*` tests. The
   implementer can define it once in the `tests` module and share it across all hook.rs test functions.

3. **infra-001 `source` field settability**: The `source` field in `HookRequest::ContextSearch` is
   not exposed via the MCP `context_search` tool interface. The integration test for R-07 may need
   to be `xfail`. This is confirmed-pending in Stage 3c when the tester examines the actual
   infra-001 harness.

4. **`index_briefing_excludes_quarantined_entry` setup complexity**: The test needs a real store
   with `SearchService`. If the existing `index_briefing.rs` test module does not already have a
   full-service setup helper, the implementer will need to build one using the pattern from
   `listener.rs` integration tests (which construct a full `AppServices` stack).

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "crt-028 architectural decisions" (category: decision, topic: crt-028) — found 4 ADR entries (#3333–#3336): ADR-001 tail-bytes, ADR-002 pairing, ADR-003 degradation contract, ADR-004 source allowlist. All directly informed test plan structure.
- Queried: `/uni-knowledge-search` for "hook.rs testing patterns graceful degradation" — found entry #3335 (ADR-003 degradation), #3026 (hook.rs isolation pattern crt-025), #3253 (non-negotiable test name verification), #3301 (graceful degradation via empty fallback crt-027). All informed the R-01 gate test design.
- Stored: entry #3338 "hook.rs unit test patterns: tempfile fixtures, no-tokio constraint, seek boundary coverage" via `/uni-context-store` (topic: testing, category: pattern).
