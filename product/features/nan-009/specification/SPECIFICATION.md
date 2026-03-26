# SPECIFICATION: nan-009 — Phase Context in Eval Scenarios for Phase-Stratified Metrics

GH Issue: #400

---

## Objective

The eval harness (nan-007/nan-008) aggregates all retrieval metrics across a single undifferentiated query corpus, making it impossible to measure whether retrieval quality differs by workflow phase (design, delivery, bugfix). This feature adds a `phase` field to `ScenarioContext` and `ScenarioResult`, propagates it through the extraction-replay-report pipeline, and introduces a per-phase aggregate section (section 6) in the eval report. The result is the primary measurement instrument required by ASS-032 Loop 2 before phase-conditioned retrieval scoring can be activated.

---

## Functional Requirements

### FR-01 — Scenario extraction: SQL selection
The `eval scenarios` SQL query in `eval/scenarios/output.rs` must SELECT the `phase` column from `query_log`. Currently absent from the SELECT list; the column exists in the database schema since col-028 (GH #403).

### FR-02 — Scenario extraction: struct field
`ScenarioContext` in `eval/scenarios/types.rs` must include a `phase: Option<String>` field decorated with `#[serde(default, skip_serializing_if = "Option::is_none")]`. The field must be populated from the `phase` column returned by the SQL query.

### FR-03 — Scenario extraction: row mapping
`build_scenario_record` in `eval/scenarios/extract.rs` must read `phase` from the query result row using `row.try_get::<Option<String>, _>("phase")?` and assign it to `context.phase`.

### FR-04 — Result passthrough: runner copy
`ScenarioResult` in `eval/runner/output.rs` must include `phase: Option<String>` decorated with `#[serde(default)]` only — no `skip_serializing_if`. The runner copy must emit an explicit `"phase": null` for null-phase results so that downstream consumers (including `eval report`) can distinguish "phase was null" from "phase field absent." The field must be set in `replay_scenario` by copying `record.context.phase.clone()` onto the constructed result.

### FR-05 — Result passthrough: report module copy
The report module's local `ScenarioResult` copy in `eval/report/mod.rs` must include `phase: Option<String>` decorated with `#[serde(default)]`. Both copies of `ScenarioResult` must carry `phase` in sync. (`skip_serializing_if` is optional on the report-side copy as it is read-only in the report path; `#[serde(default)]` is mandatory for backward compat.)

### FR-06 — Phase is metadata only during replay
During `eval run` replay, `phase` must not be injected into `ServiceSearchParams` or `AuditContext`. The replay reproduces the query as originally issued; phase signal must not influence the execution path being measured.

### FR-07 — Per-phase aggregation function
A function `compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats>` must be added (in `eval/report/aggregate.rs` or a new `eval/report/aggregate_phase.rs` if the file approaches 500 lines). It must:
- Group results by `result.phase`, treating `None` as a distinct key.
- Within each group, compute mean P@K, mean MRR, mean CC@k, mean ICD, and scenario count.
- Return a `Vec<PhaseAggregateStats>` sorted alphabetically by phase label, with the `"(unset)"` bucket last.
- Be entirely synchronous (no async, no tokio, no database access).

### FR-08 — Per-phase report section (section 6)
`render_phase_section` must be added to `eval/report/render.rs`. It must:
- Render section 6 "Phase-Stratified Metrics" as a Markdown table with columns: Phase, Count, P@K, MRR, CC@k, ICD.
- Render one row per distinct phase value.
- Render the `"(unset)"` bucket row last.
- Omit the section entirely (not render an empty table) when all scenario results have `phase = None`.

### FR-09 — Section renumbering
The existing Distribution Analysis section must become section 7. All section-number references in `render.rs`, `mod.rs`, and documentation must be updated to reflect: section 6 = Phase-Stratified Metrics, section 7 = Distribution Analysis.

### FR-10 — Phase label in Notable Ranking Changes (section 2)
In `render_report`, the per-scenario header line in section 2 "Notable Ranking Changes" must include the phase label when `ScenarioResult.phase` is non-null. Phase is read directly from `ScenarioResult` in the renderer; `NotableEntry` must not be extended.

### FR-11 — Documentation update
`docs/testing/eval-harness.md` must be updated to:
- Document the `context.phase` field in the scenario format reference, stating the field is populated from `query_log.phase`.
- Document the known phase vocabulary (`design`, `delivery`, `bugfix`) and note the vocabulary is protocol-defined and may evolve; new values appear in the per-phase table automatically without a code change.
- Document section 6 "Phase-Stratified Metrics" in the report output reference.
- Note that phase population requires MCP-sourced sessions that called `context_cycle`; UDS-only and pre-col-028 corpora will produce no phase section.

---

## Non-Functional Requirements

### NFR-01 — Backward compatibility: deserialization
Pre-nan-009 scenario JSONL files and result JSON files (without a `phase` field) must deserialize without error. Missing `phase` defaults to `None` via `#[serde(default)]`. No existing scenario file or result file may be broken by this change.

### NFR-02 — Backward compatibility: serialization
`ScenarioContext.phase = None` must not emit an explicit `"phase": null` key in JSONL output. `#[serde(skip_serializing_if = "Option::is_none")]` is mandatory on the extraction-side struct. This preserves the wire shape for pre-nan-009 consumers and avoids changing the format of scenario files created before this feature (see SR-01).

### NFR-03 — Synchronous report path
No tokio runtime, no async functions, and no database access may be introduced in any report code path (`eval/report/`). `compute_phase_stats` and `render_phase_section` must be pure synchronous functions over in-memory data.

### NFR-04 — File size limit
No modified or created file may exceed 500 lines. If `aggregate.rs` approaches this limit, `compute_phase_stats` must be placed in a new `aggregate_phase.rs` sub-file.

### NFR-05 — No compile-time dual-type guard
No compile-time mechanism enforces synchrony between the two `ScenarioResult` copies. This is an accepted constraint from the existing architecture (pattern #3550). A mandatory round-trip integration test (see AC-11) substitutes as the runtime enforcement mechanism.

---

## Acceptance Criteria

### AC-01 — Phase present in extraction output when non-null
`context.phase` is present in JSONL output of `eval scenarios` when the source `query_log` row has a non-null `phase` value.
- Verification: integration test — insert row with `phase = "delivery"`, run extraction, assert `context.phase == "delivery"` in output JSONL.

### AC-02 — Phase absent from extraction output when null
When the source `query_log` row has `phase = NULL`, the extracted scenario JSONL omits the `phase` key entirely (not `"phase": null`). This preserves backward-compatible wire shape.
- Verification: integration test — insert row with `phase = NULL`, run extraction, assert JSONL for that scenario contains no `"phase"` key.

### AC-03 — Phase carried through to ScenarioResult
`phase` is present on `ScenarioResult` in per-scenario JSON produced by `eval run`, carrying the value from `context.phase` (or absent if null, per NFR-02 treatment applied uniformly).
- Verification: unit test on `replay_scenario` output struct; round-trip test in AC-11.

### AC-04 — Phase-Stratified Metrics section rendered when phase data exists
`eval report` renders section 6 "Phase-Stratified Metrics" when at least one scenario result has a non-null phase. The section is omitted entirely (not rendered as an empty table or empty heading) when all phase values are null.
- Verification: golden-output render test — one run with mixed-phase data asserts section 6 present; one run with all-null phases asserts section 6 absent.

### AC-05 — Section 6 table contents correct
Section 6 shows one row per distinct phase value. Each row includes: phase name, scenario count, mean P@K, mean MRR, mean CC@k, mean ICD, all computed over scenarios matching that phase. The `"(unset)"` bucket row appears last.
- Verification: unit test on `compute_phase_stats` with controlled input; golden-output render test.

### AC-06 — Pre-nan-009 result files deserialize cleanly
Pre-nan-009 result JSON files (without `phase` field) are deserialized by `eval report` without error; missing `phase` defaults to `null`/`None` via `#[serde(default)]`.
- Verification: unit test deserializing a JSON string without any `"phase"` key into the report module's `ScenarioResult`; assert no error and `phase == None`.

### AC-07 — Documentation updated
`docs/testing/eval-harness.md` documents: the `context.phase` field; the known vocabulary
as a snapshot (`design`, `delivery`, `bugfix`, `(unset)` for missing — not a fixed
allowlist); section 6 in the report reference; the note that phase requires MCP-sourced
sessions with `context_cycle`; and the migration-based governance model (new values appear
automatically, retroactive relabeling uses a `query_log` data migration).
- Verification: documentation review; confirm all four items above are present.

### AC-08 — Phase label in section 2 Notable Ranking Changes
Section 2 "Notable Ranking Changes" in `eval report` includes the phase label alongside each scenario header line when phase is non-null. Header lines for null-phase scenarios are unchanged.
- Verification: golden-output render test with mixed-phase results; assert phase label present for non-null scenarios and absent for null-phase scenarios in section 2 output.

### AC-09 — Unit tests for extraction and aggregation
Unit tests cover:
1. `ScenarioContext` with non-null phase serializes and the `phase` key is present.
2. `ScenarioContext` with null phase serializes and the `phase` key is absent (no `"phase": null`).
3. `compute_phase_stats` groups results correctly when results have mixed phase values.
4. `compute_phase_stats` returns no rows (or suppresses section) when all phases are null.
5. Section 6 is omitted from rendered output when no non-null phases are present.
- Verification: each item above corresponds to a named unit test.

### AC-10 — SQL extraction integration test
An integration test inserts a `query_log` row with `phase = "delivery"` and verifies that the extracted JSONL contains `context.phase = "delivery"`. An integration test inserts a row with `phase = NULL` and verifies the key is absent from JSONL.
- Verification: integration test in `eval/scenarios/tests.rs`, using the existing `insert_query_log_row` helper extended to accept `phase: Option<&str>`.

### AC-11 — Round-trip integration test (dual-type guard)
An end-to-end round-trip integration test covers: scenario extraction with a non-null phase → `eval run` replay → `eval report` rendering. The test asserts that:
1. The rendered section 6 contains a row for the non-null phase.
2. `phase` is non-null in the `ScenarioResult` JSON produced by `eval run`.

A partial update to only one of the two `ScenarioResult` copies must cause this test to fail (addressing SR-03).
- Verification: integration test that writes scenario JSONL, runs replay, checks result JSON, runs report, checks section 6 output.

### AC-12 — Golden-output render test for section order
A golden-output test for `eval report` render exists and is updated before delivery. The test asserts the exact section order: section 6 = "Phase-Stratified Metrics", section 7 = "Distribution Analysis". A section-order regression (e.g., Distribution Analysis rendered before Phase-Stratified Metrics) must cause this test to fail (addressing SR-02).
- Verification: golden-output test file checked in alongside the implementation.

---

## Domain Models

### ScenarioContext
The metadata captured at scenario extraction time from `query_log`. Fields include `agent_id`, `feature_cycle`, `session_id`, `retrieval_mode`, and (post-nan-009) `phase`.

### ScenarioRecord
The unit of work for `eval run`. Contains a `query_text`, a `context` (`ScenarioContext`), and expected results. Stored as JSONL in the scenario output directory.

### ScenarioResult
The output of replaying one `ScenarioRecord`. Contains per-scenario metric values and (post-nan-009) `phase` copied from the source context. Two independent copies exist: one in `eval/runner/output.rs` (produced) and one in `eval/report/mod.rs` (consumed). Both must carry `phase`.

### PhaseAggregateStats
A new struct representing aggregate metrics for one phase stratum. Fields: `phase_label: String` (the phase value, or `"(unset)"` for null), `count: usize`, `mean_p_at_k: f64`, `mean_mrr: f64`, `mean_cc_at_k: f64`, `mean_icd: f64`.

### Phase Vocabulary
Phase is a soft vocabulary key: `query_log.phase` is `TEXT` with no schema-level enum or
CHECK constraint. The harness records and displays whatever the log contains; no validation
against a fixed set is performed. Known values as of nan-009 (snapshot, not allowlist):
`"design"`, `"delivery"`, `"bugfix"`. Vocabulary governance is migration-based — new phase
strings appear in the report automatically when new rows use them; retroactive relabeling
of old rows is a `query_log` data migration, not a schema change. The null bucket renders
as `"(unset)"`. Documentation must describe the known values as a current snapshot and
must note the migration-based governance model.

### Phase Population Requirement
`query_log.phase` is populated only for MCP-sourced sessions that called `context_cycle` (col-028, GH #403). UDS-sourced rows have `phase = NULL` by definition. Pre-col-028 rows also have `phase = NULL`. The phase section in the report will be absent for corpora built entirely from such sessions.

---

## User Workflows

### Workflow 1 — Extracting phase-labelled scenarios
1. User runs `eval scenarios` against a database with post-col-028 MCP sessions.
2. The command SELECTs `phase` from `query_log`.
3. Extracted JSONL files include `"phase": "delivery"` (or similar) in the `context` object when the source row is non-null; no `phase` key appears when null.

### Workflow 2 — Replaying scenarios and preserving phase
1. User runs `eval run` over scenario JSONL files.
2. `replay_scenario` copies `context.phase` onto each `ScenarioResult`.
3. Per-scenario result JSON includes `"phase"` when non-null; key is absent for null-phase results.
4. Phase does not influence how the search is invoked (measurement purity).

### Workflow 3 — Generating a phase-stratified report
1. User runs `eval report` over result JSON files containing mixed-phase data.
2. `compute_phase_stats` groups results by phase label.
3. Report section 6 "Phase-Stratified Metrics" renders a Markdown table with one row per phase.
4. Report section 2 "Notable Ranking Changes" includes the phase label on each non-null-phase scenario header.
5. If all results have null phase, section 6 is omitted; section numbering shifts accordingly (Distribution Analysis becomes the effective section 6 in rendering but retains the section-7 heading).

### Workflow 4 — Consuming pre-nan-009 result files
1. User runs `eval report` over legacy result JSON files (no `phase` field).
2. Deserialization succeeds via `#[serde(default)]`; `phase` defaults to `None`.
3. All results have `phase = None`; section 6 is omitted; report is otherwise identical to pre-nan-009 output.

---

## Constraints

1. **Backward-compatible scenario format.** `ScenarioContext.phase` is `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Existing JSONL files without this field deserialize cleanly. No `"phase": null` key is emitted for null-phase records.

2. **Dual-type constraint.** `runner/output.rs` and `report/mod.rs` maintain independent copies of `ScenarioResult`. Both must gain `phase: Option<String>` in sync. Pattern #3550 documents this constraint. A round-trip integration test (AC-11) is the mandatory enforcement mechanism.

3. **Phase is metadata only during replay.** `phase` must not be injected into `ServiceSearchParams` or `AuditContext` during `eval run`. Replay must reproduce the query as issued to measure the current retrieval pipeline without phase signal.

4. **Report section is synchronous.** `eval/report/` is an entirely synchronous module. No tokio, no async, and no database access may be introduced in any report code path, including `compute_phase_stats` and `render_phase_section`.

5. **Null-phase label.** In the per-phase table, rows where phase is null are grouped under the label `"(unset)"` and sorted last. The label `"(unset)"` is canonical; `"(none)"` must not be used anywhere in the implementation, tests, or documentation (resolves SR-04 inconsistency).

6. **No mandatory phase filter on `eval scenarios`.** No `--phase` CLI flag is added. Phase filtering can be performed with existing tooling (jq/grep on JSONL output).

7. **Max 500 lines per file.** New functions must be placed in existing modules or in a new `aggregate_phase.rs` sub-file if `aggregate.rs` approaches the limit.

8. **No phase enum validation.** The harness records whatever `query_log.phase` contains. No allowlist, enum, or variant check is performed on phase values at extraction, replay, or report time.

---

## Dependencies

| Dependency | Source | Notes |
|---|---|---|
| `query_log.phase` column | col-028 (GH #403) | Already present in schema; `QueryLogRecord.phase: Option<String>`. Dependency satisfied. |
| `eval/scenarios/` module | nan-007 | Provides `ScenarioContext`, `ScenarioRecord`, `build_scenario_record`, `insert_query_log_row` test helper. |
| `eval/runner/` module | nan-007 | Provides `ScenarioResult` (runner copy), `replay_scenario`. |
| `eval/report/` module | nan-007/nan-008 | Provides `compute_aggregate_stats`, `render_report`, report module `ScenarioResult` copy. Section numbering changes apply here. |
| `serde` + `serde_json` | Existing workspace deps | `#[serde(default)]`, `#[serde(skip_serializing_if)]` attributes used on new fields. |
| Pattern #3255 | Unimatrix knowledge base | `serde(default)` alone does not suppress null on serialization — `skip_serializing_if = "Option::is_none"` required on the producing side. |
| Pattern #3550 | Unimatrix knowledge base | Dual-type constraint: three-site sync (types.rs, runner/output.rs, report/mod.rs). |
| Pattern #3426 | Unimatrix knowledge base | Golden-output test required for section-order regression guard. |
| Pattern #3526 | Unimatrix knowledge base | Round-trip integration test strategy for dual-type and schema boundary risk. |
| ADR #3522 | Unimatrix knowledge base | nan-008 precedent: round-trip integration test guards dual-type copy and section order. |

---

## NOT in Scope

- Phase-conditioned retrieval scoring (`w_phase_explicit` activation, phase-affinity matrix). This feature adds measurement only.
- `--phase` filter flag on `eval scenarios` or `eval run` CLI.
- Changes to regression detection logic (report section 5).
- Per-phase delta columns (requires paired baseline/candidate per stratum; deferred).
- NEER metric (requires session-level tracking; deferred from nan-008).
- Changes to `eval run` replay execution logic beyond the metadata passthrough.
- Phase enum validation or allowlist enforcement anywhere in the pipeline.
- Changes to the baseline recording procedure (`eval-baselines/log.jsonl`).
- Per-phase × profile cross-product table (deferred to a later iteration).

---

## Open Questions

None. All open decisions from SCOPE.md have been resolved:

- **RD-01**: Section 6 = Phase-Stratified Metrics; section 7 = Distribution Analysis.
- **RD-02**: One row per phase (not per phase × profile).
- **RD-03**: Phase vocabulary is free-form; known values documented as `design`, `delivery`, `bugfix`; null bucket renders as `"(unset)"`; no enum validation.
- **RD-04**: Phase label in section 2 read from `ScenarioResult` directly in renderer; `NotableEntry` tuple not extended.
- **SR-04**: Null-phase label is canonically `"(unset)"`. Human-confirmed decision: `"(unset)"` unambiguously signals field-not-populated and cannot collide with any real phase value (real values never use parentheses).
- **SR-01**: `ScenarioContext.phase` uses `#[serde(default, skip_serializing_if = "Option::is_none")]`; null phase is not emitted as `"phase": null`.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for eval harness phase stratification, serde null-suppression, golden-output regression testing, dual-type constraint -- found patterns #3255 (serde skip_serializing_if), #3426 (golden-output section-order guard), #3526 (round-trip integration test strategy), #3550/#3512 (dual-type constraint), #3555 (phase gap), ADR #3522 (nan-008 round-trip precedent).
