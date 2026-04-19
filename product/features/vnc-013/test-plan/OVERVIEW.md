# vnc-013 Test Plan Overview
## Canonical Event Normalization for Multi-LLM Hook Providers

---

## Test Strategy Summary

This feature is a normalization layer at the hook ingest boundary. The blast radius
spans 6 source files and 2 config files across 2 crates. Testing is organized into
three tiers:

1. **Unit tests** — pure function assertions, all in-process, no I/O. Cover 100% of
   the new `normalize_event_name()` function and all dispatch arm changes.
2. **Integration tests (unit-level)** — `#[tokio::test]` tests within crate `mod tests`
   blocks that exercise cross-crate serde boundaries and multi-step dispatch paths.
3. **Integration harness (infra-001)** — MCP JSON-RPC protocol-level tests via the
   compiled `unimatrix-server` binary for the two ACs requiring server round-trips
   (AC-02 and AC-09).

**Gate prerequisite**: AC-14 unit tests (R-01 scenarios 1–3: mcp_context.tool_name
promotion, non-cycle fallthrough, missing key degradation) must be green before any
other Gemini BeforeTool AC is implemented or tested. These three tests are the
earliest-warning signal for the entire Gemini dispatch path.

---

## Risk-to-Test Mapping

| Risk | Priority | Component(s) | Test File(s) | Scenarios |
|------|----------|-------------|-------------|-----------|
| R-01: mcp_context.tool_name silent fallthrough | Critical | normalization, wire-protocol | normalization.md | 4 |
| R-02: ImplantEvent.provider None at construction sites | High | normalization, wire-protocol | normalization.md, wire-protocol.md | 3 |
| R-03: Codex --provider omission silent mislabel | High | normalization, reference-configs | normalization.md, reference-configs.md | 4 |
| R-04: Approach A fallback contract regression | High | source-domain-derivation | source-domain-derivation.md | 4 |
| R-05: Rework detection entered for Gemini AfterTool | High | normalization | normalization.md | 4 |
| R-06: test_parse_rows_unknown_event_type_passthrough break | Med | source-domain-derivation | source-domain-derivation.md | 3 |
| R-07: rework_candidate string escapes to hook column | Med | normalization | normalization.md | 3 |
| R-08: Gemini SessionEnd dispatch path missed | Med | normalization | normalization.md | 3 |
| R-09: Blast radius miss — file without AC coverage | Med | all | OVERVIEW.md (checklist) | 1 |
| R-10: extract_event_topic_signal degradation for Gemini | Med | normalization | normalization.md | 2 |
| R-11: Gemini AfterTool response fields null | Low | normalization | normalization.md | 1 |
| R-12: Gemini matcher regex invalid | Low | reference-configs | reference-configs.md | 2 |
| R-13: HookInput backward deserialization regression | Low | wire-protocol | wire-protocol.md | 2 |

**Total minimum unit test scenarios**: ~35
**Integration harness scenarios**: AC-02 and AC-09 (2 new integration tests)

---

## Test Ordering: Gate Prerequisite

The implementer MUST write and green-bar the following three tests before any other
Gemini BeforeTool AC implementation attempt:

1. `test_gemini_mcp_context_tool_name_promotion` — cycle_start output verified (AC-14)
2. `test_gemini_before_tool_non_cycle_fallthrough` — non-cycle passthrough (AC-14)
3. `test_mcp_context_missing_tool_name_degrades_gracefully` — missing key no panic (AC-14)

Rationale: a failure in the promotion adapter produces a silent `RecordEvent {
event_type: "PreToolUse" }` with exit code 0. There is no error signal. The test is
the only way to detect a broken promotion step before investing time on other ACs.

---

## Blast Radius Coverage Checklist (R-09)

Every file in the blast radius must have at least one passing AC before gate sign-off.
The tester verifies this checklist at Stage 3c execution:

| File | Validating ACs | Verified |
|------|----------------|---------|
| `crates/unimatrix-engine/src/wire.rs` | AC-05, AC-14 | [ ] |
| `crates/unimatrix-server/src/uds/hook.rs` | AC-01–AC-05, AC-11, AC-12, AC-14, AC-15, AC-17, AC-18 | [ ] |
| `crates/unimatrix-server/src/uds/listener.rs` | AC-06, AC-07(a), AC-16 | [ ] |
| `crates/unimatrix-server/src/background.rs` | AC-07(b) | [ ] |
| `crates/unimatrix-server/src/services/observation.rs` | AC-07(c), AC-08 | [ ] |
| `crates/unimatrix-server/src/main.rs` | AC-15, AC-17, AC-18 | [ ] |
| `.gemini/settings.json` | AC-10 | [ ] |
| `.codex/hooks.json` | AC-19 | [ ] |

---

## Cross-Component Test Dependencies

```
wire-protocol (HookInput/ImplantEvent structs)
    │
    └─ normalization (build_request, normalize_event_name)
           │
           ├─ source-domain-derivation (Site A: listener.rs — depends on ImplantEvent.provider)
           │
           └─ reference-configs (AC-10, AC-19 — independent of code changes)
```

- `normalization.md` tests that use `HookInput.mcp_context` depend on `wire-protocol`
  changes being in place first (the field must exist on the struct).
- `source-domain-derivation.md` Site A test depends on `ImplantEvent.provider` existing.
- `source-domain-derivation.md` Sites B and C depend only on `DomainPackRegistry` (unchanged).
- `reference-configs.md` tests are file existence checks — independent of code.

---

## Integration Harness Plan (infra-001)

### Suite Selection

This feature touches hook-to-listener write path and lifecycle flow. The relevant
suites per the selection table:

| Suite | Run? | Reason |
|-------|------|--------|
| `smoke` | YES | Mandatory minimum gate |
| `tools` | YES | Hook dispatch changes affect how events reach tools; verify no regression |
| `lifecycle` | YES | AC-02 and AC-09 require cycle_events write + context_cycle_review round-trip |
| `protocol` | NO | No protocol changes |
| `confidence` | NO | No confidence system changes |
| `contradiction` | NO | No contradiction detection changes |
| `security` | NO | No security boundary changes (mcp_context injection risk is covered by unit tests) |
| `volume` | NO | No schema or storage changes |
| `edge_cases` | NO | Edge cases covered by unit tests for this normalization-layer feature |

### Existing Suite Coverage

The existing `lifecycle` suite covers `context_cycle` flows using Claude Code events.
These tests MUST continue to pass — normalization is additive for Claude Code.

The existing `tools` suite tests all 12 MCP tools. Smoke tests cover the critical
store/search/correct paths. These must pass unchanged.

### New Integration Tests Required

Two new integration tests are required to close AC-02 and AC-09. These tests cannot
be written as pure unit tests because they exercise the full hook-to-listener-to-DB
write path, which requires the running binary.

**Test 1 — `test_gemini_before_tool_cycle_start_written` (lifecycle suite)**

File: `product/test/infra-001/suites/test_lifecycle.py`

Scenario: Inject a synthetic Gemini `BeforeTool` + `context_cycle(type="start")`
payload through the hook endpoint. Assert:
- `cycle_events` row exists with `event_type = "cycle_start"`
- `source_domain = "gemini-cli"` in the written `ObservationRecord`
- Hook exits 0

Uses `server` fixture (fresh DB). Requires binary to be running with UDS available.

Note: This test operates at the binary level — it invokes `unimatrix hook BeforeTool`
with a synthesized stdin payload, not through the MCP interface. This may require a
subprocess fixture or a dedicated hook-level fixture if not already present. If
significant harness infrastructure is needed, file a GH Issue and cover AC-02 via
a `#[tokio::test]` integration test within the crate instead (see `observation.rs`
`mod tests` for the pattern).

**Test 2 — `test_gemini_cycle_review_finds_gemini_cycle` (lifecycle suite)**

File: `product/test/infra-001/suites/test_lifecycle.py`

Scenario: After Test 1 writes `cycle_start`/`cycle_stop` events via the Gemini path,
call `context_cycle_review` via MCP and assert it returns non-empty results for the
feature. Uses `server` fixture.

Assessment: If the infra-001 harness does not already have a subprocess/stdin hook
injection fixture, these tests should be implemented as `#[tokio::test]` crate-level
integration tests rather than infra-001 tests. The tester must decide during Stage 3c
based on what fixtures exist.

### Commands for Stage 3c

```bash
# Mandatory smoke gate
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# Relevant suites
python -m pytest suites/test_tools.py suites/test_lifecycle.py -v --timeout=60
```

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Test Plan Component | Key Test |
|-----------|------------------|--------------------|----|
| SR-01: Codex mislabel | R-03 | reference-configs.md + normalization.md | test_normalize_codex_with_provider_hint |
| SR-02: Gemini AfterTool response null | R-11 | normalization.md | test_gemini_after_tool_response_fields_degrade |
| SR-03: DB-read-path "unknown" regression | R-04 | source-domain-derivation.md | test_approach_a_fallback_for_stop_event |
| SR-04: rework_candidate guard scope | R-07 | normalization.md | test_rework_candidate_guard_fires_in_debug |
| SR-05: Codex not live-testable | R-03 (partial) | reference-configs.md | (synthetic only, noted) |
| SR-06: context_cycle_review Gemini | R-01 | normalization.md + lifecycle integration | AC-09 integration test |
| SR-07: 6-file blast radius | R-09 | OVERVIEW.md checklist | per-file AC checklist |
| SR-08: mcp_context silent fallthrough | R-01 | normalization.md | test_gemini_mcp_context_tool_name_promotion |
| SR-09: extract_event_topic_signal | R-10 | normalization.md | test_gemini_before_tool_topic_signal |
