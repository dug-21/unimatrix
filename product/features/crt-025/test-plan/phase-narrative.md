# Test Plan: Phase Narrative (Component 9)

Files: `crates/unimatrix-observe/src/phase_narrative.rs`, `types.rs`
       `crates/unimatrix-server/src/mcp/tools.rs` (cycle_review handler)
Risks: R-04, R-08, R-12, R-13, AC-12, AC-13, AC-14

---

## Unit Test Expectations

`build_phase_narrative` is a pure function — ideal for exhaustive unit testing.
All tests are inline in `phase_narrative.rs`.

### Type Definitions (compile-time)

**`test_phase_narrative_types_defined`** (structural)
- Assert: `PhaseNarrative`, `CycleEventRecord`, `PhaseCategoryComparison` compile
- Assert: `RetrospectiveReport.phase_narrative` is `Option<PhaseNarrative>`
- Assert: `#[serde(skip_serializing_if = "Option::is_none")]` annotation present
  (verified by round-trip serialization below)

### `build_phase_narrative` — Phase Sequence Construction

**`test_build_phase_narrative_empty_events_empty_sequence`** (R-13 edge)
- Arrange: `events = []`, empty distributions
- Act: `build_phase_narrative(&[], &empty_dist, &empty_cross_dist)`
- Assert: `phase_sequence = []`, `rework_phases = []`, `per_phase_categories = {}`,
  `cross_cycle_comparison = None`
- Assert: no panic

**`test_build_phase_narrative_start_with_next_phase`**
- Arrange: `events = [CycleEventRecord { event_type: "cycle_start", next_phase: Some("scope"), .. }]`
- Assert: `phase_sequence = ["scope"]`

**`test_build_phase_narrative_phase_end_transition`**
- Arrange: events: start(next_phase="scope"), phase_end(phase="scope", next_phase="design")
- Assert: `phase_sequence = ["scope", "design"]`

**`test_build_phase_narrative_full_lifecycle`**
- Arrange: events: start(next_phase="scope"), phase_end(phase="scope",next_phase="design"),
  phase_end(phase="design",next_phase="implementation"), stop
- Assert: `phase_sequence = ["scope", "design", "implementation"]`

### Rework Detection

**`test_build_phase_narrative_rework_phase_detected`**
- Arrange: events that produce `phase_sequence = ["scope", "design", "scope"]` (rework)
- Assert: `rework_phases = ["scope"]`

**`test_build_phase_narrative_no_rework_no_rework_phases`**
- Arrange: linear sequence (no repeats)
- Assert: `rework_phases = []`

### `phase_sequence` from orphaned events (R-13)

**`test_build_phase_narrative_orphaned_phase_end_no_start`** (R-13 Critical)
- Arrange: `events = [CycleEventRecord { event_type: "cycle_phase_end", phase: Some("scope"), next_phase: Some("design"), .. }]`
  — NO prior `cycle_start` event
- Act: `build_phase_narrative(&events, ...)`
- Assert: no panic
- Assert: `phase_sequence` includes "design" (or "scope" depending on implementation)
  and is non-empty — orphaned events produce a partial but valid narrative

**`test_build_phase_narrative_phase_end_only_sequence`** (R-13)
- Arrange: only `cycle_phase_end` events, no `cycle_start`
- Assert: returns valid `PhaseNarrative`, no panic, `phase_sequence` is plausible

### Per-Phase Category Distribution

**`test_build_phase_narrative_per_phase_categories`**
- Arrange: `current_dist` maps `("scope", "decision") -> 3`, `("design", "pattern") -> 5`
- Assert: `per_phase_categories["scope"]["decision"] == 3`
- Assert: `per_phase_categories["design"]["pattern"] == 5`

**`test_build_phase_narrative_empty_entries_no_categories`** (edge case from Risk Strategy)
- Arrange: `cycle_events` rows exist, but `current_dist` is empty (no feature_entries with phase)
- Assert: `per_phase_categories = {}` — no panic, empty map returned

### Cross-Cycle Comparison (R-04, FR-10)

**`test_cross_cycle_comparison_none_when_zero_prior_features`** (R-04, FR-10.2)
- Arrange: `cross_dist` has data for zero prior features
- Assert: `cross_cycle_comparison = None`

**`test_cross_cycle_comparison_none_when_one_prior_feature`** (R-04, FR-10.2 boundary)
- Arrange: `cross_dist` has data for exactly one prior feature
- Assert: `cross_cycle_comparison = None` (below the 2-feature threshold)

**`test_cross_cycle_comparison_some_when_two_prior_features`** (R-04, FR-10.2)
- Arrange: two prior features each with `(design, decision) -> 2`
- Assert: `cross_cycle_comparison = Some([PhaseCategoryComparison { phase: "design", category: "decision", cross_cycle_mean: 2.0, sample_features: 2, .. }])`

**`test_cross_cycle_comparison_correct_mean`** (R-12)
- Arrange: current feature has `(design, decision) -> 10`; priors `"f1": 2`, `"f2": 2`
- Assert: `cross_cycle_mean = 2.0` (not 4.67 — current feature is excluded)
- Assert: `sample_features = 2`

**`test_cross_cycle_excludes_current_feature_data`** (R-12 Critical)
- Arrange: explicitly pass `current_dist` with 10 entries as current-feature data;
  `cross_dist` with 2 prior features of 2 entries each
- Assert: cross-cycle mean = 2.0 (not influenced by current-feature counts)

### `RetrospectiveReport` Serialization (R-08)

**`test_retrospective_report_phase_narrative_none_omitted`** (R-08, AC-13, FR-09.3)
- Arrange: `RetrospectiveReport { phase_narrative: None, .. }`
- Act: `serde_json::to_string(&report)`
- Assert: resulting JSON does NOT contain `"phase_narrative"` key at all
  (not `"phase_narrative": null`)

**`test_retrospective_report_phase_narrative_some_serialized`** (R-08, AC-12)
- Arrange: `RetrospectiveReport { phase_narrative: Some(PhaseNarrative { .. }), .. }`
- Act: serialize
- Assert: JSON contains `"phase_narrative"` key with non-null object value

---

## Integration Test Expectations

### Server-level: `context_cycle_review` with CYCLE_EVENTS data (AC-12)

**`test_cycle_review_phase_narrative_present_with_events`** (AC-12)
- Arrange: seed `cycle_events` rows for `cycle_id = "crt-025-test"` via `insert_cycle_event`
- Act: call `context_cycle_review(feature_cycle="crt-025-test", format="json")`
- Assert: JSON response contains `phase_narrative` key
- Assert: `phase_narrative.phase_sequence` is a non-empty array
- Assert: `phase_narrative.rework_phases` is a list (may be empty)

**`test_cycle_review_phase_narrative_absent_no_events`** (AC-13, R-08)
- Act: `context_cycle_review(feature_cycle="old-feature-no-events", format="json")`
- Assert: JSON response does NOT contain `phase_narrative` key

### infra-001 `tools` suite (new tests from OVERVIEW.md)

**`test_cycle_review_includes_phase_narrative`** (AC-12)
- Seed: start + phase-end + stop via `context_cycle` calls
- Assert: `context_cycle_review` JSON has `phase_narrative`

**`test_cycle_review_no_phase_narrative_for_old_feature`** (AC-13)
- Assert: no `phase_narrative` key for feature with no CYCLE_EVENTS

### Cross-cycle comparison threshold (AC-14)

**`test_cycle_review_cross_cycle_present_two_prior_features`** (AC-14)
- Seed: `context_store` with phase-tagged entries under two prior feature cycles
- Assert: `phase_narrative.cross_cycle_comparison` is non-null

**`test_cycle_review_cross_cycle_absent_fewer_than_two`** (AC-14)
- Only one prior feature with phase data
- Assert: `cross_cycle_comparison` absent or null in response

---

## SQL Query Verification (FR-09.1 — ordering)

The cycle events query must use `ORDER BY timestamp ASC, seq ASC` (ADR-002):

**`test_cycle_events_query_uses_timestamp_seq_ordering`** (R-07)
- Insert events with different timestamps (and potentially same seq)
- Assert narrative sequence matches insertion timestamp order, not `seq` order alone
- Verifies ADR-002 advisory seq, timestamp as true ordering

---

## Assertions

- `build_phase_narrative` with empty input never panics (R-13)
- Cross-cycle threshold at 2 features, not 1 (R-04)
- Self-exclusion: current feature not in cross-cycle mean (R-12)
- `phase_narrative = None` serializes as absent key, not null value (R-08)
- `phase_narrative = Some(...)` serializes correctly (AC-12)
