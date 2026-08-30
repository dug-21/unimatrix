# Scope Risk Assessment: crt-020

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Cold-start backlog: first tick after upgrade processes all historical sessions (up to 30-day window, potentially thousands). Batch cap of 500 may require many ticks to drain, during which newly-closed sessions queue behind the backlog. | Med | High | Architect must decide batch ordering (oldest-first vs newest-first) and document expected drain time. Cap must fit within `TICK_TIMEOUT = 120s`. |
| SR-02 | Pair accumulation counter storage is an open design question. Counter location (entries column, new table, or COUNTERS KV) affects atomicity guarantees, GC behavior, and whether a second schema migration is needed beyond v13. | Med | High | Architect decides with explicit atomicity and GC-lifecycle rationale. Wrong choice risks counter orphans or double-counting after entry deletion. |
| SR-03 | Confidence recomputation inline per vote may cause `TICK_TIMEOUT` breach if a tick processes the batch cap (500 sessions × N entries each). The existing confidence refresh already runs in the same tick after this step. | Med | Med | Architect must quantify worst-case duration at 500 sessions × average entries. Consider deferring confidence recomputation to the subsequent refresh step rather than inlining per-vote. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Double-counting between real-time Stop hook path and background tick. SCOPE.md (Resolved Decision #5) states the Stop hook sets `implicit_votes_applied = 1` to prevent this, but the real-time path (`run_confidence_consumer`) currently applies votes from `signal_queue` for different signal types — an entry could receive one explicit helpful vote via the real-time path and one implicit helpful vote via the background path for the same session. | High | Med | Spec writer must define precisely which vote types are deduplicated by the flag. The flag prevents background re-processing but does not prevent the real-time path from also voting on the same injected entries. Architect must verify the two paths are truly disjoint in what they vote on. |
| SR-05 | Signal quality dilution: implicit success votes treat all entries injected in a session as equally causal for the success outcome. A session injecting 20 entries credits all 20, even if 19 were irrelevant. At scale, low-signal entries accumulate votes as fast as high-signal ones. | Med | High | Spec writer should consider a session injection-count cap (e.g., skip implicit votes when more than N entries were injected — too diluted to be meaningful). Reference crt-018's per-entry utility scoring as a complementary guard. |
| SR-06 | `TimedOut` sessions: SCOPE.md (§Background Research) notes `status = TimedOut` with `outcome = NULL`. The resolved decision is zero signal. But some TimedOut sessions may have `outcome IS NOT NULL` if outcome was resolved before the timeout. The filter `status IN (Completed, TimedOut) AND outcome IS NOT NULL` would process them. This may or may not be intended. | Low | Med | Spec writer must clarify whether TimedOut-with-outcome sessions should be included or explicitly excluded by status filter only. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `record_usage_with_confidence` is the shared write path for both real-time (UsageService) and background (implicit votes). It uses `BEGIN IMMEDIATE` transactions. Concurrent tick + real-time MCP calls contend on the same SQLite write lock. Under high session-close rates the background step could block or be blocked. | Med | Low | Architect must verify SQLite WAL mode is enabled (it is, per existing architecture) and document expected contention behavior. No schema change needed, but tick must handle `SQLITE_BUSY` gracefully. |
| SR-08 | crt-019 dependency: `alpha0`/`beta0` Bayesian prior parameters must be snapshotted before `spawn_blocking`. If crt-019 is not merged or its `ConfidenceStateHandle` API changes before crt-020 implementation, the snapshot pattern breaks. crt-020 is gated on crt-019. | Med | Low | Confirm crt-019 is merged and API-stable before crt-020 implementation begins. The dependency is declared but the interface must be verified at implementation time. |
| SR-09 | Injection log JOIN correctness: `scan_injection_log_by_sessions` already chunks by 50 session IDs to avoid large IN clauses. A COUNT DISTINCT bug here (similar to the one caught in crt-018 — Unimatrix #1044) would cause vote inflation. The dedup logic (HashSet in Rust) is correct only if the scan returns all rows for the session. | Med | Med | Spec writer must require integration tests that assert exactly 1 vote per unique entry_id regardless of how many injection_log rows exist per (session_id, entry_id). This exact scenario is called out in AC-10/AC-11 — architect must ensure the store method enforces it. |

## Assumptions

- **SCOPE.md §Background Research (injection_log)**: Assumes `scan_injection_log_by_sessions` returns all rows for a session, not a paginated subset. If any pagination limit exists, some entries could be silently skipped. Should be verified against the implementation in col-010.
- **SCOPE.md §Background Research (GC)**: Assumes the 15-minute tick always outpaces the 30-day GC window. This holds at typical load but breaks if the server is offline for >30 days, after which historical sessions are irrecoverably lost before crt-020 processes them. Acceptable by non-goal #8 but worth noting to the architect.
- **SCOPE.md §Resolved Design Decisions #1**: Pair accumulation assumes the pending counter persists across server restarts. If the counter is in-memory, a restart between two rework sessions causes the counter to reset and the unhelpful vote is never applied. Counter storage must be persistent.

## Design Recommendations

- **SR-04**: Architect must trace the Stop hook signal flow end-to-end and confirm the two vote paths are disjoint. If not, the `implicit_votes_applied` flag alone is insufficient — a finer-grained dedup per (session_id, entry_id, vote_type) may be needed. ADR-006 (Vote Correction Atomicity, Unimatrix #57) is the precedent.
- **SR-01 + SR-02**: Architect should resolve the pair accumulation counter location (open question) and batch ordering in a single ADR that also addresses cold-start drain behavior. These are coupled decisions.
- **SR-05**: Spec writer should define the injection-count dilution threshold as an explicit constant (`IMPLICIT_VOTE_MAX_INJECTIONS_PER_SESSION`) with a rationale rather than leaving it unbounded. Background tick pattern #1542 flags the need to define semantics for error cases before implementation — apply the same discipline to dilution semantics.
- **SR-03**: Architect should benchmark or estimate tick duration under max batch before committing to inline confidence recomputation. If duration is tight, defer recomputation to the existing confidence refresh step that immediately follows.
