# Test Plan: replay_fix
# `eval/scenarios/replay.rs`

## Component Responsibilities

One-line fix: add `current_phase: record.context.phase.clone()` to the
`ServiceSearchParams` struct literal in `replay.rs`. This is the bridge between
recorded scenario context and live scoring — without it, all eval replays have
`current_phase = None`, making AC-12 a vacuous gate (R-02).

`extract.rs` and `output.rs` already select and propagate `phase` through
`ScenarioContext.phase: Option<String>`. The gap is exclusively in `replay.rs`.

No other change to `replay.rs` is in scope.

---

## Unit Test Expectations

### AC-16 / Eval Harness replay.rs Fix (covers R-02)

**`test_replay_forwards_current_phase_to_service_search_params`**
- Arrange:
  - Construct a `ScenarioRecord` (or equivalent replay input struct) with
    `context.phase = Some("delivery".to_string())`.
  - Run the replay conversion logic that produces `ServiceSearchParams`.
- Act: inspect the `ServiceSearchParams` produced.
- Assert: `params.current_phase == Some("delivery".to_string())`.
- This test directly verifies the one-line fix is present and propagating correctly.

**`test_replay_forwards_none_phase_when_context_phase_absent`**
- Arrange: `ScenarioRecord` with `context.phase = None`.
- Assert: `params.current_phase == None`.
- Verifies that `phase.clone()` on `None` produces `None` (not `Some("")`).

### R-02 / Diff Constraint (code review check, not a Rust test)

At code review:
- The diff of `replay.rs` must add exactly one line:
  `current_phase: record.context.phase.clone(),`
- The diff must NOT touch `extract.rs` or `output.rs`.
- Confirm the fix is at the `ServiceSearchParams` struct literal construction
  site (approximately line 80 of `replay.rs`).

---

## Feature-Level Verification (AC-12 / AC-16)

### Eval Scenario Output Inspection

After AC-16 is implemented, run the eval harness:

```bash
cargo run --bin eval -- --scenarios <path_to_scenarios>
```

Inspect the scenario output file and confirm at least one row has a non-null
`current_phase` value. Example:

```json
{ "current_phase": "delivery", "candidate_id": 42, ... }
```

This confirms the field is forwarded through the replay path and is visible in
eval output. Without this evidence, AC-12 PASS must be rejected at Gate 3b (NFR-05).

### AC-12 / Regression Gate (requires AC-16)

After AC-16 is verified:
1. Run eval with `w_phase_explicit = 0.05` (the new default).
2. Compare metrics against col-030 baselines:
   - MRR ≥ 0.35
   - CC@5 ≥ 0.2659
   - ICD ≥ 0.5340
3. Also run with `w_phase_explicit = 0.0` for sensitivity comparison (R-11).
4. Report all three metrics for both weight values.

Gate 3b must reject AC-12 PASS if:
- AC-16 is not complete, OR
- The scenario output file does not contain non-null `current_phase` values.

---

## Edge Cases

- Scenario records with `context.phase = None`: must produce `current_phase = None`
  in `ServiceSearchParams` (not panic, not `Some("")`).
- Pre-col-028 scenario records (no `phase` field): handled by `Option<String>` —
  deserialization of missing field as `None`. Verify this does not cause a replay
  error.

---

## Covered Risks

| Risk | Test |
|------|------|
| R-02 (vacuous AC-12 gate — replay.rs not forwarding current_phase) | `test_replay_forwards_current_phase_to_service_search_params`; eval output inspection; Gate 3b process enforcement |
| R-11 (w_phase_explicit = 0.05 regression) | AC-12 eval gate run; sensitivity test at w=0.0 vs w=0.05 |
