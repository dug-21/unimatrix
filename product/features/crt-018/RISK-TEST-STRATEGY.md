# Risk-Based Test Strategy: crt-018

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Classification priority logic misorders categories, causing entries to receive wrong classification when matching multiple criteria (e.g., auto entry with low success AND zero helpful_count could be Noisy or Ineffective) | High | High | Critical |
| R-02 | NULL/empty topic on entries or NULL feature_cycle on sessions causes silent misclassification — entries dropped from analysis or assigned wrong topic activity status | High | High | Critical |
| R-03 | SQL JOIN between injection_log and sessions produces incorrect aggregation when an entry is injected multiple times into the same session (COUNT vs COUNT DISTINCT mismatch) | High | Med | High |
| R-04 | Calibration bucket boundary handling — confidence=0.1 assigned to wrong bucket due to inclusive/exclusive bound logic (lower inclusive, upper exclusive, except 0.9-1.0 inclusive both) | Med | High | High |
| R-05 | Division by zero in utility_score when total outcome count is zero (all sessions have NULL outcome, or entry has injections but joined sessions all lack outcomes) | High | Med | High |
| R-06 | compute_effectiveness_aggregates exceeds 500ms budget at scale due to full-table scan on calibration query (Query 3: no GROUP BY, returns all injection_log x sessions rows) | Med | Med | Medium |
| R-07 | Session GC deletes injection_log rows mid-computation, causing inconsistent state between the four SQL queries within compute_effectiveness_aggregates | Med | Low | Medium |
| R-08 | StatusReport effectiveness field breaks existing JSON consumers when present (new field in serialized output even with skip_serializing_if, if consumers use strict parsing) | Med | Med | Medium |
| R-09 | Settled classification incorrectly applied — entry has no sessions for its topic in available window, but also has no historical success injection (should NOT be Settled, but if the condition check is wrong it becomes Settled instead of Unmatched) | Med | Med | Medium |
| R-10 | Noisy classification leaks to non-auto trust sources if NOISY_TRUST_SOURCES constant is misconfigured or .contains() comparison is case-sensitive while trust_source storage is inconsistent | Low | Med | Low |
| R-11 | spawn_blocking task panic in Phase 8 crashes the entire compute_report or leaves effectiveness as uninitialized value instead of None | High | Low | Medium |
| R-12 | Markdown/summary format output garbles existing sections when effectiveness data contains unexpected characters in entry titles (e.g., pipe characters breaking markdown tables) | Low | Med | Low |
| R-13 | Aggregate utility ratio in SourceEffectiveness produces NaN or infinity when a trust source has entries but zero total injection sessions | Med | Med | Medium |

## Risk-to-Scenario Mapping

### R-01: Classification Priority Ordering
**Severity**: High
**Likelihood**: High
**Impact**: Entries receive wrong category labels, leading to incorrect effectiveness reports. An auto-extracted entry with 5 failed injections and 0 helpful_count should be Noisy (highest priority), not Ineffective.

**Test Scenarios**:
1. Entry matching both Noisy and Ineffective criteria (auto trust, 0 helpful, >= 3 injections, < 30% success) — must classify as Noisy
2. Entry matching both Unmatched and Settled criteria (zero injections, topic inactive) — must classify as Settled (has historical success)
3. Entry matching no negative criteria (injected, > 30% success, non-auto trust) — must default to Effective
4. Entry with exactly INEFFECTIVE_MIN_INJECTIONS (3) and exactly 30% success rate — boundary: should NOT be Ineffective (< 30% required)
5. Entry with 3 injections and 29.9% success rate — boundary: should be Ineffective

**Coverage Requirement**: Every pairwise combination of overlapping categories tested. Priority order Noisy > Ineffective > Unmatched > Settled > Effective verified with entries that match multiple criteria simultaneously.

### R-02: NULL Topic/Feature Cycle Misclassification
**Severity**: High
**Likelihood**: High
**Impact**: Entries with NULL topic silently disappear from classification, or sessions with NULL feature_cycle incorrectly influence topic activity. Historical precedent: Unimatrix #756, #981 — NULL feature_cycle caused silent downstream failures in retrospective pipeline.

**Test Scenarios**:
1. Entry with NULL topic — must be classified with topic "(unattributed)", not dropped
2. Entry with empty string topic — must be treated same as NULL (mapped to "(unattributed)")
3. Session with NULL feature_cycle — must be excluded from active_topics set but included in injection outcome JOIN
4. Session with empty string feature_cycle — same handling as NULL
5. Entry with "(unattributed)" topic and no matching sessions — must classify as Settled or Unmatched based on injection history

**Coverage Requirement**: Full NULL/empty matrix for both entries.topic and sessions.feature_cycle, with injection_log rows connecting them.

### R-03: COUNT vs COUNT DISTINCT Session Aggregation
**Severity**: High
**Likelihood**: Med
**Impact**: If an entry is injected 3 times into the same session (e.g., via multiple search calls), COUNT without DISTINCT inflates injection_count to 3. This could push an entry past INEFFECTIVE_MIN_INJECTIONS threshold incorrectly, or inflate calibration bucket counts.

**Test Scenarios**:
1. Entry injected 3 times into 1 session — injection_count must be 1 (distinct sessions), not 3
2. Entry injected once each into 3 different sessions — injection_count must be 3
3. Calibration query: multiple injections in same session should each produce a calibration row (confidence may differ per injection within same session)

**Coverage Requirement**: Store integration test with duplicate injection_log rows for same (entry_id, session_id) pair.

### R-04: Calibration Bucket Boundary Handling
**Severity**: Med
**Likelihood**: High
**Impact**: Confidence values at bucket boundaries (0.1, 0.2, ..., 0.9, 1.0) assigned to wrong bucket, skewing calibration analysis. The spec requires lower-inclusive, upper-exclusive except for the final bucket (0.9-1.0 inclusive both ends).

**Test Scenarios**:
1. Confidence = 0.0 — bucket [0.0, 0.1)
2. Confidence = 0.1 — bucket [0.1, 0.2), NOT [0.0, 0.1)
3. Confidence = 0.9 — bucket [0.9, 1.0] (inclusive)
4. Confidence = 1.0 — bucket [0.9, 1.0] (inclusive, not out of bounds)
5. Confidence = 0.09999999 — bucket [0.0, 0.1) (floating point edge)
6. Confidence = 0.5 — bucket [0.5, 0.6)
7. Empty calibration data — 10 empty buckets returned, not panic

**Coverage Requirement**: Boundary values at every bucket edge, plus floating-point precision edge cases.

### R-05: Division by Zero in Utility Score
**Severity**: High
**Likelihood**: Med
**Impact**: Panic or NaN in utility_score when denominator is zero. This can happen when all sessions for an entry have NULL outcome (excluded from computation), leaving total=0.

**Test Scenarios**:
1. utility_score(0, 0, 0) — must return 0.0, not panic/NaN
2. Entry with injections into sessions that all have NULL outcome — success_count=0, rework_count=0, abandoned_count=0
3. Large values: utility_score(u32::MAX, 0, 0) — no overflow

**Coverage Requirement**: Zero denominator, zero numerator, and large value cases.

### R-06: Query Performance at Scale
**Severity**: Med
**Likelihood**: Med
**Impact**: context_status latency exceeds 500ms, degrading agent experience. Query 3 (calibration rows) is the highest risk — it returns one row per injection_log record joined with sessions, potentially 10K+ rows loaded into Rust memory.

**Test Scenarios**:
1. Performance benchmark with 500 entries, 10,000 injection_log rows, 200 sessions — must complete within 500ms
2. Empty database — must complete near-instantly, not degrade
3. Verify Query 1 uses idx_injection_log_entry (EXPLAIN QUERY PLAN)

**Coverage Requirement**: At least one benchmark test at the stated scale. Index usage verified for the GROUP BY query.

### R-07: GC Race Condition During Computation
**Severity**: Med
**Likelihood**: Low
**Impact**: Session GC runs between Query 1 and Query 3, deleting injection_log rows. Query 1 sees entries with injections; Query 3 misses calibration rows for those same injections. Result: inconsistent counts.

**Test Scenarios**:
1. Verify that all four queries run within a single lock_conn() scope (code review)
2. Verify that gc_sessions cannot acquire the connection while compute_effectiveness_aggregates holds it

**Coverage Requirement**: Code review confirms single connection lock scope. No concurrent GC test needed if lock scope is verified.

### R-08: JSON Output Compatibility
**Severity**: Med
**Likelihood**: Med
**Impact**: Existing JSON consumers of context_status fail when the new effectiveness field appears. Even with skip_serializing_if, if effectiveness data exists, the field is present.

**Test Scenarios**:
1. JSON output without injection data — effectiveness field must be absent (None, skipped)
2. JSON output with injection data — effectiveness field present, parseable
3. Verify skip_serializing_if = "Option::is_none" annotation on the field
4. Verify EffectivenessReportJson is serializable/deserializable round-trip

**Coverage Requirement**: Both present and absent cases. Verify no breakage in existing JSON structure by deserializing full output.

### R-09: Settled Classification Logic Error
**Severity**: Med
**Likelihood**: Med
**Impact**: Entries incorrectly labeled Settled when they have no historical success injection — they should be Unmatched (if topic active) or fall through to Effective default.

**Test Scenarios**:
1. Entry with inactive topic + historical success injection — Settled (correct)
2. Entry with inactive topic + NO historical success injection + zero injections — should NOT be Settled; should be Unmatched or Effective depending on topic activity
3. Entry with inactive topic + historical injections but all rework/abandoned — not Settled (no success outcome)

**Coverage Requirement**: Settled requires both conditions: topic inactive AND at least one success-outcome injection.

### R-10: NOISY_TRUST_SOURCES Case Sensitivity
**Severity**: Low
**Likelihood**: Med
**Impact**: trust_source stored as "Auto" or "AUTO" would bypass the .contains(&"auto") check, causing entries to escape Noisy classification.

**Test Scenarios**:
1. trust_source = "auto" — matches NOISY_TRUST_SOURCES
2. trust_source = "agent" — does not match
3. Verify trust_source values in existing codebase are consistently lowercase

**Coverage Requirement**: Unit test with matching and non-matching trust sources.

### R-11: Phase 8 spawn_blocking Failure Handling
**Severity**: High
**Likelihood**: Low
**Impact**: If the spawn_blocking task panics (e.g., rusqlite error unwrapped), the JoinError propagates and could crash compute_report or leave StatusReport in a partial state.

**Test Scenarios**:
1. Store returns StoreError from compute_effectiveness_aggregates — effectiveness must be None, rest of report unaffected
2. Verify the pattern matches existing contradiction scan graceful degradation (catch error, set None)
3. Verify no unwrap() on spawn_blocking result — must use match or unwrap_or

**Coverage Requirement**: Error path test with simulated store failure. Code review for unwrap() usage.

### R-12: Markdown Table Injection via Entry Titles
**Severity**: Low
**Likelihood**: Med
**Impact**: Entry title containing `|` or newlines breaks markdown table rendering in the effectiveness section.

**Test Scenarios**:
1. Entry with title containing pipe character `|` — verify table renders correctly (escaped or sanitized)
2. Entry with title containing newline — verify no table breakage

**Coverage Requirement**: At least one test with special characters in title.

### R-13: SourceEffectiveness Aggregate Utility NaN
**Severity**: Med
**Likelihood**: Med
**Impact**: If a trust source has entries but none were injected, aggregate_utility computation divides by zero, producing NaN that propagates into JSON output (invalid JSON in some serializers).

**Test Scenarios**:
1. Trust source with entries but zero total injections — aggregate_utility must be 0.0
2. Trust source with entries, all injected into sessions with NULL outcomes — aggregate_utility must be 0.0
3. Verify f64 NaN is never produced (serde_json rejects NaN)

**Coverage Requirement**: Unit test for aggregate_by_source with zero-injection trust sources.

## Integration Risks

- **Store-to-Engine data contract**: `EffectivenessAggregates` struct is the boundary between store SQL and engine pure functions. If `EntryInjectionStats.injection_count` uses COUNT (not COUNT DISTINCT), the engine classifies incorrectly because it trusts the store's aggregation. No runtime validation exists at this boundary.
- **Entry metadata JOIN consistency**: `compute_effectiveness_aggregates()` and `load_entry_classification_meta()` are separate queries. An entry could be deleted between the two calls, producing an entry_id in injection stats with no matching metadata. The classifier must handle orphaned injection stats gracefully.
- **Phase 8 ordering in compute_report**: Phase 8 must not depend on Phase 1-7 results unless explicitly documented. If it reads `active_entries` computed in Phase 1, and Phase 1 is refactored, Phase 8 breaks silently.
- **StatusReport field serialization order**: Adding `effectiveness` to StatusReport must not change the serialization order of existing fields, which could break consumers that rely on field position (unlikely but possible in summary format).

## Edge Cases

- **Empty knowledge base**: Zero entries, zero sessions, zero injection_log rows. All queries return empty. Report should have all zero counts, empty lists, None data window timestamps.
- **All entries Unmatched**: Knowledge base has entries and active sessions, but no injection_log records (hook pipeline never ran). Every entry classified as Unmatched.
- **Single entry, single session**: Minimum meaningful data. One entry injected once into one session with success outcome. Should classify as Effective with 100% success rate.
- **Entry injected into session with no outcome yet**: Session status = "active", outcome = NULL. Must be excluded from effectiveness computation entirely (not counted as failure).
- **Confidence at exact boundaries**: confidence = 0.0, 0.1, 0.5, 0.9, 1.0 — all must land in correct calibration bucket.
- **Maximum data volume**: 500 entries, 10,000 injection_log rows, 500 sessions — must complete within 500ms budget.
- **All sessions are rework**: utility_score = 0.5 for every entry. All entries with >= 3 injections become Effective (0.5 >= 0.3 threshold). No Ineffective entries despite poor outcomes.
- **Entry with topic matching no session feature_cycle**: Topic "design-patterns" but no session has feature_cycle "design-patterns". Topic is inactive, entry could be Settled (if has success injection) or falls through.
- **u32 overflow**: Entry with > 2^32 injections — unlikely but utility_score should use u32 safely without overflow risk.

## Security Risks

- **No external input**: This feature is read-only analytics on internal data. No new MCP tool parameters, no user-supplied queries, no file paths. The only input is existing database content.
- **SQL injection**: All SQL queries use parameterized statements via rusqlite. Entry titles and topics are read from the database, not from user input. No SQL injection surface.
- **Output size amplification**: An attacker who can create entries could flood the knowledge base to produce an enormous effectiveness report. Mitigated by the top-10 cap on ineffective/unmatched lists, but the by_source and calibration tables grow with trust source diversity (bounded to 5 known sources).
- **Information disclosure via context_status**: Effectiveness data reveals which entries are ineffective. This is the intended behavior; no access control change needed since context_status is already available to all agents.

## Failure Modes

- **Store query failure**: StoreError propagates to StatusService, which catches it and sets `effectiveness = None`. Existing status report fields are unaffected. Summary shows "Effectiveness: no injection data" (graceful degradation per NFR-06).
- **Empty injection_log**: Not an error. Report produced with all entries as Unmatched/Settled, data window shows zero sessions. Output explicitly states "no injection data" in summary format.
- **spawn_blocking panic**: JoinError caught by StatusService. Same handling as store query failure — effectiveness = None, rest of report proceeds.
- **Corrupt injection_log data**: entry_id in injection_log references a deleted entry (no FK constraint). Injection stats exist for an entry_id not in entry_classification_meta. The classifier should skip orphaned entries rather than panic.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Multi-table JOIN performance) | R-06 | ADR-001: Consolidated single Store method with one lock_conn(), SQL-side GROUP BY, existing indexes verified. Calibration query (R-06) remains the highest performance risk. |
| SR-02 (Session GC sliding window) | R-07, R-09 | ADR-003: DataWindow struct included in output. GC race mitigated by single connection lock (R-07). Settled classification must check for historical success injection (R-09). |
| SR-03 (StatusReport output bloat) | R-08, R-12 | skip_serializing_if on JSON, capped lists (top 10), one-line summary format. Markdown section is additive. |
| SR-04 (Scope creep into automated actions) | — | Architecture enforces read-only: no writes in any effectiveness code path. Pure functions in engine, SELECT-only in store. Accepted by design. |
| SR-05 (Noisy limited to "auto") | R-10 | ADR-004: NOISY_TRUST_SOURCES array constant. Adding "neural" is a one-line change. Case sensitivity risk (R-10) remains. |
| SR-06 (NULL topic/feature_cycle) | R-02 | ADR-002: Explicit "(unattributed)" sentinel for NULL/empty topic. Sessions with NULL feature_cycle excluded from active_topics but included in injection JOIN. Historical precedent: Unimatrix #756, #981. |
| SR-07 (StatusAggregates pattern break) | R-06 | ADR-001: Follows StatusAggregates pattern (Unimatrix #704, #708). Single method, single connection lock. |
| SR-08 (Rework weight miscalibration) | — | Named constants (OUTCOME_WEIGHT_*) defined in effectiveness.rs. Weights are tunable at compile time. No runtime risk — accepted as product decision. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 10 scenarios |
| High | 3 (R-03, R-04, R-05) | 13 scenarios |
| Medium | 5 (R-06, R-07, R-08, R-09, R-13) | 12 scenarios |
| Low | 3 (R-10, R-11, R-12) | 6 scenarios |
