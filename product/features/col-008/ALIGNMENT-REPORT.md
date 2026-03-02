# Alignment Report: col-008 Compaction Resilience

## Assessment Summary

| Check | Status | Details |
|-------|--------|---------|
| Vision Alignment | PASS | Directly implements "compaction resilience" from product vision's col-008 description |
| Milestone Fit | PASS | Correctly scoped to M5 Collective phase (hook-driven delivery) |
| Scope Gaps | PASS | All 12 acceptance criteria covered by architecture and specification |
| Scope Additions | PASS | No scope additions beyond SCOPE.md |
| Architecture Consistency | PASS | Consistent with col-006 UDS patterns, col-007 session state, existing async dispatch |
| Risk Completeness | PASS | 12 risks, 47 scenarios, all scope risks traced |

**Overall: 6 PASS. Zero VARIANCE. Zero FAIL.**

## Vision Alignment Analysis

### Product Vision Match

The product vision (PRODUCT-VISION.md) explicitly lists col-008 as:

> "Compaction Resilience | col-008 | PreCompact hook that calls context_briefing for the active session's role and task context. Injects critical knowledge into the compacted window via stdout. Ensures agents don't lose Unimatrix context on compaction."

The designed feature matches this description precisely:
- Implements the PreCompact hook handler
- Injects knowledge into the compacted window via stdout
- Uses a prioritized payload (more targeted than raw context_briefing) with briefing-style fallback
- Ensures agents retain critical decisions, conventions, and previously-injected knowledge after compaction

The architecture goes beyond the vision's "calls context_briefing" phrasing by using a session-aware injection history replay (ADR-002) as the primary strategy, with a briefing-style fallback. This is strictly better — session-aware compaction defense is exactly what ASS-014 recommended — and does not deviate from the vision's intent.

### Core Value Proposition Alignment

The vision states: "Relevant expertise is injected into every prompt, survives context compaction, and feeds confidence signals back without explicit agent action."

col-008 implements the "survives context compaction" leg. Combined with col-007 (injection) and col-009 (confidence feedback), the three features complete the invisible delivery pipeline described in the vision.

### Three-Leg Architecture Alignment

| Leg | Vision Description | col-008 Role |
|-----|-------------------|--------------|
| Files | Define the process | Not affected |
| Unimatrix | Holds the expertise | Source of compaction payload entries |
| Hooks | Connect them automatically | PreCompact hook delivers preserved knowledge |

col-008 strengthens the Hooks leg without modifying the Files or Unimatrix legs.

## Milestone Fit

col-008 is in the Collective phase (M5), specifically the hook-driven delivery features (col-006 through col-011). It depends on col-006 (transport, COMPLETE) and col-007 (injection pipeline, IN IMPLEMENTATION). It feeds into col-009 (SessionState reuse) and col-010 (session persistence).

The dependency chain is correct:
```
col-006 (transport) -> col-007 (injection) -> col-008 (compaction defense)
                                            -> col-009 (confidence signals)
                                            -> col-010 (session persistence)
```

## Scope Gap Analysis

Every acceptance criterion from SCOPE.md is addressed:

| AC-ID | SCOPE Criterion | Architecture Coverage | Specification Coverage |
|-------|----------------|----------------------|----------------------|
| AC-01 | PreCompact hook sends CompactPayload | Component 1 (hook handler) | FR-01 |
| AC-02 | UDS dispatches CompactPayload, returns BriefingContent | Component 3 (dispatcher) | FR-03 |
| AC-03 | Per-session injection history maintained | Component 2 (SessionRegistry) | FR-02 |
| AC-04 | Payload includes injected entries, decisions prioritized | ADR-003 | FR-03.2-FR-03.5 |
| AC-05 | Token budget enforced with priority allocation | ADR-003 | FR-03.4, constants |
| AC-06 | Fallback when no injection history | ADR-002 | FR-03.6 |
| AC-07 | Graceful degradation, server unavailable | Inherited from col-006 | FR-01.4, NFR-03 |
| AC-08 | SessionState lifecycle (register/close) | Component 2 | FR-02.4, FR-02.9 |
| AC-09 | ContextSearch gains session_id field | Component 5 | FR-04.1, FR-05.2 |
| AC-10 | Structured plain text formatting | Component 3 | FR-06 |
| AC-11 | Existing MCP tests pass | Constraint | NFR-04 |
| AC-12 | Server-side processing under 15ms | ADR-002 (ID-based) | NFR-01 |

No gaps identified.

## Scope Addition Check

The designed feature does not add scope beyond SCOPE.md. Specifically:
- No disk-based compaction cache (explicit non-goal, maintained)
- No confidence feedback (deferred to col-009)
- No injection recording to redb (deferred to col-010)
- No embedding at PreCompact time (per ADR-002)
- No adaptive injection volume (deferred)
- No correction chain tracking (deferred)

## Architecture Consistency

| Pattern | Existing Usage | col-008 Usage | Consistent? |
|---------|---------------|---------------|-------------|
| Arc parameter passing to UDS listener | col-007 expanded signature | Add SessionRegistry Arc | Yes |
| Async dispatch_request() | col-007 async migration | CompactPayload handler is async | Yes |
| Defensive serde parsing (#[serde(default)]) | ADR-006, HookInput, ContextSearch | session_id on ContextSearch | Yes |
| Graceful degradation (exit 0, no stdout) | col-006 FR-03.7, col-007 AC-08 | PreCompact same pattern | Yes |
| In-memory session state (HashMap by session_id) | col-007 CoAccessDedup | SessionRegistry extends pattern | Yes |
| Mutex poison recovery | vnc-004 CategoryAllowlist | SessionRegistry uses same pattern | Yes |
| Token budget as byte constant | col-007 MAX_INJECTION_BYTES | MAX_COMPACTION_BYTES same pattern | Yes |
| Fire-and-forget vs synchronous classification | col-007 is_fire_and_forget | CompactPayload excluded (synchronous) | Yes |

No architectural inconsistencies identified.

## Risk Completeness

The RISK-TEST-STRATEGY.md identifies 12 risks with 47 test scenarios. All 8 scope risks (SR-01 through SR-08) are traced to architecture risks or accepted explicitly. The risk coverage is comprehensive:

- High priority: 4 risks, 17 scenarios (budget overflow, injection tracking, fire-and-forget classification, session lifecycle)
- Medium priority: 6 risks, 22 scenarios (lock contention, stale entries, fallback quality, session_id mismatch, CoAccessDedup regression, entry fetch failures)
- Low priority: 2 risks, 8 scenarios (latency, wire compatibility)

## Variances Requiring Human Approval

None. The feature design aligns with the product vision, follows established patterns, and stays within scope.
