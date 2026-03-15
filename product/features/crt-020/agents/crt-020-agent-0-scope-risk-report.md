# crt-020 Scope Risk Agent Report

## Output

SCOPE-RISK-ASSESSMENT.md written to:
`product/features/crt-020/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 1 (SR-04) |
| Med | 6 (SR-01, SR-02, SR-03, SR-05, SR-07, SR-08, SR-09) |
| Low | 1 (SR-06) |

Total: 9 risks across 3 categories (Technology, Scope Boundary, Integration).

## Top 3 Risks for Architect/Spec Writer Attention

**SR-04 (High / Med) — Double-counting between real-time Stop hook path and background tick.**
The `implicit_votes_applied` flag prevents the background tick from reprocessing a session, but it does not prevent the real-time `run_confidence_consumer` path from also voting on the same injected entries before the flag is set. The two paths must be proven disjoint at the vote-source level, not just at the session level. Architect must trace both paths end-to-end and produce an ADR confirming disjointness or prescribing a finer-grained dedup.

**SR-02 (Med / High) — Pair accumulation counter location is unresolved and affects atomicity + GC.**
The SCOPE.md leaves counter storage as an open architect decision. The choice determines whether: (a) the counter survives server restarts, (b) the counter is GC'd when an entry is deleted, and (c) whether a second schema migration beyond v13 is needed. Wrong choice risks orphaned counters or a counter that silently resets on restart, causing the unhelpful vote to never fire.

**SR-01 (Med / High) — Cold-start backlog on first upgrade.**
The first tick after upgrade processes all historical sessions in the 30-day GC window. At 500 sessions/tick × 15-minute intervals, draining a large backlog takes hours. During that window, newly-closed sessions queue behind historical ones (oldest-first ordering). Architect must decide batch ordering and document expected drain time. Tick timeout is 120s — worst-case duration at batch cap must be verified.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — found #1044 (COUNT DISTINCT bug caught by risk strategy in crt-018), #167 (gate result handling)
- Queried: /uni-knowledge-search for "outcome rework confidence signal double counting" — found #99 (ADR-002 server-layer deduplication), #57 (ADR-006 vote correction atomicity)
- Queried: /uni-knowledge-search for "risk pattern recurring" (category: pattern) — found #1542 (Background Tick Writers: Define Error Semantics for Consecutive Counters Before Implementation)
- Queried: /uni-knowledge-search for "SQLite join injection log session outcome" — found #1043 (Subquery Dedup Before JOIN Aggregation), #1044 (COUNT DISTINCT lesson)
- Stored: entry #1611 "Implicit Vote Features: Real-Time + Background Path Disjointness Must Be Explicit" via /uni-store-pattern — novel pattern not previously captured, visible across any feature combining real-time + background vote paths
