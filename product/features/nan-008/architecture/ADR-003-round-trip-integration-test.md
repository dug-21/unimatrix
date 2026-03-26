## ADR-003: Round-Trip Integration Test Guards Dual Type Copy and Section Order

### Context

Two independent high-severity risks converge on the same failure mode:

**SR-01 (dual type copy divergence)**: `runner/output.rs` and `report/mod.rs`
maintain independent type copies of `ScoredEntry`, `ProfileResult`, and
`ComparisonMetrics`. When a field is added in one copy but not the other,
`serde(default)` silently produces zero-valued metrics with no compile error
and no failing test unless a test explicitly checks for non-zero values in
the report output.

**SR-06 (section-order regression in render.rs)**: Adding section 6
(Distribution Analysis) to `render.rs` is a formatter-style change where
section order regressions are a documented recurring pattern. Individual unit
tests of rendering functions do not catch a misplaced or duplicate section
unless they check the full report string with position assertions.

Both risks produce silent, plausible-looking output. A report that is missing
CC@k/ICD columns or has section 6 before section 5 is not obviously wrong
from a build perspective.

A single integration test that:
1. Constructs a `ScenarioResult` with non-zero `cc_at_k`, `icd`, `cc_at_k_delta`,
   `icd_delta` values
2. Writes it to a temp directory as JSON
3. Calls `run_report` against that directory
4. Asserts the full content of the rendered Markdown

...catches both failure modes simultaneously. If the report copy is missing a
field, the CC@k or ICD value in the report will be 0.0 (or absent), causing
the assertion to fail. If section 6 is misplaced, the position assertion
fails.

### Decision

Add one integration test to `report/tests.rs`:

```
test_report_round_trip_cc_at_k_icd_fields_and_section_6
```

The test:
1. Creates a `ScenarioResult` with `cc_at_k: 0.857`, `icd: 1.234`,
   `cc_at_k_delta: 0.143`, `icd_delta: 0.211` (all non-zero, non-trivially
   round values to make accidental matches visible).
2. Serializes it to JSON and writes to a TempDir.
3. Calls `run_report`.
4. Asserts:
   - `content.contains("0.857")` or the CC@k column header appears in section 1
   - `content.contains("## 6. Distribution Analysis")`
   - Position order: `pos(## 1.) < pos(## 2.) < ... < pos(## 5.) < pos(## 6.)`
   - Section 6 contains at least the CC@k range table or ICD guidance text

This test is sufficient to catch SR-01 for all new fields and SR-06 for the
new section. It does not duplicate the existing section-order test
(`test_report_contains_all_five_sections`) — that test is extended to also
assert `## 6. Distribution Analysis`.

Unit tests for individual render functions (CC@k range table formatting,
ICD guidance text, etc.) remain valuable but are not substitutes for this
round-trip test.

### Consequences

- A single integration test provides the primary guard for both SR-01 and SR-06.
- Delivery agents must not skip the round-trip test even when individual unit
  tests pass.
- The existing five-section order test (`test_report_contains_all_five_sections`)
  must be updated to include section 6 in its assertions.
- Test helpers in `report/tests.rs` (`make_profile_result`, `make_scenario_result`)
  must be updated to include the new fields so construction compiles after the
  type changes.
