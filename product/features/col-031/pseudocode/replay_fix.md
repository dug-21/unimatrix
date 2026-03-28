# col-031: eval/runner/replay.rs AC-16 Fix — Pseudocode

File: `crates/unimatrix-server/src/eval/runner/replay.rs`
Status: MODIFIED (one line change)

---

## Purpose

Forward `record.context.phase` into `ServiceSearchParams.current_phase` so that
eval replay scenarios carry the workflow phase into the scoring pipeline. Without
this fix, all eval runs have `current_phase = None`, `phase_explicit_norm = 0.0`
for all candidates, making AC-12 a vacuous regression gate (R-02, SR-03).

This is a one-line change. No other change to `replay.rs` is in scope.

---

## Context: Why This Fix Is Needed

`replay.rs` line 80 (in `replay_scenario`) already sets `phase` as metadata:

```rust
phase: record.context.phase.clone(), // metadata passthrough only — never forwarded to ServiceSearchParams
```

The comment is accurate about the current state — it is metadata passthrough only.
The fix is to ALSO forward `phase` to `ServiceSearchParams` in `run_single_profile`.

The `ScenarioContext.phase: Option<String>` field already exists (it was added in
col-028 via `extract.rs` and `output.rs`). The gap is entirely in `replay.rs`.

---

## Existing Code (relevant section in `run_single_profile`)

```rust
// Current code at lines 96-108:
let params = ServiceSearchParams {
    query: record.query.clone(),
    k,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: Some(record.context.agent_id.clone()),
    retrieval_mode,
    session_id: None,
    category_histogram: None,
};
```

---

## Fix: Add `current_phase` Field

The struct literal must have `current_phase` added as a field.
`current_phase` is a new field added to `ServiceSearchParams` by the
`search_scoring.md` changes.

Change the `ServiceSearchParams` literal from the above to:

```
let params = ServiceSearchParams {
    query: record.query.clone(),
    k,
    filters: None,
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: Some(record.context.agent_id.clone()),
    retrieval_mode,
    session_id: None,
    category_histogram: None,
    current_phase: record.context.phase.clone(),  // col-031: AC-16 — forward phase to scoring
};
```

The added line is:
```
current_phase: record.context.phase.clone(),
```

`record.context.phase` is `Option<String>`. The `.clone()` matches the type of
`ServiceSearchParams.current_phase: Option<String>`.

---

## Non-Separability from AC-12 (ADR-004, NFR-05)

This fix is a hard prerequisite for AC-12. Gate 3b must reject any AC-12 PASS claim
that is submitted without verified evidence of non-null `current_phase` values in
eval scenario output. The two acceptance criteria are non-separable:

- AC-16 fix applied (this change) + eval run executed ->
  eval output contains non-null `current_phase` for rows where `query_log.phase` is set
- Only then is AC-12 non-vacuous.

---

## Diff

The complete diff for this file is exactly one line added to the `ServiceSearchParams`
struct literal in `run_single_profile`. No other change to `replay.rs` is permitted.

```diff
     let params = ServiceSearchParams {
         query: record.query.clone(),
         k,
         filters: None,
         similarity_floor: None,
         confidence_floor: None,
         feature_tag: None,
         co_access_anchors: None,
         caller_agent_id: Some(record.context.agent_id.clone()),
         retrieval_mode,
         session_id: None,
         category_histogram: None,
+        current_phase: record.context.phase.clone(),  // col-031: AC-16 — forward phase to scoring
     };
```

---

## Files NOT Changed (AC-16 scope clarification)

| File | Reason not changed |
|------|-------------------|
| `eval/scenarios/output.rs` | Already selects `phase` in SQL (col-028 complete) |
| `eval/scenarios/extract.rs` | Already reads `phase` from row and populates `ScenarioContext.phase` |
| `eval/scenarios/types.rs` | `ScenarioContext.phase: Option<String>` already present |

The implementation agent must NOT touch these files as part of AC-16. If they need
changes, flag them as a separate issue.

---

## Error Handling

None. This is a struct field assignment. `Option<String>::clone()` is infallible.

---

## Key Test Scenarios

### AC-16: Eval output contains non-null `current_phase` values

```
// After applying the replay.rs fix, run the eval harness against the
// col-030 baseline scenario file.
// Inspect the scenario output JSONL (or JSON) for at least one record
// where "phase" is non-null.
//
// Precondition: the scenario file must have been extracted with col-028
// schema active, so at least some records have non-null phase in
// ScenarioContext.phase.
//
// Verification: check eval output file for:
//   { "phase": "delivery", ... }  <- non-null phase
//
// This confirms the fix forwarded current_phase to the scoring pipeline.
```

### Diff constraint: only one line changed

```
// git diff eval/runner/replay.rs must show exactly one line added:
//   +        current_phase: record.context.phase.clone(),
// No other lines in replay.rs may be changed.
```

### R-02: Gate 3b process enforcement

```
// Gate 3b must reject any AC-12 PASS submission that does not include
// the eval output file showing non-null current_phase values.
// This is a process check, not a code test, but must be enforced at gate.
```
