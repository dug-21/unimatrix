# Scope Risk Assessment: crt-055

**Mode**: scope-risk | **Date**: 2026-06-16 | **Consumer** of the crt-054 producer contract (SCOPE §"Producer contract").

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Silent-zero / empty-clobber regression (#750 class, lesson #5022). The new aggregate columns sit behind the same single `store_cycle_review()` writer with three presence guards; a second writer or a recompute that ignores data-presence re-introduces zero-clobber. | High | High | Architect MUST persist new columns ONLY via the one full-pipeline writer; recompute via clear-memo-and-fall-through, never a second writer near the memo/check_stored_review site. Pin the per-metric source-presence gate (SCOPE Constraint 2,3). |
| SR-02 | SUMMARY_SCHEMA_VERSION 4→5 bump alone does not flush stale cached reviews (#5022) — staleness is advisory-only; the recompute path must auto-refresh pre-v5 rows when source present and retain byte-identical when purged. | High | Med | Reuse #758's typed staleness enum + data-presence gate; do not add a "use force=true" advisory on the purged-stale path. Add the three assertions from #5022 (data-present recompute, purged retain, force+purged no-clobber). |
| SR-03 | Three-path schema bump (#4153) — migration upgrade block, fresh-create `db.rs`, and migration test assertions must all move together; `cycle_review_index` ALTER must be `pragma_table_info`-guarded per crt-047 template. | Med | Med | Follow the crt-047 v23→v24 column template exactly; update the migration-version test assertion in the same change. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Producer-contract drift: crt-054's prior scope had it owning `cycle_review_index` columns, `SUMMARY_SCHEMA_VERSION`, and `store_cycle_review`. The now-binding contract strips all of that. Residual stale knowledge (ADR-008 / #5006) and any un-regenerated crt-054 artifact could re-import removed scope. | High | Low | crt-054's ARCHITECTURE was already fully regenerated producer-only and ADR-008 corrected (Unimatrix #5032); the architect must still `context_correct` #5006 and diff crt-054 ADR-001..010 against §Producer contract field-by-field at design start. Treat the contract as the single source on any conflict. |
| SR-05 | Bytes-vs-tokens contradiction (prior crt-054↔crt-055). Contract resolves to bytes-only with NO token-named field. Any re-introduction of `token_bytes_per_unit` or a "tokens (est.)" column violates the leak/honesty boundary. | Med | Low | Add a guard test asserting no token-named field on `CycleReviewRecord`/`RetrospectiveReport`; verify crt-054 ADR-005 (#5030) holds and no `reread`/`compaction` regex class exists. |
| SR-06 | Dual reload metrics collapsing into one number. `context_reload` (cross-session) and `compaction_reread` (post-compaction within-cycle) are two columns/two gates/one engine — easy to conflate into a single overlap window. | Med | Med | Pin each metric's exact overlap window before building; keep two columns, never re-collapse (SCOPE Constraint 5). |
| SR-07 | Scope creep via the folded point-issues (#556/#320/#593/#206-4) and rank 1–3 aggregate column shapes (Open Q1) — undefined columns invite over-building or response-only/durable confusion (#206-4, Open Q3). | Med | Med | Lock rank 1–3 column shapes against ass-077 RQ-2 at design; decide #206-4 durability explicitly; keep the catalog tiny and high-precision (Open Q4). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | Held-route believable-zero: `activity_snapshot()` must fold on BOTH registered and held routes; a held-route miss yields a real zero indistinguishable from "no activity" (#1 producer regression risk). crt-055 consumes the result and surfaces it. | High | Med | Drive the fail-loud guard off `raw_signals_available` per-metric; add the regression test asserting a non-empty fold for a representative TS-client cycle (SCOPE Constraint 4b). |
| SR-09 | Read-before-purge ordering: crt-055 must read `activity_snapshot()` before the crt-052 Wave-B hold purge zeroes the buffer. Wrong ordering silently zeroes the fold columns. | High | Med | Pin the review-pipeline read site ahead of `purge_cycle_transcripts`; assert ordering in test (SCOPE Constraint 6). |
| SR-10 | Cross-cycle attribution + integer-width: `compaction_events` rows and folds join to a cycle via the session→`feature_cycle` declaration chain at review; producer widths (`u64`/`u32`/`[u32;N]`) land into i64 columns. Truncation or mis-attribution corrupts aggregates. | Med | Med | Use checked/saturating conversion at the persist boundary; assert undeclared-session folds die fail-loud (not a fabricated zero); confirm `compacted_at` seconds-granularity matches PostToolUse `ts` for the gate comparison. |
| SR-11 | Multi-compaction boundary selection (Open Q2): a session with N `compaction_events` rows needs a defined `compaction_reread` gate (earliest/latest/per-boundary). Undefined choice yields inconsistent or double-counted re-reads. | Low | Med | Resolve the boundary-selection rule at design; document it as a fixed reckoning detail. |

## Assumptions

- **(SCOPE §Producer contract, §Dependencies)** crt-054 will be re-scoped and delivered to the contract exactly. If crt-054 ships its prior wider scope, crt-055's migration collides and the two features double-own `cycle_review_index`. (SR-04)
- **(SCOPE §Dependencies — crt-052 Wave B)** the transcript hold is ON, unconditional, and survives drains to review. If it is off or disableable, Surface B durability and SR-08/SR-09 collapse.
- **(SCOPE §In scope item 3, #758 merged)** `compute_context_reload_pct` is live and stable; crt-055 only promotes it to a durable column. If #758's signal is unstable, SR-06's cross-session column is unreliable.
- **(SCOPE §In scope item 4)** `activity_snapshot()` is content-free and metadata-only; the leak gate stays structural. If any content read enters the persist path (R-A, default NO), the leak boundary is violated.

## Design Recommendations

1. **Diff crt-054's ADRs against the contract first** (SR-04/SR-05): architect `context_correct`s #5006, then field-by-field reconciles crt-054 ADR-001..010 vs §Producer contract before any crt-055 design. Any drift is fixed in §Producer contract first.
2. **Treat SR-01/SR-02 as the dominant risk** (lesson #5022): single writer, clear-memo-fall-through recompute, per-metric data-presence gate, and the three #5022 assertions. This is the highest-likelihood failure class for this surface.
3. **Sequence the fail-loud presentation guard first** within the feature (SCOPE In-scope item 1) — lowest risk, de-risks the believable-zero class (SR-08) before the aggregate columns land.
4. **Pin overlap windows and read-ordering before building** (SR-06/SR-09): two reload columns never collapsed; `activity_snapshot()` read strictly before the crt-052 purge.
