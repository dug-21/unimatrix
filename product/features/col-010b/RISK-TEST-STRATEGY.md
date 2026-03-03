# Risk-Based Test Strategy: col-010b

Feature: Retrospective Evidence Synthesis & Lesson-Learned Persistence
Author: col-010b-agent-3-risk
Date: 2026-03-02

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Evidence truncation mutates in-memory report — narrative synthesis and lesson-learned content receive truncated evidence | High | Med | Critical |
| R-02 | `PROVENANCE_BOOST` applied at one callsite but missed at the other — inconsistent ranking between MCP and hook paths | High | Med | High |
| R-03 | Fire-and-forget lesson-learned embedding fails silently — entry written with `embedding_dim = 0`, invisible to `context_search` until next supersede | Med | Med | Medium |
| R-04 | Concurrent supersede race — two active lesson-learned entries for same feature_cycle | Med | Low | Medium |
| R-05 | Narrative synthesis produces empty or misleading summaries for edge-case evidence patterns | Med | Med | Medium |
| R-06 | `evidence_limit` default change breaks existing tests asserting on evidence array lengths | Low | Low | Low |
| R-07 | `lesson-learned` category absent from CategoryAllowlist at runtime — writes silently skipped | Low | Med | Low |
| R-08 | `recommendations` field breaks existing JSON consumers that don't expect it | Low | Low | Low |
| R-09 | Lesson-learned content is empty string when report has hotspots but no narratives (JSONL path with empty claims) | Med | Low | Medium |

---

## Risk-to-Scenario Mapping

### R-01: Evidence Truncation Mutates In-Memory Report

**Severity**: High
**Likelihood**: Med
**Impact**: If truncation modifies the report in-place before narrative synthesis or lesson-learned content extraction runs, narratives lose access to full evidence arrays. Clustered timestamps and sequence patterns cannot be computed from truncated data. Lesson-learned entries contain incomplete information.

**Test Scenarios**:
1. Build a report with 10 evidence items per hotspot. Call the context_retrospective handler with default `evidence_limit=3`. Verify the serialized response has <=3 evidence items per hotspot. Verify the lesson-learned entry content (after background task completes) references information from all 10 evidence items (not just 3).
2. Verify the in-memory report object retains all evidence items after serialization (not mutated).
3. Unit test: `synthesize_narratives()` receives a `HotspotFinding` with 10 evidence items. Verify the narrative clusters reference events from all 10 items.

**Coverage Requirement**: The clone-and-truncate pattern (ADR-001) must be verified by asserting full evidence availability to downstream consumers after truncation.

---

### R-02: Provenance Boost Divergence Between Callsites

**Severity**: High
**Likelihood**: Med
**Impact**: If `PROVENANCE_BOOST` is applied in `tools.rs` but missed in `uds_listener.rs` (or vice versa), agents using the hook path see different ranking than agents using MCP directly. Lesson-learned entries rank with the boost in one path but without it in the other.

**Test Scenarios**:
1. Unit test: compute rerank score for two entries (one `lesson-learned`, one `convention`) with identical `similarity=0.8` and `confidence=0.6`. Verify the difference is exactly `PROVENANCE_BOOST = 0.02`.
2. Integration test via MCP `context_search`: insert a lesson-learned entry and a convention entry with equal stored confidence. Search. Verify lesson-learned ranks first.
3. Integration test via ContextSearch hook path: same entries. Verify lesson-learned ranks first.
4. Code review check: verify `PROVENANCE_BOOST` is referenced from `confidence.rs` at both application sites, not duplicated as a literal.

**Coverage Requirement**: Both application sites must be exercised by separate integration tests. The constant must be imported from a single definition.

---

### R-03: Fire-and-Forget Embedding Failure

**Severity**: Med
**Likelihood**: Med
**Impact**: ONNX embedding failure results in a lesson-learned entry with `embedding_dim = 0`. The entry is invisible to `context_search` (not in VECTOR_MAP). The caller receives a successful retrospective response with no indication the knowledge entry is unsearchable.

**Test Scenarios**:
1. Mock embed service to return an error. Call `context_retrospective` with >= 1 hotspot. Verify: tool returns valid report; `tracing::warn!` logged; lesson-learned entry exists with `embedding_dim = 0`; entry is retrievable via `context_lookup(category: "lesson-learned")`.
2. Call `context_retrospective` again with healthy embed service. Verify: failed entry superseded; new entry has `embedding_dim > 0`; `context_search` returns new entry.
3. Verify `context_retrospective` response time does not include embedding time (mock with 300ms delay).

**Coverage Requirement**: Both failure and recovery paths must be tested.

---

### R-04: Concurrent Supersede Race

**Severity**: Med
**Likelihood**: Low
**Impact**: Two simultaneous `context_retrospective` calls for the same feature_cycle produce two active lesson-learned entries.

**Test Scenarios**:
1. Simulate two concurrent calls via `tokio::join!`. Verify at most 2 active entries exist. Verify next single call reduces to exactly 1.
2. Single-call deterministic test: verify exactly 1 active entry after one retrospective call.

**Coverage Requirement**: Tolerated known limitation. Tests assert upper bound (<=2), not exactly 1, to avoid flaky tests under concurrency.

---

### R-05: Narrative Synthesis Edge Cases

**Severity**: Med
**Likelihood**: Med
**Impact**: Empty evidence arrays, single-event hotspots, non-numeric sleep durations, and file paths in unexpected formats produce empty or misleading narratives.

**Test Scenarios**:
1. Empty evidence array: `synthesize_narratives` with a hotspot that has 0 evidence items. Verify: summary is non-empty (uses hotspot claim as fallback); clusters is empty; top_files is empty; sequence_pattern is None.
2. Single evidence event: one event produces one cluster with event_count=1.
3. Non-monotone sleep values: durations [30, 60, 30, 120]. Verify `sequence_pattern = None`.
4. Monotone sleep values: durations [30, 60, 90, 120]. Verify `sequence_pattern = Some("30s->60s->90s->120s")`.
5. Top files with > 5 distinct files: verify only 5 returned; summary mentions remaining count.
6. Evidence with no parseable file paths: `top_files` is empty.

**Coverage Requirement**: All edge cases listed must have unit tests.

---

### R-06: Evidence Limit Default Change

**Severity**: Low
**Likelihood**: Low
**Impact**: Existing tests asserting exact evidence array lengths fail after the default changes from unlimited to 3.

**Test Scenarios**:
1. R-09 gate audit: verify no existing integration tests assert on `hotspot.evidence.len()` values.
2. Verify `evidence_limit = 0` produces output identical to pre-col-010b format.
3. Verify default `evidence_limit = 3` with synthetic 13-hotspot report produces <= 10240 bytes.

**Coverage Requirement**: Audit documented in PR. Both evidence_limit=0 and evidence_limit=3 exercised in integration tests.

---

### R-07: CategoryAllowlist Absent for lesson-learned

**Severity**: Low
**Likelihood**: Med
**Impact**: If `"lesson-learned"` is removed from the allowlist at runtime, FR-06.4 skips the write with a logged error. No retrospective knowledge persists. AC-06/07/08 fail silently.

**Test Scenarios**:
1. Verify `categories.rs` INITIAL_CATEGORIES includes `"lesson-learned"` (static code check + unit test).
2. Simulate poisoned allowlist (remove `"lesson-learned"`). Call `context_retrospective`. Verify: `tracing::error!` logged; retrospective report returned successfully; no lesson-learned entry exists.

**Coverage Requirement**: Both allowlist-present and allowlist-absent paths tested.

---

### R-08: New `recommendations` Field Breaks JSON Consumers

**Severity**: Low
**Likelihood**: Low
**Impact**: Existing callers that parse `RetrospectiveReport` JSON may fail on unexpected `recommendations` field.

**Test Scenarios**:
1. Verify `recommendations` uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. When empty, the field is absent from JSON.
2. Verify deserialization of pre-col-010b JSON (without `recommendations` field) succeeds with `recommendations = vec![]`.

**Coverage Requirement**: Serde roundtrip test with and without the field.

---

### R-09: Empty Lesson-Learned Content

**Severity**: Med
**Likelihood**: Low
**Impact**: JSONL path with hotspots that have empty claims produces a lesson-learned entry with empty content. The entry has `embedding_dim > 0` (embedded on empty string) but returns no useful knowledge on `context_search`.

**Test Scenarios**:
1. Build a report with hotspots that have empty claims and no narratives. Verify `build_lesson_learned_content()` returns non-empty content (falls back to rule_name at minimum).
2. Verify lesson-learned content always includes at least the hotspot rule names and measured values.

**Coverage Requirement**: Content generation function must be tested with edge case inputs.

---

## Integration Risks

### IR-01: Narrative Synthesis Depends on `from_structured_events()` Path

The structured-events path was delivered in col-010 P0. If P0's implementation has bugs in session scanning or injection log aggregation, narratives will be computed on incorrect data. `narratives = Some(...)` implies the structured path was used, giving the caller false confidence in the synthesis quality.

**Test Scenario**: Verify `from_structured_events()` returns correct session counts and injection records before computing narratives. Integration test: populate SESSIONS + INJECTION_LOG, run retrospective, verify session_count matches expected.

### IR-02: Evidence Truncation Order vs. Narrative Synthesis

The truncation must happen AFTER narrative synthesis and lesson-learned content extraction, but BEFORE serialization. If the order is wrong, narratives are correct but the serialized response contains full evidence (truncation not applied) or narratives are based on truncated evidence.

**Test Scenario**: End-to-end integration test: report with 10 evidence items, evidence_limit=3. Verify: serialized response has <=3 items per hotspot; narratives reference information from all 10 items.

---

## Edge Cases

### EC-01: Zero Hotspots, Zero Recommendations

A feature cycle with no hotspots and no recommendations. No lesson-learned entry is written. `narratives` is None or Some([]). `recommendations` is empty. Both `skip_serializing_if` conditions apply.

### EC-02: evidence_limit = 1

Single evidence item per hotspot. Clusters still computed on full in-memory evidence. Serialized response has exactly 1 item per hotspot.

### EC-03: All Hotspot Types Unrecognized

A report where all hotspots have rule_names that don't match any recommendation template. `recommendations` is empty. No lesson-learned entry is written (0 hotspots condition is false, but recommendations is empty — however, FR-06.1 triggers on `hotspots.len() > 0 OR recommendations.len() > 0`, so the lesson-learned IS written because hotspots.len() > 0).

### EC-04: Very Large Evidence Arrays

A hotspot with 1000 evidence items. Truncation to 3. Narrative synthesis processes all 1000 (in-memory). Clone is O(1000) per hotspot. Acceptable.

### EC-05: Feature Cycle with No SESSIONS Data

`from_structured_events()` returns empty report. Falls back to JSONL path. `narratives = None`. `recommendations` populated from JSONL hotspots. If hotspots exist, lesson-learned entry is written.

---

## Security Risks

### SR-SEC-01: Lesson-Learned Content from Aggregated Hook Data

**Untrusted input**: Observation records originating from hook events contribute to hotspot findings and evidence.
**Damage**: A malicious agent triggering thousands of hotspot-qualifying events could inflate lesson-learned content with misleading patterns. The entry gets `trust_source = "system"` (0.7) and `PROVENANCE_BOOST` at search time.
**Blast radius**: Lesson-learned entries surface in `context_search` and `context_briefing` for future agents.
**Mitigation**: Threshold-based hotspot detection limits trivial injection. The `helpful_count` / `unhelpful_count` feedback mechanism can surface low-quality entries over time. No specific mitigation in col-010b v1.

### SR-SEC-02: Provenance Boost Gaming

**Risk**: An agent could create entries with `category = "lesson-learned"` directly via `context_store` to gain the `PROVENANCE_BOOST` ranking advantage.
**Mitigation**: `context_store` requires Write capability and goes through content scanning. The boost is only 0.02 (smaller than co-access max 0.03). The CategoryAllowlist gates `lesson-learned` writes. Not a significant concern.

---

## Failure Modes

### FM-01: ONNX Embed Service Unavailable

**Expected**: Lesson-learned entry written with `embedding_dim = 0`. Logged at `warn`. Entry queryable by metadata. Next retrospective supersedes with properly embedded entry.

### FM-02: CategoryAllowlist Poisoned

**Expected**: Lesson-learned write skipped. Logged at `error`. Retrospective response returned successfully. Knowledge not persisted until allowlist restored.

### FM-03: `from_structured_events()` Error

**Expected**: Falls back to JSONL path (or returns empty report if neither has data). `narratives = None`. Recommendations still computed from JSONL hotspots.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — Fire-and-forget embedding failure | R-03 | Entry written with embedding_dim=0. Recovery via supersede. |
| SR-02 — Nested async complexity | — | Follows established `tokio::spawn` + `spawn_blocking` pattern. No new complexity. |
| SR-03 — Serde additive fields | R-08 | `skip_serializing_if` on both new fields. Roundtrip tests verify backward compat. |
| SR-04 — evidence_limit breaks tests | R-06 | R-09 gate audit. Current tests do not assert on evidence array lengths. |
| SR-05 — Dual representation truncation | R-01 | ADR-001: clone-and-truncate. Full evidence preserved for synthesis. |
| SR-06 — Synthesis edge cases | R-05 | 6 specific edge case scenarios covering empty, single, non-monotone patterns. |
| SR-07 — Provenance boost two callsites | R-02 | Import from single constant. Both sites tested independently. |
| SR-08 — Concurrent supersede race | R-04 | Inherited known limitation. Tolerated. |
| SR-09 — Structured path stability | IR-01 | Integration test verifies P0 structured path before extending. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios |
| High | 1 (R-02) | 4 scenarios |
| Medium | 4 (R-03, R-04, R-05, R-09) | 13 scenarios |
| Low | 3 (R-06, R-07, R-08) | 7 scenarios |
| **Total** | **9** | **27+ scenarios** |

**Integration test focus areas** (ordered by risk):
1. Clone-and-truncate correctness — full evidence available to synthesis after truncation (R-01)
2. Provenance boost consistency across both search callsites (R-02)
3. Fire-and-forget embedding failure and recovery via supersede (R-03)
4. Narrative synthesis edge cases — empty, single, non-monotone (R-05)
5. `evidence_limit = 0` backward compatibility (R-06)
