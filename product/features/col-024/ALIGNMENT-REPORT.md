# Alignment Report: col-024

> Reviewed: 2026-03-24
> Artifacts reviewed:
>   - product/features/col-024/architecture/ARCHITECTURE.md
>   - product/features/col-024/specification/SPECIFICATION.md
>   - product/features/col-024/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-024/SCOPE.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances the observability and self-learning pipeline central to the product vision |
| Milestone Fit | PASS | Belongs to the Wave 1 / Wave 1A observation pipeline work; no future-milestone capabilities introduced |
| Scope Gaps | PASS | All five SCOPE.md goals and all twelve acceptance criteria are addressed across the three source documents |
| Scope Additions | WARN | Architecture introduces two open questions that go beyond SCOPE.md without requesting approval |
| Architecture Consistency | PASS | Architecture is internally consistent with the spec and SCOPE.md constraints; ADRs are sound |
| Risk Completeness | PASS | Risk register covers all scope risks (SR-01 through SR-07), integration risks, edge cases, security risks, and failure modes at appropriate depth |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | SCOPE.md Goal 4 (add `load_cycle_observations` to trait) | Architecture correctly places the new method on `ObservationSource` in `unimatrix-observe`; spec FR-01 matches. No gap. |
| Simplification | SCOPE.md Non-Goal: no `ObservationRecord` changes | All three source documents confirm `ObservationRecord` type is unchanged; `topic_signal` is storage-level only. |
| Addition (open question) | ARCHITECTURE.md §Open Questions 1: optional `tracing::debug!` mismatch log | SCOPE.md and SPECIFICATION.md do not request a mismatch-detection log when extracted signal differs from registry feature. Architecture raises it as a decision for the implementation team. This is an in-scope clarification, not a scope addition, but it is unresolved. |
| Addition (open question) | ARCHITECTURE.md §Open Questions 2: distinguishing "no cycle_events" from "cycle_events but no match" in the fallback log | SCOPE.md does not request this distinction. Architecture suggests it would require an extra count-only query and recommends deferring. This is also an unresolved implementation-team decision. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications. See WARN detail below.

### WARN: Two open questions left unresolved in ARCHITECTURE.md

**What**: ARCHITECTURE.md §Open Questions lists two items left explicitly to "the implementation team":

1. Whether to add a `tracing::debug!` on signal mismatch (extracted signal != registry feature).
2. Whether to add a count-only query to distinguish "no cycle_events rows" from "cycle_events rows exist but no observation matched" in the fallback log.

**Why it matters**: These are low-risk implementation-time choices, not scope deviations. However, leaving them open at this stage means the implementation team has discretion to expand scope (adding the mismatch log or the count-only query) without a formal approval path. Item 2 specifically would introduce an additional SQL query on the hot fallback path, which is a performance consideration absent from SCOPE.md's NFR-03 and NFR-04.

**Recommendation**: Close both open questions before implementation begins. For item 1: the SCOPE.md §Resolved Design Decisions entry 4 explicitly states "not logged" — the architecture should inherit this decision and close OQ-01 as "do not add mismatch log." For item 2: ARCHITECTURE.md already recommends deferral; close OQ-02 as "deferred, no count-only query" to match SCOPE.md's scope boundary.

Neither item requires escalation if the implementation team agrees to close them consistent with SCOPE.md. Flag only if the team chooses to implement either addition.

---

## Detailed Findings

### Vision Alignment

The product vision (Wave 1 / Wave 1A) states that the observation pipeline is the behavioral signal collection layer, and that the retrospective tool (`context_cycle_review`) is central to workflow learning. The vision explicitly describes the problem col-024 addresses: session attribution that is unreliable due to asynchronous writes. The vision's emphasis on "session-conditioned self-improving relevance" depends on observation records being correctly attributed to cycles — col-024 closes a known attribution gap.

The `cycle_events`-first lookup path is a direct support for the vision's stated intelligence pipeline: correct attribution feeds the detection rules, which feed pattern storage, which feeds W3-1 training. The `topic_signal` enrichment ensures that observations written after `context_cycle(start)` — including those produced by the intelligence pipeline's own tool calls — carry the cycle attribution required for retrospective analysis.

No vision principles are contradicted. The feature does not reach into Wave 2 (deployment), Wave 3 (GNN training), or any other milestone's scope.

### Milestone Fit

col-024 falls within Wave 1 / Wave 1A. Specifically:

- The `cycle_events` schema (schema v15) was introduced in WA-1 (`crt-025`, PR #338). col-024 consumes that schema as a read-only dependency.
- WA-1 introduced `current_phase` in `SessionState`. col-024 introduces `feature` on the same `SessionState`. These are parallel additions at the same architectural level.
- The feature explicitly states "No schema migration is needed" (ARCHITECTURE.md §Schema Dependencies; SPECIFICATION.md NFR-02) — schema v15 is already in place.

There is no future-milestone scope introduced. The open-ended window behavior documented in ADR-005 and NFR-03 is consistent with the vision's "best-effort attribution for abandoned cycles" acceptance. No Wave 2+ capabilities (HTTP transport, OAuth, GGUF, GNN training) are referenced or implied.

### Architecture Review

The architecture is well-structured and internally consistent:

- The three-component breakdown (trait addition, `SqlObservationSource` implementation, `context_cycle_review` handler restructuring) maps cleanly to SCOPE.md's five goals.
- The `enrich_topic_signal` free function (ADR-004) correctly addresses SCOPE.md SR-05 (per-site enrichment drift risk) and is more disciplined than inline enrichment at each call site.
- The named unit-conversion helper `cycle_ts_to_obs_millis` (ADR-002) directly addresses SCOPE.md SR-01 (timestamp unit mismatch) and the "raw `* 1000` forbidden" rule is stated explicitly.
- The single-`block_sync`-entry pattern (ADR-001) correctly addresses SCOPE.md SR-02 (blocking async runtime with multi-step loop).
- The structured fallback log (ADR-003) directly addresses SCOPE.md SR-06 (silent enrichment gap).
- ADR-005 (open-ended window at `unix_now_secs()`, no max-age cap) matches SCOPE.md's §Resolved Design Decisions item 2 precisely.

The architecture's integration surface table (§Integration Surface) duplicates the new-surface table (§New surface introduced). This is a minor documentation inconsistency, not a functional issue.

**Potential ARCHITECTURE.md self-inconsistency**: §Open Questions item 2 states that distinguishing the two empty-result cases "requires a count-only query on `cycle_events` before loading; defer unless operational evidence warrants it." However, SCOPE.md §Assumptions already acknowledges the ambiguity ("empty result from `load_cycle_observations` reliably signals no cycle_events rows" is listed as an assumption). The SCOPE.md scoping of the fallback log (SR-06 recommendation: "add a structured log event when the primary path returns empty") matches ADR-003, and does not request the count-only query distinction. The architecture's deferral recommendation is correct; the open question should be closed rather than left open.

### Specification Review

The specification covers all fourteen acceptance criteria from SCOPE.md with no gaps. The functional requirements (FR-01 through FR-15) map one-to-one with SCOPE.md Goals 1–5 and the acceptance criteria.

Noteworthy specification quality:

- FR-06 (empty-on-no-cycle-events semantics returning `Ok(vec![])` not `Err`) closes FM-01 precisely and is verified by AC-03.
- FR-13 and FR-14 together close the enrichment contract: best-effort (FR-13) and non-override (FR-14). These match SCOPE.md §Resolved Design Decisions items 3 and 4.
- NFR-03 includes the volume threshold trigger ("revisit at 20 K rows per window") that SCOPE.md SR-07 specifically requested.
- The domain model section clearly documents the timestamp unit semantics (`cycle_events.timestamp` = seconds; `observations.ts_millis` = milliseconds), reducing implementation misread risk.

SPECIFICATION.md §Open Questions OQ-01 asks the architect whether a shared helper or inline enrichment is preferred. ARCHITECTURE.md ADR-004 answers this by choosing the shared `enrich_topic_signal` helper. OQ-01 is therefore resolved by the architecture; the spec could note this is answered rather than leaving it as an open question.

SPECIFICATION.md §Open Questions OQ-02 asks the architect whether the single `block_sync` envelope is the right boundary. ARCHITECTURE.md ADR-001 answers this by mandating the single block entry. OQ-02 is therefore also resolved. Both spec open questions are answered; no material gap.

AC-13 (no raw `* 1000` literals) is a code-review acceptance criterion rather than a testable behavior. It appears correctly in the spec and risk strategy. Its verification method ("Code review: search for raw `* 1000` in the new implementation") is appropriate for a naming-convention constraint.

### Risk Strategy Review

The risk-test strategy is comprehensive. All seven scope risks (SR-01 through SR-07) from SCOPE-RISK-ASSESSMENT.md appear in the traceability table at the bottom of RISK-TEST-STRATEGY.md, with architecture resolutions cited. No scope risk is unaddressed.

Notable strengths:

- R-01 (timestamp unit) and R-02 (missing enrichment site) are correctly elevated to Critical priority, matching the severity of their potential impact (silent attribution failures affecting all post-col-024 features).
- R-05 (double `block_sync`) cites historical evidence (#735, #1688) grounding the severity in actual project failures — this is exactly the kind of historical signal that elevates a medium-severity risk to High priority.
- FM-01 (error vs `Ok(vec![])`) correctly specifies that the legacy fallback must NOT activate on a SQL error — only on `Ok(vec![])`. This is a precise behavioral contract that prevents a whole class of silent fallback activations on unexpected errors.
- E-02 (`cycle_phase_end` rows) and E-03 (multiple `cycle_start` without intervening `cycle_stop`) identify genuine edge cases in the pairing algorithm that the specification does not fully specify. The risk strategy correctly flags E-03 as requiring defined behavior and a test, which is appropriate given the SCOPE.md §Background Research section describes `cycle_start` / `cycle_stop` pairing without specifying the malformed-sequence case.

One minor gap: E-03 (malformed event log with two consecutive `cycle_start` rows) is flagged as requiring defined behavior, but neither ARCHITECTURE.md nor SPECIFICATION.md defines that behavior. This is a documentation gap, not a functional risk, since the implementation team will be forced to make a choice. Recommend the implementation team document the chosen behavior (e.g., "treat second start as a new open window; pair with next stop") in a code comment citing E-03.

S-04 (`block_sync` holding write pool for three SQL steps) is flagged as a known limitation with no required code change. This is consistent with SCOPE.md's §Constraints entry for `ObservationSource` sync trait. The write pool `max_connections=1` concern is real; the characterization as "infrequent call" is accurate for `context_cycle_review`. No action required.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- found #2298 (config key semantic divergence, dsn-001), #2063 (single-file topology vs vision language, nxs-011), #3337 (architecture diagram headers diverge from spec, crt-028). The pattern in #3337 (architecture informal headers diverging from spec) was checked: no such divergence found in col-024. All other patterns were not directly applicable to this feature type.
- Stored: nothing novel to store -- the variances in col-024 are feature-specific (two unresolved open questions in ARCHITECTURE.md that should be closed before implementation). The observation that "open questions in architecture docs should be closed before the implementation gate" is an existing convention; no new pattern to store.
