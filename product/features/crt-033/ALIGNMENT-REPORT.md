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
| Scope Gaps | WARN | FR-09 / AC-09 diverge from SCOPE's `query_log.feature_cycle` source; substitution is sound but undisclosed to user |
| Scope Additions | PASS | No unrequested capabilities added |
| Architecture Consistency | PASS | Architecture correctly resolves all scope risks; ADRs present and internally consistent |
| Risk Completeness | PASS | 13 risks, 39 scenarios; all scope risks traced; known historical traps (entry #3539, #2266, #2249) applied |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `pending_cycle_reviews` query source | SCOPE specifies `query_log.feature_cycle` (OQ-02 in spec); inspection reveals this column does not exist. Spec substitutes `cycle_events` with `event_type='cycle_start'`. Semantics change: "Unimatrix was queried during this cycle" → "a cycle_start event was recorded." Rationale is documented in the spec (OQ-02) and risk strategy (I-01). Acceptable, but changes observable behavior. |
| Simplification | Schema cascade checklist | SCOPE cites 5 touchpoints; architecture enumerates 7 (includes `server.rs` assertions and previous migration test rename). Superset — more thorough than SCOPE requested. |
| Simplification | `raw_signals_available` field type | SCOPE domain model shows `raw_signals_available: bool`; SQL table uses `INTEGER NOT NULL DEFAULT 1`; spec notes "stored as INTEGER (1/0); mapped to bool in Rust." Architecture's `CycleReviewRecord` shows `raw_signals_available: i32`. Spec shows `bool`. Delivery must reconcile sqlx type mapping — the edge case is called out in RISK-TEST-STRATEGY edge cases table. No functional gap. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications. One WARN is noted for human awareness.

---

## Detailed Findings

### Vision Alignment

The product vision describes Unimatrix as a "self-learning knowledge integrity engine" where "nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage." The retrospective pipeline (`context_cycle_review`) is a first-class citizen of this engine: it converts session observations into actionable learning signals, hotspot findings, and lessons. crt-033 makes retrospective results durable and idempotent rather than ephemeral and non-deterministic. This is maintenance of foundational integrity, not speculative enhancement.

The vision's Intelligence & Confidence gap table includes "Intelligence pipeline is additive boosts, not a learned function" and calls out W3-1 as the path to a session-conditioned relevance function. W3-1 depends on quality behavioral signals from retrospective passes. Without durable review records, GH #409 cannot safely purge the raw signals that retrospectives consume — leaving the learning pipeline unable to manage its own storage. crt-033 closes this gap by providing the prerequisite gate.

The vision also emphasizes "tamper-evident from first write to last." Storing retrospective reviews in a structured indexed table (vs recomputing from raw signals that may have drifted) strengthens the integrity and reproducibility guarantees for the learning record.

**Conclusion**: PASS. The feature is foundational infrastructure for the intelligence pipeline's durability and the prerequisite for data retention (#409). It does not contradict any strategic direction.

---

### Milestone Fit

crt-033 is in the Cortical phase (learning and drift). It targets the retrospective memoization gap that is upstream of GH #409 (retention pass). The vision's roadmap places the learning pipeline improvements in Wave 1 and Wave 1A (now largely complete based on the recent commits: crt-031 category lifecycle, crt-032 w_coac reduction). crt-033 is a natural follow-on that completes the retrospective durability story — it does not build Wave 2 or Wave 3 capabilities, does not implement OAuth, containerization, or the GNN. The scope is appropriately bounded.

The 90-day K-window default (NFR-05) is coordinated with GH #409 which is described in the vision as intelligence-driven retention — a roadmapped item. The constant reconciliation dependency is acknowledged and documented.

**Conclusion**: PASS. Fits Cortical phase. No future milestone capabilities are pulled in ahead of schedule.

---

### Architecture Review

The architecture is well-structured with six modified/new components clearly enumerated. All four ADRs are present and make decisions on the four scope risks flagged in SCOPE-RISK-ASSESSMENT (SR-01 through SR-07):

- **ADR-001** (synchronous write): Addresses SR-06 and R-02/R-09. Correct: fire-and-forget would break the #409 gate contract.
- **ADR-002** (unified SUMMARY_SCHEMA_VERSION): Addresses SR-03. Accepted trade-off documented.
- **ADR-003** (direct serde, no DTO): Addresses SR-01. Justified by the serde audit table (23 types, all confirmed). The ADR includes a defense-in-depth fallthrough on deserialization failure — important for production resilience.
- **ADR-004** (K-window 90-day default): Addresses SR-04. Pins the constant at `PENDING_REVIEWS_K_WINDOW_DAYS` in `services/status.rs`, not inlined.

The SR-07 discriminator (COUNT query against `cycle_events` to distinguish "purged" from "never existed") is correctly placed in the handler flow and does not reuse the empty-attributed-observations result alone. The architecture control flow diagrams are detailed and internally consistent.

The schema cascade table (7 touchpoints) is more thorough than SCOPE.md's 5-touchpoint description — the architecture adds `server.rs` schema_version assertions (touchpoint 5) and previous migration test rename (touchpoint 6). This is additive and desirable.

One minor structural note: the architecture's `CycleReviewRecord` shows `raw_signals_available: i32` (matching SQLite INTEGER semantics), while the specification's domain model shows `raw_signals_available: bool`. This is a field-type ambiguity that delivery must resolve consistently with sqlx's mapping conventions. The RISK-TEST-STRATEGY correctly identifies this as an edge case (Edge Cases table: "`raw_signals_available` mapping: SQLite INTEGER 0/1 → Rust bool"). No gap in awareness — but no definitive resolution is stated.

**Conclusion**: PASS. Architecture resolves all scope risks with documented ADRs. The i32/bool type ambiguity is flagged in the risk strategy and does not constitute a design gap; delivery must confirm the sqlx binding.

---

### Specification Review

The specification is thorough and disciplined. Observations:

1. **FR-09 diverges from SCOPE's `query_log.feature_cycle` source.** SCOPE states: "Add `pending_cycle_reviews: Vec<String>` to `StatusReport`: cycles that have `cycle_events` rows (raw signals exist) but no `CYCLE_REVIEW_INDEX` row." The SCOPE also references `query_log.feature_cycle` as the column for the K-window pending set query. The specification discovered that `query_log.feature_cycle` does not exist (OQ-02), and substitutes `cycle_events` with `event_type='cycle_start'` as the source of truth. This is documented in the spec as OQ-02 and in the risk strategy as I-01. The SCOPE's Goals section (goal 6) uses `cycle_events` language, so the substitution is consistent with the user's original intent — but the SCOPE's Background Research section (SQL block) references `query_log.feature_cycle`. This is a WARN because the user should be aware of the substitution and its semantic implications (a cycle with a `cycle_start` but zero Unimatrix queries will appear as pending).

2. **NFR-03 (4MB ceiling) is a spec addition over SCOPE.** SCOPE says "estimated JSON size well under 1MB" and SR-02 rates the unbounded blob as Med/Low. The spec adds a hard 4MB ceiling enforced at the store layer. This is a scope addition in the narrow sense (SCOPE had no size enforcement, spec adds one), but it is a purely defensive measure that addresses a documented scope risk. No user approval is needed — it reduces risk without expanding capability.

3. **OQ-01 (ambiguous empty observations when no stored record exists)** is correctly surfaced as an open question. The spec accepts `ERROR_NO_OBSERVATION_DATA` for both sub-cases when no stored record exists. The architecture adds the `cycle_events` COUNT discriminator for the `force=true` path (SR-07 resolution), but when no stored record exists and observations are empty, the discriminator is described in the architecture's force=true flow but FR-06 in the spec does not reference it. The spec's FR-05 SR-07 contract describes using the stored record's `raw_signals_available` field — but when there is no stored record, the discriminator is needed to determine whether `ERROR_NO_OBSERVATION_DATA` is the right response or whether a `cycle_start` event should be flagged. The risk strategy's R-04 scenario 4 says "assert the handler queries `cycle_events` to distinguish purged vs never-existed before checking `cycle_review_index`." There is a minor tension between the spec (which says `ERROR_NO_OBSERVATION_DATA` when `get_cycle_review()` returns `None`, FR-06) and the risk strategy (which implies the discriminator runs before `get_cycle_review`). This does not constitute a VARIANCE — both documents are internally consistent if the discriminator is run first and the stored-record lookup is secondary — but delivery should confirm the exact execution order.

4. **Workflow 5 (GH #409 gate)** correctly describes the gate contract. The note "writes `raw_signals_available = 0` is NOT done by #409 — that is set when `force=true` is attempted post-purge" is accurate per FR-05. This is a useful disambiguation for the #409 author.

5. **AC-02b lists 5 touchpoints** while the architecture cascade table lists 7. The spec's AC-02b is the gate check — if it only checks 5, two touchpoints (`server.rs` assertions and previous migration test rename) may not be caught by AC-02b's code review gate. The risk strategy's R-01 correctly references all 7 touchpoints from the architecture. The AC-02b undercount is a WARN: delivery teams following only the spec's AC-02b could miss touchpoints 5 and 6 from the architecture.

**Conclusion**: WARN on two items — (a) the `query_log.feature_cycle` → `cycle_events` substitution changes observable semantics and should be acknowledged by the user; (b) AC-02b enumerates 5 cascade touchpoints but the architecture documents 7 — delivery should use the architecture's 7-touchpoint list as the authoritative cascade checklist.

---

### Risk Strategy Review

The risk strategy is exemplary in traceability and coverage. All scope risks from SCOPE-RISK-ASSESSMENT are mapped in the Scope Risk Traceability table. Historical entries (#3539, #2266, #2249, #3619, #2125) are applied to elevate severity/likelihood of the relevant risks. The "Risks requiring new tests beyond the explicit ACs" callout at the bottom of the coverage summary is notable — it adds six scenarios not covered by explicit ACs (R-06 scenario 3, R-07 scenarios 3-6, R-08 scenario 3, R-09 scenario 1, R-11 scenarios 1-3, R-12 scenario 1). These are all meaningful edge cases.

Security risks are addressed proportionately: the only caller-supplied inputs are `feature_cycle` (parameterized SQL, existing validation) and `force: bool` (no injection surface). The blast radius analysis for a compromised `cycle_review_index` (stale advice, not code execution) is correctly bounded.

Failure modes table addresses all key failure paths including the corruption/fallthrough path from ADR-003.

One gap: the failure mode for `get_cycle_review` read failure is described in the failure modes table ("Handler falls through to full computation — treat as a miss") but this behavior is not in the spec. FR-01 says "If a row exists, the handler returns the stored record immediately" — it does not specify what happens when the read itself errors. The failure mode table adds a non-trivial recovery path (graceful miss) that the spec does not require. This is a gap between the risk strategy and the spec, not a gap with the scope. Delivery should confirm whether read failure → graceful miss is the intended behavior and add it to the spec if so.

**Conclusion**: PASS. Risk strategy is thorough and historically grounded. The unspecified read-failure fallthrough is an internal spec/risk-strategy gap that delivery should resolve.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found 3 entries:
  - #2298: config key semantic divergence pattern (applied: checked TOML/const semantics in ADRs — no mismatch found)
  - #3742: optional future branch in architecture must match scope intent (applied: checked the K-window open question — correctly deferred to delivery, not left as an unresolved VARIANCE)
  - #3337: architecture diagram informal headers diverge from spec — testers assert against wrong strings (applied: checked `pending_cycle_reviews` query source discrepancy between architecture SQL and spec SQL — both use `cycle_events`; consistent)
- Stored: nothing novel to store — the `query_log.feature_cycle` column substitution (scope doc references non-existent column, spec silently substitutes a different source) is a one-off discovery; the general pattern of "spec researchers should verify column existence before referencing in scope SQL examples" is adjacent to entry #3337 and not distinct enough to warrant a new entry. If this same pattern recurs in a subsequent feature, entry should be stored then.
