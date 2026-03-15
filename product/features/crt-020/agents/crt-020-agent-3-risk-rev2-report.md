# Agent Report: crt-020-agent-3-risk-rev2

## Task
Update RISK-TEST-STRATEGY.md for crt-020 after scope change (ADR-001: no implicit unhelpful votes in v1).

## Actions Taken

### Artifacts Read
- SCOPE.md — confirmed v1 design: success-only signal, unhelpful deferred
- ARCHITECTURE.md — confirmed simplified algorithm: single `record_usage_with_confidence` call, no `implicit_unhelpful_pending` table
- ADR-001 (No Implicit Unhelpful v1) — eliminations: `implicit_unhelpful_pending` table, `increment_pending_and_drain_ready`, `gc_pending_counters`, all rework branching
- ADR-002 (Cold-Start Batch Cap) — ordering and cap confirmed unchanged
- ADR-003 (Double-Count Prevention) — `implicit_votes_applied` flag strategy confirmed unchanged
- ADR-004 (Inline Confidence Recomputation) — single closure pattern confirmed; note: ADR-004 references "both calls" (helpful and unhelpful) but the unhelpful call no longer exists; snapshot is used for the single helpful call
- SPECIFICATION.md — FR-02 explicitly confirms zero signal for all non-success outcomes
- SCOPE-RISK-ASSESSMENT.md — all 9 SR-XX risks traced

### Removed Items

| Removed | Reason |
|---------|--------|
| R-01 (pair accumulation partial write) | `increment_pending_and_drain_ready` function eliminated by ADR-001 |
| R-08 (orphan rows in `implicit_unhelpful_pending`) | Table eliminated by ADR-001 |
| R-12 (entry in both helpful and unhelpful ID sets) | Only one `record_usage_with_confidence` call remains; unhelpful_ids is always `[]` |
| E-03 (rework batch with no threshold drains) | No rework processing of any kind in v1 |
| E-05 (`implicit_unhelpful_pending` row for entry_id = 0) | Table eliminated |
| S-04 (corrupted `implicit_unhelpful_pending` blast radius) | Table eliminated |
| SR-02 row traceability | Resolved by ADR-001, not by an architecture risk — mapped to `—` with explanation |

### Re-numbered Risks

Former IDs → New IDs (after removing 3 risks, R-02 through R-16 compressed):

| Former | New | Description |
|--------|-----|-------------|
| R-02 | R-01 | Vote inflation (dedup) |
| R-03 | R-02 | Stop hook missing flag |
| R-04 | R-03 | Migration guard |
| R-05 | R-04 | Mark-before-vote ordering |
| R-06 | R-05 | Confidence snapshot lock |
| R-07 | R-06 | NULL ended_at sessions |
| R-09 | R-07 | Cold-start starvation |
| R-10 | R-08 | Chunk boundary split |
| R-11 | R-09 | Stale prior |
| R-13 | R-10 | TimedOut with non-NULL outcome |
| R-14 | R-11 | GC ordering race |
| R-15 | R-13 | Signal quality dilution |
| R-16 | R-14 | Tick timeout breach |

### New Risk Added

**R-12: Rework/Abandoned Sessions Silently Included in Helpful IDs** (Med / Low / Med)

The ADR-001 simplification removes all rework branching, creating a new risk: a missing or incorrect outcome filter in `apply_implicit_votes` could pass non-success session entries into `helpful_ids`. This is the most likely implementation error after stripping the pair-accumulation code — the developer may iterate all sessions in the batch without filtering on `outcome == "success"`. Four test scenarios defined; mixed-outcome batch test is the primary detector.

### SR-02 Traceability Update
SR-02 (pair accumulation counter storage location) is now resolved by ADR-001 (design decision not to build the feature at all), not by an architecture-level risk. Row updated to `—` in the Architecture Risk column with rationale.

### Other Updates
- R-03 scenario 5: added explicit assertion that `implicit_unhelpful_pending` does NOT exist in v13 (regression guard against the removed table being inadvertently created)
- R-04 scenario 3: removed reference to rework/pending counter update (was part of "happy path" assertion)
- E-08 added: mixed-outcome batch where all sessions are marked applied but only success sessions get votes — this is the behavioral contract for the simplified algorithm
- F-03 updated: noted that with ADR-001, only the single helpful write is at risk of double-application (no second unhelpful call)
- ADR-004 note: the ADR references "both helpful and unhelpful calls" using the same snapshot; updated R-09 scenarios to reflect only the single call

## Risk Count (Updated)

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 1 | R-01 |
| High | 4 | R-02, R-03, R-04, R-05 |
| Medium | 9 | R-06, R-07, R-08, R-09, R-10, R-11, R-12, R-13 (Low), R-14 (Low) |
| Low | 2 | R-13, R-14 |

Total: 14 risks (down from 16).

## New Risks Introduced by Simplification

1 new risk: **R-12** (outcome-filter implementation correctness) — Medium severity, Low likelihood. The simplification creates a new failure mode: the removed branching logic (which explicitly separated success/rework paths) was also an implicit correctness guard. Without it, an implementation that iterates all sessions without checking `outcome` would silently inflate helpful votes for failed sessions. R-12 is the test strategy's guard against this regression.

## Knowledge Stewardship
- Queried: N/A — this is a revision pass on existing risk assessment; no new historical intelligence needed (prior agent already queried Unimatrix for this feature)
- Stored: nothing novel — the pattern "scope simplification that removes a data structure creates a new implementation-correctness risk for the remaining filter logic" is feature-specific, not cross-feature pattern worthy of storage
