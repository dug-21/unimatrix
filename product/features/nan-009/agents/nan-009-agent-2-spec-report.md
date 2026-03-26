# Agent Report: nan-009-agent-2-spec

## Output

SPECIFICATION.md written to:
`product/features/nan-009/specification/SPECIFICATION.md`

## Acceptance Criteria Count

12 acceptance criteria (AC-01 through AC-12).

## Key Decisions Made

### Serde null-suppression (SR-01)
`ScenarioContext.phase` (extraction side, `types.rs`) carries `#[serde(default, skip_serializing_if = "Option::is_none")]`. Null-phase scenarios emit no `"phase"` key in JSONL — not `"phase": null`. The report-side `ScenarioResult` copy uses `#[serde(default)]` only (sufficient for read-only deserialization). This resolves SR-01 explicitly in the spec.

### Golden-output render test is mandatory (SR-02)
AC-12 mandates a golden-output test asserting exact section order (section 6 = Phase-Stratified Metrics, section 7 = Distribution Analysis). It is a required acceptance criterion, not advisory. Addresses pattern #3426 risk.

### Round-trip integration test is mandatory (SR-03)
AC-11 mandates an end-to-end integration test: extraction → replay → report. The test asserts `phase` is non-null in both the result JSON and the rendered section 6. A partial two-copy update (only one `ScenarioResult` gets `phase`) must cause this test to fail. Closes the dual-type compile-time gap.

### Null-phase label is canonically `"(none)"` (SR-04)
SCOPE.md Constraint 5 used `"(unset)"` while Goals/AC used `"(none)"`. Specification resolves this in favour of the Goals/AC wording: `"(none)"` everywhere. `"(unset)"` is explicitly prohibited in implementation, tests, and documentation.

### Phase is suppressed (not empty-table) when all-null (SR-05)
AC-04 makes the omission condition explicit and testable: section 6 must be entirely absent from rendered output when all results have null phase. A golden-output test covers both the present and absent cases.

### Documentation must include UDS/pre-col-028 caveat (SR-06)
FR-11 includes the requirement that `docs/testing/eval-harness.md` note that phase requires MCP-sourced sessions that called `context_cycle`; UDS-only and pre-col-028 corpora will produce no phase section.

## Key Constraints Surfaced

1. Three-site sync: `types.rs`, `runner/output.rs`, `report/mod.rs` — all must carry `phase`. No compile-time guard exists; AC-11 round-trip test is the substitute.
2. Serialization asymmetry: `skip_serializing_if` mandatory on the producing side only; report-side uses `#[serde(default)]` alone.
3. Report path must remain fully synchronous — no async, no tokio, no DB access in `compute_phase_stats` or `render_phase_section`.
4. Phase must not influence `ServiceSearchParams` or `AuditContext` during replay (measurement purity constraint).
5. Null-phase label is `"(none)"`, not `"(unset)"` — canonical choice resolves SCOPE.md internal inconsistency.

## Knowledge Stewardship

- Queried: /uni-query-patterns for eval harness phase stratification, serde null-suppression, golden-output regression, dual-type constraint -- found patterns #3255, #3426, #3526, #3550, #3512, #3555 and ADR #3522. All were directly applied to requirement and AC authoring.
