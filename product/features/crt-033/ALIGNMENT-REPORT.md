# Alignment Report: crt-033

> Reviewed: 2026-03-29
> Artifacts reviewed:
>   - product/features/crt-033/architecture/ARCHITECTURE.md
>   - product/features/crt-033/specification/SPECIFICATION.md
>   - product/features/crt-033/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-033/SCOPE.md
> Scope risk: product/features/crt-033/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Idempotent retrospective directly supports the self-learning platform narrative |
| Milestone Fit | PASS | Cortical phase maintenance work; prerequisite for retention pass (#409) already in roadmap orbit |
| Scope Gaps | PASS | `query_log.feature_cycle` substitution (OQ-02) fully documented in spec and architecture; semantics shift acknowledged |
| Scope Additions | PASS | No unrequested capabilities added |
| Architecture Consistency | PASS | 7-touchpoint cascade table in architecture matches spec AC-02b exactly; all ADRs resolve scope risks |
| Risk Completeness | PASS | 13 risks, 39 scenarios; all scope risks traced; known historical traps (entry #3539, #2266, #2249) applied |

**Variances requiring approval: 0**
**FAIL: 0 | WARN: 0 | VARIANCE: 0**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `pending_cycle_reviews` query source | SCOPE references `query_log.feature_cycle` (column does not exist). Architecture (ADR-004, OQ-02 CLOSED) and spec (OQ-02 CLOSED, FR-09) adopt `cycle_events` with `event_type='cycle_start'` as the authoritative source. Semantics shift documented: "formally started" rather than "Unimatrix was queried." Both documents consistent; risk strategy addresses this as I-01. |
| Simplification | Schema cascade checklist | SCOPE cites 5 touchpoints; architecture enumerates 7 (adds `server.rs` assertions and previous migration test rename). Spec AC-02b explicitly lists the same 7 touchpoints, matching the architecture. Superset — more thorough than SCOPE requested. |
| Simplification | `raw_signals_available` field type | SCOPE domain model shows `raw_signals_available: bool`; SQL table uses `INTEGER NOT NULL DEFAULT 1`; spec notes "stored as INTEGER (1/0); mapped to bool in Rust." Delivery must confirm the sqlx binding. Edge case is called out in RISK-TEST-STRATEGY edge cases table. No functional gap. |

---

## Variances Requiring Approval

None. All checks PASS.

---

## Detailed Findings

### Vision Alignment

The product vision describes Unimatrix as a "self-learning knowledge integrity engine" where "nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage." The retrospective pipeline (`context_cycle_review`) is a first-class citizen of this engine: it converts session observations into actionable learning signals, hotspot findings, and lessons. crt-033 makes retrospective results durable and idempotent rather than ephemeral and non-deterministic. This is maintenance of foundational integrity, not speculative enhancement.

The vision's Intelligence & Confidence gap table includes "Intelligence pipeline is additive boosts, not a learned function" and calls out W3-1 as the path to a session-conditioned relevance function. W3-1 depends on quality behavioral signals from retrospective passes. Without durable review records, GH #409 cannot safely purge the raw signals that retrospectives consume — leaving the learning pipeline unable to manage its own storage. crt-033 closes this gap by providing the prerequisite gate.

The vision also emphasizes "tamper-evident from first write to last." Storing retrospective reviews in a structured indexed table (vs recomputing from raw signals that may have drifted) strengthens the integrity and reproducibility guarantees for the learning record.

**Conclusion**: PASS. The feature is foundational infrastructure for the intelligence pipeline's durability and the prerequisite for data retention (#409). It does not contradict any strategic direction.

---

### Milestone Fit

crt-033 is in the Cortical phase (learning and drift). It targets the retrospective memoization gap that is upstream of GH #409 (retention pass). The vision's roadmap places learning pipeline improvements in Wave 1 and Wave 1A (now largely complete: crt-031 category lifecycle, crt-032 w_coac reduction). crt-033 is a natural follow-on that completes the retrospective durability story — it does not build Wave 2 or Wave 3 capabilities, does not implement OAuth, containerization, or the GNN. The scope is appropriately bounded.

The 90-day K-window default (NFR-05) is coordinated with GH #409 which is described in the vision as intelligence-driven retention — a roadmapped item. The constant reconciliation dependency is acknowledged and documented.

**Conclusion**: PASS. Fits Cortical phase. No future milestone capabilities are pulled in ahead of schedule.

---

### Architecture Review

The architecture is well-structured with six modified/new components clearly enumerated. All four ADRs are present and make decisions on the scope risks flagged in SCOPE-RISK-ASSESSMENT:

- **ADR-001** (synchronous write): Addresses SR-06 and R-02/R-09. Correct: fire-and-forget would break the #409 gate contract.
- **ADR-002** (unified SUMMARY_SCHEMA_VERSION): Addresses SR-03. Accepted trade-off documented.
- **ADR-003** (direct serde, no DTO): Addresses SR-01. Justified by the serde audit table (23 types, all confirmed). Includes a defense-in-depth fallthrough on deserialization failure.
- **ADR-004** (K-window via `cycle_events.cycle_start`, 90-day default): Addresses SR-04 and OQ-02. The architecture's integration points table, SQL block, and open questions section all reference `cycle_events`; no residual `query_log.feature_cycle` reference remains.

The SR-07 discriminator (force=true path: use `get_cycle_review()` return value as the discriminator, not a COUNT query against `cycle_events`) is correctly placed in the handler control flow diagrams. Architecture OQ-01 and OQ-02 are both marked CLOSED.

The schema cascade table (7 touchpoints, rows 1-7) matches specification AC-02b (7 touchpoints, items 1-7) exactly. No discrepancy between architecture and spec on cascade scope.

The `CycleReviewRecord` field `raw_signals_available` appears as `i32` in the architecture's integration surface table (matching SQLite INTEGER semantics) and as `bool` in the spec's domain model. This delivery-time mapping concern is flagged in the risk strategy edge cases table and does not constitute a design gap.

**Conclusion**: PASS. Architecture resolves all scope risks with documented ADRs. OQ-02 resolved with `cycle_events` throughout. Seven cascade touchpoints match spec.

---

### Specification Review

The specification is thorough and disciplined. Observations:

1. **FR-09 and AC-09 correctly use `cycle_events` as the source for `pending_cycle_reviews`.** The SCOPE's Goals section (goal 6) uses `cycle_events` language; the spec's adoption of `cycle_events.cycle_start` with `event_type='cycle_start'` is consistent with that intent. OQ-02 in the spec documents the resolution and its semantic implications explicitly. The semantics shift (a cycle with a `cycle_start` but zero Unimatrix queries will appear as pending) is acknowledged in the spec, architecture, and risk strategy I-01.

2. **AC-02b enumerates 7 cascade touchpoints**, matching the architecture's 7-item cascade table. The previously noted discrepancy (5 vs 7) is resolved. Code review gate now covers all touchpoints including `server.rs` schema_version assertions (item 6) and previous migration test rename (item 7).

3. **NFR-03 (4MB ceiling)** is a spec addition over SCOPE. SCOPE says "estimated JSON size well under 1MB" and SR-02 rates the unbounded blob as Med/Low. The spec adds a hard 4MB ceiling enforced at the store layer. This is a purely defensive measure addressing a documented scope risk — no capability expansion.

4. **OQ-01 (ambiguous empty observations when no stored record exists)** is closed. The spec accepts `ERROR_NO_OBSERVATION_DATA` for both sub-cases when no stored record exists (FR-06). This is consistent with the architecture's handler control flow for the force=true + purged-signals path.

5. **Workflow 5 (GH #409 gate)** correctly describes the gate contract. The disambiguation that `raw_signals_available = 0` is NOT set by #409 (it is set when `force=true` is attempted post-purge) is accurate per FR-05.

6. **Failure mode for `get_cycle_review` read error** is described in the RISK-TEST-STRATEGY failure modes table ("Handler falls through to full computation — treat as a miss") but is not in the spec. This is an internal gap between risk strategy and spec. Delivery should confirm the intended behavior and add it to the spec if the graceful-miss path is required.

**Conclusion**: PASS. All substantive concerns from the prior review are resolved. The read-failure fallthrough gap is an internal spec/risk-strategy discrepancy to be resolved at delivery, not a scope or vision concern.

---

### Risk Strategy Review

The risk strategy is exemplary in traceability and coverage. All scope risks from SCOPE-RISK-ASSESSMENT are mapped in the Scope Risk Traceability table. Historical entries (#3539, #2266, #2249, #3619, #2125) are applied to elevate severity/likelihood of the relevant risks. The "Risks requiring new tests beyond the explicit ACs" callout at the bottom of the coverage summary is notable — it adds six scenarios not covered by explicit ACs (R-06 scenario 3, R-07 scenarios 3-6, R-08 scenario 3, R-09 scenario 1, R-11 scenarios 1-3, R-12 scenario 1). These are all meaningful edge cases.

The OQ-02 schema substitution is specifically addressed in Integration Risk I-01: "The specification substitutes `cycle_events` with `event_type='cycle_start'`... Testers must verify the `event_type = 'cycle_start'` filter is correctly applied and that the result set matches operator expectations." R-07 scenarios 1-6 provide the corresponding test coverage.

Security risks are addressed proportionately: the only caller-supplied inputs are `feature_cycle` (parameterized SQL, existing validation) and `force: bool` (no injection surface). The blast radius analysis for a compromised `cycle_review_index` (stale advice, not code execution) is correctly bounded.

Failure modes table addresses all key failure paths including the corruption/fallthrough path from ADR-003.

**Conclusion**: PASS. Risk strategy is thorough and historically grounded. No unresolved gaps.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found 3 entries:
  - #2298: config key semantic divergence pattern (applied: checked TOML/const semantics in ADRs — no mismatch found)
  - #3742: optional future branch in architecture must match scope intent (applied: checked K-window open question — correctly resolved in ADR-004, not left as unresolved VARIANCE)
  - #3337: architecture diagram informal headers diverge from spec — testers assert against wrong strings (applied: confirmed `pending_cycle_reviews` query source is now consistent between architecture SQL and spec SQL — both use `cycle_events.cycle_start`)
- Stored: nothing novel to store — the prior-round WARN on AC-02b touchpoint count mismatch (spec listed 5, architecture listed 7) was corrected in this revision; the pattern of "spec AC lists fewer cascade touchpoints than architecture" is adjacent to existing entry #3539 (schema cascade checklist) and does not add a new distinct pattern. The `query_log.feature_cycle` non-existence pattern (scope doc references non-existent column, spec substitutes a different source) recurred from the first round of review but was already noted as adjacent to #3337. No new cross-feature pattern warrants storage from this re-review.
