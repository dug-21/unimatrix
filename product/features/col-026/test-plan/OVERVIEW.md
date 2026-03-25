# Test Plan Overview — col-026: Unimatrix Cycle Review Enhancement

## Overall Test Strategy

col-026 spans five components across three crates. The test strategy follows three tiers:

1. **Unit tests** — pure computation functions tested in isolation with synthetic fixtures.
   Extend existing test modules in `retrospective.rs`, `knowledge_reuse.rs`, and `report.rs`.
   No isolated scaffolding created.
2. **Integration tests** — the MCP JSON-RPC interface exercised via infra-001. Phase Timeline
   tests seed `cycle_events` through the UDS write path (context_cycle tool). These validate
   behavior only observable through the full server binary.
3. **Static assertions** — grep-based snapshot tests for threshold language audit compliance
   (R-11) and prohibited inline `* 1000` conversions (R-01 scenario 3).

All new tests are appended to the existing `#[cfg(test)]` modules in the relevant files.
The `make_report()` helper in `retrospective.rs` must be extended to include new fields
(`goal`, `cycle_type`, `attribution_path`, `is_in_progress`, `phase_stats`) after the
struct change. Existing helper calls must compile without modification (new fields are
`Option<…>` and default to `None`).

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test File(s) | Key Scenarios |
|---------|----------|-------------|--------------|---------------|
| R-01 | Critical | phase-stats | `tools.rs` unit test module | boundary millisecond test; static grep for `* 1000` |
| R-02 | Critical | phase-stats | `tools.rs` unit test module | no `cycle_phase_end` rows; zero-duration; empty phase name |
| R-03 | Critical | phase-stats | `tools.rs` unit test module | 8 keyword scenarios incl. "compass" |
| R-04 | Critical | knowledge-reuse-extension | `knowledge_reuse.rs` tests | partial lookup; all-empty; zero-ID skip |
| R-05 | Critical | retrospective-report-extensions | `types.rs` inline tests + `retrospective.rs` | three-state derivation + formatter rendering |
| R-06 | High | formatter-overhaul | `retrospective.rs` | all 16 direction-table metrics; outlier exclusion |
| R-07 | High | formatter-overhaul | `retrospective.rs` | golden section-order test; all 11 headers in sequence |
| R-08 | High | formatter-overhaul | `retrospective.rs` | `format_claim_with_baseline` three paths; 9 claim formats |
| R-09 | High | phase-stats / formatter | `tools.rs` integration test | all three attribution path labels |
| R-10 | High | formatter-overhaul | `retrospective.rs` | multi-phase hotspot annotation; out-of-bounds timestamp |
| R-11 | Med | formatter-overhaul | static grep test | count-snapshot of `threshold` occurrences in detection code |
| R-12 | Med | phase-stats / formatter | `retrospective.rs` | `Some(vec![])` vs `None` JSON shape; handler canonicalization |
| R-13 | Low | retrospective-report-extensions / knowledge-reuse-extension | CI build | `cargo build` gate |

---

## Acceptance Criteria to Test Mapping

| AC-ID | Component | Primary Test |
|-------|-----------|-------------|
| AC-01 | formatter-overhaul | `test_header_rebrand` |
| AC-02 | formatter-overhaul | `test_header_goal_present`, `test_header_goal_absent` |
| AC-03 | formatter-overhaul | `test_cycle_type_classification` (5 keyword groups) |
| AC-04 | phase-stats | `test_attribution_path_labels` |
| AC-05 | retrospective-report-extensions | `test_is_in_progress_three_states` |
| AC-06 | formatter-overhaul | `test_phase_timeline_table` |
| AC-07 | formatter-overhaul | `test_phase_timeline_rework_annotation` |
| AC-08 | formatter-overhaul | `test_finding_phase_annotation` |
| AC-09 | formatter-overhaul | `test_burst_notation_rendering` |
| AC-10 | formatter-overhaul | `test_what_went_well_present`, `test_what_went_well_absent` |
| AC-11 | formatter-overhaul | `test_section_order` |
| AC-12 | formatter-overhaul | `test_knowledge_reuse_section` |
| AC-13 | formatter-overhaul | `test_no_threshold_language`, `test_no_allowlist_in_compile_cycles` |
| AC-14 | formatter-overhaul | `test_session_table_enhancement` |
| AC-15 | formatter-overhaul | `test_top_file_zones` |
| AC-16 | retrospective-report-extensions | `test_json_format_new_fields` |
| AC-17 | all | `cargo test -p unimatrix-server -- context_cycle_review` |
| AC-18 | knowledge-reuse-extension | `test_knowledge_reuse_serde_backward_compat` |
| AC-19 | recommendation-fix | `test_recommendation_compile_cycles_above_threshold` (updated), `test_permission_friction_recommendation_independence` |

---

## Cross-Component Test Dependencies

1. `make_report()` in `retrospective.rs` is shared by all formatter tests. It must be updated
   to include `None` for all new `RetrospectiveReport` fields after the struct change. This is
   compile-enforced; failure to update causes a compilation error.

2. `FeatureKnowledgeReuse {}` literal in `retrospective.rs` test fixtures must include all new
   fields when `FeatureKnowledgeReuse` gains required fields. Same compile-time enforcement.

3. `compute_knowledge_reuse` signature changes break all existing call sites in
   `knowledge_reuse.rs` tests. All must be updated to pass the new `entry_meta_lookup` closure.

4. Phase Timeline tests in `retrospective.rs` depend on `PhaseStats` struct which is defined in
   `unimatrix-observe/src/types.rs`. The type must be stable before formatter tests can compile.

5. The `test_section_order` golden test (R-07) depends on all new sections being present, so it
   is the last formatter test to be activated — it verifies the whole system together.

---

## Integration Harness Plan

### Suite Selection

col-026 touches `context_cycle_review` handler logic, the MCP response formatter, and the
`RetrospectiveReport` JSON shape. Based on the suite selection table:

| Reason | Suites to Run |
|--------|--------------|
| Formatter + tool logic changes | `tools`, `protocol` |
| New fields on JSON response | `lifecycle` (restart persistence), `tools` |
| Any change | `smoke` (mandatory minimum gate) |

**Suites to run in Stage 3c**: `smoke`, `tools`, `protocol`, `lifecycle`.

`security`, `confidence`, `contradiction`, `volume`, `edge_cases`, `adaptation` are not
required — col-026 does not touch security scanning, confidence math, contradiction
detection, or schema.

### Existing Suite Coverage of col-026 Behavior

The `tools` suite already exercises `context_cycle_review` with `format=markdown` and
`format=json`. Existing tests validate:
- Tool is discoverable and callable
- Empty feature cycle returns no-data response
- JSON format returns parseable JSON

These tests become regression guards for the header rebrand (AC-01) and section order (AC-11).
Any existing test asserting `# Retrospective:` in the output will fail after the rebrand —
the implementation agent must update those assertions.

### New Integration Tests Required

Two behaviors are only fully verifiable through the MCP interface (cycle_events seeded via
UDS write path):

**Test 1**: Phase Timeline populated when cycle_events exist.
- Fixture: `server` (fresh DB, default fixture).
- Seed via `context_cycle` calls: `cycle_start`, `cycle_phase_end` (scope→design), `cycle_stop`.
- Call `context_cycle_review(feature_cycle=..., format="markdown")`.
- Assert response contains `## Phase Timeline` and at least one table row.
- Location: `suites/test_tools.py` (extend `test_cycle_review_*` block).

**Test 2**: `is_in_progress` field present in JSON when cycle has no `cycle_stop`.
- Fixture: `server`.
- Seed: `cycle_start` only (no `cycle_stop`).
- Call `context_cycle_review(feature_cycle=..., format="json")`.
- Parse JSON; assert `is_in_progress == true`.
- Location: `suites/test_tools.py`.

**Test 3**: Knowledge reuse cross-feature split appears in markdown.
- Fixture: `server`.
- Store entries with `feature_cycle = "prior-feature"`, then call `context_cycle_review` for
  a different feature cycle that served those entries.
- Assert response contains `Cross-feature` and `Intra-cycle` in Knowledge Reuse section.
- Location: `suites/test_lifecycle.py` (extend knowledge-reuse lifecycle block).

### Failure Triage Rules (Stage 3c)

Per USAGE-PROTOCOL.md decision tree:
1. Failure in code changed by col-026 → fix the code, re-run, document in report.
2. Pre-existing failure → file GH Issue with `[infra-001]` prefix, mark `@pytest.mark.xfail(reason="Pre-existing: GH#NNN")`, continue.
3. Bad test assertion → fix the test, document in report.

Never fix unrelated integration test failures in the col-026 PR.

---

## Scope Boundaries

- Phase Timeline integration tests seed `cycle_events` via the `context_cycle` MCP tool only.
  Do NOT write directly to the SQLite DB in integration tests.
- `format_claim_with_baseline` is a private function. Test it via its public caller
  (`format_retrospective_markdown`) with synthetic detection output, not via direct invocation.
- The `compute_phase_stats` function is internal to `tools.rs` (or extracted to a module).
  Test it directly in a `#[cfg(test)]` block in the same file using `pub(crate)` visibility.
- `R-11` (threshold audit snapshot) is implemented as a `#[test]` that calls `grep` logic at
  runtime, or alternatively as a CI script. The test agent will implement the simpler approach
  (a `#[test]` using `std::fs::read_to_string` to scan detection files).
