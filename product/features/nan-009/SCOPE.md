# nan-009: Phase-Stratified Eval Scenarios

## Problem Statement

The eval harness (nan-007/nan-008) treats all queries identically regardless of which
workflow phase (design, delivery, bugfix) generated them. Every metric — P@K, MRR,
CC@k, ICD — is aggregated across the full scenario corpus as a single undifferentiated
population. This makes the harness blind to the question that motivates Loop 2 of the
self-learning architecture (ASS-032): "does the retrieval pipeline perform differently
by phase, and are phase-conditioned improvements measurable?"

Phase-conditioned retrieval is a planned scoring improvement (activating the
`w_phase_explicit = 0.0` placeholder). Without phase-stratified eval output, there is
no instrument to measure whether it actually improves per-phase recall. Adding a `phase`
field to the scenario context and producing per-phase aggregate rows in the report fills
this gap.

The `query_log` table already has a `phase` column (delivered in col-028, GH #403).
The dependency on GH #397 mentioned in the issue is therefore already satisfied.

## Goals

1. Extend `ScenarioContext` in `eval/scenarios/types.rs` to include a `phase: Option<String>` field, populated from `query_log.phase` during `eval scenarios` extraction.
2. Extend the `eval scenarios` SQL query in `eval/scenarios/output.rs` to SELECT the `phase` column.
3. Extend `eval/scenarios/extract.rs` (`build_scenario_record`) to map `query_log.phase` into `ScenarioContext.phase`.
4. Extend `ScenarioResult` in `eval/runner/output.rs` to carry `phase: Option<String>` (passthrough from scenario context; not computed).
5. Extend `eval report` to produce a per-phase aggregate section (section 6): for each distinct phase value present in the result files, compute and display mean P@K, MRR, CC@k, and ICD, plus scenario count. Existing Distribution Analysis shifts to section 7.
6. Extend the `eval report` summary section to label each scenario with its phase in the per-scenario ranking-changes detail view (section 2), so phase is visible in the human-reviewed artifacts.
7. Update `docs/testing/eval-harness.md` to document the `phase` field in the scenario context and the per-phase aggregate section.

## Non-Goals

- **No phase-conditioned retrieval logic.** This feature adds measurement instrumentation only. Activating `w_phase_explicit` and building the phase-affinity matrix are separate features.
- **No new CLI flags.** No `--phase` filter argument on `eval scenarios` or `eval run`. Filtering by phase can be done with existing tooling (grep/jq on the JSONL). Avoid scope creep.
- **No changes to the regression detection logic** (section 5 of the report). Phase is informational in this feature; it does not gate anything.
- **No per-phase P@K/MRR delta columns.** The per-phase section shows absolute means only; delta computation requires paired baseline/candidate within each phase stratum, which adds complexity. Phase delta is deferred until the feature produces more phase-labelled data.
- **No NEER metric.** Novel Entry Exposure Rate requires session-level tracking across queries. Deferred as in nan-008.
- **No changes to `eval run` replay logic.** Phase is carried through as metadata on the scenario record; it does not influence how the search is invoked.
- **No phase enum validation.** Phase values are free-form strings from the session state. The harness records whatever the log contains; it does not enforce a fixed vocabulary.
- **No changes to the baseline recording procedure** (eval-baselines/log.jsonl). That file records corpus-wide baselines; per-phase breakdown is in the eval report output only.

## Background Research

### query_log.phase already exists (col-028, GH #403)

`crates/unimatrix-store/src/query_log.rs` — `QueryLogRecord.phase: Option<String>` is
present and fully implemented. The column is inserted as `?9` and read at index 9 in all
three scan functions. The comment on the field says "col-028: workflow phase at query time;
None for UDS rows." This is the exact column nan-009 needs to read.

### eval scenarios SQL does not select phase (gap confirmed)

`crates/unimatrix-server/src/eval/scenarios/output.rs`, line 108 — the SELECT in
`do_scenarios` reads:
```
SELECT query_id, session_id, query_text, retrieval_mode, source,
       result_entry_ids, similarity_scores
FROM query_log
```
The `phase` column is absent. This is the primary gap. The column exists in the DB;
it is simply not fetched.

### ScenarioContext has no phase field (gap confirmed)

`crates/unimatrix-server/src/eval/scenarios/types.rs` — `ScenarioContext` has four
fields: `agent_id`, `feature_cycle`, `session_id`, `retrieval_mode`. No `phase` field.
`build_scenario_record` in `extract.rs` reads from the row by column name; adding `phase`
requires a new `row.try_get::<Option<String>, _>("phase")?` call and adding the field to
the struct.

### ScenarioRecord flows through the entire pipeline

`ScenarioRecord` is deserialized in `eval/runner/replay.rs::load_scenarios`, then passed
to `replay_scenario` and `run_single_profile`. The `context` field is used to build
`ServiceSearchParams` and `AuditContext`. Phase is not currently used in replay; it would
be passed through to `ScenarioResult` as metadata only.

### Report aggregation is phase-unaware

`eval/report/aggregate.rs::compute_aggregate_stats` groups results by profile name only.
There is no grouping dimension for phase. The per-phase section requires a new aggregation
function, `compute_phase_stats`, that groups `ScenarioResult`s by `scenario.phase` and
computes the same mean metrics as `compute_aggregate_stats` but within each phase stratum.

`ScenarioResult` in `eval/runner/output.rs` does not carry `phase`. To enable per-phase
aggregation in the report, `ScenarioResult` must carry the phase value from the scenario
that produced it. This is a passthrough: `replay.rs::replay_scenario` reads
`record.context.phase` and stores it on the result.

### Dual-type constraint (pattern #3550)

`runner/output.rs` and `report/mod.rs` maintain independent copies of result types.
Both must be updated when new fields are added. Adding `phase: Option<String>` to
`ScenarioResult` requires updating both copies. The report module's copy uses
`#[serde(default)]` on all optional fields; `phase` must follow this pattern.

### nan-008 non-goal note (now un-deferred)

nan-008 SCOPE.md explicitly deferred "No ICD per-phase breakdown. The per-phase
breakdown of ICD requires #397 (phase-in-scenarios) and is deferred." Since #397
(col-028) is already complete, nan-009 is the natural successor.

### ASS-032 research context

The Research Synthesis (RESEARCH-SYNTHESIS.md) describes Loop 2 as a
phase-conditioned frequency table built from QUERY_LOG over a rolling 30-day window.
The eval harness is described as needing phase-stratified metrics as the primary
measurement instrument for Loop 2. This feature delivers exactly that instrument,
without implementing the learning loop itself.

### Test infrastructure

Existing tests in `eval/scenarios/tests.rs` insert rows via `insert_query_log_row`,
which already binds `phase` as `Option::<String>::None` (IR-03 in the comment). Tests
for phase extraction need rows with non-null phase values. The helper in tests.rs should
be extended to accept `phase: Option<&str>` or a new overloaded helper added.

## Proposed Approach

### Change 1 — Scenario extraction

**`eval/scenarios/types.rs`**: Add `phase: Option<String>` to `ScenarioContext`.

**`eval/scenarios/output.rs`**: Add `phase` to the SELECT column list.

**`eval/scenarios/extract.rs`**: Add `row.try_get::<Option<String>, _>("phase")?` and
populate `context.phase`.

### Change 2 — Result passthrough

**`eval/runner/output.rs`**: Add `phase: Option<String>` to `ScenarioResult` with
`#[serde(default)]`.

**`eval/runner/replay.rs`**: In `replay_scenario`, set
`phase: record.context.phase.clone()` on the constructed `ScenarioResult`.

**`eval/report/mod.rs`**: Add `phase: Option<String>` with `#[serde(default)]` to the
report module's local `ScenarioResult` copy.

### Change 3 — Per-phase aggregate section (report section 6)

**`eval/report/aggregate.rs`**: Add `compute_phase_stats(results)` that groups by
`result.phase`, then within each group computes mean P@K, MRR, CC@k, ICD, and scenario
count. Phase key `None` renders as `"(unset)"` in output. Returns a
`Vec<PhaseAggregateStats>` sorted alphabetically by phase name, with `"(unset)"` last.
One row per phase (not per phase × profile) — first iteration. Delta columns give
cross-profile signal without multiplying rows.

**`eval/report/render.rs`**: Add `render_phase_section` that produces section 6
"Phase-Stratified Metrics" as a Markdown table. Only rendered when at least one scenario
has a non-null phase. Existing section 6 (Distribution Analysis) shifts to section 7.

**`eval/report/mod.rs`**: Wire `compute_phase_stats` into `run_report` and pass result
to `render_report` / `render_phase_section`.

### Change 4 — Phase label in ranking-changes section

**`eval/report/render.rs`**: In `render_report` section 2 "Notable Ranking Changes",
add `phase` to the per-scenario header line alongside `scenario_id` and `query`. This
requires passing phase through `find_notable_ranking_changes` return type or reading it
from the `ScenarioResult`.

### Change 5 — Documentation

**`docs/testing/eval-harness.md`**: Document `context.phase` in the scenario format
reference. Document section 6 (Phase-Stratified Metrics) in the report output reference.

## Acceptance Criteria

- AC-01: `context.phase` is present in JSONL output of `eval scenarios` when the source
  `query_log` row has a non-null `phase` value.
- AC-02: `context.phase` is `null` in JSONL output when the source `query_log` row has
  `phase = NULL` (backwards-compatible; no existing scenario files are broken).
- AC-03: `phase` is present on `ScenarioResult` in per-scenario JSON produced by
  `eval run`, carrying the value from `context.phase` (or `null` if absent).
- AC-04: `eval report` section 6 "Phase-Stratified Metrics" is rendered when at least
  one scenario result has a non-null phase. Section 6 is omitted entirely (not rendered
  as empty) when all phase values are null.
- AC-05: Section 6 shows one row per distinct phase value with: phase name, scenario
  count, mean P@K, mean MRR, mean CC@k, mean ICD. All values computed over scenarios
  that match that phase.
- AC-06: Pre-nan-009 result JSON files (without `phase` field) are deserialized by
  `eval report` without error; missing `phase` defaults to `null` via
  `#[serde(default)]`.
- AC-07: The `phase` field is documented in the scenario context format reference in
  `docs/testing/eval-harness.md`.
- AC-08: Section 2 "Notable Ranking Changes" in `eval report` includes the phase label
  alongside each scenario header line when phase is non-null.
- AC-09: Unit tests cover: `ScenarioContext` with non-null phase serializes `phase`
  field; `ScenarioContext` with null phase serializes `phase: null`; `compute_phase_stats`
  groups correctly when results have mixed phase values; `compute_phase_stats` returns
  empty when all phases are null; section 6 is omitted when no non-null phases present.
- AC-10: The `eval scenarios` SQL query selects `phase` from `query_log`, confirmed by
  an integration test that inserts a row with `phase = "delivery"` and verifies the
  extracted JSONL contains `context.phase = "delivery"`.

## Constraints

1. **Backward-compatible scenario format.** `ScenarioContext.phase` is `Option<String>`;
   existing scenario JSONL files without this field deserialize cleanly via serde's
   `#[serde(default)]`. All consumers of `ScenarioRecord` must tolerate a missing phase.

2. **Dual-type constraint.** `runner/output.rs` and `report/mod.rs` maintain independent
   copies of `ScenarioResult`. Both must gain `phase: Option<String>` with
   `#[serde(default)]` in sync. Pattern #3550 documents this constraint.

3. **Phase is metadata only during replay.** The phase value from the scenario context
   must NOT be injected into `ServiceSearchParams` or `AuditContext` during `eval run`
   replay. Replay must reproduce the query as issued (without phase signal) so that the
   eval measures how well the *current* retrieval pipeline handles phase-labelled queries,
   not a phase-injected execution.

4. **Report section is synchronous.** The report module is entirely synchronous. No tokio,
   no async, no database access may be introduced in any report code path.

5. **Phase key `None` renders as "(unset)".** In the per-phase table, rows where phase
   is null are grouped under the label `"(unset)"` and sorted last, making the table
   readable without dropping data.

6. **No mandatory phase filter on `eval scenarios`.** The `--source` flag already filters
   by `mcp`/`uds`. Phase filtering is not added to this CLI surface in nan-009.

7. **Max 500 lines per file.** Existing files are well within this limit; new functions
   should be placed in existing modules or in a new `aggregate_phase.rs` sub-file if
   `aggregate.rs` approaches the limit.

## Resolved Decisions

**RD-01: Section ordering.** Phase-Stratified Metrics is section 6; Distribution Analysis
shifts to section 7. Reviewers read phase stratification first to understand *where* a
change had effect, then Distribution Analysis for *what* the distribution looks like.
Chronological reading order in a shipping-gate document.

**RD-02: Table granularity.** One row per phase for the first iteration. The `(phase ×
profile)` cross-product view is the right eventual target but doubles row count (3 phases
× 2–3 profiles) and makes diffs hard to read. Delta columns already provide cross-profile
signal. Add the cross-product view in a later iteration once the format is validated.

**RD-03: Phase is a soft vocabulary key.** The `query_log.phase` column is `TEXT` with no
schema-level enum or CHECK constraint. The harness must not validate against a fixed set —
phase strings flow through as free-form `Option<String>`. Vocabulary governance is
**migration-based, not schema-based**: new session types appear automatically in the report
when new rows use a new phase string; retroactive relabeling of old rows is a data migration
(`UPDATE query_log SET phase = ...`), not a schema change. Documentation states the current
known values (`design`, `delivery`, `bugfix`) as a snapshot, not as a closed allowlist, and
notes that new values appear automatically and retroactive relabeling uses a data migration.

**RD-04: Phase label in Notable Ranking Changes (section 2).** Read phase directly from
`ScenarioResult` in the renderer rather than extending the `NotableEntry` tuple type.
Simpler and avoids ripple changes to the tuple shape.

## Tracking

GH Issue: #400
