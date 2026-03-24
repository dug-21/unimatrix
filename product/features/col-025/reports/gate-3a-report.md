# Gate 3a Report: col-025

> Gate: 3a (Component Design Review — rework re-check, iteration 1)
> Date: 2026-03-24
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All seven components match ARCHITECTURE.md decomposition; ADR-001 through ADR-006 all reflected |
| Specification coverage | PASS | All 18 FRs, all 18 ACs, and all NFRs have corresponding pseudocode and test coverage |
| Risk coverage | PASS | All 14 risks traced to at least one named test scenario; all 9 gate-3c non-negotiables present |
| Interface consistency | WARN | Minor: OVERVIEW.md Component 7 table row still shows `src/uds/hook.rs` but OQ-04 section + subagent-start-injection pseudocode clarify implementation lives in `dispatch_request` in `src/uds/listener.rs` |
| Pseudocode/test-plan agreement — CONTEXT_GET_INSTRUCTION constant location | PASS | **REWORK RESOLVED**: OVERVIEW.md and format-index-table-header pseudocode now both specify `src/services/index_briefing.rs` with import into `briefing.rs`, matching ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md, and ADR-006 |
| Pseudocode/test-plan agreement — UDS empty-string behavior | PASS | **REWORK RESOLVED**: cycle-event-handler pseudocode Step 3b no longer has `.filter(|s| !s.is_empty())` on the UDS path; comment explicitly states "Empty string stored verbatim"; matches spec FR-11, ADR-005, and test plan `test_uds_goal_empty_string_stored_verbatim` |
| Pseudocode/test-plan agreement — make_session_state signature | PASS | **REWORK RESOLVED**: briefing-query-derivation pseudocode now specifies 3-parameter `make_session_state(feature, signals, current_goal)` with NO `_with_goal` variant; matches session-state-extension test plan exactly |
| Knowledge stewardship compliance | PASS | All agent reports contain stewardship block with queried/stored entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

All seven components map directly to ARCHITECTURE.md:

- Component 1 (schema-migration-v16): Pseudocode matches — `ALTER TABLE cycle_events ADD COLUMN goal TEXT` with `pragma_table_info` idempotency guard (pattern #1264), `CURRENT_SCHEMA_VERSION` bump to 16, `insert_cycle_event` 8-parameter signature, `get_cycle_start_goal` with `LIMIT 1` and correct SQL. `sqlx::query_scalar::<_, Option<String>>` with `.flatten()` correctly handles `Ok(Some(None))` (row present, goal NULL) vs `Ok(None)` (row absent) — both return `Ok(None)` to caller.

- Component 2 (session-state-extension): `current_goal: Option<String>` added to `SessionState`, initialized to `None` in `register_session`, `set_current_goal` with Mutex poison recovery via `unwrap_or_else(|e| e.into_inner())` — matching ARCHITECTURE.md exactly.

- Component 3 (cycle-event-handler): Goal extraction placed in the synchronous section before fire-and-forget spawn, consistent with `set_current_phase` placement invariant. UDS byte guard (truncate + warn) present. `truncate_at_utf8_boundary` helper included. No empty-string filter on UDS path (FR-11 scope is MCP-only).

- Component 4 (mcp-cycle-wire-protocol): `goal: Option<String>` on `CycleParams`. Validation at handler layer: trim → empty-normalize → byte check with `CallToolResult::error` on rejection (ADR-005). Goal propagates to `ImplantEvent` payload via Claude Code's hook machinery per architecture note.

- Component 5 (session-resume): Goal lookup in `SessionRegister` arm after `register_session`, before `HookResponse::Ack`. `unwrap_or_else` degradation with `tracing::warn!` (ADR-004). Feature-cycle emptiness guard prevents spurious DB call when no feature is set.

- Component 6 (briefing-query-derivation): `synthesize_from_session` body replaced with `state.current_goal.clone()` — pure sync O(1) per NFR-04. Three-step waterfall preserved with empty-goal guard at step 2. `extract_top_topic_signals` removal guidance present.

- Component 7 (subagent-start-injection): Goal check is at the TOP of the `HookRequest::ContextSearch` arm in `dispatch_request` (listener.rs), gated on `source.as_deref() == Some("SubagentStart")`. Routes to `IndexBriefingService` when goal is present and non-empty; falls through to existing ContextSearch path when goal is absent (ADR-003). `BriefingContent` response format used, consistent with existing CompactPayload path.

- Component 8 (format-index-table-header): `CONTEXT_GET_INSTRUCTION` constant defined in `src/services/index_briefing.rs`, imported into `src/mcp/response/briefing.rs` for use in `format_index_table`. Header prepended once on non-empty output; empty input still returns `""`. `strip_briefing_header` test helper specified (ADR-006, R-11).

ADR-001 through ADR-006 are all reflected in the appropriate pseudocode components. `MAX_GOAL_BYTES = 1024` is consistent across all documents (ADR-005 WARN-1 was resolved in prior design phase). All technology choices consistent: rusqlite/sqlx positional binds, tokio async, `tracing::warn!` severity.

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

All 18 functional requirements are covered:

| FR | Pseudocode Component | Coverage |
|----|---------------------|----------|
| FR-01 (goal param on start only) | mcp-cycle-wire-protocol, cycle-event-handler | Goal silently ignored on PhaseEnd/Stop; `None` passed to insert |
| FR-02 (persistence in same write) | cycle-event-handler | Fire-and-forget spawn with goal captured before the spawn |
| FR-03 (MAX_GOAL_BYTES, two behaviors) | mcp-cycle-wire-protocol (hard reject), cycle-event-handler (truncate) | Correct per ADR-005 |
| FR-04 (SessionState.current_goal, two paths) | session-state-extension, cycle-event-handler, session-resume | Both start and resume paths covered |
| FR-05 (resume null-safety) | session-resume | `unwrap_or_else` + warn; `HookResponse::Ack` always returned |
| FR-06 (synthesize_from_session returns current_goal) | briefing-query-derivation | Pure body replacement, O(1) clone |
| FR-07 (three-step priority unchanged structure) | briefing-query-derivation | Step 1/2/3 waterfall preserved, only step 2 body changed |
| FR-08 (CompactPayload uses goal) | briefing-query-derivation | Shared function; no additional wiring needed per ADR-002 |
| FR-09 (SubagentStart goal primary) | subagent-start-injection | Explicit branch, goal wins over prompt_snippet |
| FR-10 (schema migration idempotency) | schema-migration-v16 | pragma_table_info guard present |
| FR-11 (empty/whitespace → None at MCP) | mcp-cycle-wire-protocol | trim + empty check before byte guard; UDS path explicitly does NOT normalize |
| FR-12 (CONTEXT_GET_INSTRUCTION header) | format-index-table-header | Constant defined in `src/services/index_briefing.rs`, prepended once on non-empty output |

NFR-01 through NFR-06 are addressed. All 18 acceptance criteria are mapped in test-plan/OVERVIEW.md AC coverage table and per-component test plans.

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**:

All 14 risks from RISK-TEST-STRATEGY.md have at least one named test scenario. All 9 non-negotiable gate-3c scenarios are present:

1. `test_v15_to_v16_migration_idempotent` — schema-migration-v16 test plan
2. `test_subagent_start_goal_present_routes_to_index_briefing` — subagent-start-injection test plan
3. `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` — subagent-start-injection test plan
4. `test_subagent_start_goal_absent_uses_existing_transcript_path` — subagent-start-injection test plan
5. `test_uds_goal_truncation_at_utf8_char_boundary` — cycle-event-handler test plan
6. `test_insert_cycle_event_full_column_assertion` — schema-migration-v16 test plan
7. `test_resume_db_error_degrades_to_none_with_warn` — session-resume test plan
8. `test_format_index_table_starts_with_instruction_header_exactly_once` — format-index-table-header test plan
9. `test_uds_truncate_then_overwrite_last_writer_wins` — cycle-event-handler test plan

R-04 has all five precedence branches covered (T-SAI-01 through T-SAI-05). R-03 covers all four DB return variants: `Ok(Some)`, `Ok(None)` from missing row, `Ok(None)` from NULL goal column, and `Err(...)`. R-13 (truncate-then-overwrite) is covered by gate-3c scenario 9 as an integrated test.

Integration risks (MCP → ImplantEvent → UDS round-trip, shared `derive_briefing_query` path, SubagentStart wiring) are mapped to infra-001 tests in test-plan/OVERVIEW.md.

### Check 4: Interface Consistency

**Status**: WARN

**Evidence**:

Shared types in OVERVIEW.md are consistent with per-component pseudocode:

- `SessionState.current_goal: Option<String>` — consistent across all five server components.
- `insert_cycle_event` 8-parameter signature — consistent between schema-migration-v16 and cycle-event-handler.
- `get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>>` — consistent between OVERVIEW.md and schema-migration-v16.
- `set_current_goal(&self, session_id: &str, goal: Option<String>)` — consistent between OVERVIEW.md and session-state-extension.
- `MAX_GOAL_BYTES = 1024` — all documents aligned.
- `CONTEXT_GET_INSTRUCTION` defined in `src/services/index_briefing.rs`, imported in `briefing.rs` — consistent between OVERVIEW.md and format-index-table-header pseudocode.
- `make_session_state(feature, signals, current_goal: Option<&str>)` — 3-parameter form now consistent between briefing-query-derivation pseudocode and session-state-extension test plan.

**Residual WARN (carried from prior gate, not resolved by rework)**: OVERVIEW.md Component table (line 22) still lists `src/uds/hook.rs` for the subagent-start-injection component. The OQ-04 resolution section within the same OVERVIEW.md (lines 213–230) correctly explains that the implementation must live in `dispatch_request` in `src/uds/listener.rs`. The subagent-start-injection pseudocode and its test plan also correctly specify `src/uds/listener.rs`. The IMPLEMENTATION-BRIEF.md wave plan says `listener.rs`. The WARN is a stale table row only — the authoritative narrative is unambiguous and an implementor will not be misled. Does not block implementation.

### Check 5: Pseudocode/Test-Plan Agreement — CONTEXT_GET_INSTRUCTION Constant Location

**Status**: PASS (rework resolved)

**Evidence**:

The prior FAIL was: OVERVIEW.md and format-index-table-header pseudocode placed the constant in `src/mcp/response/briefing.rs`, contradicting ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md, and ADR-006 which specify `src/services/index_briefing.rs`.

After rework:

- **OVERVIEW.md** (lines 38-42): "Component 8 (format-index-table-header) owns `format_index_table` in `src/mcp/response/briefing.rs` and `CONTEXT_GET_INSTRUCTION` in `src/services/index_briefing.rs` (per ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md, and ADR-006; the constant is defined alongside `MAX_GOAL_BYTES` and re-exported or imported into `briefing.rs` for use in `format_index_table`)."

- **OVERVIEW.md Constants section** (lines 128-131): Both constants explicitly annotated as `// src/services/index_briefing.rs`.

- **format-index-table-header pseudocode** (file header + lines 18-21): Header now lists both affected files. The constant block says `Add to src/services/index_briefing.rs` with explicit import: `use crate::services::index_briefing::CONTEXT_GET_INSTRUCTION;`.

- **format-index-table-header test plan** (file header): Also lists both `src/services/index_briefing.rs` (constant) and `src/mcp/response/briefing.rs` (function).

All documents are now consistent: constant in `src/services/index_briefing.rs`, function in `src/mcp/response/briefing.rs`, constant imported for use. This matches ARCHITECTURE.md Integration Surface table, IMPLEMENTATION-BRIEF.md Constants section, and ADR-006 §Decision.

### Check 6: Pseudocode/Test-Plan Agreement — UDS Empty-String Behavior

**Status**: PASS (rework resolved)

**Evidence**:

The prior FAIL was: cycle-event-handler pseudocode had `.filter(|s| !s.is_empty())` on the UDS path, conflicting with spec FR-11 (MCP-only normalization) and test plan `test_uds_goal_empty_string_stored_verbatim`.

After rework, the cycle-event-handler pseudocode Step 3b (lines 38-47):

```
let raw_goal: Option<String> = event
    .payload
    .get("goal")
    .and_then(|v| v.as_str())
    // UDS path: no whitespace or empty-string normalization.
    // Whatever arrives is used (after truncation). Empty string stored verbatim.
    // (MCP path normalizes empty strings to None at the handler; UDS does not.)
    .map(|s| s.to_string());
```

The `.filter(|s| !s.is_empty())` is absent. The comment is explicit. The Error Handling table (line 191) states: "Goal empty string in payload | Stored verbatim as `Some("")` on UDS path — no empty-string normalization (ADR-005, FR-11 scope is MCP-path only)".

Test plan edge case row: `test_uds_goal_empty_string_stored_verbatim` — "UDS does not normalize whitespace; empty string stored as empty".

Pseudocode and test plan now agree. Both are consistent with spec FR-11, ADR-005 §Consequences, and the NOT-in-scope list item "Whitespace normalization on the UDS path."

### Check 7: Pseudocode/Test-Plan Agreement — make_session_state Helper Signature

**Status**: PASS (rework resolved)

**Evidence**:

The prior FAIL was: briefing-query-derivation pseudocode proposed a `make_session_state_with_goal` variant while the session-state-extension test plan specified extending the existing signature to 3 parameters.

After rework, briefing-query-derivation pseudocode (lines 13-16 and 148-175):

- States explicitly: "Extend the `make_session_state` test helper to a 3-parameter signature `(feature, signals, current_goal)`."
- Updated helper definition specifies `current_goal: Option<&str>` as the third parameter.
- States explicitly: "There is NO separate `make_session_state_with_goal` variant — tests that need a goal simply pass `Some("...")` as the third argument to `make_session_state`."
- All existing call sites updated to `make_session_state(feature, signals, None)`.

session-state-extension test plan (lines 119-131): specifies `make_session_state(feature, signals, current_goal: Option<&str>)` with all existing call sites updated to pass `None`.

Both documents now agree on the 3-parameter approach. No `_with_goal` variant exists in either document.

### Check 8: Knowledge Stewardship Compliance

**Status**: PASS

SPECIFICATION.md has a Knowledge Stewardship section with Queried entries covering schema migration, SessionState/derive_briefing_query, SubagentStart/UDS injection, and MAX_GOAL_BYTES/CONTEXT_GET_INSTRUCTION. RISK-TEST-STRATEGY.md has both Queried entries (four distinct queries) and a Stored entry with reason ("nothing novel to store — R-02/R-06 already patterns #2933/#3180; R-11 is col-025-specific"). IMPLEMENTATION-BRIEF.md is the coordinator artifact and does not require a stewardship block.

---

## Rework Required

None. All three previously failing checks are resolved. Gate 3a passes.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "gate 3a rework validation pseudocode test plan contradiction" — checking whether patterns from the initial gate review are stored.
- Stored: nothing novel to store — the three failure patterns found in the initial gate (constant location divergence, spec behavior vs pseudocode, test-helper signature disagreement) were col-025-specific instances identified in the first gate run. The rework successfully resolved all three. If these recur across multiple features they could become a cross-feature validation lesson; there is no evidence of recurrence requiring a new pattern entry at this time.
