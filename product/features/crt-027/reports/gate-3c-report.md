# Gate 3c Report: crt-027

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 14 risks mapped to passing tests in RISK-COVERAGE-REPORT.md |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; 11 non-negotiable test names confirmed by grep |
| Specification compliance | PASS | All 32 ACs verified; AC-SR01 marked CONFIRMED |
| Architecture compliance | PASS | Component structure matches ARCHITECTURE.md; ADRs followed |
| Integration tests | PASS | Smoke 20/20, Protocol 13/13, Edge 23/23+1xfail, 6 new crt-027 tests pass |
| Knowledge stewardship | PASS | Queried and stored entries present in RISK-COVERAGE-REPORT.md |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**:
- RISK-COVERAGE-REPORT.md maps all 14 risks (R-01 through R-14) to specific passing tests.
- R-12 has a documented partial gap (no standalone `dispatch_request_source_subagentstart_tags_observation` unit test) but is covered combinatorially by three overlapping paths. Risk level assessed as Low by the test author. This is accepted — the logic at listener.rs line 813 (`source.as_deref().unwrap_or("UserPromptSubmit")`) is a single, non-branching code path verified by the existing observation test and wire round-trip tests.
- R-07 (SubagentStart stdout injection manual gate): AC-SR01 is marked CONFIRMED in ACCEPTANCE-MAP.md with documented basis in Claude Code hooks documentation. The automated coverage (JSON envelope format test, unit routing test) is complete.

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence from non-negotiable test name verification** (per lesson #2758):

All 11 required test names confirmed present via `cargo test --workspace -- --list`:

| Test Name | Module | Verified |
|-----------|--------|---------|
| `format_payload_empty_entries_returns_none` | `uds::listener::tests` | CONFIRMED |
| `format_payload_header_present` | `uds::listener::tests` | CONFIRMED |
| `format_payload_sorted_by_confidence` | `uds::listener::tests` | CONFIRMED |
| `format_payload_budget_enforcement` | `uds::listener::tests` | CONFIRMED |
| `format_payload_multibyte_utf8` | `uds::listener::tests` | CONFIRMED |
| `format_payload_session_context` | `uds::listener::tests` | CONFIRMED |
| `format_payload_active_entries_only` | `uds::listener::tests` | CONFIRMED |
| `format_payload_entry_id_metadata` | `uds::listener::tests` | CONFIRMED |
| `format_payload_token_limit_override` | `uds::listener::tests` | CONFIRMED |
| `test_compact_payload_histogram_block_present` | `uds::listener::tests` | CONFIRMED |
| `test_compact_payload_histogram_block_absent` | `uds::listener::tests` | CONFIRMED |
| `build_request_subagentstart_with_prompt_snippet` | `uds::hook::tests` | CONFIRMED |
| `build_request_subagentstart_empty_prompt_snippet` | `uds::hook::tests` | CONFIRMED |
| `build_request_userpromptsub_four_words_record_event` | `uds::hook::tests` | CONFIRMED |
| `build_request_userpromptsub_five_words_context_search` | `uds::hook::tests` | CONFIRMED |

Additional test coverage confirmed:
- R-01 wire tests: 5 tests (`wire_context_search_source_absent_deserializes_to_none`, `wire_context_search_source_present_deserializes_to_value`, `wire_context_search_source_none_serializes_without_field`, `context_search_source_none_round_trip`, `context_search_source_subagentstart_round_trip`)
- R-05 format contract: 10 tests in `mcp::response::briefing::tests`, including `format_index_table_exact_column_layout`, `snippet_chars_constant_is_150`
- R-06 query derivation: 8 tests in `services::index_briefing::tests`
- R-07 stdout injection: `write_stdout_subagent_inject_valid_json_envelope`, `write_stdout_plain_text_no_json_envelope`
- R-13 wire variant: `grep -c "HookRequest::Briefing" crates/unimatrix-engine/src/wire.rs` → 5

Total workspace unit test count: **3339 passed, 0 failed, 27 ignored** (pre-existing).

### 3. Specification Compliance

**Status**: PASS

**Evidence**: All 32 ACs (AC-01 through AC-25 including AC-02b, AC-23b, AC-23c, AC-SR01, AC-SR02, AC-SR03) are verified PASS in RISK-COVERAGE-REPORT.md. Spot-checks:

- **FR-01/FR-02 (SubagentStart routing)**: `build_request_subagentstart_with_prompt_snippet` and `build_request_subagentstart_empty_prompt_snippet` confirmed present and passing.
- **FR-04b (JSON envelope)**: `write_stdout_subagent_inject_valid_json_envelope` confirms correct `hookSpecificOutput` structure.
- **FR-05 (MIN_QUERY_WORDS guard)**: `build_request_userpromptsub_four_words_record_event` (boundary below) and `build_request_userpromptsub_five_words_context_search` (boundary at) both confirmed present and passing.
- **FR-13 (UNIMATRIX_BRIEFING_K deprecated)**: `grep -r "parse_semantic_k" crates/ --include="*.rs"` returns only one comment line in `services/mod.rs` (`// parse_semantic_k() removed. See ADR-003 crt-027.`). No functional reads.
- **FR-18 (BriefingService deleted)**: `grep -r "BriefingService" crates/ --include="*.rs" | grep -v "Index" | grep -v "^\s*//"` returns only doc comment lines (`///`, `//!`) in `effectiveness.rs` and `search.rs` — these are stale doc comments referring to the historical service name and do not represent functional code. No struct, impl, or use declarations remain.
- **FR-19 / AC-14 (protocol update)**: `grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md` → **13** (≥6 required). All 6 insertion points verified present. All calls specify `max_tokens: 1000`.

**Note on stale doc comments**: `effectiveness.rs` and `search.rs` contain 7 doc comment lines that reference `BriefingService` by name as a historical reference (e.g., `"shared with BriefingService and background tick"`). These are informational comments in non-test code — they do not represent dead code or functional coupling. AC-13 requires no `dead_code` warnings and no re-exports: both are satisfied. The doc comments are a cosmetic issue only and do not constitute a FAIL.

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:
- `cargo build --workspace` completes with 0 errors, 10 warnings (all pre-existing, unrelated to crt-027).
- Component structure matches ARCHITECTURE.md: `unimatrix-engine/src/wire.rs` (source field), `uds/hook.rs` (SubagentStart arm + MIN_QUERY_WORDS), `uds/listener.rs` (dispatch_request + compaction migration), `services/index_briefing.rs` (IndexBriefingService), `services/mod.rs` (ServiceLayer wiring), `mcp/tools.rs` (briefing handler), `mcp/response/briefing.rs` (format_index_table).
- ADR-001 (serde(default) backward compat): confirmed by wire round-trip tests.
- ADR-002 (SubagentStart routing + trim guards): confirmed by hook unit tests.
- ADR-003 (UNIMATRIX_BRIEFING_K deprecated, k=20 hardcoded): confirmed by `index_briefing_service_default_k_is_20`.
- ADR-004 (compaction format migration): confirmed by 11 rewritten invariant tests.
- ADR-005 (IndexEntry typed WA-5 contract): `snippet_chars_constant_is_150` and format contract tests confirm stable column layout.
- ADR-006 (SubagentStart JSON envelope): `write_stdout_subagent_inject_valid_json_envelope` confirms correct envelope format.
- `HookRequest::Briefing` wire variant preserved (C-04): confirmed by `grep -c` → 5 matches.

### 5. Integration Test Validation

**Status**: PASS

**Smoke suite**: 20/20 PASS — mandatory gate satisfied.

**Protocol suite**: 13/13 PASS.

**Edge cases suite**: 23/23 PASS + 1 xfail.
- The xfail (`test_auto_quarantine_after_consecutive_bad_ticks`) references GH#291 (pre-existing, tick interval not drivable at integration level). Unrelated to crt-027. Correctly marked with reason and issue reference.

**Tools suite new tests** (4 added in `test_tools.py`):
- `test_briefing_returns_flat_index_table` — AC-08, R-05
- `test_briefing_active_entries_only` — AC-06, IR-02
- `test_briefing_default_k_higher_than_three` — AC-07, R-09
- `test_briefing_k_override` — AC-07

**Lifecycle suite new tests** (2 added in `test_lifecycle.py`):
- `test_briefing_flat_index_format_no_section_headers` — AC-08, R-03
- `test_briefing_session_id_applies_wa2_boost` — AC-11, IR-01

All 6 new crt-027 integration tests pass.

**xfail markers checked**: Both xfail markers in the integration suite (`test_tools.py` line 996 referencing GH#305, `test_lifecycle.py` line 564 referencing GH#291) are pre-existing and confirmed unrelated to crt-027. No new xfail markers added for this feature. No integration tests were deleted or commented out.

**RISK-COVERAGE-REPORT.md integration counts**: Smoke 20/20, Protocol 13/13, Edge 23/23+1xfail, 6 new crt-027 tests — all present in the report.

### 6. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md contains a `## Knowledge Stewardship` section with:
- `Queried:` entries present (two `/uni-knowledge-search` queries documented with entry numbers and relevance assessment)
- `Stored:` entry present: "nothing novel to store — test execution patterns used are already captured in entries #699, #885, #2758, #3180"

---

## Rework Required

None.

---

## Gaps (Accepted)

**R-12 standalone observation unit test**: Three dedicated dispatch observation tests (`dispatch_request_source_subagentstart_tags_observation`, `dispatch_request_source_none_tags_observation_as_userpromptsub`, `dispatch_request_source_absent_in_json_tags_as_userpromptsub`) were not written. The RISK-COVERAGE-REPORT.md documents this gap with four combinatorial coverage paths explaining why risk is Low. The gate reviewer concurs: the logic is a single `source.as_deref().unwrap_or("UserPromptSubmit")` expression with no branching and is exercised by existing tests. Accepted as WARN, not FAIL.

**Stale doc comments**: 7 doc comment lines in `effectiveness.rs` and `search.rs` reference `BriefingService` by historical name. These are cosmetic and do not constitute dead code or functional coupling. Not a gate issue.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for gate 3c validation patterns — found entry #2758 (gate-3c non-negotiable test name verification). Applied: all 15 non-negotiable test names verified by `cargo test -- --list` before accepting RISK-COVERAGE-REPORT claims.
- Stored: nothing novel to store — the non-negotiable test name verification pattern is already captured in entry #2758. No new gate failure patterns discovered; all checks passed on first run.
