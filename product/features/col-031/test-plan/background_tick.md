# Test Plan: background_tick
# `crates/unimatrix-server/src/background.rs`

## Component Responsibilities

Threads `PhaseFreqTableHandle` through three function signatures:
- `spawn_background_tick` (public, one new parameter at the end).
- `background_tick_loop` (private async, same parameter propagated).
- `run_single_tick` (private async, same parameter; calls `PhaseFreqTable::rebuild` after `TypedGraphState::rebuild`).

On success: writes the rebuilt table under a write lock (`*guard = new_table`).
On error: retains existing state, emits `tracing::error!`.

Lock acquisition order in `run_single_tick`:
`EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`.
This order must be documented in a code comment at the lock sequence site (R-12, NFR-03).

---

## Unit Test Expectations

Tests in `#[cfg(test)] mod tests` inside `background.rs`. The existing tests for
`parse_tick_interval_str` show the convention. For `run_single_tick` tests, use
a test store (mocked or `TestDb`).

### AC-04 / Tick Success Path

**`test_run_single_tick_swaps_phase_freq_table_on_success`**
- Arrange:
  - Cold-start handle: `PhaseFreqTable::new_handle()`.
  - A `TestDb` store with seeded `query_log` rows (phase="delivery", some entry_ids within lookback).
- Act: call `run_single_tick(…, phase_freq_table_handle)` (or simulate the rebuild call directly).
- Assert: after the tick, `handle.read().use_fallback == false`.
- Assert: handle contains entries in its table.

**`test_run_single_tick_retains_state_on_rebuild_error`**  ← AC-04 failure path, R-09
- Arrange:
  - Prepopulate the handle with a known table (e.g., one entry in `("delivery", "decision")` bucket, `use_fallback = false`).
  - Use a store that returns an error from `query_phase_freq_table` (inject via trait mock or a corrupted TestDb).
- Act: invoke the rebuild logic (simulate the error path in `run_single_tick`).
- Assert: handle still contains the pre-tick table — `use_fallback == false` and
  the original entries are still present.
- Assert: `tracing::error!` was emitted (verify via tracing subscriber capture or
  observe indirectly that no panic occurred and the handle was not reset to cold-start).

Note: injecting a store error requires either a mock `Store` trait or a TestDb
with a deliberately broken query. The architecture uses `unimatrix_core::Store`
as the trait — if the trait is object-safe, a test double is feasible. If not,
test the error path via code inspection + a doc test.

### R-01 / Wiring Completeness (compile-level gate covers R-01, R-14)

**`test_spawn_background_tick_accepts_phase_freq_table_handle`**
- This test is implicit: if `spawn_background_tick` does not accept
  `PhaseFreqTableHandle` as a required parameter, `cargo build --workspace` fails.
- Explicit test: construct a `PhaseFreqTableHandle` and pass it to
  `spawn_background_tick` in a unit test; the test must compile and the handle
  must be threaded through to `run_single_tick` (verified by checking the handle
  is not newly constructed inside `run_single_tick`).

### R-12 / Lock Acquisition Order

**Lock order code comment check** (Gate 3b enforcement, not a Rust test):
- At code review: verify `run_single_tick` has an inline comment at the lock
  sequence site naming the three handles in order:
  `// Lock order: EffectivenessStateHandle → TypedGraphStateHandle → PhaseFreqTableHandle`.
- No lock held simultaneously across multiple handles.
- Grep `run_single_tick` for `phase_freq_table.write()` and `typed_graph_state.write()`
  occurrences; `typed_graph_state.write()` must appear before `phase_freq_table.write()`
  in file order.

---

## Integration Test Expectations

### R-01 / End-to-End Wiring After Tick

The critical integration scenario for R-01 is observable only through the full
tick → scoring pipeline:

**`test_run_single_tick_propagates_phase_freq_handle`**  ← R-01 integration
- Arrange:
  - Seed `query_log` rows with `phase = "delivery"`, `result_entry_ids = "[42]"`.
  - Cold-start `PhaseFreqTableHandle` shared between tick and `SearchService`.
- Act: run one tick cycle (call `run_single_tick` or equivalent).
- Assert: after the tick, `SearchService::search()` with `current_phase = "delivery"`
  returns at least one candidate with `phase_explicit_norm > 0.0`.
- This test proves the handle is not the default cold-start stub — i.e., the wiring
  is not silent bypass.

If a direct `run_single_tick` call is not feasible in tests, this scenario is
covered by the infra-001 `test_search_phase_affinity_influences_ranking` test
(see OVERVIEW.md), which exercises the full tick via the MCP server binary.

---

## Covered Risks

| Risk | Test |
|------|------|
| R-01 (silent wiring bypass) | `test_run_single_tick_propagates_phase_freq_handle` (integration); `cargo build --workspace` (compile gate); grep audit |
| R-09 (rebuild failure overwrites state) | `test_run_single_tick_retains_state_on_rebuild_error` |
| R-12 (lock acquisition order) | Code comment check at lock sequence site; grep order audit |
| R-14 (test helper sites miss parameter) | `cargo build --workspace` compile gate |
