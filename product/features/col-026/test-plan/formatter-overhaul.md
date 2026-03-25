# Test Plan: formatter-overhaul

**Crate**: `unimatrix-server/src/mcp/response/retrospective.rs`
**Risks covered**: R-06, R-07, R-08, R-10, R-11, R-12
**ACs covered**: AC-01 through AC-15, AC-17

---

## Component Scope

This component rewrites the section ordering, adds five new sections (rebrand header, Phase
Timeline, What Went Well, burst notation, enhanced Knowledge Reuse), modifies three existing
sections (Sessions, Findings, Recommendations), and adds `format_claim_with_baseline` for
threshold language replacement. All tests extend the existing `#[cfg(test)] mod tests` block.

The existing `make_report()` helper must be extended to include all new `Option` fields with
`None` defaults after the `RetrospectiveReport` struct change. Existing tests must compile
without modification (AC-17).

---

## Unit Test Expectations

### R-07 / AC-11: Section Order (Golden Test)

#### Test: `test_section_order` (R-07, AC-11)

**The most critical formatter test.** Verifies all 11 sections appear in the specified order.

**Setup**: Build a `RetrospectiveReport` with all sections populated:
- `goal = Some("Implement feature X")`, `cycle_type = Some("Delivery")`,
  `attribution_path = Some("cycle_events-first (primary)")`.
- `is_in_progress = Some(true)`.
- `phase_stats = Some(vec![...])` (at least one entry).
- `recommendations = vec![...]`.
- `baseline_comparison = Some([...])` with Outlier entries (universal and phase-scoped).
- `session_summaries = Some([...])`.
- `hotspots = vec![...]` with evidence.
- `feature_knowledge_reuse = Some(...)` with cross-feature entries.
- `rework_session_count = Some(1)`.
- `phase_narrative = Some(...)`.

**Assert sequence** (each header found after the previous one in the output string):

```rust
let expected_order = [
    "# Unimatrix Cycle Review",    // Header (section 1)
    "## Recommendations",           // section 2
    "## Phase Timeline",            // section 3
    "## What Went Well",            // section 4
    "## Sessions",                  // section 5
    "## Outliers",                  // section 7 (Baseline Outliers — per SPEC §FR-12)
    "## Findings",                  // section 8 (after Baseline Outliers)
    "## Phase Outliers",            // section 9
    "## Knowledge Reuse",           // section 9
    "## Rework",                    // section 10 or "## Context Reload"
    "## Phase Narrative",           // section 11
];
let mut last_pos = 0;
for header in &expected_order {
    let pos = text.find(header).expect(&format!("{} not found", header));
    assert!(pos > last_pos, "{} appeared before previous section", header);
    last_pos = pos;
}
```

**Additional assert**: `"## Recommendations"` does not appear after `"## Findings"`.

#### Test: `test_recommendations_before_findings` (R-07, AC-11)

Simpler version: render a report with both recommendations and findings. Assert
`text.find("## Recommendations") < text.find("## Findings")`.

### AC-01 / AC-17: Header Rebrand

#### Test: `test_header_rebrand` (AC-01)

**Assert**:
- Output starts with `# Unimatrix Cycle Review —`.
- Output does NOT contain `# Retrospective:`.

```rust
let text = extract_text(&format_retrospective_markdown(&report));
assert!(text.contains("# Unimatrix Cycle Review —"));
assert!(!text.contains("# Retrospective:"));
```

Note: existing test `test_header_contains_feature_cycle` asserts `"# Retrospective: nxs-010"`.
This test WILL fail after the rebrand and must be updated to assert `"# Unimatrix Cycle Review — nxs-010"`.
This is an AC-17 permitted update (AC-17 says "existing tests pass", meaning they pass after
the implementation — the implementation agent updates the assertion to match the new behavior).

### AC-02: Goal Line Presence/Absence

#### Test: `test_header_goal_present` (AC-02)

**Setup**: `report.goal = Some("Implement feature X".to_string())`.
**Assert**: Output contains `**Goal**: Implement feature X` (or `Goal:` per spec FR-02).

#### Test: `test_header_goal_absent` (AC-02)

**Setup**: `report.goal = None`.
**Assert**: Output does NOT contain `Goal:`. No blank line in place of goal.

### AC-03: Cycle Type Classification

#### Test: `test_cycle_type_classification` (AC-03)

**Data-driven test** over all 5 keyword groups. For each row in the spec FR-03 table:

| Input goal substring | Expected `cycle_type` |
|---------------------|-----------------------|
| `"design the API"` | `"Design"` |
| `"implement the new tool"` | `"Delivery"` |
| `"fix the crash bug"` | `"Bugfix"` |
| `"refactor the parser"` | `"Refactor"` |
| `"organize the workspace"` | `"Unknown"` |
| `None` (goal absent) | `"Unknown"` |

**Assert**: Formatter renders `Cycle type: {expected}` for each case.

#### Test: `test_cycle_type_first_match_priority` (AC-03)

**Scenario**: Goal contains keywords matching both Design and Delivery (e.g., `"design and implement"`).
**Assert**: `cycle_type == "Design"` (first-match rule from spec FR-03).

### AC-04 / R-09: Attribution Path Labels

#### Test: `test_attribution_path_labels` (AC-04, R-09)

Three separate assertions for each path label:

```rust
for (label, expected) in [
    ("cycle_events-first (primary)", "Attribution: cycle_events-first (primary)"),
    ("sessions.feature_cycle (legacy)", "Attribution: sessions.feature_cycle (legacy)"),
    ("content-scan (fallback)", "Attribution: content-scan (fallback)"),
] {
    let mut report = make_report();
    report.attribution_path = Some(label.to_string());
    let text = extract_text(&format_retrospective_markdown(&report));
    assert!(text.contains(expected));
}
```

#### Test: `test_attribution_path_absent_when_none` (R-09)

`report.attribution_path = None` → no `Attribution:` line in output.

### AC-05 / R-05: `is_in_progress` Status Rendering

These tests are co-located with the `render_header` tests (extend existing `// render_header` block).

#### Test: `test_header_status_in_progress` (AC-05)

`is_in_progress = Some(true)` → output contains `Status: IN PROGRESS`.

#### Test: `test_header_status_omitted_when_some_false` (AC-05)

`is_in_progress = Some(false)` → no `Status:` line.

#### Test: `test_header_status_omitted_when_none` (AC-05)

`is_in_progress = None` → no `Status:` line.

### AC-06 / AC-07: Phase Timeline Table

#### Test: `test_phase_timeline_table` (AC-06)

**Setup**:

```rust
report.phase_stats = Some(vec![
    PhaseStats {
        phase: "scope".to_string(),
        pass_number: 1, pass_count: 1,
        duration_secs: 2520,  // 42m
        session_count: 2, record_count: 73,
        agents: vec!["researcher".to_string()],
        tool_distribution: ToolDistribution { read: 40, execute: 10, write: 15, search: 8 },
        knowledge_served: 3, knowledge_stored: 0,
        gate_result: GateResult::Pass,
        gate_outcome_text: Some("passed".to_string()),
        hotspot_ids: vec![],
    }
]);
```

**Assert**:
- Output contains `## Phase Timeline`.
- Output contains `| scope |`.
- Output contains `0h 42m` (duration).
- Output contains `3↓ 0↑` (knowledge served/stored).
- Output contains `PASS` (gate result).

#### Test: `test_phase_timeline_rework_annotation` (AC-07)

**Setup**: Two `PhaseStats` entries for `phase = "design"` with `pass_count = 2`.
**Assert**:
- Table contains two rows for design.
- Output contains `**Rework**: design`.
- Output contains `pass 1`.
- Output contains per-pass duration/records.

#### Test: `test_phase_timeline_absent_when_phase_stats_none` (AC-06)

`report.phase_stats = None` → output does NOT contain `## Phase Timeline`.
Output contains `No phase information captured.` (single line, no section header).

#### Test: `test_phase_timeline_absent_when_phase_stats_empty` (R-12)

`report.phase_stats = Some(vec![])` → same as `None` rendering.
Output: `No phase information captured.` (no section header).

#### Test: `test_phase_timeline_empty_phase_name` (R-02)

**Setup**: `PhaseStats.phase = ""`.
**Assert**: Formatter renders `—` or `(unknown)` in the Phase column, not an empty cell.

### AC-08: Finding Phase Annotation

#### Test: `test_finding_phase_annotation` (AC-08)

**Setup**:
- `report.hotspots = vec![make_finding("compile_cycles", Severity::Warning, 15.0, 2)]`.
- `report.phase_stats = Some(vec![...])` where the first `PhaseStats` has `hotspot_ids = vec!["F-01".to_string()]`.
**Assert**: Output contains `— phase: {phase_name}` in the finding header for F-01.

#### Test: `test_finding_no_phase_annotation_when_phase_stats_none` (AC-08)

`phase_stats = None` → finding header has no `— phase:` annotation.

### AC-09: Burst Notation Evidence Rendering

#### Test: `test_burst_notation_rendering` (AC-09)

**Setup**: Finding with 3 evidence records at timestamps `1000000`, `1720000`, `2680000` ms.
**Assert**:
- Output contains `Timeline:`.
- Output contains `+0m`.
- Output contains `Peak:`.
- Output does NOT contain `ts=`.

#### Test: `test_burst_notation_single_evidence` (AC-09, edge case from R-02)

**Setup**: Finding with exactly 1 evidence record.
**Assert**: Output contains `Timeline: +0m(1)`. No truncation marker. `Peak:` line present.

#### Test: `test_burst_notation_truncation_at_ten` (AC-09)

**Setup**: Finding with 12 evidence clusters (via narratives fixture).
**Assert**: Output contains `...` truncation marker. At most 10 burst entries before `...`.

### AC-10 / R-06: What Went Well Section

#### Test: `test_what_went_well_present` (AC-10, R-06)

**Setup**: `baseline_comparison` with at least one `Normal` status metric where current is
favorable per direction table (e.g., `parallel_call_rate: current=0.8, mean=0.5, status=Normal`).
**Assert**: Output contains `## What Went Well`.

#### Test: `test_what_went_well_absent_no_favorable` (AC-10)

**Setup**: `baseline_comparison` with only `Outlier` metrics.
**Assert**: Output does NOT contain `## What Went Well`.

#### Test: `test_what_went_well_absent_no_baseline` (AC-10)

`baseline_comparison = None` → no `## What Went Well` section.

#### Test: `test_what_went_well_excludes_outlier_metrics` (R-06)

**Setup**: `parallel_call_rate` is favorable (higher-is-better, current > mean) but
`status = Outlier`.
**Assert**: NOT included in What Went Well.

#### Test: `test_what_went_well_direction_table_all_16_metrics` (R-06)

**The critical R-06 test.** Data-driven over all 16 metrics from SPECIFICATION §FR-11.

For each metric in the direction table:
1. Construct a `BaselineComparison` where `current_value` is favorable per direction.
2. Set `status = Normal`.
3. Assert metric appears in What Went Well.
4. Construct a second comparison where `current_value` is UNFAVORABLE.
5. Assert metric does NOT appear in What Went Well.

The 16 metrics must be enumerated. From SPECIFICATION §FR-11 (10 listed in ARCHITECTURE.md,
full 16 in spec):

```rust
// Higher-is-better metrics (current > mean is favorable)
let higher_is_better = ["parallel_call_rate", "follow_up_issues_created", ...]; // all HIB per spec
// Lower-is-better metrics (current < mean is favorable)
let lower_is_better = ["bash_for_search_count", "permission_friction_events",
    "post_completion_work_pct", "coordinator_respawn_count", "sleep_workaround_count",
    "context_reload_pct", "reread_rate", "compile_cycles", ...]; // all LIB per spec
```

**Assert**: Classification of ALL 16 metrics matches the spec table exactly. Any
mis-classification causes a test failure.

#### Test: `test_what_went_well_sample_count_guard` (R-06)

**Scenario**: Metric has `stddev == 0` and `sample_count < 3`.
**Assert**: Not included in What Went Well (insufficient baseline).

#### Test: `test_metric_not_in_direction_table_excluded` (R-06)

**Scenario**: `BaselineComparison` with `metric_name = "unknown_metric"`, `status = Normal`,
current is lower than mean.
**Assert**: NOT included in What Went Well.

### AC-12: Enhanced Knowledge Reuse Section

#### Test: `test_knowledge_reuse_section` (AC-12)

**Setup**: `FeatureKnowledgeReuse` with all new fields:
- `delivery_count = 10`, `total_stored = 3`.
- `cross_feature_reuse = 6`, `intra_cycle_reuse = 4`.
- `by_category = {"decision": 6, "pattern": 4}`.
- `top_cross_feature_entries = vec![EntryRef { id: 42, title: "ADR-003", feature_cycle: "col-024", category: "decision", serve_count: 4 }]`.
- `category_gaps = vec!["procedure"]`.

**Assert**:
- Output contains `**Total served**: 10`.
- Output contains `**Stored this cycle**: 3`.
- Output contains `Cross-feature (prior cycles)` and `6`.
- Output contains `Intra-cycle` and `4`.
- Output contains `#42 ADR-003` in the top entries table.
- Output does NOT contain `category_gaps`.
- Output does NOT contain `cross_session_count`.

#### Test: `test_knowledge_reuse_zero_delivery` (AC-12)

`delivery_count = 0` → output contains `No knowledge entries served.`.

#### Test: `test_knowledge_reuse_no_cross_feature_omits_table` (AC-12)

`top_cross_feature_entries = vec![]` → no `Top cross-feature entries` table.

### AC-13 / R-08: Threshold Language Removal

#### Test: `test_format_claim_with_baseline_baseline_path` (R-08, AC-13)

**Scenario**: Claim `"43 compile cycles (threshold: 10) -- 4.3x typical"`.
`baseline_comparison` entry for `compile_cycles` has `mean=8.0`, `stddev=2.5`, `z_score=14.0`.
**Assert**:
- Output contains `(baseline: 8.0 ±2.5, +14.0σ)`.
- Output does NOT contain `threshold`.

#### Test: `test_format_claim_with_baseline_ratio_fallback` (R-08, AC-13)

**Scenario**: Same claim but `stddev == 0.0`.
**Assert**: Output ends with `(4.3× typical)`. No division by zero.

#### Test: `test_format_claim_with_baseline_no_threshold_pattern` (R-08, AC-13)

**Scenario**: Claim `"43 compile cycles"` (no threshold substring).
**Assert**: Claim rendered unchanged. No content stripped. No suffix added.

#### Test: `test_format_claim_threshold_zero_value` (R-08, AC-13)

**Scenario**: Claim contains `threshold: 0`. `measured = 5.0`.
**Assert**: No division-by-zero panic. Ratio annotation skipped (or shows safe fallback).

#### Test: `test_no_threshold_language` (R-08, AC-13)

**Data-driven test** over all 9 enumerated detection sites. For each site, construct a
`HotspotFinding` with the claim string format produced by that detection rule. Render via
`format_retrospective_markdown`. Assert rendered output does not match pattern `threshold[\s:]+[\d.]+`.

The 9 sites enumerated in ARCHITECTURE.md §Component 5:
1. `context_load_before_first_write_kb` (detection/agent.rs line 71)
2. `lifespan` (detection/agent.rs line 136)
3. `file_breadth` (detection/agent.rs line 217)
4. `reread_rate` (detection/agent.rs line 282)
5. `mutation_spread` (detection/agent.rs line 342)
6. `compile_cycles` (detection/agent.rs line 413)
7. `edit_bloat` (detection/agent.rs line 474)
8. `adr_count` (detection/scope.rs line 190)
9. `permission_retries` (detection/friction.rs line 68)

#### Test: `test_no_allowlist_in_compile_cycles` (AC-13, AC-19)

**Scenario**: Render a report with a `compile_cycles` hotspot finding.
**Assert**: Rendered output for the finding does NOT contain "allowlist".

### R-10: Hotspot Phase Annotation — Multi-Phase

#### Test: `test_finding_phase_multi_evidence` (R-10)

**Scenario**: Finding has evidence in Phase A (3 events) and Phase B (7 events). `phase_stats`
has two entries; Phase B's `hotspot_ids` includes `"F-01"`.
**Assert**: F-01 header includes `— phase: {phase_B_name}` (higher-count wins).

#### Test: `test_finding_phase_no_phase_stats` (R-10)

`phase_stats = None` → no `— phase:` annotation. No panic.

#### Test: `test_finding_phase_out_of_bounds_timestamp` (R-10)

**Scenario**: Finding evidence timestamp is before the first phase window start.
**Assert**: No annotation applied (no out-of-bounds array access). No panic.

### R-11: Threshold Audit Snapshot

#### Test: `test_threshold_language_count_snapshot` (R-11)

**Implementation**: A `#[test]` that reads all detection source files and `report.rs`, counts
occurrences of `threshold:` in claim string contexts (lines containing `claim`, `format!`,
or `action`), and asserts the count matches a known baseline (9 sites + any explicitly added).

```rust
#[test]
fn test_threshold_language_count_snapshot() {
    let detection_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../unimatrix-observe/src/detection/");
    let report_file = concat!(env!("CARGO_MANIFEST_DIR"), "/../../unimatrix-observe/src/report.rs");
    // Read all .rs files, count "threshold:" in claim contexts
    // Assert count == EXPECTED_THRESHOLD_SITES (set during implementation)
}
```

### AC-14: Session Table Enhancement

#### Test: `test_session_table_enhancement` (AC-14)

**Setup**: `SessionSummary` with `tool_distribution` having all four categories and
`agents_spawned = vec!["uni-architect", "uni-rust-dev"]`.
**Assert**:
- Session table header contains tool distribution column (`NR NE NW NS` or similar).
- Session table header contains `Agents` column.
- Row renders agent names.

Note: existing `test_session_table_two_rows` will need updating if the table structure changes.
This is an AC-17 permitted update.

### AC-15: Top File Zones

#### Test: `test_top_file_zones` (AC-15)

**Setup**: `SessionSummary.top_file_zones = vec![("crates/server/src".to_string(), 42), ("crates/store/src".to_string(), 18)]`.
**Assert**: Rendered output contains `Top file zones:` line with both paths.

---

## Static Assertion (R-11 Support)

The `test_threshold_language_count_snapshot` test uses `include_str!` or `std::fs::read_to_string`
to scan detection files. It documents the known count of threshold-containing claim strings.
When a new detection rule is added and a new threshold site appears, this count changes,
failing the test and alerting the developer to update `format_claim_with_baseline`.

---

## AC-17: Existing Test Survival

The following existing tests in `retrospective.rs` will fail after the col-026 changes and
MUST be updated by the implementation agent (these are not regressions — they are expected
spec-driven changes to existing assertions):

| Test | Current assertion | Updated assertion |
|------|------------------|-------------------|
| `test_header_contains_feature_cycle` | `# Retrospective: nxs-010` | `# Unimatrix Cycle Review — nxs-010` |
| `test_markdown_output_starts_with_header` | `starts_with("# Retrospective: test-001")` | `starts_with("# Unimatrix Cycle Review — test-001")` |
| `test_all_none_optional_fields_valid_markdown` | `assert!(!text.contains("## Sessions"))` etc. | Add `assert!(!text.contains("## Phase Timeline"))`, `assert!(!text.contains("## What Went Well"))` |
| `test_full_report_all_sections` | Checks for existing sections | Must also check `## Recommendations` appears before `## Findings` |
| `test_single_optional_feature_knowledge_reuse` | `FeatureKnowledgeReuse { delivery_count: 10, cross_session_count: 3, ... }` | Add new fields to the literal |

The implementation agent verifies AC-17 by running `cargo test -p unimatrix-server -- retrospective`
before merging.

---

## Edge Cases

- Report with `phase_stats = Some([...])` but `hotspots = vec![]`: Phase Timeline renders,
  Findings section omitted. No crash.
- Report with What Went Well candidates but empty `baseline_comparison = Some([])`:
  section omitted.
- Knowledge Reuse section when `top_cross_feature_entries.len() == 5`: all 5 rendered. No
  truncation at 5.
- Goal string containing a newline: formatted on a single line (newlines stripped or escaped).
  Test `test_header_goal_with_newline`.
- `gate_outcome_text` containing markdown injection (`\n## Injected`): formatter renders
  in-line, does not create a spurious section header. Test `test_gate_outcome_text_injection`.
