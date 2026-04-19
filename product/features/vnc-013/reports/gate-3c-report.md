# Gate 3c Report: vnc-013

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 risks mapped to passing tests; RISK-COVERAGE-REPORT.md present |
| Test coverage completeness | WARN | AC-02/AC-09 covered at unit level only; full subprocess integration test deferred (harness substrate gap, documented in OVERVIEW.md) |
| Specification compliance | PASS | All 20 ACs verified; 18 full PASS, 2 PASS (unit-level with documented gap) |
| Architecture compliance | PASS | All six blast-radius files updated; DomainPackRegistry unchanged; no DB schema changes; config files present |
| Knowledge stewardship | PASS | Tester agent report has Queried: and Stored: entries |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: `product/features/vnc-013/testing/RISK-COVERAGE-REPORT.md` maps all 13 risks from `RISK-TEST-STRATEGY.md` to passing tests. No risk is listed as uncovered.

| Risk | Coverage | Result |
|------|----------|--------|
| R-01 (mcp_context promotion) | `test_gemini_mcp_context_tool_name_promotion`, `test_gemini_before_tool_non_cycle_fallthrough`, `test_mcp_context_missing_tool_name_degrades_gracefully`, `test_cycle_event_provider_propagated` | PASS |
| R-02 (ImplantEvent.provider None) | `test_implant_event_provider_set_for_record_event_variants`, `test_cycle_event_provider_propagated`, `test_implant_event_provider_none_not_serialized` | PASS |
| R-03 (Codex mislabel) | `test_run_codex_provider_hint`, `test_codex_post_tool_use_skips_rework_path`, AC-19 config check | PASS |
| R-04 (Approach A fallback regression) | `test_approach_a_fallback_for_stop_event`, `test_approach_a_fallback_for_session_start`, `test_approach_a_fallback_for_cycle_events`, `test_parse_rows_hook_path_always_claude_code` | PASS |
| R-05 (Rework detection for Gemini AfterTool) | `test_gemini_after_tool_skips_rework_path`, `test_codex_post_tool_use_skips_rework_path`, `test_claude_code_post_tool_use_enters_rework_path` | PASS |
| R-06 (test_parse_rows_unknown_event_type_passthrough contract) | `test_parse_rows_unknown_event_type_passthrough` (comment updated, DEFAULT_HOOK_SOURCE_DOMAIN assertion added) | PASS |
| R-07 (rework candidate escapes to hook column) | `test_rework_candidate_guard_fires_in_debug` (#[cfg(debug_assertions)], #[should_panic]), `test_post_tool_use_failure_arm_unchanged` | PASS |
| R-08 (Gemini SessionEnd dispatch path) | `test_gemini_session_end_produces_session_close`, `test_normalize_event_name_gemini_unique_names` | PASS |
| R-09 (blast radius miss) | Per-file checklist verified; all 6 blast-radius files have passing AC coverage | PASS |
| R-10 (extract_event_topic_signal degradation) | `test_gemini_before_tool_topic_signal_extraction` | PASS |
| R-11 (AfterTool response fields null) | `test_gemini_after_tool_response_fields_degrade_gracefully` | PASS |
| R-12 (Gemini matcher regex invalid) | AC-10 config validation; `.gemini/settings.json` confirmed with `mcp_unimatrix_.*` matcher | PASS |
| R-13 (backward deserialization regression) | `test_hook_input_deserializes_without_new_fields`, `test_implant_event_deserializes_without_provider` | PASS |

Gate prerequisite (R-01 scenarios 1-3 / AC-14) confirmed green: all three unit tests pass, establishing the mcp_context promotion path is sound before any other Gemini BeforeTool AC is exercised.

---

### 2. Test Coverage Completeness

**Status**: WARN

**Evidence**: The RISK-TEST-STRATEGY.md specifies ~35 scenarios across 28 unit and 7 integration tests. The RISK-COVERAGE-REPORT.md shows:
- Unit tests: 4,725 total (0 failures, 28 pre-existing ignored for ONNX runtime)
- Integration smoke gate: 23 passed, 0 failed
- Integration suites (test_tools.py + test_lifecycle.py): 166 passed, 7 xfailed, 2 xpassed, 0 failed

**AC-02 and AC-09 gap**: The RISK-TEST-STRATEGY.md requires integration tests that inject synthetic Gemini `BeforeTool`+`context_cycle` events through the full hook-to-listener path (subprocess/stdin) and verify `cycle_events` writes and `context_cycle_review` behavior. These are covered at unit level only:
- AC-02: `test_gemini_mcp_context_tool_name_promotion` verifies `build_request()` produces `RecordEvent { event_type: "cycle_start" }` for the Gemini path. The `cycle_events` write path in `listener.rs::handle_cycle_event()` is unchanged from pre-vnc-013 and is covered by existing `test_dispatch_cycle_start_*` tests.
- AC-09: `context_cycle_review` uses canonical event names in its SQL queries. Since normalization ensures Gemini `BeforeTool` arrives as canonical `"PreToolUse"` and `context_cycle` interception produces `"cycle_start"`, existing lifecycle suite tests covering `context_cycle_review` exercise the relevant code paths.

The OVERVIEW.md correctly identified the harness substrate gap (no subprocess/stdin fixture in infra-001) before implementation. This is a documented, accepted limitation — not a test deletion or xfail bypass. The individual code paths are each exercised; the missing piece is an end-to-end subprocess injection test.

**xfailed/xpassed tests**: Seven pre-existing xfails have GH issue markers (confirmed in test_lifecycle.py lines 564, 704, 1511, 2072, 2131, 2194, 2245) and are unrelated to vnc-013. Two pre-existing xpasses (`test_search_multihop_injects_terminal_active` GH#406, `test_inferred_edge_count_unchanged_by_cosine_supports`) indicate the underlying issues were fixed; their xfail markers can be removed in a follow-up PR. No integration tests were deleted or commented out.

RISK-COVERAGE-REPORT.md includes integration test counts (smoke: 23, suites: 166+7+2).

---

### 3. Specification Compliance

**Status**: PASS

**Evidence**: All 20 acceptance criteria verified. Spot-checked key implementations:

**FR-01 (normalize_event_name)**: Implemented in `hook.rs` lines 96-118 as a one-parameter public function (inference path) plus `map_to_canonical()` private function (hint path), split from the spec's two-parameter design per the pseudocode's documented rationale. Behavioral contract is identical: Gemini-unique names infer `"gemini-cli"`, Claude Code names pass through as `"claude-code"`, unknown names produce `("__unknown__", "unknown")`. The spec's `provider_hint = Some(p)` path is handled by `run()` calling `map_to_canonical()` and setting `hook_input.provider = Some(hint.clone())` directly — identical contract, different encapsulation. Gate 3b approved this deviation as matching pseudocode intent.

**FR-02 (wire protocol)**: `HookInput.provider: Option<String>` with `#[serde(default)]`, `HookInput.mcp_context: Option<serde_json::Value>` with `#[serde(default)]`, `ImplantEvent.provider: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — all confirmed in `wire.rs` at the correct struct locations with correct serde attributes.

**FR-06 (source_domain derivation)**:
- Site A (`listener.rs`): `source_domain` is NOT stored in the `observations` table and is NOT part of `ObservationRow`. The live write path threads `ImplantEvent.provider` through `extract_observation_fields()` but the `observations` table has no `source_domain` column (NFR-03, C-01). The `listener.rs:1894` reference in the spec points to `content_based_attribution_fallback()`, a DB read helper that correctly uses `DEFAULT_HOOK_SOURCE_DOMAIN.to_string()` — this is Approach A simplified (registry not accessible in that sync closure per gate-3b finding).
- Site B (`background.rs:1338-1345`): Approach A pattern — `registry.resolve_source_domain()` with `DEFAULT_HOOK_SOURCE_DOMAIN` fallback. Confirmed.
- Site C (`services/observation.rs:594-600`): Approach A pattern; `_registry` prefix removed; `DEFAULT_HOOK_SOURCE_DOMAIN` constant defined at line 569. Confirmed.

The known limitation (Gemini events stored as canonical "PreToolUse" will return "claude-code" on DB read paths) is the accepted OQ-4 behavior per ARCHITECTURE.md and is consistent with NFR-03.

**FR-09 (reference configs)**:
- `.gemini/settings.json`: Exists, valid JSON, 4 events (BeforeTool, AfterTool, SessionStart, SessionEnd), matcher `mcp_unimatrix_.*` on tool-scoped events, `--provider gemini-cli` on all commands. Format matches Gemini CLI v0.31+.
- `.codex/hooks.json`: Exists, valid JSON, 4 events (PreToolUse, PostToolUse, SessionStart, Stop), `--provider codex-cli` on all commands, Codex bug #16732 caveat present in `_comment` field.

**Non-functional requirements**: NFR-01 (synchronous hook.rs) confirmed. NFR-03 (no DB schema changes) confirmed — observations table INSERT statements show no source_domain column. NFR-07 (no new crate dependencies) confirmed.

---

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

**Component boundaries**: 4-layer architecture (wire extension, normalization, source_domain derivation, reference configs) maintained across 3 crates. All layer responsibilities confirmed in implementation.

**Blast radius coverage**:

| File | Change | Status |
|------|--------|--------|
| `crates/unimatrix-engine/src/wire.rs` | provider + mcp_context on HookInput; provider on ImplantEvent with correct serde attrs | PASS |
| `crates/unimatrix-server/src/uds/hook.rs` | normalize_event_name(), map_to_canonical(), run() two-path normalization, build_request() rework gate, mcp_context promotion, debug_assert at build_request() entry | PASS |
| `crates/unimatrix-server/src/uds/listener.rs` | Site A: DEFAULT_HOOK_SOURCE_DOMAIN in content_based_attribution_fallback; debug_assert in extract_observation_fields() for AC-16 | PASS |
| `crates/unimatrix-server/src/background.rs` | Site B: Approach A registry-with-fallback at lines 1338-1345 | PASS |
| `crates/unimatrix-server/src/services/observation.rs` | Site C: Approach A at lines 594-600; DEFAULT_HOOK_SOURCE_DOMAIN const at line 569; _registry prefix removed | PASS |
| `crates/unimatrix-server/src/main.rs` | Hook variant has provider: Option<String>; dispatch passes to run() | PASS |
| `.gemini/settings.json` | New reference config | PASS |
| `.codex/hooks.json` | New reference config | PASS |

**DomainPackRegistry zero-change invariant** (AC-13): `domain/mod.rs` not modified. Builtin claude-code pack contains only canonical event names (PreToolUse, PostToolUse, SubagentStart, SubagentStop). Gemini event names never reach the registry.

**ADR compliance**: ADR-001 (canonical = Claude Code names), ADR-002 (provider field on wire), ADR-003 (named mcp_context field), ADR-004 (Approach A for DB read path), ADR-005 (rework gate = provider != "claude-code"), ADR-006 (--provider codex-cli mandatory) all implemented correctly.

**Build**: `cargo build --workspace` passes with 0 errors (18 pre-existing warnings in unimatrix-server, unrelated to vnc-013).

**cargo audit**: Not installed in this environment. Pre-existing environment limitation noted in gate-3b. No new crate dependencies introduced (NFR-07 confirmed).

---

### 5. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: `product/features/vnc-013/agents/vnc-013-agent-4-tester-report.md` contains `## Knowledge Stewardship` section with:
- `Queried:` entries: `mcp__unimatrix__context_briefing` — entries #4311, #4312, #3386, #3253, #3806 surfaced; applied to AC-14 gate-prerequisite framing.
- `Stored:` entry: "nothing novel to store — the gate-prerequisite pattern for silent-exit-0 normalization failures is already captured in entry #4311." Reason provided.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- AC-02/AC-09 harness substrate gap is a feature-specific finding (hook subprocess injection fixture missing from infra-001), not a reusable cross-feature lesson. Risk coverage patterns for hook normalization features are already captured in entries #4298, #4311.
