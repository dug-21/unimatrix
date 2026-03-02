# Acceptance Map: col-008 Compaction Resilience

## AC-to-Component Traceability

| AC-ID | Criterion | Component(s) | FR(s) | Risk(s) | Test Scenario(s) |
|-------|-----------|-------------|-------|---------|-------------------|
| AC-01 | PreCompact hook sends CompactPayload | hook-handler | FR-01 | R-10 | R-10.1, R-10.2, R-10.3 |
| AC-02 | UDS dispatches CompactPayload, returns BriefingContent | compact-dispatch | FR-03 | R-02, R-11 | R-02.1-R-02.3, R-11.1-R-11.3 |
| AC-03 | Per-session injection history maintained | session-registry, injection-tracking | FR-02, FR-04 | R-05, R-12 | R-05.1-R-05.5, R-12.1-R-12.3 |
| AC-04 | Payload includes injected entries, decisions prioritized | compact-dispatch | FR-03.2-FR-03.5 | R-03 | R-03.1-R-03.6 |
| AC-05 | Token budget enforced with priority allocation | compact-dispatch | FR-03.4, FR-06 | R-03 | R-03.1-R-03.6 |
| AC-06 | Fallback from category lookups when no history | compact-dispatch | FR-03.6 | R-04 | R-04.1-R-04.5 |
| AC-07 | Graceful degradation, server unavailable | hook-handler | FR-01.4 | — | Inherited from col-007 pattern |
| AC-08 | SessionState lifecycle (register/close) | session-registry, injection-tracking | FR-02.4, FR-02.9, FR-04.4, FR-04.5 | R-06 | R-06.1-R-06.3 |
| AC-09 | ContextSearch includes session_id | wire-protocol | FR-04.1, FR-05.2 | R-09 | R-09.1-R-09.4 |
| AC-10 | Formatted as structured plain text | compact-dispatch | FR-06 | R-03 | R-03.2 (UTF-8) |
| AC-11 | Existing MCP tests pass | all | NFR-04 | R-07, R-09 | R-07.1-R-07.4, R-09.1-R-09.4 |
| AC-12 | Server-side processing under 15ms | compact-dispatch | NFR-01 | R-08 | R-08.1-R-08.3 |

## Component-to-AC Traceability

| Component | AC(s) Covered | Primary Responsibility |
|-----------|-------------|----------------------|
| session-registry | AC-03, AC-08, AC-11 | Per-session state management, injection history, CoAccessDedup absorption |
| compact-dispatch | AC-02, AC-04, AC-05, AC-06, AC-10, AC-12 | CompactPayload handling, budget allocation, formatting, fallback |
| hook-handler | AC-01, AC-07 | PreCompact arm, fire-and-forget classification, stdout output |
| wire-protocol | AC-09, AC-11 | CompactPayload/BriefingContent activation, ContextSearch session_id |
| injection-tracking | AC-03, AC-08 | ContextSearch -> record_injection, SessionRegister/Close lifecycle |

## Risk-to-AC Traceability

| Risk ID | Covered AC(s) | Risk Description |
|---------|--------------|-----------------|
| R-01 | AC-03 | SessionRegistry lock contention |
| R-02 | AC-02 | Stale entries in compaction payload |
| R-03 | AC-04, AC-05, AC-10 | Token budget overflow / invalid UTF-8 |
| R-04 | AC-06 | Fallback returns empty payload |
| R-05 | AC-03 | Injection tracking fails silently |
| R-06 | AC-08 | Session_id mismatch across events |
| R-07 | AC-11 | CoAccessDedup behavior regression |
| R-08 | AC-12 | CompactPayload latency exceeds budget |
| R-09 | AC-09, AC-11 | Wire protocol backward incompatibility |
| R-10 | AC-01 | PreCompact classified as fire-and-forget |
| R-11 | AC-02 | Entry fetch failures during payload construction |
| R-12 | AC-03 | SessionRegister not called before ContextSearch |

## Verification Coverage

| AC-ID | Unit Tests | Integration Tests | Benchmark Tests |
|-------|-----------|-------------------|-----------------|
| AC-01 | build_request returns CompactPayload | Hook process end-to-end | — |
| AC-02 | — | CompactPayload dispatch returns BriefingContent | — |
| AC-03 | SessionRegistry methods | ContextSearch -> get_state -> CompactPayload | — |
| AC-04 | format_compaction_payload with known entries | — | — |
| AC-05 | Payload byte count <= MAX_COMPACTION_BYTES | — | — |
| AC-06 | — | CompactPayload without injection history | — |
| AC-07 | Hook with no socket exits 0 | — | — |
| AC-08 | register_session / clear_session | — | — |
| AC-09 | ContextSearch round-trip with/without session_id | — | — |
| AC-10 | Payload format verification | — | — |
| AC-11 | — | Full test suite pass | — |
| AC-12 | — | — | 10 iterations p95 < 15ms |
