# Agent Report: crt-036-agent-2-spec

## Deliverable

`product/features/crt-036/specification/SPECIFICATION.md`

## Summary

Specification complete. 17 acceptance criteria (AC-01a, AC-01b, AC-02 through AC-17),
12 functional requirements, 7 non-functional requirements, full domain model, and
explicit NOT-in-scope section.

## Key Decisions Made

1. **AC-01 split into AC-01a and AC-01b.** The two deletion sites in `status.rs` and
   `tools.rs` are independently named, independently verifiable line items. A reviewer
   cannot check one and assume the other. SR-04 mitigation fully captured.

2. **max_cycles_per_tick added to RetentionConfig.** SR-01 + SR-02 risk recommendations
   called for a configurable batch cap. Added as `max_cycles_per_tick` (default 10,
   range [1, 1000]) with its own AC-16 and validate() requirement. Added to the
   `[retention]` config block.

3. **Unattributed cleanup guards active sessions.** SR-06 risk: scope said prune
   unattributed sessions unconditionally, but this could disrupt in-flight retrospectives.
   FR-05 specifies the guard: sessions with `feature_cycle IS NULL` AND `status = 'Active'`
   are excluded. AC-06 verifies both the pruning and the active-session guard.

4. **raw_signals_available targeted UPDATE, not INSERT OR REPLACE.** SR-05 risk:
   `store_cycle_review()` INSERT OR REPLACE would clobber `summary_json`. FR-06 specifies
   a bare `UPDATE cycle_review_index SET raw_signals_available = 0 WHERE feature_cycle = ?`.
   Captured as Constraint 6.

5. **PhaseFreqTable mismatch warning specified as FR-10 / AC-17.** SR-07 requested this
   as a tick-time diagnostic. Specified as `tracing::warn!` comparing `query_log_lookback_days`
   coverage against the oldest retained cycle's `computed_at`.

6. **cascade delete order constraint formalised.** injection_log before sessions is a
   hard ordering requirement. AC-08 includes an inversion test that confirms the test
   catches the constraint.

7. **Open questions elevated to architect, not left as TBDs.** Two SR items (SR-05 path
   confirmation, SR-07 query reuse) are noted as design notes in the Open Questions
   section with explicit "architect must confirm / must not deviate" language.

## Functional Requirements Count

12 functional requirements (FR-01 through FR-12).

## Key Constraints Captured

- No schema migration (migration remains at version 19 / crt-035).
- observations and query_log have no feature_cycle column — two-hop join mandatory.
- injection_log must be deleted before sessions (FK cascade order).
- write_pool_server() max_connections=1 — connection must release between cycles.
- pool.begin() / tx.commit() API required (not raw BEGIN SQL).
- raw_signals_available update must use targeted UPDATE, not INSERT OR REPLACE.
- Both 60-day DELETE sites removed unconditionally (not flagged out).
- crt-033 gate unconditional.
- RetentionConfig loaded once at startup, passed by value.
- Unattributed prune guards active sessions.

## Gaps / Ambiguities Found

None blocking specification. Two items escalated as architect design notes:

1. SR-05: Whether targeted UPDATE path is available or must be added to the store API.
2. SR-07: Whether oldest-cycle boundary for FR-10 reuses the purgeable-cycle resolution
   query result or requires a separate query.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 16 results; entries #3914
  (two-hop join GC pattern), #3911 (run_maintenance procedure), #3793 (crt-033 ADR-001
  write_pool_server constraint), #3686 (col-031 ADR-002 PhaseFreqTable), and #3822
  (background tick idempotency) were directly applied.
