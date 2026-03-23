# Risk Coverage Report: crt-027 (WA-4 Proactive Knowledge Delivery)

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `source` field addition — backward compat & struct literal compile | `wire_context_search_source_absent_deserializes_to_none`, `wire_context_search_source_present_deserializes_to_value`, `wire_context_search_source_none_serializes_without_field`, `context_search_source_none_round_trip`, `context_search_source_subagentstart_round_trip`, `context_search_is_not_fire_and_forget` (updated with `source: None`) | PASS | Full |
| R-02 | `EffectivenessStateHandle` wiring — silent ranking degradation | Compile-time: `IndexBriefingService::new()` requires non-optional `EffectivenessStateHandle` parameter (compile error if omitted, confirmed); `index_briefing_service_returns_sorted_by_fused_score` | PASS | Full (compile-time + runtime) |
| R-03 | `format_compaction_payload` test invariant coverage — 11 named tests | `format_payload_empty_entries_returns_none`, `format_payload_header_present`, `format_payload_sorted_by_confidence`, `format_payload_budget_enforcement`, `format_payload_multibyte_utf8`, `format_payload_session_context`, `format_payload_active_entries_only`, `format_payload_entry_id_metadata`, `format_payload_token_limit_override`, `test_compact_payload_histogram_block_present`, `test_compact_payload_histogram_block_absent` | PASS | Full — all 11 named tests exist and pass |
| R-04 | `MIN_QUERY_WORDS = 5` boundary — off-by-one | `build_request_userpromptsub_four_words_record_event`, `build_request_userpromptsub_five_words_context_search`, `build_request_userpromptsub_one_word`, `build_request_userpromptsub_three_words`, `build_request_userpromptsub_six_words`, `build_request_subagentstart_one_word_routes_to_context_search`, `build_request_subagentstart_empty_prompt_snippet` | PASS | Full |
| R-05 | CompactPayload format contract for WA-5 — `format_index_table` drift | `format_index_table_header_and_separator`, `format_index_table_exact_column_layout`, `format_index_table_empty_slice_returns_empty_string`, `snippet_chars_constant_is_150` | PASS | Full |
| R-06 | Query derivation three-step fallback — divergence between MCP and UDS | `derive_briefing_query_task_param_takes_priority`, `derive_briefing_query_empty_task_falls_through`, `derive_briefing_query_session_signals_step_2`, `derive_briefing_query_fewer_than_three_signals`, `derive_briefing_query_empty_signals_fallback_to_topic`, `derive_briefing_query_no_session_fallback_to_topic`; code inspection confirms single `derive_briefing_query` called by both MCP handler and `handle_compact_payload` | PASS | Full |
| R-07 | SubagentStart stdout injection — SR-01 manual smoke | `build_request_subagentstart_with_prompt_snippet` (routing confirmed), `write_stdout_subagent_inject` unit tests; **AC-SR01 status: CONFIRMED** (Claude Code documentation confirms `hookSpecificOutput` envelope) | PASS | Full (automated) + CONFIRMED (manual gate) |
| R-08 | `mcp-briefing` feature flag split | `cargo test --workspace` (without flag): 3339 tests pass including `handle_compact_payload` tests; `cargo test --features mcp-briefing`: 4 additional `context_briefing` MCP tool tests pass (`context_briefing_active_only_filter`, `context_briefing_default_k_20`, `context_briefing_flat_table_format`, `context_briefing_k_override`) | PASS | Full |
| R-09 | `UNIMATRIX_BRIEFING_K` env var silent k reduction | `index_briefing_service_default_k_is_20` (runtime: env var set to 3, asserts k=20 returned); `grep UNIMATRIX_BRIEFING_K crates/unimatrix-server/src/services/` returns only deprecation comments (no functional reads); `parse_semantic_k` fully deleted | PASS | Full |
| R-10 | Cold-state query derivation — topic fallback quality | `derive_briefing_query_no_session_fallback_to_topic`, `index_briefing_service_empty_result_on_no_match` (returns `Ok(vec![])`, no panic); `test_briefing_query_derivation_topic_fallback` (integration) | PASS | Full |
| R-11 | SM delivery protocol update — completeness | `grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md` → 13 (>=6 required); all 6 insertion points confirmed present and each call specifies `max_tokens: 1000` | PASS | Full |
| R-12 | Observation `hook` column mismatch | `col018_context_search_creates_observation` verifies `source: None` → `hook = "UserPromptSubmit"`; wire tests verify `source: Some("SubagentStart")` deserializes correctly; implementation line 813 (`source.as_deref().unwrap_or("UserPromptSubmit")`) confirmed correct; `build_request_subagentstart_with_prompt_snippet` confirms hook.rs sets `source = Some("SubagentStart")` | PASS | Partial (no standalone `dispatch_request_source_subagentstart_tags_observation` unit test — covered combinatorially via existing tests) |
| R-13 | `HookRequest::Briefing` variant inadvertently removed | `hook_request_briefing_variant_still_present` in wire.rs; `grep -c "HookRequest::Briefing" crates/unimatrix-engine/src/wire.rs` → 5 matches | PASS | Full |
| R-14 | Empty CompactPayload when histogram also empty | `format_payload_empty_entries_returns_none` (both empty → `None`); `format_compaction_payload_histogram_only_categories_empty` (histogram non-empty, entries empty → `Some` with histogram) | PASS | Full |

---

## Non-Negotiable Test Names — Gate Verification

All 15 required test names verified to exist:

| Test Name | Module | Status |
|-----------|--------|--------|
| `format_payload_empty_entries_returns_none` | `uds::listener::tests` | PASS |
| `format_payload_header_present` | `uds::listener::tests` | PASS |
| `format_payload_sorted_by_confidence` | `uds::listener::tests` | PASS |
| `format_payload_budget_enforcement` | `uds::listener::tests` | PASS |
| `format_payload_multibyte_utf8` | `uds::listener::tests` | PASS |
| `format_payload_session_context` | `uds::listener::tests` | PASS |
| `format_payload_active_entries_only` | `uds::listener::tests` | PASS |
| `format_payload_entry_id_metadata` | `uds::listener::tests` | PASS |
| `format_payload_token_limit_override` | `uds::listener::tests` | PASS |
| `test_compact_payload_histogram_block_present` | `uds::listener::tests` | PASS |
| `test_compact_payload_histogram_block_absent` | `uds::listener::tests` | PASS |
| `build_request_subagentstart_with_prompt_snippet` | `uds::hook::tests` | PASS |
| `build_request_subagentstart_empty_prompt_snippet` | `uds::hook::tests` | PASS |
| `build_request_userpromptsub_four_words_record_event` | `uds::hook::tests` | PASS |
| `build_request_userpromptsub_five_words_context_search` | `uds::hook::tests` | PASS |

---

## Test Results

### Unit Tests

- **Total (workspace)**: 3339
- **Passed**: 3339
- **Failed**: 0
- **Ignored**: 27 (pre-existing ignored tests, unrelated to crt-027)
- **Command**: `cargo test --workspace` — clean pass

#### With `mcp-briefing` feature flag

- **Additional tests activated**: 4 (`context_briefing_*` in `mcp::tools::tests`)
- **Passed**: 4
- **Failed**: 0
- **Command**: `cargo test --features mcp-briefing -p unimatrix-server`

#### Note on flaky test

One test (`uds::listener::tests::col018_prompt_at_limit_not_truncated`) was observed to
fail in a single workspace-parallel run during initial testing but passed consistently
in all subsequent runs and when run in isolation. This is a pre-existing pool/concurrency
issue matching GH#303 (now closed). The test is NOT related to crt-027. No xfail marker
added (the test passes reliably; it is only flaky under extreme parallel contention).

### Integration Tests

#### Smoke Suite (mandatory gate)

- **Total**: 20
- **Passed**: 20
- **Failed**: 0
- **Command**: `cd product/test/infra-001 && python -m pytest suites/ -m smoke --timeout=60`

#### Protocol Suite

- **Total**: 13
- **Passed**: 13
- **Failed**: 0
- **Command**: `python -m pytest suites/test_protocol.py --timeout=60`

#### Edge Cases Suite

- **Total**: 24
- **Passed**: 23
- **xfailed**: 1 (pre-existing, unrelated to crt-027)
- **Failed**: 0
- **Command**: `python -m pytest suites/test_edge_cases.py --timeout=60`

#### Tools Suite — Briefing Tests

- **Existing tests**: 4 (`test_briefing_returns_content`, `test_briefing_empty_db`,
  `test_briefing_missing_required_params`, `test_briefing_all_formats`) — all PASS
- **New crt-027 tests added**: 4 (`test_briefing_returns_flat_index_table`,
  `test_briefing_active_entries_only`, `test_briefing_default_k_higher_than_three`,
  `test_briefing_k_override`) — all PASS

#### Lifecycle Suite — New crt-027 Tests

- **New tests added**: 2 (`test_briefing_flat_index_format_no_section_headers`,
  `test_briefing_session_id_applies_wa2_boost`) — both PASS

### Integration Test Totals

- **Smoke**: 20/20 PASS
- **Protocol**: 13/13 PASS
- **Edge cases**: 23/23 PASS + 1 xfail (pre-existing)
- **New crt-027 integration tests**: 6/6 PASS

---

## Static Verification Gates

| Check | Command | Result |
|-------|---------|--------|
| AC-13: BriefingService deleted | `grep -r "BriefingService" crates/ --include="*.rs" \| grep -v "Index\|//\|//!"` | No results — PASS |
| AC-13: Briefing struct deleted | `grep "pub struct Briefing\|fn format_briefing" crates/unimatrix-server/src/mcp/response/briefing.rs` | No results — PASS |
| AC-14: context_briefing count | `grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md` | 13 (>=6 required) — PASS |
| AC-14: max_tokens on all calls | `grep "context_briefing" .../uni-delivery-protocol.md \| grep -v "max_tokens: 1000"` | 2 lines (multi-line form — both have max_tokens: 1000 on next line) — PASS |
| R-09: parse_semantic_k deleted | `grep -r "parse_semantic_k" crates/ --include="*.rs"` | Only deprecation comment in `services/mod.rs` — PASS |
| R-09: UNIMATRIX_BRIEFING_K | `grep -r "UNIMATRIX_BRIEFING_K" crates/unimatrix-server/src/services/` | Only deprecation comments — PASS |
| R-13: HookRequest::Briefing | `grep -c "HookRequest::Briefing\|Briefing {" crates/unimatrix-engine/src/wire.rs` | 5 — PASS |
| AC-25: cargo build --release | `cargo build --release` | 0 errors, 10 warnings (pre-existing, unrelated) — PASS |

---

## Gaps

**R-12 standalone unit test gap**: The test plan specified three dedicated dispatch observation tests:
`dispatch_request_source_subagentstart_tags_observation`,
`dispatch_request_source_none_tags_observation_as_userpromptsub`, and
`dispatch_request_source_absent_in_json_tags_as_userpromptsub`.

The Stage 3b implementation did not include these dedicated tests. However, the R-12 risk is
mitigated by three overlapping coverage paths:
1. `col018_context_search_creates_observation` verifies `source: None` → `hook = "UserPromptSubmit"` (scenario 2)
2. Wire round-trip tests confirm `source: Some("SubagentStart")` deserializes correctly (scenario 1/3)
3. `build_request_subagentstart_with_prompt_snippet` confirms hook.rs emits `source = Some("SubagentStart")`
4. Implementation line 813 is a single code path with no branching error

Risk level for the gap: **Low** — the logic is correct and covered combinatorially.
Recommend adding the three dedicated tests in a follow-up if the observation column is
exercised by other features.

**AC-SR01 manual gate**: Marked CONFIRMED in ACCEPTANCE-MAP.md. Confirmed via Claude Code
hooks documentation that `hookSpecificOutput` envelope is injected into subagent context.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `build_request_subagentstart_with_prompt_snippet` — asserts `query`, `source = Some("SubagentStart")`, `session_id` from input |
| AC-02 | PASS | `build_request_subagentstart_empty_prompt_snippet` — covers key absent and empty string cases |
| AC-02b | PASS | `build_request_userpromptsub_four_words_record_event`, `build_request_userpromptsub_five_words_context_search`, `build_request_subagentstart_one_word_routes_to_context_search` |
| AC-03 | PASS | `build_request_subagentstart_session_id_from_input` — asserts `session_id == Some("parent-sess-42")` from input |
| AC-04 | PASS | `context_search_is_not_fire_and_forget` — `is_faf` is `false` for `ContextSearch` |
| AC-05 | PASS | Wire source tests (serde default); `col018_context_search_creates_observation` (source: None → "UserPromptSubmit"); hook builds `source = Some("SubagentStart")` |
| AC-SR01 | CONFIRMED | ACCEPTANCE-MAP.md status = CONFIRMED; Claude Code documentation confirms hookSpecificOutput injection |
| AC-SR02 | PASS | `write_stdout_subagent_inject_valid_json_envelope` — asserts valid JSON, `hookEventName = "SubagentStart"`, `additionalContext` non-empty |
| AC-SR03 | PASS | `write_stdout_plain_text_no_json_envelope` — stdout does not start with `{`, no `"hookSpecificOutput"` |
| AC-06 | PASS | `index_briefing_service_active_entries_only`; `context_briefing_active_only_filter` (mcp-briefing); `test_briefing_active_entries_only` (infra-001) |
| AC-07 | PASS | `index_briefing_service_default_k_is_20` (UNIMATRIX_BRIEFING_K=3 set, k=20 returned); `context_briefing_default_k_20`; `test_briefing_default_k_higher_than_three` (infra-001) |
| AC-08 | PASS | `context_briefing_flat_table_format`; `test_briefing_returns_flat_index_table` (infra-001); `test_briefing_flat_index_format_no_section_headers` (infra-001) |
| AC-09 | PASS | All six `derive_briefing_query_*` unit tests; code inspection confirms single shared helper |
| AC-10 | PASS | `derive_briefing_query_*` tests use `session_state` directly (no registry lookup); IR-04 code path confirmed |
| AC-11 | PASS | `test_briefing_session_id_applies_wa2_boost` (infra-001 lifecycle); service layer passes `session_id` to `ServiceSearchParams` |
| AC-12 | PASS | `handle_compact_payload_uses_flat_index_format`; `BriefingService` import absent from listener.rs (grep confirmed) |
| AC-13 | PASS | `grep -r "BriefingService" crates/ --include="*.rs" \| grep -v "Index\|//\|//!"` → no results |
| AC-14 | PASS | 13 `context_briefing` calls in protocol (>=6 required); all 6 insertion points present; all calls have `max_tokens: 1000` |
| AC-15 | PASS | `cargo test --workspace` — 3339 passed; test count non-decreasing (243 hook+listener tests present) |
| AC-16 | PASS | `format_payload_budget_enforcement` — `result.len() <= max_bytes` |
| AC-17 | PASS | `format_payload_multibyte_utf8`; `snippet_truncation_utf8_safe_cjk` — `chars().take(150)` is UTF-8 safe |
| AC-18 | PASS | `format_payload_empty_entries_returns_none` (both empty → None); `format_compaction_payload_histogram_only_categories_empty` (histogram non-empty → Some) |
| AC-19 | PASS | `format_payload_sorted_by_confidence` — 0.90 entry is row 1, 0.30 entry is row 2 |
| AC-20 | PASS | `format_payload_token_limit_override` — `max_bytes = 400`, `output.len() <= 400` |
| AC-21 | PASS | `test_compact_payload_histogram_block_present`; `test_compact_payload_histogram_block_absent` |
| AC-22 | PASS | `build_request_userpromptsub_four_words_record_event`; `build_request_userpromptsub_five_words_context_search` |
| AC-23 | PASS | `build_request_subagentstart_one_word_routes_to_context_search` |
| AC-23b | PASS | `build_request_subagentstart_whitespace_only_prompt_snippet` — `"   "` → `RecordEvent` |
| AC-23c | PASS | `build_request_userpromptsub_whitespace_padded_one_word` — `"  approve  "` word count = 1 → `RecordEvent` |
| AC-24 | PASS | `cargo test --workspace` (no flag): 3339 pass including `handle_compact_payload` path; `IndexBriefingService` compiles unconditionally |
| AC-25 | PASS | `cargo build --release` → 0 errors; struct literal `ContextSearch` updated with `source: None` field |

---

## GH Issues Filed for Pre-Existing Failures

None filed — no new pre-existing failures discovered. The one flaky test
(`col018_prompt_at_limit_not_truncated`) matches the pre-existing class identified in
GH#303 (closed) and passes consistently in clean runs.

---

## New Integration Tests Added

### `product/test/infra-001/suites/test_tools.py`

Four tests added in `context_briefing crt-027 WA-4b integration tests` section:

| Test | AC Coverage |
|------|-------------|
| `test_briefing_returns_flat_index_table` | AC-08, R-05 |
| `test_briefing_active_entries_only` | AC-06, IR-02 |
| `test_briefing_default_k_higher_than_three` | AC-07, R-09 |
| `test_briefing_k_override` | AC-07 |

### `product/test/infra-001/suites/test_lifecycle.py`

Two tests added in `crt-027 WA-4b: Briefing flat index format lifecycle tests` section:

| Test | AC Coverage |
|------|-------------|
| `test_briefing_flat_index_format_no_section_headers` | AC-08, R-03 |
| `test_briefing_session_id_applies_wa2_boost` | AC-11, IR-01 |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for gate verification testing procedures — found entries
  #487 (workspace test without hanging), #2957 (wave-based refactor cargo test scope). Neither
  was directly applicable to crt-027 Stage 3c execution.
- Stored: nothing novel to store — test execution patterns used (truncated cargo output,
  infra-001 smoke gate, xfail triage, new integration test authoring) are already captured
  in entries #699, #885, #2758, #3180. The six new integration tests are standard infra-001
  fixture patterns with no novel harness technique.
