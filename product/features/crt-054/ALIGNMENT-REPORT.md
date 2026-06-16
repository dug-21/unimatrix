# Alignment Report: crt-054

> Reviewed: 2026-06-16
> Re-scope: producer-only (2026-06-16). This report OVERWRITES the prior wider-scope ALIGNMENT-REPORT.md (2026-06-14).
> Artifacts reviewed:
>   - product/features/crt-054/architecture/ARCHITECTURE.md
>   - product/features/crt-054/specification/SPECIFICATION.md
>   - product/features/crt-054/RISK-TEST-STRATEGY.md
> Scope source: product/features/crt-054/SCOPE.md (incl. Re-scope note) + SCOPE-RISK-ASSESSMENT.md
> Binding contract: product/features/crt-055/SCOPE.md §"Producer contract"
> Vision source: product/PRODUCT-VISION.md; goal #4677 (self-learning), RQ-8 boundary (ass-077)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances self-learning (#4677) as a content-free knowledge surface; RQ-8 "informs, never controls" held as a hard edge across all three docs. |
| Milestone Fit | PASS | Cortical (learning & drift) producer half; v1 boundary tight; ass-078 deferred signals + vnc-036 wire change explicitly shelved to measured need. |
| Scope Gaps | PASS | Both producer surfaces (A `compaction_events`, B `activity_snapshot()`) + `[transcript_signals]` config fully covered by FR/AC. No SCOPE item dropped. |
| Scope Additions | PASS | No additions beyond SCOPE. `high_water`-populated and the Wave B startup assert are SCOPE-sanctioned, not new scope. |
| Architecture Consistency | PASS | Architecture, spec, and risk strategy agree on surfaces, seams, ADR index, and the producer/consumer split with crt-055. |
| Risk Completeness | PASS | SR-01..SR-10 each map to ≥1 architecture risk (R-01..R-14), an ADR, and an AC; believable-zero family is the correctly-prioritized Critical cluster. |

Counts: PASS 6, WARN 0, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Coverage | Surface A — `compaction_events` table | SCOPE In-scope §1-3 → FR-A1..A7, AC-01..04. Insert-only, content-free, written regardless of declaration, next `CURRENT_SCHEMA_VERSION` bump on a NEW table only. Fully covered. |
| Coverage | Surface B — `activity_snapshot()` fold | SCOPE In-scope §4-5 → FR-B1..B10, AC-05..09/12. Both routes; never persisted; rides crt-052 hold to review. Fully covered. |
| Coverage | `[transcript_signals]` config | SCOPE In-scope §6 → FR-C1..C4, AC-10/11. Domain-neutral `error`/`refusal` default, `validate()`-bounded, one shared `RegexSet`. Fully covered. |
| Simplification | `high_water` populated (was deferred) | SCOPE Re-scope note: populated now (buffer already tracks it) to avoid a second migration; reserved for future precise gating; no wire change. Rationale documented (FR-A4, R-13). Acceptable. |
| Exclusion (honored) | All `cycle_review_index` work, `SUMMARY_SCHEMA_VERSION`, reload reckoning, token/cost fields, vnc-036 wire change, orchestration/FinOps | SCOPE §Out-of-scope → spec §"NOT in Scope" + AC-15 (grep-level negative gate). All exclusions explicitly restated and test-enforced. No leakage. |

No scope gaps. No scope additions. Simplifications are SCOPE-authorized with rationale.

## Variances Requiring Approval

None.

The design holds the RQ-8 "informs, never controls" boundary as a hard edge and contains no drift toward orchestration, control, or content capture (see Detailed Findings → Vision Alignment). No human-attention variances surfaced.

## Detailed Findings

### Vision Alignment

The feature advances goal #4677 (self-learning intelligence): "behavioral signals from agent workflows" are an explicit learning input. crt-054 supplies two such content-free signals (a durable compaction event; a byte/delta/signature fold) that let crt-055 surface trustworthy *information about the process*. This is squarely within "knowledge engine that understands workflow context" and outside "not an orchestration engine" (PRODUCT-VISION lines 11, 23, 25).

The RQ-8 guardrail is stated identically and load-bearingly in all three documents, not as boilerplate:
- ARCHITECTURE §1: "every signal crt-054 produces informs, never controls. Disqualifying test: 'does this counter control / bill / schedule / block execution?' If yes, out of lane."
- SPECIFICATION Objective + NFR-2: "The fold informs, never controls (RQ-8); bytes is the honest unit — never tokens, never cost."
- RISK-TEST-STRATEGY Security Risks: "Blast radius if Surface B's accumulator were compromised: a corrupted *count* (informs, never controls — RQ-8); it cannot bill, schedule, or block execution."

Content-capture drift is structurally foreclosed, not merely promised:
- `ActivitySnapshot` is "structurally incapable of carrying transcript bytes (no `Vec<u8>`/`String`/`&[u8]` content field)" with metadata-only `Debug` and no `Display` (FR-B4/B7, NFR-1, AC-08). This is the ADR-002 content-opacity contract (#4740), enforced by a structural test mirroring `test_candidates_structurally_absent`.
- The R-A guardrail (every signal is a running fold or a discrete server-seam event, never a query over the assembled buffer) is a binding constraint (Constraint 1) carried into NFR-1 and AC-08.
- Surface A is content-free: "No payload, no `tracing` of content" (SCOPE §1.3); `session_id` written via parameterized INSERT, no content/path column (AC-03, Security Risks).

Bytes-only honesty (the secondary RQ-8 facet — no cost/orchestration foothold) is enforced as a hard edge: NFR-2 + AC-15 grep-level negative test asserts no `token_*` symbol and resolves the prior crt-054↔crt-055 bytes-vs-token contradiction in favor of bytes-only.

Architectural-principle fit: Surface A respects the "audit log is append-only" spirit (insert-only, never UPDATE/DELETE); the fold respects "in-memory hot path" (Arc-borne, no DB read at query time); graceful degradation is honored (poisoned mutex → empty per #4764; INSERT failure → logged + fail-loud absence, never a panic). No principle is violated.

Goal-advancement check: the feature advances exactly one strategic goal (self-learning #4677) and makes no claim on the others — proportionate for a producer-half infrastructure feature. No over-build toward future-milestone capability (consistent with the "avoid overstating defensive structure" lesson — integrity/defensiveness are managed as documented rationale, not elevated to scope).

### Milestone Fit

Tight v1 boundary, correctly disciplined:
- ass-078 deferred signals (turn-size percentiles, thrash/rolling-hash, entropy, language/code-fence, mean-turn-size) are explicitly shelved "with measured evidence only" (SCOPE §Out-of-scope; spec §NOT-in-Scope).
- vnc-036 (precise per-compaction wire byte boundary) SHELVED; `high_water` captured server-side instead, "reopen only with measured need."
- The default signal catalog is deliberately tiny (`error`, `refusal`) with "under-catalog and let domains extend via config" — the domain-agnostic-platform posture, not SDLC-literal hardcoding.

No future-milestone capability is built ahead of need. This is the correct milestone-discipline posture.

### Architecture Review

The producer/consumer split is clean and consistent with the crt-055 binding contract (verified against crt-055/SCOPE.md §"Producer contract", lines 59-67, 146-147): crt-055 owns `cycle_review_index`, `store_cycle_review`, `SUMMARY_SCHEMA_VERSION` (4→5), and the `compaction_reread` reckoning; crt-054 owns only `compaction_events` + `activity_snapshot()` + `[transcript_signals]`. The two features ALTER different tables (no migration-content collision); the shared `CURRENT_SCHEMA_VERSION` counter is correctly identified as a merge-order coordination point (NFR-8, ADR-008, SR-04/R-04), not a code seam.

Stale-knowledge handling is correct: ADR-008 residue (#5006 claiming crt-054 owns v4/v29 on `cycle_review_index`) is flagged for `context_correct`, and ADR-001/004/009 residue (snapshot latches, `[u32;16]` literal, `reread`/`compaction` classes, `token_bytes_per_unit`) is flagged for regeneration against the new SCOPE rather than edited — preventing re-introduction of removed scope (SR-05/R-12, AC-15). The ADR index (§4) marks each prior ADR's correction status explicitly.

The two highest-risk seams (the `listener.rs:1854` INSERT under handler locks; the held-route fold) are correctly identified and bounded by ADR-007 (no lock held across the INSERT; `high_water` captured then guard dropped) and ADR-001 (accumulator embedded in the buffer so both routes fold by construction). Component table, data-flow, and integration-surface sections are mutually consistent.

### Specification Review

Every SCOPE In-scope item and binding constraint traces to a testable FR/NFR and an AC. The spec's domain models, entity relationships, and "NOT in Scope" section restate the producer-only boundary verbatim, including the negative items (no `saw_compaction`/`reload` latch, no `reread`/`compaction` class, no token field). AC-15 makes the exclusions a positive test obligation (grep-level negative gate), which is the right mechanism for preventing stale-scope regression. The believable-zero ACs (AC-06 held-route, AC-07 read-before-purge) are correctly marked mandatory integration tests that a registered-only/unit-only test does not satisfy — closing the #750/#5025 failure class. No requirement contradicts the contract.

### Risk Strategy Review

Complete and well-prioritized. The four Critical risks are the believable-zero family at two seams (R-01 routing / R-02 sequencing), the lock graph at the INSERT seam (R-03), and the shared schema-version counter (R-04) — each grounded in prior load-bearing lessons (#5025, #4799, #4095, #760, #3753). Scope Risk Traceability shows every SR-01..SR-10 mapping to ≥1 architecture risk, an ADR, and a verifying AC, with the explicit assertion "No scope risk is dropped." The Security Risks section correctly identifies the two untrusted-content entry points (transcript delta bytes, operator regex) and shows content-opacity + linear-time `RegexSet` as the structural controls — and ties the blast-radius analysis back to RQ-8 (a corrupted count cannot control execution). Failure-mode posture ("every absence is fail-loud, never a believable zero") is the correct governing principle for this feature and is consistent across all three documents.

## Knowledge Stewardship
- Queried: goal #4677 (self-learning) + all four strategic-goal entries (#4671/4673/4677/4678/4946); /uni-query-patterns for `vision` alignment patterns -- no high-relevance vision-alignment pattern returned (top hit #2298 config-key divergence, 0.38; nothing on producer-boundary / informs-never-controls drift).
- Stored: nothing novel to store -- the RQ-8 "informs, never controls" boundary and content-opacity structural enforcement are already captured as binding constraints in SCOPE + ADR-002 (#4740) and the crt-055 contract; this feature's alignment is clean with no recurring cross-feature misalignment pattern. The findings are feature-specific (producer/consumer split, stale-scope regeneration) and do not generalize beyond the crt-054/crt-055 pair.
