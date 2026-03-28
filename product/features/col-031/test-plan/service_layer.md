# Test Plan: service_layer
# `crates/unimatrix-server/src/services/mod.rs`

## Component Responsibilities

Adds `phase_freq_table: PhaseFreqTableHandle` field to `ServiceLayer`. In
`with_rate_config()` (or equivalent construction site), calls
`PhaseFreqTable::new_handle()` and assigns to the field. Exposes
`phase_freq_table_handle()` accessor returning `Arc::clone(&self.phase_freq_table)`.
Passes the handle to `SearchService::new()` as a required parameter.

This component is the single construction site. All subsequent consumers receive the
handle via `Arc::clone` — the background tick receives it via `main.rs` calling the
accessor, `SearchService` receives it via the `new()` constructor call inside
`with_rate_config()`.

---

## Unit Test Expectations

Tests in `#[cfg(test)] mod tests` inside `services/mod.rs`.

### AC-05 / ServiceLayer Wiring

**`test_service_layer_creates_phase_freq_table_handle`**
- Arrange: construct a `ServiceLayer` via `with_rate_config()` using a test store.
- Act: call `service_layer.phase_freq_table_handle()`.
- Assert: the returned handle is not `None` (accessor returns the Arc).
- Assert: the returned handle is in cold-start state initially (`use_fallback = true`).

**`test_phase_freq_table_handle_is_arc_cloned_not_moved`**
- Call `phase_freq_table_handle()` twice.
- Assert both returned handles refer to the same underlying `Arc`
  (`Arc::ptr_eq(handle1, handle2) == true`).
- This proves the accessor clones (cheap) rather than moves (compile error on second call).

**`test_service_layer_phase_freq_table_shared_with_search_service`**
- Construct `ServiceLayer`.
- Write a new table to the handle via `service_layer.phase_freq_table_handle()`.
- Verify the change is visible through the `SearchService` that was constructed
  inside `with_rate_config()`.
- Note: this requires either a public accessor on `SearchService` for its
  `phase_freq_table` field, or observing the change through `SearchService::search`
  behavior. If direct field access is not public, this test can be structural only
  (verify at code review that the same Arc is passed to both).

### R-14 / Handle is Non-Optional

**No `Option<PhaseFreqTableHandle>` at any site** (compile gate, not a Rust test):
- Code review: confirm `ServiceLayer.phase_freq_table` is `PhaseFreqTableHandle`,
  not `Option<PhaseFreqTableHandle>`.
- Code review: confirm `SearchService.phase_freq_table` is `PhaseFreqTableHandle`,
  not `Option<PhaseFreqTableHandle>`.
- `cargo build --workspace` must pass cleanly (ADR-005).

---

## Integration Test Expectations

The `ServiceLayer` wiring is verified indirectly by the R-01 integration test in
`background_tick.md`: a shared handle that is correctly wired through `ServiceLayer`
is observed to change after a tick, which is visible in `SearchService::search` results.

The infra-001 `test_search_cold_start_phase_score_identity` test also exercises
`ServiceLayer` construction indirectly (fresh server, cold-start handle).

---

## 7-Site Grep Audit (Gate requirement for AC-05 and R-14)

The following sites must all receive a `PhaseFreqTableHandle` parameter. The audit
must be performed and documented before delivery is declared complete:

| Site | File |
|------|------|
| `SearchService::new(…)` call in `ServiceLayer::with_rate_config` | `services/mod.rs` |
| `SearchService::new` definition | `services/search.rs` |
| `spawn_background_tick(…)` call in `main.rs` | `src/main.rs` |
| `spawn_background_tick` definition | `background.rs` |
| `background_tick_loop` definition | `background.rs` |
| `run_single_tick` definition | `background.rs` |
| All test helper sites: `server.rs`, `shutdown.rs`, `test_support.rs`, `listener.rs`, `eval/profile/layer.rs` | (each file) |

```bash
# Audit command (run at Gate 3b):
grep -rn "SearchService::new\|spawn_background_tick" crates/unimatrix-server/src/
```

Every occurrence must pass a `PhaseFreqTableHandle` argument — never a freshly
constructed `PhaseFreqTable::new_handle()` inside the call (that would be a silent
bypass creating an unshared handle).

---

## Covered Risks

| Risk | Test |
|------|------|
| R-01 (silent wiring bypass) | `test_service_layer_phase_freq_table_shared_with_search_service`; grep audit |
| R-14 (test helper sites miss parameter) | `cargo build --workspace`; grep audit |
