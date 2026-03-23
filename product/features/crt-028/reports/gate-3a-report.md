# Gate 3a Report: crt-028

> Gate: 3a (Design Review)
> Date: 2026-03-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All three components match architecture decomposition; interfaces match defined contracts |
| Specification coverage | PASS | All FRs and NFRs have corresponding pseudocode; no scope additions |
| Risk coverage | PASS | All 13 risks map to test scenarios; all 5 non-negotiable gate tests present with correct function names |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; data flow coherent |
| Knowledge stewardship compliance | PASS | Both design-phase agents have complete stewardship blocks with Queried/Stored entries |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

Component decomposition in pseudocode/OVERVIEW.md matches ARCHITECTURE.md exactly:

| Component | Architecture | Pseudocode |
|-----------|-------------|------------|
| `hook.rs` | 3 functions + run() mod + constants + ExchangeTurn | hook.md: all 4 functions + run() changes + all constants + ExchangeTurn |
| `listener.rs` | `sanitize_observation_source` helper, single replacement | listener.md: identical scope |
| `index_briefing.rs` | doc comment + regression test | index_briefing.md: identical scope |

Data flow in pseudocode/OVERVIEW.md is a direct reproduction of ARCHITECTURE.md data-flow diagram. No topology differences.

ADR compliance confirmed:

- ADR-001 (tail-bytes): Pseudocode hook.md uses explicit clamp `seek_back = window.min(file_len)` with `SeekFrom::End(-(seek_back as i64))` — the recommended clamp form, not the conditional branch. Correct.
- ADR-002 (adjacent-record pairing): Pseudocode implements look-ahead exactly as specified; edge cases (unmatched tool_use → empty snippet, orphaned tool_result → skip) correctly handled.
- ADR-003 (graceful degradation): `extract_transcript_block` returns `Option<String>`; inner closure pattern used; no `Result` escapes. Call site uses `and_then` chain. Contract structurally enforced.
- ADR-004 (source allowlist): `sanitize_observation_source` is an exhaustive match with `_` fallback; no length cap (correct per ADR). Sole write-gate pattern documented in function doc comment.

**No technology/architecture concerns.**

---

### Specification Coverage

**Status**: PASS

Coverage of functional requirements against pseudocode:

| FR | Requirement | Coverage |
|----|-------------|----------|
| FR-01.1–1.7 | Transcript extraction pipeline | hook.md: extract_transcript_block, BufRead from tail window, HookInput.transcript_path read from `Option<String>` |
| FR-02.1–2.7 | Output format with `===` headers, most-recent-first, section separator | hook.md: header/footer literals correct; prepend_transcript handles all 4 cases |
| FR-03.1–3.8 | Tool representation format, key-param map, snippet truncation | hook.md: extract_key_param with 10-tool map, TOOL_RESULT_SNIPPET_BYTES truncation, empty snippet for unmatched tool_use |
| FR-04.1–4.4 | MAX_PRECOMPACT_BYTES constant, budget enforcement | hook.md: constant defined with correct doc comment, budget loop halts at first overflow |
| FR-05.1–5.5 | Prepend behavior, 4 explicit cases | hook.md: prepend_transcript implements all 4 cases explicitly |
| FR-06.1–6.7 | Graceful degradation | hook.md: all failure paths → None; no stderr; exit 0 maintained |
| FR-07.1–7.6 | Source field allowlist | listener.md: sanitize_observation_source, sole write gate, all 6 test cases |
| FR-08.1–8.4 | Quarantine regression test + doc comment | index_briefing.md: real-store test with quarantined entry, doc comment warning against filter removal |

NFR compliance:
- NFR-01 (exit 0): Structurally enforced by Option<String> return type
- NFR-02 (synchronous I/O): All pseudocode uses `std::io` only; tokio not referenced
- NFR-05 (UTF-8 safety): `truncate_utf8` reused for all three truncation sites (TOOL_RESULT_SNIPPET_BYTES, TOOL_KEY_PARAM_BYTES, budget-fill)
- NFR-06 (constant at module level): Declared as module-level `const` with doc comment

**Minor gap (WARN)**: FR-03.3 lists only 6 tools in the key-param map (Bash, Read, Edit, Write, Glob, Grep). The Architecture and pseudocode define 10 tools (also MultiEdit, Task, WebFetch, WebSearch). The pseudocode and architecture are internally consistent; the spec table is incomplete. This is a spec gap, not a pseudocode defect — the architecture's extended map is the authoritative source per ARCHITECTURE.md "Key-Param Extraction Map (OQ-3 Settled)". The pseudocode correctly implements the 10-tool version.

**No scope additions found.** Pseudocode implements exactly what is specified; no unrequested features are added.

---

### Risk Coverage

**Status**: PASS

All 13 risks from RISK-TEST-STRATEGY.md map to test scenarios. Verification of non-negotiable gate tests:

**R-01 (non-negotiable)**: Required: "missing transcript → briefing still written"
- Present: `extract_transcript_block_missing_file_returns_none` (hook.md, line 34)
- Present: `prepend_transcript_none_block_writes_briefing` (hook.md, line 41)
- PASS

**R-03 (non-negotiable)**: Required: zero-byte file and window-boundary seek behavior
- Present: `extract_transcript_block_zero_byte_file_returns_none` (hook.md, line 57)
- Present: `extract_transcript_block_file_equals_window_reads_from_start` (hook.md, line 63)
- Present: `extract_transcript_block_file_one_byte_over_window_seeks` (hook.md, line 71)
- Present: `extract_transcript_block_window_minus_one_reads_from_start` (hook.md, line 77)
- PASS — all 4 boundary cases covered, including `file_len = 0`, `= window`, `= window-1`, `= window+1`

**R-07 (non-negotiable)**: Required: `sanitize_observation_source` unit test, all 6 cases
- Present: `sanitize_observation_source_all_six_cases` (listener.md, line 19; OVERVIEW.md table)
- All 6 cases from ADR-004 enumerated explicitly (listener.md lines 21–26)
- PASS

**R-08 (non-negotiable)**: Required: quarantine exclusion in `IndexBriefingService::index()`
- Present: `index_briefing_excludes_quarantined_entry` (index_briefing.md, line 41)
- Uses real store path as required; mutation-test property documented
- PASS

**R-10 (non-negotiable)**: Required: tool-only turn emitted; pure-thinking turn suppressed
- Present: `build_exchange_pairs_tool_only_assistant_turn_emits_pairs` (hook.md, line 211)
- Present: `build_exchange_pairs_thinking_only_turn_suppressed` (hook.md, line 219)
- OQ-SPEC-1 resolution correctly implemented: emit when tool_use present, suppress when both text and tool_use absent
- PASS

Full risk-to-test mapping is present in OVERVIEW.md table with explicit test function names for every risk.

---

### Interface Consistency

**Status**: PASS

Cross-component interface verification:

| Interface | OVERVIEW.md | hook.md | listener.md | index_briefing.md |
|-----------|-------------|---------|-------------|-------------------|
| `extract_transcript_block(path: &str) -> Option<String>` | Defined | Implements | N/A | N/A |
| `build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn>` | Defined | Implements | N/A | N/A |
| `prepend_transcript(transcript: Option<&str>, briefing: &str) -> String` | Defined | Implements | N/A | N/A |
| `extract_key_param(tool_name: &str, input: &serde_json::Value) -> String` | Defined | Implements | N/A | N/A |
| `sanitize_observation_source(source: Option<&str>) -> String` | Defined | N/A | Implements | N/A |
| `ExchangeTurn` enum (3 variants) | Defined | Implements | N/A | N/A |
| No-change boundaries (wire.rs, handle_compact_payload) | Documented | Documented | Documented | N/A |

All shared types in OVERVIEW.md (ExchangeTurn variants, constant values) match per-component pseudocode exactly. No contradictions found between component files.

**Single minor inconsistency (WARN)**: `prepend_transcript` separator format.

- SPECIFICATION.md FR-05.1 specifies: `{transcript_block}\n\n{briefing_content}` (double newline = blank line between blocks)
- pseudocode/hook.md `prepend_transcript` Case 1: `format!("{}\n\n{}", t, briefing)` — CORRECT, matches spec
- test-plan/hook.md `prepend_transcript_both_present_separator_present` (line 289): asserts `"block\nbriefing"` (single `\n`) with comment "single newline separator per FR-05.1"

The test plan's assertion for this specific test contradicts both the spec (FR-05.1 requires `\n\n`) and the pseudocode (`format!("{}\n\n{}", ...)` produces `\n\n`). The pseudocode is correct; the test plan contains a wrong assertion. This will cause a test failure if the test is implemented as written. The implementer must use `\n\n` (double newline) in the assertion, not `\n`.

---

### Knowledge Stewardship Compliance

**Status**: PASS

Both design-phase agents provided stewardship sections:

**crt-028-agent-1-pseudocode** (`agents/crt-028-agent-1-pseudocode-report.md`):
- `Queried:` entries present — queried `/uni-query-patterns` for PreCompact transcript pattern (found #3331) and crt-028 architectural decisions (found ADR entries #3333–#3336)
- No `Stored:` entry — stewardship block ends after the Queried entries without a Stored/Declined entry

This is a borderline gap: the report has a `Queried:` section but no explicit `Stored:` or "nothing novel to store" entry. Gate instructions require `Stored:` or a "nothing novel" explanation. Treating as WARN (the block is present and queries are documented; the missing `Stored:` line is an omission, not an absence of the section).

**crt-028-agent-2-testplan** (`agents/crt-028-agent-2-testplan-report.md`):
- `Queried:` entries present — 2 queries documented (ADR entries and hook.rs testing patterns)
- `Stored:` entry present — entry #3338 stored via `/uni-context-store`
- PASS

**Note on architect/risk-strategist reports**: The architect (`crt-028-agent-1-architect-report.md`) and risk strategist (`crt-028-agent-3-risk-report.md`) are active-storage agents. Their stewardship was reviewed in prior gate cycles (not scope of this 3a spawn which focuses on pseudocode + test plans). The RISK-TEST-STRATEGY.md itself contains a stewardship block (`Queried:` + `Stored: nothing novel to store -- {reason}`) — PASS.

---

## Warnings

| Warning | Severity | Location | Note |
|---------|----------|----------|------|
| FR-03.3 key-param map in spec lists only 6 tools; architecture/pseudocode has 10 | WARN (spec gap) | SPECIFICATION.md FR-03.3 | Pseudocode correctly implements 10-tool version from architecture. Spec should be updated to match. No block on implementation. |
| Test plan separator assertion for `prepend_transcript_both_present_separator_present` uses `\n` not `\n\n` | WARN (test defect) | test-plan/hook.md line 289 | Pseudocode and spec both require `\n\n`. Implementer must use `\n\n` in the assertion. Test plan comment "per FR-05.1" is wrong about what FR-05.1 says. |
| Pseudocode agent report missing explicit `Stored:` or "nothing novel" entry in stewardship block | WARN (stewardship) | agents/crt-028-agent-1-pseudocode-report.md | Block present; Queried entries present; Stored/Declined line absent. |
| `OQ-B` from pseudocode agent: blank-line grouping between exchange pairs not explicitly handled | WARN (impl note) | pseudocode/hook.md `format_turn`, agent report OQ-B | FR-02.3 requires blank line between pairs. The pseudocode joins turns with `"\n"` but does not insert blank lines between exchange-pair boundaries. Implementer must address this during coding. |

---

## Rework Required

None. All FAILs are absent. WARNs are acceptable for gate passage.

The two most important warnings for the implementer:

1. **Test separator fix**: `prepend_transcript_both_present_separator_present` must assert `"block\n\nbriefing"` (double newline), not `"block\nbriefing"`. The pseudocode is correct; fix the test assertion during implementation.

2. **Blank line between exchange pairs**: `format_turn` joins all turns with `"\n"` but FR-02.3 and ARCHITECTURE.md output format require a blank line between complete exchange pairs. The implementer must insert `""` separators between pairs (e.g., after the last ToolPair or AssistantText of each exchange group before the next UserText). See pseudocode agent OQ-B.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the separator inconsistency between spec and test plan is a one-off defect, not a cross-feature pattern. The interface consistency check pattern and stewardship completeness check are already captured in existing gate lesson entries.
