# Agent Report: crt-020-agent-3-risk

## Output

- Produced: `product/features/crt-020/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 2 | R-01, R-02 |
| High | 4 | R-03, R-04, R-05, R-06 |
| Medium | 10 | R-07–R-16 |
| Low | 2 | R-15, R-16 |

Plus: 4 integration risks (I-01–I-04), 7 edge cases (E-01–E-07), 4 security risks (S-01–S-04), 5 failure modes (F-01–F-05).

## Top Risks

**R-01 (Critical)**: `increment_pending_and_drain_ready` partial-write under crash leaves `implicit_unhelpful_pending` inconsistent. Atomic transaction semantics must be explicitly tested under error injection.

**R-02 (Critical)**: Vote inflation from injection_log multi-row per (session_id, entry_id). The HashSet dedup in `apply_implicit_votes` must be tested with 3–5 rows per entry per session. Historical evidence: Unimatrix #1044 (COUNT DISTINCT bug in crt-018 caught by risk strategy at implementation time — same class of defect).

**R-03 (High)**: Stop hook close path does not set `implicit_votes_applied = 1` on all code paths (normal Stop, sweep, abandoned). Any missed path silently allows background tick double-processing for those sessions.

**R-05 (High)**: If mark-applied is called before votes are written, a crash between the two leaves sessions permanently silenced with no retry possible. The safe ordering is votes-first, mark-second — with the accepted residual that mark failure causes a double-write on next tick (F-03).

## Coverage Gaps

- ARCHITECTURE.md Open Question 1 (`apply_implicit_votes` location: `background.rs` vs `implicit_votes.rs`) is unresolved. The tester must verify no circular crate dependency once resolved.
- ARCHITECTURE.md Open Question 3 (TimedOut sessions with non-NULL outcome): ADR-002 SQL resolves this as `status = 1` only, but AC-02 in SCOPE.md says `status IN (1, 2)`. Risk R-13 flags the discrepancy — the test must assert the SQL filter used at implementation time matches ADR-002.
- No specification file exists yet. AC-01 through AC-11 from SCOPE.md were used as the basis for the risk register. When SPECIFICATION.md is produced, re-verify risks against the formal acceptance criteria.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned gate failures — found #1044 (COUNT DISTINCT bug, crt-018), directly elevates R-02 to Critical.
- Queried: `/uni-knowledge-search` for risk patterns — found #1542 (background tick error semantics), informed F-01/F-02.
- Queried: `/uni-knowledge-search` for atomic read-modify-write — found #57 (ADR-006 Vote Correction Atomicity), informed R-01 scenario design.
- Stored: entry #1616 "Background Tick Dedup Flags: Write Votes Before Marking Applied — Residual Double-Write Risk on Mark Failure" via `/uni-store-pattern`. Novel pattern not previously captured; visible across any future feature using session-level dedup flags in background ticks.
