# Alignment Report: crt-020

> Reviewed: 2026-03-15
> Agent ID: crt-020-vision-guardian-final2
> Artifacts reviewed:
>   - product/features/crt-020/architecture/ARCHITECTURE.md
>   - product/features/crt-020/specification/SPECIFICATION.md
>   - product/features/crt-020/RISK-TEST-STRATEGY.md
> Scope source: product/features/crt-020/SCOPE.md
> Scope risk source: product/features/crt-020/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Supersedes: prior ALIGNMENT-REPORT.md (crt-020-vision-guardian-final, 2026-03-15)

---

## Context: Changes Since Prior Review

The prior report (crt-020-vision-guardian-final) returned HOLD with two WARNs and one FAIL:

- **FAIL-01**: RISK-TEST-STRATEGY.md contained obsolete pair-accumulation risks — resolved. The updated RISK-TEST-STRATEGY.md removes those risks; 14 risks remain, all applicable to the current design.
- **WARN-A**: ARCHITECTURE.md Open Question 1 (module location for `apply_implicit_votes`) unresolved — resolved. ADR-005 (#1639) closes this: free async fn in `background.rs`, server crate. ARCHITECTURE.md references ADR-005.
- **WARN-B**: Log level mismatch between ARCHITECTURE.md (`tracing::debug!`) and SPECIFICATION.md NFR-06 (`tracing::info!`) — resolved. SPECIFICATION.md NFR-06 now specifies `tracing::debug!`.

This review re-evaluates all checks against the updated artifacts.

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances the "Invisible Delivery" confidence loop described in PRODUCT-VISION.md |
| Milestone Fit | PASS | Correctly scoped to Search Quality Enhancements milestone, Track A, P2; dependencies declared and met |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria addressed in source docs |
| Scope Additions | PASS | No unapproved expansions; all additions are logically required derivations from SCOPE.md goals |
| Architecture Consistency | PASS | All open questions closed; ADRs complete; component signatures consistent |
| Risk Completeness | PASS | 14 risks cover all SCOPE-RISK-ASSESSMENT.md entries; one minor stale reference in I-04 (WARN) |

**Overall Status: PASS**

One minor observation is recorded below under Detailed Findings — Risk Strategy Review. It does not block delivery.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Implicit unhelpful votes removed | SCOPE.md Resolved Decision #1 and Non-goal #10 document this. ADR-001 (superseded) is retained for historical reference. SPECIFICATION.md "NOT In Scope" and domain model table are consistent. No implementation artifact remains. |
| Simplification | SR-05 signal quality dilution (no injection cap) | SCOPE-RISK-ASSESSMENT.md SR-05 recommended `IMPLICIT_VOTE_MAX_INJECTIONS_PER_SESSION`. Accepted as out-of-scope for v1. RISK-TEST-STRATEGY.md R-13 classifies it Low/High. Documented acceptance, not a hidden gap. |
| Residual (non-blocking) | SCOPE.md §Proposed Approach uses `IMPLICIT_VOTE_BATCH_SIZE` | SCOPE.md line 230 retains the older constant name; all three source documents and SCOPE.md ACs use `IMPLICIT_VOTE_BATCH_LIMIT`. This is a SCOPE.md artifact, not a source document inconsistency. Does not affect delivery. |

No scope gaps. No unapproved scope additions.

---

## Variances Requiring Approval

None.

---

## Detailed Findings

### Vision Alignment

PRODUCT-VISION.md (lines 103–104) describes crt-020:

> "Close the feedback loop for automated pipelines that never pass `helpful: true`. Join `injection_log` with resolved session outcomes post-close. For entries injected in successful sessions: add 1 implicit helpful vote... Deduped per session per entry. Run as a background tick operation. Uses existing `helpful_count`/`unhelpful_count` fields — no schema change. Depends on: crt-019 (formula calibrated to use votes), crt-018 (session outcome infrastructure). P2."

The source documents implement this precisely, with one deliberate and documented deviation from the vision text: the "0.5 implicit unhelpful vote" is not implemented in v1. The product vision was written before SCOPE.md Resolved Decision #1 crystallised the no-implicit-unhelpful approach. The conservative direction is correct — applying unattributable negative signals to entries injected in failed sessions would undermine confidence trustworthiness, which is one of the product's core commitments:

> "Unimatrix ensures what agents remember is trustworthy, correctable, and auditable."

The simplification serves the vision rather than cutting against it.

All three source documents advance the product's three-leg architecture (Files / Unimatrix / Hooks): this feature makes the Hooks leg produce confidence signal without any agent cooperation, which is the explicit "Invisible Delivery" principle.

### Milestone Fit

PRODUCT-VISION.md places crt-020 as the terminal dependency of Track A:

```
Track A (confidence)
────────────────────
crt-019
  ├─► crt-018b
  └─► crt-020
```

SPECIFICATION.md Constraint C-06 and the Dependencies table explicitly gate on crt-019 and crt-018b. MEMORY.md confirms both are shipped. The feature does not introduce Graph Enablement capabilities, future milestone objects, or new crate dependencies. The schema change (one column and one index on `sessions`, v12 → v13) is the minimum required and correctly scoped.

### Architecture Review

All open questions are closed. ADR coverage is complete:

| ADR | Decision | Unimatrix ID | Status |
|-----|----------|-------------|--------|
| ADR-001 | Pair accumulation counter (superseded) | #1612 | Superseded by success-only simplification; retained for historical reference |
| ADR-002 | Batch cap 500, oldest-first ordering | #1613 | Resolves SR-01, R-07 |
| ADR-003 | `implicit_votes_applied` flag for double-count prevention | #1614 | Resolves SR-04, R-02, R-04 |
| ADR-004 | Inline confidence recomputation | #1615 | Resolves SR-03, R-14 |
| ADR-005 | `apply_implicit_votes` location — free async fn in background.rs | #1639 | Resolves former WARN-A |

Component signatures are consistent between the Component Breakdown section and the Integration Surface table:

| Function | Breakdown signature | Surface table signature | Match |
|----------|--------------------|-----------------------|-------|
| `scan_implicit_vote_candidates` | `fn(conn, limit) -> Vec<SessionId>` | `fn(&Connection, usize) -> Result<Vec<u64>, rusqlite::Error>` | Yes |
| `get_injection_entry_ids` | `fn(conn, session_ids) -> HashMap<SessionId, Vec<EntryId>>` | `fn(&Connection, &[u64]) -> Result<HashMap<u64, Vec<u64>>, rusqlite::Error>` | Yes |
| `mark_implicit_votes_applied` | `fn(conn, session_ids)` | `fn(&Connection, &[u64]) -> Result<(), rusqlite::Error>` | Yes |
| `apply_implicit_votes` | `async fn(&Store, &ConfidenceStateHandle) -> Result<ImplicitVoteSweepStats, ServiceError>` | Same | Yes |

The Stop hook integration (FR-10, FR-11) is correctly specified: the Stop hook sets `implicit_votes_applied = 1` at session close; the background tick is the sole applier of votes. The two paths are disjoint by design (ADR-003). No circular crate dependencies are introduced — store crate functions are synchronous `&Connection` primitives; the confidence closure is passed as a parameter from the server crate, not imported by the store crate.

### Specification Review

All 13 functional requirements (FR-01 through FR-13) trace directly to SCOPE.md goals and constraints. All 11 acceptance criteria match the SCOPE.md acceptance criteria, with behavioral additions (AC-06 through AC-11) that are derivations of SCOPE.md goals 4 and 5.

SCOPE.md non-goals are faithfully preserved. The "NOT In Scope" section of SPECIFICATION.md enumerates every SCOPE.md non-goal with no omissions.

NFR-06 now specifies `tracing::debug!` with three named fields (sessions processed, implicit helpful votes applied, sessions skipped). This matches ARCHITECTURE.md Step 3 and is consistent with how other maintenance tick sub-steps are logged in the codebase. Prior WARN-B is resolved.

The four Open Questions for the architect (OQ-01 through OQ-04) in the specification footer are all closed in ARCHITECTURE.md. The specification retains the questions for document history — this is acceptable as long as ARCHITECTURE.md is the authoritative source for those decisions.

Session outcome taxonomy in the domain model is unambiguous:

```
Completed (1) + "success"   -> 1 helpful vote per distinct injected entry
All other status/outcome combos -> 0 signal (unhelpful_count never modified)
```

The SQL filter `WHERE status = 1` (not `status IN (1, 2)`) correctly excludes TimedOut sessions, resolving SR-06 and R-10.

### Risk Strategy Review

The updated RISK-TEST-STRATEGY.md contains 14 risks. All pair-accumulation risks (former R-01, R-08, R-12 and associated edge cases E-03, E-05, S-04) are removed. The remaining 14 risks are all applicable to the current design.

The scope risk traceability table correctly maps all nine SCOPE-RISK-ASSESSMENT.md risks:

| Scope Risk | Risk Register | Resolution |
|-----------|--------------|------------|
| SR-01 (cold-start backlog) | R-07 | ADR-002: 500-session cap, oldest-first |
| SR-02 (pair accumulation counter) | — | Moot: ADR-001 superseded; no pair accumulation |
| SR-03 (inline confidence recomputation duration) | R-14 | ADR-004: bounded at ~500ms for full batch |
| SR-04 (double-counting real-time vs background) | R-02, R-04, F-03 | ADR-003: `implicit_votes_applied` flag |
| SR-05 (signal quality dilution) | R-13 | Accepted as Low/out-of-scope for v1 |
| SR-06 (TimedOut with non-NULL outcome) | R-10 | SQL filter `status = 1` excludes TimedOut |
| SR-07 (SQLite BUSY contention) | I-01 | WAL mode + busy timeout |
| SR-08 (crt-019 API stability) | R-05, R-09 | crt-019 merged; snapshot pattern verified |
| SR-09 (injection_log scan correctness) | R-01 | HashSet dedup directly tested |

**Minor observation (non-blocking):** RISK-TEST-STRATEGY.md I-04 retains the sentence "ARCHITECTURE.md Open Question 1 (`apply_implicit_votes` module location) is unresolved." ADR-005 (#1639) resolves this. The remainder of I-04 — verifying no circular crate dependency is introduced — remains valid as an implementation check. The stale opener does not affect test coverage or implementation correctness; it is a documentation inconsistency only. No action required before delivery, though the implementer should be aware.

Coverage summary (updated by the risk strategist) is now accurate: 1 Critical risk, 4 High, 9 Medium, 2 Low.

Integration risks I-01 through I-04, edge cases E-01 through E-08 (excluding removed E-03 and E-05), and security risks S-01 through S-03 are all applicable and correctly specified.

Failure modes F-01 through F-05 are correctly described. F-03 (mark-after-vote failure causing double-count on retry) is correctly documented as a known limitation of session-level flag granularity. The residual double-count risk in F-03 affects only the implicit helpful path (one call, not two), which is accurately described in the updated document.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns (topic: vision, category: pattern) — no vision-specific alignment patterns found; returned unrelated pattern entries. No recurring misalignment patterns applicable to crt-020.
- Queried: /uni-query-patterns for alignment review scope additions, open question residue — returned outcome and duties entries; no generalized scope addition or stale reference patterns in Unimatrix.
- Stored: nothing novel to store. The pattern observed across the crt-020 review iterations ("risk strategy written before scope is locked contains obsolete coverage; require risk strategist re-run after any scope change") was noted as a candidate in the prior report. The resolution — the risk strategist did re-run and produced a clean updated document — confirms the governance process worked correctly. The pattern is worth storing only if it recurs in a future feature where the re-run is skipped. No generalized pattern entry warranted at this time.
