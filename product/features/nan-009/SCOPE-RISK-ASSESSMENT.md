# Scope Risk Assessment: nan-009

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `serde(default)` on `phase: Option<String>` suppresses deserialization errors but does **not** suppress serialization of `null` — JSONL output will include explicit `"phase":null` keys on every pre-nan-009 result, changing wire shape | Med | High | Pair `#[serde(default, skip_serializing_if="Option::is_none")]` on `ScenarioContext.phase`; confirm whether the report round-trip treats absent vs. explicit null identically (pattern #3255) |
| SR-02 | Section renumbering (new section 6, Distribution Analysis shifts to 7) will silently break any test that asserts exact report output — pattern #3426 documents this as a recurring source of undetected regressions | High | High | Ensure a golden-output test for `eval report` render exists and is updated before delivery; architect must budget for this test as required, not optional |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Dual-type constraint (pattern #3550, #3526): `ScenarioResult` has independent copies in `runner/output.rs` and `report/mod.rs`; SCOPE.md states both must be updated in sync, but no enforcement mechanism exists — a partial update compiles successfully and silently drops `phase` from the report | High | Med | Spec writer should mandate an integration round-trip test (extract → run → report) that asserts `phase` is non-null in the rendered section 6 output, making a partial update a test failure |
| SR-04 | SCOPE.md Constraint 5 says `None` renders as `"(unset)"`, but Goals §5/AC-04/AC-05 say `"(none)"` — label is inconsistent within the document | Low | High | Resolve wording before architecture; pick one label and use it uniformly so the spec and implementation agree |
| SR-05 | No `eval run` replay filter by phase is in scope, but `compute_phase_stats` will silently produce a degenerate single-bucket `"(unset)"` table when all scenarios are pre-col-028 (no phase data) — easy to misread as a bug | Med | Med | AC-04 already guards this (section omitted when all phases null); spec writer should make the omission condition explicit and testable |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `query_log.phase` is `None` for all UDS-sourced rows (col-028 comment: "None for UDS rows") — a corpus built exclusively from UDS sessions will produce an empty phase section even after nan-009 lands, potentially masking the feature as unimplemented | Med | Med | Document in eval-harness.md that phase population requires MCP-sourced sessions that called `context_cycle`; architect should consider whether a warning is emitted when the phase section is suppressed |
| SR-07 | `eval report` is fully synchronous (Constraint 4); `compute_phase_stats` must not introduce any dependency on async or database paths — risk is low given the existing module structure, but adding a new aggregation function is the most likely site for this to slip | Low | Low | Spec writer should state the synchronous constraint explicitly on `compute_phase_stats` |

## Assumptions

- **Goals §1–3 (scenario extraction)**: Assumes `query_log.phase` is reliably populated for MCP sessions post-col-028. If sessions ran before col-028 schema migration or used UDS exclusively, `phase` will be NULL for all rows — nan-009 delivers no visible output in those environments.
- **Goals §5 / Proposed Approach Change 3**: Assumes `ScenarioResult` in both `runner/output.rs` and `report/mod.rs` can be kept in sync manually. No compile-time enforcement of this invariant exists (SCOPE.md §Background "Dual-type constraint").
- **Non-Goals §3**: Assumes phase must NOT influence `ServiceSearchParams` during replay. This is correct for measurement purity but must be checked against any future feature that injects phase into retrieval — a later feature could accidentally activate the `w_phase_explicit` path during replay.

## Design Recommendations

- **SR-01 + SR-03**: Architect should decide whether `ScenarioContext` and `ScenarioResult` adopt a shared type or continue as independent copies. Introducing a shared `EvalPhase` newtype now (even as a type alias) reduces the three-site sync risk and makes the constraint visible at compile time.
- **SR-02**: Require a golden-output render test as part of the acceptance criteria. Pattern #3426 shows this is the reliable guard against section-order regressions.
- **SR-04**: Standardise the null-phase label (`"(none)"` vs `"(unset)"`) in SCOPE.md before the spec is written.
- **SR-06**: Add a note to the documentation goal (Goal §7) that phase population requires `context_cycle` to have been called in the session; omission of the section is expected for UDS-only or pre-col-028 corpora.
