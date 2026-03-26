# Alignment Report: col-028

> Reviewed: 2026-03-26
> Artifacts reviewed:
>   - product/features/col-028/architecture/ARCHITECTURE.md
>   - product/features/col-028/specification/SPECIFICATION.md
>   - product/features/col-028/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-028/SCOPE.md
> Scope risk assessment: product/features/col-028/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly enables Wave 1A session-conditioned intelligence pipeline |
| Milestone Fit | PASS | Correctly targeted at Wave 1A pre-requisite infrastructure; blocked consumers named |
| Scope Gaps | PASS | All 20 SCOPE.md acceptance criteria addressed across source documents |
| Scope Additions | WARN | SPECIFICATION.md adds AC-21 through AC-24 beyond the 20 in SCOPE.md; rationale is sound but additions were not in SCOPE.md |
| Architecture Consistency | PASS | Six components coherent; ADR decisions consistent with codebase patterns |
| Risk Completeness | PASS | 16 risk items, integration risks, edge cases, security risks, and failure modes all covered |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | AC-21 (SR-01 atomic change surface) | In SPECIFICATION.md §AC-21 but not in SCOPE.md. Addresses SR-01 from SCOPE-RISK-ASSESSMENT.md — treating analytics.rs INSERT, both SELECTs, and row_to_query_log as an atomic commit. Rationale is sound (silent runtime error risk), but was not in SCOPE.md's 20 criteria. |
| Addition | AC-22 (SR-02 migration test cascade) | In SPECIFICATION.md §AC-22 but not in SCOPE.md. Addresses SR-02. Enumeration of affected migration test files and the mandatory pre-gate grep check. Sound scope extension responding to identified risk. |
| Addition | AC-23 (SR-03 UDS compile fix) | In SPECIFICATION.md §AC-23 but not in SCOPE.md. Addresses SR-03. One-line compile-fix at uds/listener.rs:1324 required by the QueryLogRecord::new signature change. A necessary consequence of the feature — the workspace cannot compile without it. |
| Addition | AC-24 (SR-04 confirmed_entries contract doc) | In SPECIFICATION.md §AC-24 but not in SCOPE.md. Addresses SR-04. Requires a doc comment on the confirmed_entries field documenting the explicit-fetch-only semantic contract. No code change; documentation obligation. |
| Simplification | None identified | All non-goals in SCOPE.md are respected and explicitly echoed in specification §NOT in Scope. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications were found. The single WARN item is documented below for human awareness but does not block delivery.

### WARN: AC-21 through AC-24 added by specification beyond SCOPE.md

1. **What**: SPECIFICATION.md defines four acceptance criteria (AC-21–AC-24) that are not present in SCOPE.md's acceptance criteria list (AC-01–AC-20). These were introduced in response to scope risks SR-01 through SR-04 identified in SCOPE-RISK-ASSESSMENT.md.

2. **Why it matters**: SCOPE.md is the authoritative statement of what was asked for. Additions to AC scope, even well-reasoned ones, require acknowledgment. In this case the additions are:
   - AC-21: atomicity obligation for the four analytics.rs/query_log.rs sites (enforcement of SR-01 risk)
   - AC-22: pre-gate grep check for stale schema_version == 16 assertions (enforcement of SR-02 risk)
   - AC-23: UDS compile fix (mandatory consequence of FR-15 signature change — the workspace cannot compile without it)
   - AC-24: doc comment on confirmed_entries (documentation of semantic contract per SR-04)

3. **Recommendation**: Accept all four additions. AC-23 is not truly additive — it is a mandatory compile fix that follows mechanically from the signature change scoped in SCOPE.md FR-15 (QueryLogRecord::new gains a phase parameter). AC-21, AC-22, and AC-24 are risk mitigations responding to risks identified in SCOPE-RISK-ASSESSMENT.md and add no new behavior. The SCOPE-RISK-ASSESSMENT.md explicitly recommended that the specification enumerate these obligations, which the design team acted on correctly. No human resolution required unless the product owner wishes to formally amend SCOPE.md to include them.

---

## Detailed Findings

### Vision Alignment

col-028 directly serves the Wave 1A session-conditioned intelligence pipeline described in the product vision under "WA-0 comes first" and the subsequent Wave 1A section. The vision states:

> "The intelligence pipeline cannot learn from usage it cannot observe, cannot predict what agents need without knowing where they are in the cycle."

col-028 resolves the read-side phase blindness that prevents the phase-conditioned frequency table (ass-032 Loop 2), Thompson Sampling per-(phase, entry) arms, and gap detection from functioning. These are explicitly named as downstream consumers in the vision's session context vector:

> `[current_phase, ← WA-1 explicit signal`

The feature also corrects `context_briefing`'s access_weight (1→0) and `context_get`'s weight (1→2), which directly improves the quality of the confidence scoring inputs that flow into the vision's six-term fused linear ranking formula (WA-0, crt-024). The vision's self-improving relevance function requires clean signal inputs; this feature removes two known distortions.

The `confirmed_entries` field (no consumer in this feature) is specifically positioned as infrastructure for Thompson Sampling (W3-1 prerequisite). The vision explicitly notes that "past sessions cannot be retroactively reconstructed" — adding the field now rather than when Thompson Sampling ships is the correct timing. This is milestone discipline, not scope creep.

PASS — no misalignment with product vision.

### Milestone Fit

The feature is positioned within Wave 1A, post-WA-1 (crt-025, COMPLETE) and pre-ass-032/Thompson Sampling. The dependency chain in all three source documents is internally consistent:

- crt-025 (WA-1) introduced `SessionState.current_phase` and the phase-snapshot pattern (ADR-001, pattern #3027)
- col-028 extends that pattern to four read-side tools and persists it to query_log
- ass-032 and Thompson Sampling are listed as downstream consumers that are explicitly out of scope

The feature correctly defers all downstream consumers (phase-conditioned frequency table, Thompson Sampling, gap detection) to separate features, consistent with milestone discipline. The vision roadmap places these consumers in Wave 1A and Wave 3 (W3-1 GNN) — building their infrastructure now rather than deferring `confirmed_entries` until Thompson Sampling is appropriate.

PASS — no milestone boundary violations.

### Architecture Review

The architecture document defines six components with clear responsibilities:

1. **SessionState** (infra/session.rs) — adds `confirmed_entries: HashSet<u64>`. Follows the identical pattern as `signaled_entries`. Consistent with the SessionState field extension pattern (#3180).

2. **Phase Helper Free Function** (mcp/tools.rs) — `current_phase_for_session` as a module-level free function. Consistent with ADR-001 (crt-025), which established the phase snapshot contract. The free function approach (D-04) is explicitly justified as testable without handler construction.

3. **Four Read-Side Call Sites** (mcp/tools.rs) — Phase capture added to all four handlers. The weight table (search: 1 unchanged, lookup: 2 unchanged, get: 1→2, briefing: 1→0) is internally consistent across architecture, specification, and SCOPE.md. The phase-snapshot-before-await contract is correctly identified as a code review gate (not automatable).

4. **D-01 Guard** (services/usage.rs) — Guard placement in `record_briefing_usage` before `filter_access` is the correct location per ADR-003 analysis of the shared `UsageDedup.access_counted` HashSet. The architecture documents SR-07 (future bypass risk) and correctly defers any architectural restructuring of the guard to a separate ADR review.

5. **Schema Migration v16→v17** (unimatrix-store) — All eight atomic change items listed. The `pragma_table_info` pre-check follows the established pattern (v7→v8, v13→v14, v14→v15, v15→v16). Column appended as last positional parameter (?9) to preserve existing bind indices.

6. **MCP context_search query_log Write Site** — Single `get_state` call shared between `UsageContext.current_phase` and `QueryLogRecord.phase` (SR-06 mitigation). The UDS call site is correctly identified as a compile-fix-only obligation.

Component interaction diagram in ARCHITECTURE.md is complete and consistent with the specification's domain models. Integration surface table provides exact type signatures for all changed public interfaces.

PASS — architecture is internally consistent and aligned with established codebase patterns.

### Specification Review

SPECIFICATION.md provides 24 acceptance criteria (AC-01 through AC-24), 8 non-functional requirements (NFR-01 through NFR-06, plus NFR-01 consolidating two concerns), and 8 constraints (C-01 through C-08). All 20 SCOPE.md acceptance criteria map 1:1 to their specification counterparts with identical text ("AC IDs are stable across design, delivery, and test phases. Criteria AC-01 through AC-20 are carried forward unchanged from SCOPE.md").

The specification provides exact code signatures verbatim for all load-bearing declarations:
- `current_phase_for_session` function body
- `SessionState.confirmed_entries` field declaration with doc comment
- D-01 guard code block
- `QueryLogRecord` constructor signature
- Migration SQL block (v16→v17)
- analytics.rs INSERT statement
- query_log.rs SELECT statement and row_to_query_log deserializer

Five user workflows cover all primary execution paths including the D-01 guard scenario (briefing → context_get on same entry), the multi-target lookup cardinality boundary, and the migration path.

The §NOT in Scope section explicitly echoes SCOPE.md's non-goals with no omissions, including the scoring pipeline freeze, UDS semantic exclusion, and confirmed_entries consumer deferral.

One minor observation: SPECIFICATION.md §FR-02 provides the function body as pseudocode (`returns registry.get_state(session_id?)?.current_phase.clone()`) but uses Rust question-mark syntax which is not valid in a free function returning `Option<String>` (it would need `and_then` chaining). The §Exact Signatures section later provides the correct Rust code using `session_id.and_then(|sid| registry.get_state(sid)).and_then(|s| s.current_phase.clone())`. This is self-correcting within the document and not a delivery risk, but delivery should follow the §Exact Signatures version.

PASS — specification is complete and internally consistent. The FR-02 pseudocode/exact-code discrepancy is noted as low-severity; delivery should use the §Exact Signatures version.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md covers all 16 identified risks with:
- Severity and likelihood ratings for each
- Concrete test scenarios mapped to specific ACs
- Coverage requirements distinguishing automated tests from code review gates

The three Critical risks (R-01 dedup collision, R-02 positional column drift, R-03 phase snapshot race) each have an explicit minimum gate requirement:
- R-01: AC-07 integration test with a negative arm confirming the guard is load-bearing
- R-02: AC-17 round-trip read-back through the real analytics drain (not a stub)
- R-03: AC-12 code review gate plus unit tests for the free function in isolation

Four integration risks (IR-01 through IR-04) address delivery ordering (Part 2 schema changes must land before or with Part 1 handler changes), the analytics drain async gap (flush required in causal tests), and two test helper surface areas (eval/scenarios/tests.rs 15+ sites, knowledge_reuse.rs make_query_log).

Security risks (SR-SEC-01 through SR-SEC-04) correctly identify that phase values flow through SQLx parameterized binding with no injection risk, that no new untrusted input surface is introduced, and that the blast radius is analytics data corruption only — not authentication, deletion, or secret access.

The §Coverage Summary identifies the minimum five gate conditions: AC-07, AC-17, AC-22 grep check, AC-23 compile, and cargo test --workspace green. This is an appropriate triage for a feature of this scope.

Knowledge stewardship section documents all Unimatrix queries made during design and confirms that patterns #3503, #3510, #2933, and #3004 were found and applied but not re-stored (no novel patterns to add).

PASS — risk strategy is thorough, traceable to acceptance criteria, and proportional to the feature's scope and complexity.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for `vision alignment review scope variance misalignment pattern` — found entries #2298 (config key semantic divergence, dsn-001), #181 (unrelated), #3426 (formatter regression risk). No recurring vision misalignment patterns applicable to col-028.
- Stored: nothing novel to store — col-028 variances are feature-specific (AC scope extensions responding to identified SCOPE-RISK-ASSESSMENT.md risks). The pattern of specification adding ACs beyond SCOPE.md in response to a SCOPE-RISK-ASSESSMENT.md is not yet a recurring cross-feature pattern in the knowledge base; if it recurs in two more features, it warrants storing as a pattern under topic `vision`.
