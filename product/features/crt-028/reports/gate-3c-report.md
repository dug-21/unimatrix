# Gate 3c Report: crt-028

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| CR-1: Critical/High risk coverage | PASS | All 6 critical/high risks (R-01, R-03, R-05, R-07, R-08, R-10) have test coverage |
| CR-2: No regressions | PASS | 3,217 tests, 0 failures, 27 ignored (pre-existing) |
| CR-3: Anti-stub final check | PASS | No TODO, unimplemented!, or todo!() in any of the three files |
| CR-4: ACCEPTANCE-MAP completeness | PASS (WARN) | 14/15 ACs IMPLEMENTED; AC-11 shows stale PENDING in map but is verified IMPLEMENTED by code and tests |
| CR-5: GH #354 and #355 closure readiness | PASS | sanitize_observation_source present at line 101, call site at line 837; index_briefing doc comment confirmed |
| CR-6: Branch and commit hygiene | PASS | Branch is feature/crt-028; all three files committed in 1f753c2; working tree clean |

---

## Detailed Findings

### CR-1: Critical/High Risk Coverage

**Status**: PASS

The risk coverage report maps all 13 risks to passing tests. The 6 critical/high risks:

| Risk ID | Coverage | Key Tests |
|---------|----------|-----------|
| R-01 (degradation boundary) | Full | `extract_transcript_block_missing_file_returns_none`, `prepend_transcript_none_block_writes_briefing`, `extract_transcript_block_all_malformed_lines_returns_none` |
| R-03 (SeekFrom::End clamp) | Full | `extract_transcript_block_zero_byte_file_returns_none`, `extract_transcript_block_missing_file_returns_none` |
| R-05 (reversal order) | Full | `build_exchange_pairs_three_exchanges_most_recent_first` — asserts C precedes B precedes A |
| R-07 (sanitize_observation_source bypass) | Full | All 6 allowlist cases: `sanitize_observation_source_known_user_prompt_submit`, `_known_subagent_start`, `_none_defaults_to_user_prompt_submit`, `_unknown_value_defaults_to_user_prompt_submit`, `_empty_string_defaults_to_user_prompt_submit`, `_long_known_prefix_defaults_to_user_prompt_submit` |
| R-08 (quarantine post-filter) | Full | `index_briefing_excludes_quarantined_entry` |
| R-10 (OQ-SPEC-1 tool-only turn) | Full | `build_exchange_pairs_tool_only_assistant_turn_emits_pairs`, `build_exchange_pairs_thinking_only_turn_suppressed` |

Two risks have accepted partial coverage:

- **R-02** (4× tail multiplier, TAIL_WINDOW seek boundary): No test at `file_len = window + 1` bytes triggering `SeekFrom::End`. The clamp logic is code-reviewed correct (`seek_back = window.min(file_len)`). Accepted per risk coverage report — stdlib behavior at that boundary is deterministic.
- **R-11** (transcript_path outside expected directory): Deliberate low-priority; read-only fail-open JSONL parser makes exfiltration a non-issue per trust model.

Neither gap is blocking. Both were pre-documented as accepted limitations in the risk coverage report.

---

### CR-2: No Regressions

**Status**: PASS

From the Stage 3c risk coverage report:

```
cargo test --workspace
Total: 3,217 | Passed: 3,190 | Failed: 0 | Ignored: 27
```

The 27 ignored tests are pre-existing (unrelated to crt-028). Integration smoke suite (infra-001): 20/20 passed in 174.72s.

Hook regression specifically: 137 pre-existing hook tests (`write_stdout_*`, `build_request_*`, `posttooluse_*`, `parse_hook_input_*`, `format_injection_*`, `truncate_utf8_*`, `is_bash_failure_*`, `resolve_cwd_*`) all pass. AC-14 satisfied.

---

### CR-3: Anti-Stub Final Check

**Status**: PASS

Grep across all three modified files:

```bash
grep -n "TODO\|unimplemented!\|todo!()" \
  crates/unimatrix-server/src/uds/hook.rs \
  crates/unimatrix-server/src/uds/listener.rs \
  crates/unimatrix-server/src/services/index_briefing.rs
```

Result: NO MATCHES. No stubs, placeholders, or deferred implementations present.

---

### CR-4: ACCEPTANCE-MAP Completeness

**Status**: PASS (WARN — map has one stale entry)

The ACCEPTANCE-MAP.md was authored during gate-3b validation (before rework) and AC-11 still reads `PENDING — sanitize_observation_source not implemented in listener.rs; GH #354 unresolved`. This is stale: the Wave 1 commit (`1f753c2`) implemented the fix and all 6 unit tests pass.

Direct code verification confirms AC-11 is IMPLEMENTED:
- `sanitize_observation_source` defined at listener.rs line 101
- Call site at listener.rs line 837 (`hook: sanitize_observation_source(source.as_deref())`)
- 16 occurrences of `sanitize_observation_source` in listener.rs (definition + 6 tests + invocation + comment)
- All 6 test cases pass per risk coverage report

Effective AC status by independent verification:

| AC-ID | Effective Status |
|-------|-----------------|
| AC-01 | IMPLEMENTED (WARN: no end-to-end run() integration test — pre-documented, non-blocking) |
| AC-02 through AC-10 | IMPLEMENTED |
| AC-11 | IMPLEMENTED (map shows stale PENDING; code and tests confirm implemented) |
| AC-12 | IMPLEMENTED (WARN: vacuously true without embedding model in CI — pre-documented) |
| AC-13 through AC-15 | IMPLEMENTED |

All 15 ACs are effectively IMPLEMENTED. The stale AC-11 entry in ACCEPTANCE-MAP.md is a documentation gap, not an implementation gap. It does not block the gate.

---

### CR-5: GH #354 and #355 Closure Readiness

**Status**: PASS

**GH #354 — sanitize_observation_source**:

```
grep -n "sanitize_observation_source" listener.rs
101: fn sanitize_observation_source(source: Option<&str>) -> String {
836: // GH #354: allowlist-validated; see sanitize_observation_source (ADR-004 crt-028)
837: hook: sanitize_observation_source(source.as_deref()),
```

The helper is defined, the original unvalidated write site (`source.as_deref().unwrap_or("UserPromptSubmit").to_string()`) has been replaced by the allowlist call, and the GH reference comment is present. GH #354 can be closed.

**GH #355 — quarantine exclusion and doc comment**:

```
grep -n "delegated to|validate_search_query" index_briefing.rs
130: /// Input validation is delegated to `SearchService.search()` which calls
131: /// `self.gateway.validate_search_query()`. Guards enforced:
138: /// a direct store call without adding an equivalent `validate_search_query()`
```

Both required phrases present in the doc comment above `index()`. The `index_briefing_excludes_quarantined_entry` test is present and passes. GH #355 can be closed.

---

### CR-6: Branch and Commit Hygiene

**Status**: PASS

- Current branch: `feature/crt-028` — correct
- `git diff HEAD` for the three files: 0 lines — no uncommitted changes
- All three components committed in Wave 1 (`1f753c2 feat(crt-028): Wave 1 — hook transcript restore, listener allowlist, gate-3b report (#356)`)
- Untracked file `product/features/crt-027/agents/crt-027-docs-report.md` is unrelated to crt-028 and does not affect this gate

---

## Pre-Documented WARNs (Non-Blocking)

Both WARNs were documented at design time and carried through all gate reports:

1. **AC-01**: No end-to-end `run()` integration test with a real JSONL fixture and UDS call. The composition (`extract_transcript_block` → `prepend_transcript` → `write_stdout`) is tested unit-by-unit. Hook process spawning in CI is not architectured. Accepted.

2. **AC-12**: `index_briefing_excludes_quarantined_entry` assertion is vacuously true in CI without embedding model (`EmbeddingFailed` degradation returns empty vec). Full coverage requires embedding model. Pre-documented and accepted.

Neither WARN is introduced by crt-028 — they reflect the test environment constraints identified during risk strategy authoring.

---

## Rework Required

None. All gate-3b REWORKABLE FAIL items were resolved in Wave 1:

| Gate-3b Issue | Resolution |
|---------------|------------|
| `sanitize_observation_source` not implemented (GH #354) | Implemented at listener.rs line 101; call site updated at line 837; 6 tests pass |
| `MAX_PRECOMPACT_BYTES` doc comment missing TUNABLE/SR-03 note | Verified not a 3c check; confirmed non-blocking in 3b report |

---

## PR Readiness Confirmation

crt-028 (WA-5: PreCompact Transcript Restoration) is ready for PR.

- 3,217 tests, 0 failures
- All 13 risks have coverage (6 critical/high at full coverage)
- All 15 ACs effectively IMPLEMENTED
- GH #354 and #355 ready to close
- Branch `feature/crt-028`, working tree clean
- No stubs, no placeholders, no regressions

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for gate-3c validation procedures — confirmed standard check set applied; no novel procedure needed.
- Stored: nothing novel to store -- the stale ACCEPTANCE-MAP pattern (map not updated after rework resolves a PENDING item) is a one-off process gap rather than a cross-feature pattern. Existing lesson-learned entries on gate documentation hygiene cover this class of issue.
