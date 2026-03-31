# Alignment Report: crt-036

> Reviewed: 2026-03-31
> Artifacts reviewed:
>   - product/features/crt-036/architecture/ARCHITECTURE.md
>   - product/features/crt-036/specification/SPECIFICATION.md
>   - product/features/crt-036/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-036/SCOPE.md
> Scope risk source: product/features/crt-036/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly serves the intelligence pipeline's data hygiene needs and the domain-agnostic config principle |
| Milestone Fit | PASS | Correctly positioned as a Wave 1A/Cortical phase housekeeping feature; no future-wave capability introduced |
| Scope Gaps | PASS | All SCOPE.md goals, acceptance criteria, and constraints addressed in source documents |
| Scope Additions | WARN | Architecture adds `max_cycles_per_tick` field and `PhaseFreqTable` alignment guard (FR-10, ADR-003) — not in SCOPE.md, but both directly resolve SCOPE-RISK SR-01 and SR-07 and are low-risk additions |
| Architecture Consistency | VARIANCE | SPEC FR-03/FR-06 places `raw_signals_available` UPDATE inside the per-cycle transaction; ARCHITECTURE component 2 and ADR-001 place it outside as a separate write after commit — contradictory transaction boundaries require resolution |
| Risk Completeness | PASS | All scope risks (SR-01 through SR-09) traced; 16 risks registered; non-negotiable gate blockers listed and rationale solid |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `max_cycles_per_tick` config field | In SCOPE.md "Proposed Approach" the GC batch cap is described only as a rationale note ("should be capped"). Architecture and spec formalize it as a named `RetentionConfig` field with its own range validation and AC-12b/AC-16 tests. SCOPE-RISK SR-01 recommends this cap; addition is resolution of a named scope risk, not unsolicited expansion. |
| Addition | PhaseFreqTable alignment guard (FR-10, ADR-003) | SCOPE.md goal 8 documents `activity_detail_retention_cycles` as the ceiling for the PhaseFreqTable lookback window. The tick-time `tracing::warn!` diagnostic (FR-10) is not mentioned in SCOPE.md acceptance criteria but is a direct mitigation of SCOPE-RISK SR-07. Adds ~1 hour of implementation effort; consequence is only a warning log. |
| Simplification | `mark_signals_purged()` update path | SCOPE.md's "Proposed Approach" says to use `store_cycle_review()` INSERT OR REPLACE. Architecture overrides to a targeted `UPDATE` to avoid clobbering `summary_json`. The override is explicitly justified by SCOPE-RISK SR-05, which the SCOPE-RISK-ASSESSMENT itself raised. This is a correct and intentional deviation, not a gap. |

---

## Variances Requiring Approval

### VARIANCE-01: Conflicting transaction boundary for `raw_signals_available` UPDATE

**Classification**: VARIANCE

**What**: The SPECIFICATION and ARCHITECTURE contradict each other on whether the `raw_signals_available = 0` UPDATE runs inside or outside the per-cycle transaction.

- **SPECIFICATION FR-03 step 6** (line 87) lists "Set `raw_signals_available = 0`" as step 6 inside the per-cycle transaction block.
- **SPECIFICATION FR-06** (line 147) states explicitly: "This UPDATE executes within the same per-cycle transaction as the DELETE operations."
- **SPECIFICATION domain model** (line 545) lists the `raw_signals_available` UPDATE as part of the per-cycle transaction definition.
- **ARCHITECTURE component 2** (`mark_signals_purged()` description) places the call after `gc_cycle_activity()` commits and returns.
- **ARCHITECTURE data flow** (step 2c) shows `mark_signals_purged()` as a separate step after `gc_cycle_activity()` commits.
- **ADR-001** explicitly and at length justifies why `mark_signals_purged()` runs outside the transaction: "The `mark_signals_purged()` call that follows each `gc_cycle_activity()` runs as a separate single-statement write (no transaction needed — a single SQL statement is already atomic in SQLite). This means it executes after the connection is released from the GC transaction, preventing any nested connection acquisition."

These are not stylistic differences. The two designs have materially different guarantees:

- **Inside transaction (SPEC)**: If the UPDATE fails, the entire DELETE set is rolled back. The cycle's data is preserved and the flag is consistent. However, a targeted UPDATE on `cycle_review_index` inside the same `pool.begin()` transaction as the DELETEs extends the write lock to include the UPDATE. Since `mark_signals_purged()` is written as a direct `write_pool_server()` call, attempting to nest it inside a transaction acquired via `pool.begin()` on the same pool requires passing the transaction handle — the architecture's method signature `mark_signals_purged(&self, feature_cycle: &str)` uses `&self` (not a transaction reference), which means it would acquire a new connection from the pool rather than joining the existing transaction. This design would silently violate the atomicity claim while appearing correct.

- **Outside transaction (ARCHITECTURE/ADR-001)**: The `raw_signals_available` flag can be `1` for a pruned cycle if the process crashes between the DELETE commit and the UPDATE. ADR-001 accepts this as low-severity (R-13 in RISK-TEST-STRATEGY). This is the only design that is consistent with the method signature as specified.

**Why it matters**: An implementer reading the SPEC will write the UPDATE inside the transaction. An implementer reading the ARCHITECTURE will write it outside. The two interpretations produce different code. More critically, the SPEC's "inside transaction" claim cannot be mechanically satisfied by the method signature as designed — `mark_signals_purged(&self, ...)` cannot join an in-flight `pool.begin()` transaction. If the implementer attempts to honor the SPEC, they may pass a raw transaction handle, change the method signature, or call `store_cycle_review()` INSERT OR REPLACE (which the SPEC itself prohibits). Any of these deviations would cascade into R-03 territory (summary_json overwrite).

**Recommendation**: The ARCHITECTURE and ADR-001 design is correct and consistent with the method signatures specified. The SPECIFICATION must be updated: remove step 6 from the FR-03 per-cycle transaction block, remove the "within the same per-cycle transaction" clause from FR-06, and align the domain model entry. The domain model entry for "per-cycle transaction" should read: covers observations DELETE, query_log DELETE, injection_log DELETE, sessions DELETE — with `raw_signals_available` UPDATE as a subsequent, separate atomic write. This change does not affect any AC or NFR.

**Source evidence**:
- SPEC FR-03 step 6: "6. Set `raw_signals_available = 0` (FR-06) — targeted `UPDATE` on `cycle_review_index`."
- SPEC FR-06 line 147: "This UPDATE executes within the same per-cycle transaction as the DELETE operations."
- SPEC domain model line 545: "A single `BEGIN` / `COMMIT` block scoped to one `feature_cycle`, covering observations DELETE, query_log DELETE, injection_log DELETE, sessions DELETE, and the raw_signals_available UPDATE."
- ARCHITECTURE component 2: "`mark_signals_purged(feature_cycle: &str) -> Result<()>` — Targeted UPDATE [...] Uses `write_pool_server()` directly."
- ARCHITECTURE integration surface: `mark_signals_purged` signature is `async fn mark_signals_purged(&self, feature_cycle: &str) -> Result<()>` — not a transaction parameter.
- ADR-001: "The `mark_signals_purged()` call that follows each `gc_cycle_activity()` runs as a separate single-statement write."

---

## Detailed Findings

### Vision Alignment

crt-036 is a data lifecycle housekeeping feature that removes a correctness flaw in the learning engine: age-based pruning destroys data that still has learning value, and retains data that does not. The product vision (The Critical Gaps — Intelligence & Confidence section) identifies "Intelligence pipeline is additive boosts, not a learned function" and "No session-conditioned relevance" as High-severity gaps. The GNN (W3-1) and PhaseFreqTable (col-031/WA-2) are the primary consumers of `query_log` and `observations`. If those tables are pruned by wall-clock age, the training and frequency signals are degraded in ways that are invisible at runtime. crt-036 directly protects the data quality the vision depends on.

The config externalization principle (W0-3, domain agnosticism) is respected: `[retention]` follows the same `#[serde(default)]` pattern and `validate()` convention used by all other config sections. Operators can change retention policy without code changes.

The architecture does not introduce any new infrastructure not requested by the scope, does not pre-build Wave 2 or Wave 3 capabilities, and stays entirely within the background tick model established in earlier cortical features.

**Verdict: PASS.**

### Milestone Fit

crt-036 is correctly placed. The Cortical phase (`crt-*`) governs learning infrastructure — confidence evolution, co-access, graph edges, contradiction detection, retention of learning signals. A cycle-aligned GC that protects the training corpus for the GNN (W3-1) and the session frequency table (WA-2) is appropriate Cortical work. The feature does not build GNN infrastructure, does not build Wave 2 deployment infrastructure, and does not touch the confidence scoring pipeline. Schema stays at v19. No future-milestone work is bundled in.

The product vision places the evaluation harness (W1-3) as a gate condition before W3-1 ("Eval results show measurable improvement on a representative query set before model ships"). crt-036 does not interact with the eval harness — it is a prerequisite for data quality, not a pipeline change — so it does not need to satisfy or gate on W1-3 conditions.

**Verdict: PASS.**

### Architecture Review

The architecture is well-structured and directly traces to SCOPE.md design decisions. All five components (RetentionConfig, CycleGcPass, run_maintenance() GC block, Legacy DELETE removal, PhaseFreqTable guard) have clear rationale, integration surface specifications, and ADR backing.

Points of strength:
- Per-cycle transaction design (ADR-001) correctly solves SR-01 and SR-02 without over-engineering.
- `mark_signals_purged()` targeted UPDATE (SR-05 resolution) is correct and preserves `summary_json`.
- Active-session guard in `gc_unattributed_activity()` (SR-06 resolution) is specified in method detail.
- `max_cycles_per_tick` cap addition is a proportionate response to SR-01's performance concern.

One issue raised as VARIANCE-01 above: the placement of `mark_signals_purged()` relative to the per-cycle transaction is stated correctly in the ARCHITECTURE and ADR-001 but incorrectly in the SPECIFICATION. The ARCHITECTURE is the correct design.

**Verdict: PASS (ARCHITECTURE itself is internally consistent; the discrepancy is in SPECIFICATION).**

### Specification Review

The specification is comprehensive, covering all 15 SCOPE.md acceptance criteria and adding two new ACs (AC-12b for `max_cycles_per_tick` validation, AC-16 for the tick cap multi-cycle drain test, AC-17 for the PhaseFreqTable mismatch warning). These additions directly trace to SCOPE-RISK items and are proportionate.

The `max_cycles_per_tick` field addition is present in the specification's FR-01 table and in FR-11/FR-12, making it a fully specified addition. The scope additions are documented (not silently inserted).

The sole defect is in FR-03 and FR-06: the `raw_signals_available` UPDATE is described as part of the per-cycle transaction. This contradicts the ARCHITECTURE design and ADR-001. See VARIANCE-01 above.

The Not-in-Scope section correctly carries forward all SCOPE.md non-goals and adds one clarification: "Cycle-based filter in PhaseFreqTable::rebuild" is deferred to a follow-on, consistent with ADR #3686. This is a documented scope simplification, not a gap.

**Verdict: VARIANCE (single internal contradiction with ARCHITECTURE on transaction boundary).**

### Risk Strategy Review

The risk register is thorough and well-evidenced. All nine scope risks (SR-01 through SR-09) are traced in the "Scope Risk Traceability" table. SR-08 (K-window never advances if retro never called) is correctly accepted as a documented operational constraint rather than a code mitigation — consistent with SCOPE.md goal 6's explicit gating behavior.

The non-negotiable gate blockers list (8 items) is appropriate for a feature with DELETE authority over five tables. The two-independent-grep-assertions requirement for R-01 correctly reflects the known pattern (entry #3579) of delivery omissions at sites not mentioned in the primary implementation task.

The audit_log timestamp unit risk (R-12) is well-identified. The `observations.ts_millis` vs `audit_log.timestamp` unit difference is a real trap; both-sides boundary testing is the right mitigation.

R-13 (crash between commit and `mark_signals_purged`) is rated Low and is accepted in ADR-001. The RISK-TEST-STRATEGY correctly notes this risk and links it to the idempotency re-run scenario. No escalation needed.

Note: The RISK-TEST-STRATEGY does not include a scenario for VARIANCE-01 (the transaction boundary contradiction). This is expected — the risk strategy was authored alongside the architecture which has the correct design. If VARIANCE-01 is not resolved before delivery, R-03's "code review assertion: mark_signals_purged() must contain UPDATE and must NOT contain store_cycle_review" remains valid as a proxy check, but would not catch an implementation that attempts to honor the SPEC's "inside transaction" clause via a different wrong approach.

**Verdict: PASS.**

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision alignment patterns scope additions milestone discipline` (topic: vision, category: pattern) — found 3 prior patterns: #2298 (config key semantic divergence), #3742 (optional future branch in architecture must match scope intent), #3337 (architecture diagram headers diverge from spec — testers assert against wrong strings). Pattern #3337 directly informed the identification of VARIANCE-01: spec and architecture describing the same behaviour with contradictory detail is a documented recurring failure mode in this codebase.
- Stored: nothing novel to store — VARIANCE-01 is a specific instance of the already-stored pattern #3337 (spec-architecture divergence in procedural detail). The transaction-boundary contradiction is not a new pattern class; it is the same "architecture diagram informal headers diverge from spec" pattern applied to a method's transactional contract rather than a string label.
