# Agent Report: col-031-agent-7-background-tick

**Feature**: col-031 — Phase-Conditioned Frequency Table
**Component**: `crates/unimatrix-server/src/background.rs`
**Task**: Thread PhaseFreqTableHandle through the background tick and call rebuild

---

## Files Modified

- `crates/unimatrix-server/src/background.rs`

---

## Changes Made

### 1. Import added (line 47)
```rust
use crate::services::phase_freq_table::{PhaseFreqTable, PhaseFreqTableHandle};
```

### 2. `spawn_background_tick` — new parameter (last position, ADR-005)
```rust
phase_freq_table: PhaseFreqTableHandle,   // col-031: required non-optional (ADR-005)
```
Inner `tokio::spawn(background_tick_loop(...))` call extended with:
```rust
phase_freq_table.clone(), // col-031: Arc::clone via .clone()
```

### 3. `background_tick_loop` — new parameter (last position)
```rust
phase_freq_table: PhaseFreqTableHandle,   // col-031: threaded to run_single_tick
```
`run_single_tick` call extended with:
```rust
&phase_freq_table,  // col-031: passed by reference (mirrors typed_graph_state pattern)
```

### 4. `run_single_tick` — new parameter (last position)
```rust
phase_freq_table: &PhaseFreqTableHandle,   // col-031: required (ADR-005)
```

### 5. PhaseFreqTable rebuild block inserted after TypedGraphState rebuild

Full block with:
- Lock ordering comment (SR-07, NFR-03): EffectivenessStateHandle → TypedGraphStateHandle → PhaseFreqTableHandle
- Retain-on-error semantics (R-09): error/panic/timeout branches do NOT write to handle
- Success branch: write lock acquired, `*guard = new_table`, lock released
- All four outcome arms: `Ok(Ok(Ok(_)))`, `Ok(Ok(Err(_)))`, `Ok(Err(_))`, `Err(_timeout)`
- Poison recovery: `.unwrap_or_else(|e| e.into_inner())`

### 6. Three unit tests added to `#[cfg(test)] mod tests`

- `test_phase_freq_table_handle_swap_on_success` — AC-04 success path: cold-start handle becomes active after swap
- `test_phase_freq_table_handle_retain_on_error` — AC-04/R-09 error path: active state retained when rebuild fails, no write occurs
- `test_phase_freq_table_handle_is_correct_type_for_spawn` — R-14 compile gate: type is correct, can be cloned and Arc::cloned

---

## Tests

**Status**: Cannot run in isolation — peer modules (`services/mod.rs`, `services/search.rs`, `eval/runner/replay.rs`, `mcp/tools.rs`, `uds/listener.rs`) have unresolved compile errors from `current_phase` field missing in `ServiceSearchParams` and `SearchService::new` missing parameter. These are peer agent work items.

**Background-specific errors**: Zero. `cargo build --workspace 2>&1 | grep "background.rs"` returns no output.

**Expected pass count when workspace compiles**: 3 new tests (all in `background::tests` module).

---

## Build Status

**Workspace**: 4 compile errors — all from peer modules:
- `eval/runner/replay.rs` — missing `current_phase` in `ServiceSearchParams` literal (replay agent)
- `services/index_briefing.rs` — missing `current_phase` in `ServiceSearchParams` literal (search/briefing agent)
- `uds/listener.rs` — missing `current_phase` in `ServiceSearchParams` literal (service layer agent)
- `mcp/tools.rs` — missing `current_phase` in `ServiceSearchParams` literal (tools agent)

`background.rs` itself: **zero errors**.

---

## Commit

`547fc32` — `impl(background): thread PhaseFreqTableHandle through tick and call rebuild (#414)`

---

## Constraint Verification

| Constraint | Status |
|------------|--------|
| ADR-005: PhaseFreqTableHandle non-optional at all 3 background.rs sites | PASS |
| Lock ordering comment at lock sequence site (SR-07, NFR-03) | PASS |
| Retain-on-error: error branch has no write to handle (R-09) | PASS |
| Write lock uses `.unwrap_or_else(|e| e.into_inner())` | PASS |
| Rebuild placed AFTER TypedGraphState rebuild (lock ordering) | PASS |
| No `.unwrap()` in non-test code | PASS |
| No `todo!()`, `unimplemented!()`, TODO, FIXME in non-test code | PASS |
| `cargo fmt` applied | PASS |

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — surfaced entries #3682 (lock ordering ADR), #3689 (required handle threading ADR), #3213 (arc threading pattern), #1560 (background tick state cache pattern). Applied: confirmed rebuild placement after TypedGraphState, confirmed non-optional parameter requirement.
- **Stored**: entry #3693 "run_single_tick rebuild: use timeout+spawn+swap triple-nest; error branch must NOT write to handle" via `/uni-store-pattern` — documents the Ok(Ok(Ok(_))) triple-nest match shape and the R-09 silent-reset failure mode when the error branch writes T::new().
