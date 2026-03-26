# Test Plan: Report Rendering (`eval/report/render.rs`)

Component files: `render.rs` (new `render_phase_section`, modified `render_report`)

All tests are sync `#[test]` unless noted. All call `run_report` (via `report-entrypoint`)
or `render_phase_section`/`render_report` directly.

---

## Risk Coverage

| Risk | Tests in this component |
|------|------------------------|
| R-01 (Critical) | `test_report_round_trip_phase_section_null_label` |
| R-02 (Critical) | `test_report_round_trip_phase_section_7_distribution` (negative assertion + order), updated `test_report_contains_all_five_sections` |
| R-07 (Med) | `test_render_phase_section_absent_when_stats_empty` |
| R-09 (Med) | `test_render_phase_section_empty_input_returns_empty_string`, `test_report_round_trip_null_phase_only_no_section_6` |
| R-12 (Med) | `test_section_2_phase_label_non_null_present`, `test_section_2_phase_label_null_absent` |

---

## New Unit Tests

### `test_render_phase_section_empty_input_returns_empty_string` (R-09)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**: `let stats: &[PhaseAggregateStats] = &[];`

**Act**: `let output = render_phase_section(stats);`

**Assert**:
- `assert_eq!(output, "", "empty stats must produce empty string, not a heading");`
- `assert!(!output.contains("## 6."), "no section heading for empty stats");`

**Rationale**: Guards R-09 — the function contract states it returns an empty string
when called with an empty slice, not an empty table or orphaned heading. The caller
in `render_report` also guards against calling it with empty stats, but both layers
must be tested independently.

---

### `test_render_phase_section_absent_when_stats_empty` (R-07, AC-04, AC-09 item 5)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**:
Write several `ScenarioResult` JSON files where ALL `phase` fields are `None`.

**Act**: `run_report(results_dir.path(), None, &out_path).expect("run_report");`

**Assert**:
```rust
let content = std::fs::read_to_string(&out_path).unwrap();
assert!(
    !content.contains("## 6. Phase-Stratified Metrics"),
    "section 6 must be absent when all phases are null:\n{content}"
);
```

**Rationale**: This test exercises both `compute_phase_stats` returning empty AND
`render_report` skipping the section. Even if one layer has a bug, the full-pipeline
test catches the combined effect.

---

### `test_report_round_trip_null_phase_only_no_section_6` (R-09, AC-04)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

This is a named full-pipeline test focused on the all-null scenario specifically
calling through `run_report`. Write 2-3 `ScenarioResult` files with `phase: None`,
run the report, and assert `!content.contains("## 6. Phase-Stratified Metrics")`.

**Additional assertion**: `content.contains("## 6. Distribution Analysis")` is also
asserted as absent because the section should now be `## 7.`.

Wait — when all phases are null, section 6 is omitted. The Distribution Analysis
still renders as `## 7. Distribution Analysis` per the implementation (the heading
is a literal string in `render.rs`, not computed). Assert:
```rust
assert!(!content.contains("## 6. Phase-Stratified Metrics"),
    "section 6 absent when all phases null");
assert!(content.contains("## 7. Distribution Analysis"),
    "section 7 Distribution Analysis always present");
assert!(!content.contains("## 6. Distribution Analysis"),
    "old heading must never appear");
```

---

### `test_report_round_trip_phase_section_null_label` (R-01)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**: Write one `ScenarioResult` file with `phase: None` and one with
`phase: Some("delivery")`. This gives a mixed corpus where the null bucket should
render.

Actually: need at least one non-null phase to trigger section 6. So write:
- `ScenarioResult { phase: Some("delivery"), ... }`
- `ScenarioResult { phase: None, ... }`

**Act**: `run_report(...)`.

**Assert**:
```rust
assert!(content.contains("## 6. Phase-Stratified Metrics"));
assert!(content.contains("(unset)"),
    "null-phase bucket label must be '(unset)'");
assert!(!content.contains("(none)"),
    "'(none)' must never appear — canonical is '(unset)'");
```

**Rationale**: The negative assertion `!content.contains("(none)")` is the ground
truth for R-01. If the implementation uses `"(none)"`, this test fails.

---

### `test_section_2_phase_label_non_null_present` (R-12, AC-08)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**: Write one `ScenarioResult` with `phase: Some("delivery")` and a
notable ranking change (so it appears in section 2's notable entries).

**Act**: `run_report(...)`.

**Assert**:
```rust
let section_2_start = content.find("## 2. Notable Ranking Changes").unwrap();
let section_3_start = content.find("## 3. Latency Distribution").unwrap();
let section_2_content = &content[section_2_start..section_3_start];
assert!(section_2_content.contains("delivery"),
    "phase label must appear in section 2 for non-null phase scenario");
```

**Rationale**: AC-08 requires phase label in Notable Ranking Changes header lines.
Isolating the check to the section 2 substring avoids false positives from section 6.

---

### `test_section_2_phase_label_null_absent` (R-12, AC-08)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

**Arrange**: Write one `ScenarioResult` with `phase: None` (all-null corpus, so
section 6 is absent). The scenario must appear in section 2.

**Act**: `run_report(...)`.

**Assert**:
```rust
let section_2_content = &content[section_2_start..section_3_start];
assert!(!section_2_content.contains("(unset)"),
    "null-phase scenarios must not show phase label in section 2");
assert!(!section_2_content.contains("phase:"),
    "no phase annotation on null-phase scenarios in section 2");
```

**Rationale**: Null-phase header lines must be unchanged from pre-nan-009 format.

---

## Updated Existing Tests (MANDATORY)

### `test_report_contains_all_five_sections` → update to seven sections (R-02, AC-12)

**Current** (lines 78-112 in `tests.rs`): asserts five sections ending with
`"## 5. Zero-Regression Check"`.

**Required update**:
1. Add `write_result` calls for results with `phase: Some("delivery")` so section 6
   renders (tests with all-null phases would have section 6 absent, making the
   section-6 assertion fail — the test must use non-null phase data).
2. Add assertions:
   ```rust
   assert!(content.contains("## 6. Phase-Stratified Metrics"), "section 6 missing");
   assert!(content.contains("## 7. Distribution Analysis"), "section 7 missing");
   assert!(!content.contains("## 6. Distribution Analysis"),
       "old section-6 heading must not appear");
   ```
3. Update the position ordering check to include `pos6` and `pos7`:
   ```rust
   assert!(pos1 < pos2 && pos2 < pos3 && pos3 < pos4 && pos4 < pos5
           && pos5 < pos6 && pos6 < pos7,
       "sections must appear in order 1-7");
   ```

**Note**: The existing test is named `test_report_contains_all_five_sections`. The
delivery agent should rename it to `test_report_contains_all_seven_sections` or
add a new test and leave the old name if rename causes merge conflicts.

---

### `test_report_round_trip_cc_at_k_icd_fields_and_section_6` → update heading (SR-02)

**Current**: asserts `content.contains("## 6.")` (passes for the old "## 6. Distribution
Analysis").

**Required update** (two assertions):
1. Replace `content.contains("## 6.")` with the explicit heading:
   `content.contains("## 7. Distribution Analysis")`.
2. Add negative assertion:
   `assert!(!content.contains("## 6. Distribution Analysis"),
    "old heading must be absent after renumbering");`

The test name becomes slightly misleading (`section_6` in name, now testing section 7)
but renaming is optional — the assertions are what matter.

---

## Primary Integration Guard

### `test_report_round_trip_phase_section_7_distribution` (ADR-002, R-02, R-03, AC-11, AC-12)

**Location**: `eval/report/tests.rs` (sync `#[test]`)

This is the mandatory round-trip test specified in ADR-002 and IMPLEMENTATION-BRIEF.
Full specification:

**Arrange**:
1. Create a runner-side `ScenarioResult` with `phase: Some("delivery".to_string())`
   and non-trivial metric values (non-zero, non-default).
2. Serialize to JSON using `serde_json::to_string`.
3. Write the JSON to a `TempDir` file (e.g., `"delivery-01.json"`).

**Act**: `run_report(results_dir.path(), None, &out_path).expect("run_report");`

**Assert** (all five must be present):
```rust
let content = std::fs::read_to_string(&out_path).unwrap();

// (1) New section present
assert!(content.contains("## 6. Phase-Stratified Metrics"),
    "section 6 Phase-Stratified Metrics must be present");

// (2) Renumbered section present
assert!(content.contains("## 7. Distribution Analysis"),
    "section 7 Distribution Analysis must be present (was section 6)");

// (3) Phase value appears in section 6 (catches dual-type partial update, R-03)
assert!(content.contains("delivery"),
    "'delivery' phase label must appear in section 6 table");

// (4) Section order guard (SR-02, R-02)
let pos6 = content.find("## 6.").expect("section 6 must be present");
let pos7 = content.find("## 7.").expect("section 7 must be present");
assert!(pos6 < pos7, "section 6 must appear before section 7");

// (5) Old heading absent (SR-02 negative guard — pattern #3426)
assert!(!content.contains("## 6. Distribution Analysis"),
    "old '## 6. Distribution Analysis' heading must NOT appear");
```

**Why "delivery" is mandatory as a non-trivial value**: Using `phase: None` would
produce an empty `phase_stats` vec, section 6 would be omitted, and assertion (1)
would fail. Using `phase: Some("design")` or `Some("bugfix")` is also acceptable.
The requirement is that the phase is non-null so the round-trip exercises the
deserialization of `phase` on the report side — if the report-side `ScenarioResult`
copy is missing `phase`, it defaults to `None` and `compute_phase_stats` sees no
non-null phases, omitting section 6 and causing assertion (1) to fail. This is
exactly how R-03 (dual-type partial update) is detected.
