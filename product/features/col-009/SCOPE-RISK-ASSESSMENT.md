# Scope Risk Assessment: col-009

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Bincode v2 positional encoding: SIGNAL_QUEUE SignalRecord fields must be append-only forever. An early field-order mistake cannot be corrected without a second migration. | High | Low | Architect must finalize SignalRecord field order before any data is written. Treat as immutable once shipped. |
| SR-02 | `drain_signals()` requires read-then-delete in one redb write transaction. redb v3 does not support sub-transactions; a crash between read and delete would lose signals with no recovery path. | Med | Low | Accept as a soft-durability tradeoff (documented in SCOPE Non-Goals). Ensure consumers handle empty queue gracefully. |
| SR-03 | Wilson score 5-vote minimum guard means implicit signals from the first 4 sessions have zero effect on confidence. Early deployments see no feedback loop improvement. | Low | High | Inform product stakeholders: feedback loop visibly activates only after 5 sessions with injections. Document in release notes. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Session Intent Registry (`agent_actions` on `SessionState`) generalizes to intercept arbitrary agent MCP actions — scope could expand to influence compaction weighting, re-injection filtering, or other future behaviors beyond ExplicitUnhelpful exclusion. | Med | Med | Architect must define a hard interface boundary: `SessionAction` enum is used solely for signal-generation exclusion in col-009. Future consumers are added in col-010+, not col-009. |
| SR-05 | `PendingEntriesAnalysis` in-memory on McpServer accumulates across sessions with no GC until `context_retrospective` is called. Long-running servers with no retrospective calls may accumulate unbounded memory. | Med | Med | Architect must define a cap on `pending_entries_analysis` length (recommend 1000 entries max) and a time-based eviction (oldest entries dropped after 7 days). |
| SR-06 | Rework detection (PostToolUse edit-fail-edit pattern) requires the server to observe tool results, but PostToolUse hook fires AFTER the tool completes — the hook has the result in its stdin JSON. Ensure `extra` JSON parsing is sufficient to extract `exit_code` and file path without additional hook event subscriptions. | Med | Med | Spec must define exactly which fields are extracted from PostToolUse `extra` JSON and how missing fields are handled (default: assume no failure). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | col-009 extends `SessionState` (from col-008) with new fields (`signaled_entries`, `rework_events`, `agent_actions`). Any concurrent access pattern introduced by col-009's signal generation running alongside col-008's `clear_session()` must be mutex-safe. | High | Low | Architect must verify `SessionRegistry::generate_signals()` is called within the same lock scope as `clear_session()`, or uses a compare-and-swap pattern, to prevent lost signals on race. |
| SR-08 | The confidence consumer calls the existing `helpful_count` increment pathway (crt-001/crt-002). If that pathway is already under load (many concurrent MCP explicit votes), synchronous signal processing during SessionClose could contend on the same redb write transaction. | Low | Low | Use a single redb write transaction per batch drain, not per entry. Batching is already the recommended pattern in crt-001. |
| SR-09 | `RetrospectiveReport.entries_analysis` is an additive field (`#[serde(default)]`). However, if col-010's `from_structured_events()` later produces its own `entries_analysis`, two sources could produce conflicting data for the same report. | Med | Low | Spec must document that col-009's `entries_analysis` is the sole producer in schema v4. col-010 alignment is explicitly deferred to col-010 (Resolved Design Decision #4). |

## Assumptions

- **Assumption A** (Goals #2, #3): `SessionState.injection_history` is populated by col-007/col-008 before SessionClose fires. If a session closes with no injections (user opens Claude, immediately stops), the signal list is empty and no signals are written — this is correct behavior, not a bug.
- **Assumption B** (Goal #6): Stale sessions are reliably identified by last injection timestamp. If a session has no injections and no rework events (no PostToolUse calls), `last_activity_time` may be the session's registration time — which could be hours ago even for an active session. Spec must define `last_activity_time` as `max(last_injection_timestamp, last_rework_event_timestamp, registration_time)`.
- **Assumption C** (Resolved Decision #2): PostToolUse fires for every tool use including Read, Glob, Grep, etc. The rework detection must filter out tool calls that cannot represent rework (read-only tools). Spec must enumerate which tools are rework candidates (Bash, Edit, Write, MultiEdit).

## Design Recommendations

- **For the architect** (SR-01): Define and lock `SignalRecord` field order in the first ADR. Include a field-freezing comment in the source.
- **For the architect** (SR-07): The `generate_signals` + `clear_session` operation must be atomic from the perspective of the `SessionRegistry` Mutex. Design as a single locked operation `drain_and_clear_session(session_id)` to eliminate the race window.
- **For the architect** (SR-04): Define `SessionAction` as a closed enum with explicit variants only. No `Other(String)` escape hatch in col-009.
- **For the spec writer** (SR-05): Specify `PendingEntriesAnalysis` capacity limit and eviction policy as acceptance criteria.
- **For the spec writer** (SR-06, Assumption C): Define the exact PostToolUse JSON fields parsed for rework detection and enumerate rework-eligible tool names.
