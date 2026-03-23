# Test Plan Overview: crt-027 — WA-4 Proactive Knowledge Delivery

## Test Strategy

crt-027 has two interrelated work areas (WA-4a and WA-4b) spanning eight components across
three crates. The test strategy is layered:

1. **Unit tests** — cover routing logic, format contracts, serialization, and guard boundary
   values in isolation, without a running server.
2. **Integration tests** (infra-001) — exercise the MCP `context_briefing` tool and UDS
   `handle_compact_payload` path through the compiled binary.
3. **Static / compile-time verification** — confirm `BriefingService` deletion is complete
   and that no `dead_code` warnings remain.
4. **Manual smoke** — AC-SR01 (SubagentStart stdout injection via Claude Code) is a manual
   gate item; it cannot be automated.

Test count target: test count at `hook.rs`, `listener.rs`, and `tools.rs` is **non-decreasing**
relative to the pre-feature baseline (AC-15). Old tests on removed constructs are
**rewritten**, not deleted.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Primary Test File(s) | Min Scenarios |
|---------|----------|--------------|---------------------|---------------|
| R-01 | Critical | wire.rs, hook.rs, listener.rs | wire-source-field.md, hook-routing.md, listener-dispatch.md | 5 |
| R-02 | High | index-briefing-service | index-briefing-service.md | 3 |
| R-03 | Critical | listener.rs | listener-dispatch.md | 11 named tests |
| R-04 | High | hook.rs | hook-routing.md | 6 |
| R-05 | High | mcp/response/briefing.rs | index-entry-formatter.md | 4 |
| R-06 | High | index-briefing-service, tools.rs, listener.rs | index-briefing-service.md, context-briefing-handler.md | 6 |
| R-07 | High | hook.rs | hook-routing.md | 4 (1 manual) |
| R-08 | Medium | services/mod.rs, Cargo.toml | service-layer-wiring.md | 3 |
| R-09 | Medium | index-briefing-service, services/mod.rs | index-briefing-service.md | 3 |
| R-10 | Medium | index-briefing-service | index-briefing-service.md | 3 |
| R-11 | Medium | protocol-update | protocol-update.md | 3 |
| R-12 | Medium | listener.rs | listener-dispatch.md | 3 |
| R-13 | Low | wire.rs | wire-source-field.md | 1 |
| R-14 | Low | listener.rs | listener-dispatch.md | 2 |

---

## Cross-Component Test Dependencies

| Dependency | Components Involved | Test Implication |
|------------|--------------------|--------------------|
| `HookRequest::ContextSearch.source` must compile everywhere | wire.rs → hook.rs → listener.rs → all test files constructing ContextSearch | All struct-literal constructions in existing tests require `source: None` or `..` spread (R-01, AC-25) |
| `IndexBriefingService::index` is called by both MCP handler and `handle_compact_payload` | index-briefing-service.md ↔ context-briefing-handler.md ↔ listener-dispatch.md | `derive_briefing_query` must be tested as a shared helper; assert the same function is used by both callers (AC-09, AC-10) |
| `IndexEntry` + `format_index_table` defined in `mcp/response/briefing.rs`, used in listener.rs | index-entry-formatter.md ↔ listener-dispatch.md | Format contract tests (R-05) guard both callers |
| `EffectivenessStateHandle` wiring in `ServiceLayer::with_rate_config()` | service-layer-wiring.md ↔ index-briefing-service.md | Wiring test must confirm no silent snapshot zero-generation |
| `#[cfg(feature = "mcp-briefing")]` gates MCP tool only | service-layer-wiring.md, context-briefing-handler.md | Two test runs required: with and without the feature flag (R-08, AC-24) |

---

## Non-Negotiable Test Names (Gate 3c — grep required)

Per RISK-TEST-STRATEGY.md and lesson #2758, the following test names **must exist by name**
in the final implementation. Gate reviewer must grep for each:

```
format_payload_empty_entries_returns_none
format_payload_header_present
format_payload_sorted_by_confidence
format_payload_budget_enforcement
format_payload_multibyte_utf8
format_payload_session_context
format_payload_active_entries_only
format_payload_entry_id_metadata
format_payload_token_limit_override
test_compact_payload_histogram_block_present
test_compact_payload_histogram_block_absent
build_request_subagentstart_with_prompt_snippet
build_request_subagentstart_empty_prompt_snippet
build_request_userpromptsub_four_words_record_event
build_request_userpromptsub_five_words_context_search
```

---

## Integration Harness Plan

### Feature Touches

crt-027 modifies:
- `context_briefing` MCP tool output format (BriefingService → IndexBriefingService)
- UDS `handle_compact_payload` response format
- Wire protocol (`source` field on `ContextSearch`)
- `hook.rs` routing (new `SubagentStart` arm)

Based on the suite selection table:

| Suite | Reason | Priority |
|-------|--------|----------|
| `smoke` | Mandatory minimum gate — any change at all | MANDATORY |
| `tools` | `context_briefing` MCP tool output format changes | Required |
| `lifecycle` | Store→search → briefing multi-step flows; restart persistence | Required |
| `edge_cases` | Unicode, boundary values relevant to snippet truncation | Required |
| `protocol` | Wire protocol extension (`source` field), tool discovery | Required |

Suites NOT required: `security`, `volume`, `confidence`, `contradiction` — crt-027 does not
modify content scanning, capability enforcement, contradiction detection, confidence math, or
storage schema.

### Existing Suite Coverage Analysis

The `tools` suite currently has tests for `context_briefing` using the old `BriefingService`
format (section-headers format). After migration, these tests will exercise the new flat
indexed table format. Expected failures in the tools suite if the old format assertions
remain must be triaged:

- If the test asserts on `"## Decisions"` or `"## Conventions"` strings: **caused by this
  feature** — update assertions to expect flat table format.
- If the test asserts format is non-empty: survives unchanged.

### New Integration Tests Required

The following MCP-interface behaviors are new in crt-027 and have no coverage in existing
suites. These must be added to `suites/test_tools.py` and `suites/test_lifecycle.py`:

#### New tests for `suites/test_tools.py`

```python
def test_briefing_returns_flat_index_table(populated_server):
    # Call context_briefing; assert output does NOT contain "## Decisions"
    # and DOES contain flat table header with columns: #, id, topic, cat, conf, snippet
    ...

def test_briefing_active_entries_only(server):
    # Store one Active, one Deprecated entry with same topic
    # Call context_briefing; assert Deprecated entry id absent from output
    ...

def test_briefing_default_k_is_twenty(populated_server):
    # Call context_briefing with no k param against populated store (50 entries)
    # Assert up to 20 results returned (not 3, the old default)
    ...

def test_briefing_k_override(populated_server):
    # Call context_briefing with k=5; assert at most 5 entries returned
    ...
```

#### New tests for `suites/test_lifecycle.py`

```python
def test_briefing_session_id_applies_wa2_boost(server):
    # Register session, store entries in specific category, trigger histogram
    # Call context_briefing with session_id; assert histogram-category entries rank higher
    ...

def test_compact_payload_uses_flat_index_format(server):
    # Simulate PreCompact via UDS; assert response contains flat table header
    # Assert no "## Decisions" / section headers in response
    ...
```

### When to Add vs. File Issue

If harness infrastructure (new fixtures, new conftest structure) is needed to support
SubagentStart hook testing through the MCP interface, file a follow-up GH Issue rather than
expanding harness scope in this PR. The hook process is an external binary; the MCP harness
does not test it end-to-end.

---

## AC-to-Component Mapping

| AC-ID | Component Test Plan File |
|-------|--------------------------|
| AC-01 | hook-routing.md |
| AC-02 | hook-routing.md |
| AC-02b | hook-routing.md |
| AC-03 | hook-routing.md |
| AC-04 | hook-routing.md |
| AC-05 | hook-routing.md + listener-dispatch.md + wire-source-field.md |
| AC-SR01 | hook-routing.md (manual gate) |
| AC-SR02 | hook-routing.md |
| AC-SR03 | hook-routing.md |
| AC-06 | context-briefing-handler.md |
| AC-07 | context-briefing-handler.md + index-briefing-service.md |
| AC-08 | context-briefing-handler.md + listener-dispatch.md |
| AC-09 | index-briefing-service.md |
| AC-10 | index-briefing-service.md + listener-dispatch.md |
| AC-11 | context-briefing-handler.md |
| AC-12 | listener-dispatch.md |
| AC-13 | service-layer-wiring.md (grep gate) |
| AC-14 | protocol-update.md (grep gate) |
| AC-15 | All (test count gate, CI) |
| AC-16 | listener-dispatch.md |
| AC-17 | listener-dispatch.md |
| AC-18 | listener-dispatch.md |
| AC-19 | listener-dispatch.md |
| AC-20 | listener-dispatch.md |
| AC-21 | listener-dispatch.md |
| AC-22 | hook-routing.md |
| AC-23 | hook-routing.md |
| AC-23b | hook-routing.md |
| AC-23c | hook-routing.md |
| AC-24 | service-layer-wiring.md (CI gate) |
| AC-25 | wire-source-field.md + hook-routing.md |

---

## Manual Gate Item

**AC-SR01** — SubagentStart stdout injection into subagent context via Claude Code.

This cannot be automated with the current infra-001 harness (which exercises MCP only, not
the hook process binary). Required action before Gate 3c:

1. Run the Unimatrix server.
2. Spawn a subagent with a `prompt_snippet` matching a known active entry (e.g., topic "hook-routing").
3. Inspect the subagent's initial context for the injected Unimatrix entries.
4. Mark AC-SR01 status as CONFIRMED or OPEN in the RISK-COVERAGE-REPORT.md.
