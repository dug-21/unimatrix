# nan-009 Researcher Report

## Summary

Explored the full problem space for GH #400 (phase-stratified eval scenarios). All
relevant source files read. SCOPE.md written to
`product/features/nan-009/SCOPE.md`.

## Key Findings

### #397 / col-028 dependency: already complete

`query_log.phase: Option<String>` is fully implemented in
`crates/unimatrix-store/src/query_log.rs`. The column was delivered in col-028 (GH #403,
commit 8e61207). It is selected in all three scan functions and mapped at index 9 in
`row_to_query_log`. The dependency on #397 mentioned in the GH #400 description is
satisfied; nan-009 can proceed without coordination.

### Primary gap: scenarios SQL does not fetch phase

`eval/scenarios/output.rs` line 108 — the SELECT in `do_scenarios` enumerates seven
columns: `query_id, session_id, query_text, retrieval_mode, source, result_entry_ids,
similarity_scores`. The `phase` column is absent. This is the root cause of the
missing phase context.

`ScenarioContext` in `eval/scenarios/types.rs` has no `phase` field. `build_scenario_record`
in `extract.rs` does not read phase from the row.

### Change surface is well-bounded

The fix touches five files:

| File | Change |
|------|--------|
| `eval/scenarios/types.rs` | Add `phase: Option<String>` to `ScenarioContext` |
| `eval/scenarios/output.rs` | Add `phase` to SELECT, no other changes |
| `eval/scenarios/extract.rs` | Read phase from row, populate `context.phase` |
| `eval/runner/output.rs` | Add `phase: Option<String>` to `ScenarioResult` |
| `eval/runner/replay.rs` | Passthrough `record.context.phase` to `ScenarioResult` |

Plus report-side changes:

| File | Change |
|------|--------|
| `eval/report/mod.rs` | Add `phase` to local `ScenarioResult` copy (serde default) |
| `eval/report/aggregate.rs` | New `compute_phase_stats` function |
| `eval/report/render.rs` | New section 7 render function; phase label in section 2 |
| `docs/testing/eval-harness.md` | Document phase field and section 7 |

### Dual-type constraint applies

Pattern #3550 (and confirmed by reading both files): `runner/output.rs` and
`report/mod.rs` maintain independent copies of `ScenarioResult`. Both must gain
`phase: Option<String>` with `#[serde(default)]`.

### Phase must not affect replay execution

During `eval run` replay, phase is metadata only. It must NOT be injected into
`ServiceSearchParams` or `AuditContext` — doing so would make eval measure
phase-conditioned retrieval rather than current retrieval quality, invalidating
the baseline measurement.

### Existing tests already bind phase=NULL

`eval/scenarios/tests.rs` `insert_query_log_row` already binds `Option::<String>::None`
for the phase column (IR-03 comment). Tests for phase extraction only need a new helper
variant that binds a non-null phase value, or an optional parameter on the existing
helper.

## Scope Boundaries and Rationale

**Included:** Extraction, passthrough, per-phase aggregate section. These together
constitute the complete measurement instrument for Loop 2.

**Excluded:** Phase-conditioned retrieval logic (separate feature), CLI phase filter
(out of scope for this instrument), per-profile×phase breakdown (second-iteration),
NEER metric (deferred from nan-008, same rationale), changes to baseline log.jsonl
(corpus-wide baseline unchanged).

**Deferred:** Per-profile×phase table, per-phase delta columns. These require more
accumulated phase-labelled data and are better scoped after observing how many
scenarios actually carry non-null phase values.

## Risks

1. **Low phase coverage in existing snapshots.** `query_log.phase` was populated starting
   with col-028. Snapshots taken before that deployment will have all-null phase. The
   per-phase section renders only when at least one non-null phase is present, so the
   report degrades gracefully, but the instrument produces no signal from old data.

2. **Phase vocabulary is free-form.** No validation is specified; grouping is by string
   equality. If the session state produces variant spellings (e.g., "Design" vs.
   "design"), they will appear as separate rows. This is acceptable for the first
   iteration but should be noted in documentation.

## Open Questions (for human)

1. Should the per-phase table show per-profile rows `(phase × profile)` or aggregate
   across all profiles? The proposed SCOPE uses aggregate-only for simplicity.

2. Should section 7 appear before or after section 6 (Distribution Analysis)? Proposed
   ordering is 7th (after distribution).

3. Are there known phase value spellings in the production query_log beyond
   "design"/"delivery"/"bugfix"? If so, should documentation enumerate them?

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `eval harness scenario format phase stratification`
  — returned pattern #3526 (round-trip testing pattern) and #3550 (dual-type constraint).
  Both applicable; dual-type constraint directly constrains the design.
- Stored: entry #3555 "Eval harness phase gap: scenarios SQL omits query_log.phase
  despite column existing since col-028" via `/uni-store-pattern` — captures the
  specific gap and the correct fix pattern for future researchers.
