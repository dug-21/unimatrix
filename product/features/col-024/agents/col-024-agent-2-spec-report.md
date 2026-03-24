# Agent Report: col-024-agent-2-spec

## Output

- `product/features/col-024/specification/SPECIFICATION.md`

## Key Decisions

1. **FR-04 / AC-13**: Mandated a named unit-conversion helper (not a raw `* 1000` literal)
   for cycle_events seconds → ts_millis boundary conversion, directly addressing SR-01.

2. **NFR-01 single block_sync envelope**: Specified that all three steps of
   `load_cycle_observations` execute inside one `block_sync(async { ... })` call,
   addressing SR-02 (multiple nested block entries would panic or prolong blocking).

3. **FR-06 empty-on-no-rows semantics**: Distinguished between "no cycle_events rows"
   (return empty, trigger legacy fallback) and "rows exist but no matching observations"
   (also returns empty, same fallback). Flagged this as Assumption 3 from the risk
   assessment — the two cases are indistinguishable from the caller's perspective. This is
   accepted behavior; noted in the NOT in scope section.

4. **AC-14 + FR-08 observability**: Added a mandatory `tracing::debug!` requirement for
   the fallback activation, directly addressing SR-06.

5. **SR-03 open-ended windows**: Defined abandoned cycles explicitly as "best-effort" with
   no maximum window cap. The behavior (over-inclusion) is accepted and documented in
   Constraints §7.

6. **SR-04 signal mismatch**: Defined as accepted behavior — explicit signal is always
   authoritative over registry feature, no anomaly logging required (FR-14 + Constraints §8).

7. **OQ-01 enrichment helper**: Raised as open question for architect (SR-05). Not resolved
   in spec — implementation shape is architect's decision.

## Acceptance Criteria Count

14 ACs (AC-01 through AC-14). All 12 ACs from SCOPE.md are present plus two added from
risk items: AC-13 (named conversion helper, SR-01) and AC-14 (debug log on fallback,
SR-06).

## Knowledge Stewardship

- Queried: /uni-query-patterns for observation lookup, topic_signal enrichment, cycle_events schema -- found #3366 (cycle_events-first lookup pattern), #3367 (topic_signal write-time enrichment), #2999 (ADR-002 crt-025 seq advisory), #383 (ADR-002 col-012 ObservationSource independence). No conflicts with established conventions.
