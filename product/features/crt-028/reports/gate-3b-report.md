# Gate 3b Report: crt-028

> Gate: 3b (Code Review)
> Date: 2026-03-23
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| C1: Anti-stubs | PASS | No TODO, unimplemented!, todo!, or FIXME in new code |
| C2: Code vs pseudocode alignment | FAIL | hook.rs and index_briefing.rs align with pseudocode; listener.rs `sanitize_observation_source` not implemented (GH #354 unaddressed) |
| C3: Tests vs test plan alignment | FAIL | All hook.rs and index_briefing.rs tests present and passing; sanitize_observation_source tests absent (helper not implemented) |
| C4: AC coverage | FAIL | AC-11 unimplemented; AC-01 has no direct integration test (WARN); 13 of 15 ACs verified |
| C5: Security | FAIL | GH #354 not fixed: `source` field written unvalidated to observations `hook` column; `sanitize_observation_source` absent |

---

## Detailed Findings

### C1: Anti-stubs

**Status**: PASS

No `TODO`, `FIXME`, `todo!()`, or `unimplemented!()` found in any of the three modified files.

All new functions have complete implementations:
- `extract_key_param`, `build_exchange_pairs`, `extract_transcript_block`, `prepend_transcript`, `format_turn`, `get_content_array`, `extract_tool_result_snippet` — all fully implemented in hook.rs.
- Modification to `run()` (Step 5c + write_result intercept) — complete.
- `index_briefing_excludes_quarantined_entry` regression test — complete.
- Doc comment on `index()` — present.

---

### C2: Code vs Pseudocode Alignment

**Status**: FAIL

#### hook.rs — PASS with one WARN

All functions match the pseudocode specification:

**`extract_key_param`**: All 10 known tools implemented correctly (Bash→command, Read/Edit/Write/MultiEdit→file_path, Glob/Grep→pattern, Task→description, WebFetch→url, WebSearch→query). First-string-field fallback present. Truncates to `TOOL_KEY_PARAM_BYTES` via `truncate_utf8`. Matches pseudocode exactly.

**`build_exchange_pairs`**: Fail-open on malformed JSON (skips lines). OQ-SPEC-1 correctly implemented: tool-only turns emit `ToolPair` only (no `AssistantText`); thinking-only turns suppressed. Adjacent-record look-ahead for tool_result pairing (ADR-002). `Vec.reverse()` before return for reverse-chronological order. User `tool_result` blocks skipped (FR-03.6). Matches pseudocode.

**`extract_transcript_block`**: Inner closure pattern (`|| -> Option<String>`) contains all `?` operators (ADR-003). Seek clamp: `let seek_back: u64 = window.min(file_len)` — correct (Constraint 8). Zero-byte file: `seek_back == 0` → no seek → no lines → None. `BufReader` + `lines()` collection. Budget loop halts on overflow. Returns None when `output_parts.is_empty()`. Header/footer use `===` format. Matches pseudocode.

**`prepend_transcript`**: All 4 cases implemented. Case 1: `format!("{}\n\n{}", t, briefing)` — double newline separator (FR-05.1, Gate 3a Warning 1 correctly fixed). Case 2/3/4 match pseudocode.

**`run()` Step 5c**: `transcript_block` extracted before `transport.request()`, only for `CompactPayload`, using `.filter(|p| !p.is_empty()).and_then(|p| extract_transcript_block(p))`. Correct.

**`run()` write_result**: `BriefingContent` intercepted; `prepend_transcript` called; non-empty output written via `println!`; `_ => write_stdout(&response)` for all other variants. Non-SubagentStart path correctly modified. Matches pseudocode.

**`format_turn`**: Uses `\u{2192}` for the arrow character, which renders as `→` — matches the IMPLEMENTATION-BRIEF output format contract `[tool: {name}({key_param}) → {snippet}]`.

**WARN — `MAX_PRECOMPACT_BYTES` doc comment missing TUNABLE note**: The IMPLEMENTATION-BRIEF requires: `"MAX_PRECOMPACT_BYTES must carry a doc comment noting it is a tunable for a future config.toml pass (SR-03 acknowledgment)."` The pseudocode specifies:
```
/// TUNABLE: future config.toml pass may make this runtime-configurable (FR-04.4, SR-03).
```
The implementation has:
```rust
/// Maximum byte budget for the PreCompact transcript restoration block (~750 tokens).
/// Separate from MAX_INJECTION_BYTES (1400) per D-4 and AC-10.
const MAX_PRECOMPACT_BYTES: usize = 3000;
```
The TUNABLE/SR-03 acknowledgment line is absent. Non-blocking (doc comment only), but this was an explicit brief requirement.

#### listener.rs — FAIL

**`sanitize_observation_source` NOT IMPLEMENTED.**

`crates/unimatrix-server/src/uds/listener.rs` line 813 retains the original unvalidated expression:
```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

The `sanitize_observation_source(source: Option<&str>) -> String` helper was not added to `listener.rs`. The replacement call site was not updated. GH #354 is unresolved.

The pseudocode in `pseudocode/listener.md` specifies:
1. Add `sanitize_observation_source` as a private fn near `sanitize_session_id` and `sanitize_metadata_field` helpers.
2. Replace `source.as_deref().unwrap_or("UserPromptSubmit").to_string()` with `sanitize_observation_source(source.as_deref())`.

Neither change is present. The last commit to `listener.rs` is `5d9c729` (crt-027, merged before crt-028 design work).

#### index_briefing.rs — PASS

Doc comment on `index()` is present and complete. The added text contains "delegated to" and "validate_search_query" (AC-13). The WARNING note about filter removal is present (GH #355).

`index_briefing_excludes_quarantined_entry` test is present, uses a real `Store` path (via `ServiceLayer`), inserts both active and quarantined entries, and asserts quarantined entry absent from results (AC-12).

---

### C3: Tests vs Test Plan Alignment

**Status**: FAIL

#### hook.rs tests — PASS (21 new tests confirmed)

| Test Plan Scenario | Test Name in Code | Status |
|--------------------|-------------------|--------|
| `max_precompact_bytes_constant_defined` (AC-10) | `max_precompact_bytes_constant_defined` | PASS — asserts 3000 AND ≠ MAX_INJECTION_BYTES |
| Missing file → None (R-01) | `extract_transcript_block_missing_file_returns_none` | PASS |
| `prepend_transcript(None, briefing)` = briefing (R-01) | `prepend_transcript_none_block_writes_briefing` | PASS |
| All-malformed JSONL → None (AC-08) | `extract_transcript_block_all_malformed_lines_returns_none` | PASS |
| Zero-byte file → None (R-03) | `extract_transcript_block_zero_byte_file_returns_none` | PASS |
| Three-exchanges most-recent-first (R-05, AC-02) | `build_exchange_pairs_three_exchanges_most_recent_first` | PASS |
| tool_result in user turn skipped (AC-04) | `build_exchange_pairs_user_tool_result_skipped` | PASS |
| Tool-only assistant emits ToolPair (OQ-SPEC-1, R-10) | `build_exchange_pairs_tool_only_assistant_turn_emits_pairs` | PASS |
| Thinking-only suppressed (OQ-SPEC-1, R-10) | `build_exchange_pairs_thinking_only_turn_suppressed` | PASS |
| Malformed lines skipped (AC-08) | `build_exchange_pairs_malformed_lines_skipped` | PASS |
| All 10 known tools correct field (extract_key_param) | `extract_key_param_known_tools_correct_field` | PASS |
| Unknown tool fallback | `extract_key_param_unknown_tool_first_string_field_fallback` | PASS |
| No string field → "" | `extract_key_param_no_string_field_returns_empty` | PASS |
| Key-param truncation | `extract_key_param_long_value_truncated` | PASS |
| prepend double-newline separator (Gate 3a Warning 1) | `prepend_transcript_both_present_separator_present` | PASS — asserts `"block\n\nbriefing"` |
| Transcript precedes briefing | `prepend_transcript_both_present_transcript_precedes_briefing` | PASS |
| Transcript only | `prepend_transcript_transcript_only_has_headers` | PASS |
| Both empty → "" | `prepend_transcript_both_none_empty_string` | PASS |
| None transcript → briefing verbatim | `prepend_transcript_none_block_writes_briefing_verbatim` | PASS |
| Byte budget enforced (AC-05) | `extract_transcript_block_respects_byte_budget` | PASS |
| System-only lines → None (AC-09) | `extract_transcript_block_system_only_returns_none` | PASS |
| Empty path → None | `extract_transcript_block_empty_path_returns_none` | PASS |

All 162 hook tests pass (`cargo test -p unimatrix-server hook`).

**Missing tests (due to listener.rs implementation gap)**:
- `sanitize_observation_source_all_cases` — not present (helper absent, AC-11)
- `context_search_source_sanitized_in_observation` (integration) — not present

#### listener.rs tests — FAIL

No new listener.rs tests added. None possible without `sanitize_observation_source`. The 6-case allowlist test (AC-11) is absent.

#### index_briefing.rs tests — PASS with WARN

`index_briefing_excludes_quarantined_entry` is present and passes. All existing `derive_briefing_query` and `extract_top_topic_signals` tests continue to pass.

**WARN — test environment limitation**: In CI without an embedding model, `EmbedServiceHandle` starts in Loading state, causing `SearchService.search()` to return `Err(EmbeddingFailed)`. The test uses `unwrap_or_default()` which degrades to an empty result. The assertion `!quarantined_in_results` is then vacuously true (empty vec contains neither entry). The positive assertion (`active_in_results`) is guarded with `if !entries.is_empty()`. This matches the documented constraint in the spawn prompt and the test's own doc comment. Non-blocking.

---

### C4: AC Coverage

**Status**: FAIL (2 ACs failing, 1 WARN)

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | WARN | `prepend_transcript` + `extract_transcript_block` + `run()` intercept all implemented; no direct integration test that runs `run()` end-to-end with a real transcript file and asserts stdout starts with `=== Recent conversation`. The component-level tests cover all sub-functions. |
| AC-02 | IMPLEMENTED | `build_exchange_pairs_three_exchanges_most_recent_first` |
| AC-03 | IMPLEMENTED | `extract_key_param_known_tools_correct_field` + `extract_key_param_long_value_truncated` |
| AC-04 | IMPLEMENTED | `build_exchange_pairs_user_tool_result_skipped` |
| AC-05 | IMPLEMENTED | `extract_transcript_block_respects_byte_budget` |
| AC-06 | IMPLEMENTED | `prepend_transcript_none_block_writes_briefing_verbatim` |
| AC-07 | IMPLEMENTED | `extract_transcript_block_missing_file_returns_none` |
| AC-08 | IMPLEMENTED | `extract_transcript_block_all_malformed_lines_returns_none` + `build_exchange_pairs_malformed_lines_skipped` |
| AC-09 | IMPLEMENTED | `extract_transcript_block_system_only_returns_none` |
| AC-10 | IMPLEMENTED | `max_precompact_bytes_constant_defined` — asserts value 3000 and distinctness from MAX_INJECTION_BYTES |
| AC-11 | FAIL | `sanitize_observation_source` not implemented; no tests possible |
| AC-12 | IMPLEMENTED (WARN) | `index_briefing_excludes_quarantined_entry` present; embedding-unavailable degradation documented |
| AC-13 | IMPLEMENTED | Doc comment on `index()` contains "delegated to" and "validate_search_query" |
| AC-14 | IMPLEMENTED | All 162 hook tests pass; existing tests unmodified |
| AC-15 | IMPLEMENTED | Structural enforcement: `extract_transcript_block` returns `Option<String>`; `run()` always returns `Ok(())` |

---

### C5: Security

**Status**: FAIL

#### No new `?` operators escaping `extract_transcript_block`

PASS. The inner closure pattern (`let inner = || -> Option<String> { ... }; inner()`) contains all `?` operators. No `Result` propagates from `extract_transcript_block`. ADR-003 contract structurally enforced.

#### No hardcoded paths or credentials

PASS. No hardcoded secrets, API keys, or credentials in new code.

#### No SQL injection

PASS. No new SQL queries in hook.rs or index_briefing.rs.

#### `write_stdout` structural integrity

PASS. Non-`BriefingContent` responses delegate to `write_stdout(&response)` unchanged. Only `BriefingContent` is intercepted in the new `else` branch. The SubagentStart path is fully unmodified.

#### GH #354 — Source field allowlist NOT FIXED

**FAIL**. The `source` field from `HookRequest::ContextSearch` is written unvalidated to the `hook TEXT NOT NULL` column in the observations table. `listener.rs` line 813:

```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

This allows any string value sent by a hook process over UDS to be written verbatim to the database column. The `sanitize_observation_source` allowlist helper was not added. This is the sole security fix in scope for crt-028 (GH #354, ADR-004, SR-05). It remains unaddressed.

#### cargo audit

Not installed in this environment (`cargo audit` command not found). Pre-existing condition; not introduced by crt-028. WARN only.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `sanitize_observation_source` not implemented in listener.rs (GH #354 unresolved) | rust-dev | Add `sanitize_observation_source(source: Option<&str>) -> String` helper near `sanitize_session_id` and `sanitize_metadata_field` helpers (around line 83 in listener.rs). Replace line 813 `source.as_deref().unwrap_or("UserPromptSubmit").to_string()` with `sanitize_observation_source(source.as_deref())`. Add unit test `sanitize_observation_source_all_cases` covering all 6 cases from ADR-004 (AC-11). See `pseudocode/listener.md` for exact code. |
| `MAX_PRECOMPACT_BYTES` doc comment missing TUNABLE/SR-03 note | rust-dev | Add `/// TUNABLE: future config.toml pass may make this runtime-configurable (FR-04.4, SR-03).` line to the doc comment on `MAX_PRECOMPACT_BYTES` constant (hook.rs line 36-38). |

---

## ACs Now IMPLEMENTED

The following ACs are verified by code and tests and should be updated to IMPLEMENTED:

- AC-02, AC-03, AC-04, AC-05, AC-06, AC-07, AC-08, AC-09, AC-10, AC-12 (WARN), AC-13, AC-14, AC-15

AC-01 is WARN (implementation present, end-to-end integration test absent — non-blocking).
AC-11 is FAIL (requires listener.rs rework).

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the "listener component skipped while hook component implemented" failure mode is a one-off delivery gap rather than a cross-feature pattern. The core gate check methodology and security-fix traceability patterns are already captured in existing lesson-learned entries.
