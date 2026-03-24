# col-024 Test Plan: OVERVIEW
# Cycle-Events-First Observation Lookup and Topic Signal Write-Time Enrichment

## Test Strategy Summary

col-024 has two independent change surfaces:

1. **Read path** — new `load_cycle_observations` method on `ObservationSource` trait, implemented by
   `SqlObservationSource`, exercised by the restructured `context_cycle_review` three-path fallback.
2. **Write path** — new `enrich_topic_signal` helper applied at four write sites in `listener.rs`.

Both surfaces are testable at unit level without the MCP layer: the read path via
`#[tokio::test]` tests in `services/observation.rs`, the write path via unit tests directly on
the `enrich_topic_signal` function. The `context_cycle_review` fallback order (AC-04) requires a
mock or integration-style test of the three-path dispatch logic.

### Test Levels Used

| Level | Location | What it covers |
|-------|----------|----------------|
| Unit (pure function) | `services/observation.rs` #[cfg(test)] | `load_cycle_observations`, `cycle_ts_to_obs_millis`, AC-01/02/03/11/13/15 |
| Unit (pure function) | `uds/listener.rs` #[cfg(test)] | `enrich_topic_signal`, AC-05/06/07/08 |
| Unit (trait dispatch) | `unimatrix-observe/src/source.rs` #[cfg(test)] | AC-10 — trait compiles; existing tests green |
| Integration (mock dispatch) | `mcp/tools.rs` or services-level test | AC-04/09/12/14 — three-path fallback order + debug log |
| Grep / code-review | CI step | AC-13 — no raw `* 1000` in implementation block |
| Integration harness (infra-001) | `product/test/infra-001/suites/` | Regression check; no new suite required (see below) |

### Test Naming Convention

All new tests follow the existing project pattern: `test_{function}_{scenario}`.

```
load_cycle_observations_single_window
load_cycle_observations_multiple_windows
load_cycle_observations_no_cycle_events
load_cycle_observations_no_cycle_events_count_check
load_cycle_observations_rows_exist_no_signal_match
load_cycle_observations_open_ended_window
load_cycle_observations_excludes_outside_window
load_cycle_observations_saturating_mul_overflow_guard
load_cycle_observations_empty_cycle_id
load_cycle_observations_phase_end_events_ignored
enrich_topic_signal_fallback_from_registry
enrich_topic_signal_explicit_signal_unchanged
enrich_topic_signal_mismatch_debug_log
enrich_topic_signal_no_registry_entry
enrich_topic_signal_no_feature_in_state
context_cycle_review_primary_path_used_when_non_empty
context_cycle_review_fallback_to_legacy_when_primary_empty
context_cycle_review_no_cycle_events_debug_log_emitted
```

---

## Risk-to-Test Mapping

| Risk ID | Priority | AC(s) | Test File | Test Name(s) |
|---------|----------|-------|-----------|--------------|
| R-01 (raw `* 1000` bypass) | Critical | AC-01, AC-13 | observation.rs, grep | `single_window` (positive inclusion + boundary exclusion), AC-13 grep |
| R-02 (enrichment missing at write site) | Critical | AC-05/06/07 | listener.rs | per-site tests: `enrich_record_event_path`, `enrich_context_search_path`, `enrich_rework_path`, `enrich_record_events_batch_path` |
| R-03 (empty primary not forwarded to legacy) | High | AC-04, AC-09, AC-12 | tools.rs / services | `context_cycle_review_fallback_to_legacy_when_primary_empty` |
| R-04 (enrichment overrides explicit signal) | High | AC-08 | listener.rs | `enrich_explicit_signal_unchanged`, `enrich_mismatch_debug_log` |
| R-05 (multiple block_sync calls) | High | NFR-01 | observation.rs | `load_cycle_observations_multiple_windows` (run inside `#[tokio::test]`) |
| R-06 (open-ended window over-inclusion) | High | — | observation.rs | `load_cycle_observations_open_ended_window` |
| R-07 (error instead of Ok(vec![])) | Med | AC-03 | observation.rs | `load_cycle_observations_no_cycle_events` |
| R-08 (fallback log missing/wrong level) | Med | AC-14 | tools.rs (tracing_test) | `context_cycle_review_no_cycle_events_debug_log_emitted` |
| R-09 (Rust window-filter absent) | Med | AC-02 | observation.rs | `load_cycle_observations_multiple_windows` (gap observation must be excluded) |
| R-10 (parse_observation_rows bypassed) | Med | NFR-05 | observation.rs | code review + `load_cycle_observations_single_window` (exercise parser path) |
| R-11 (session deduplication skipped) | Low | AC-02 | observation.rs | `load_cycle_observations_multiple_windows` (exact count assertion) |
| R-12 (enrichment applied outside scope) | Low | — | listener.rs | code review: `enrich_topic_signal` is `fn` (not `pub`), called only from four handler sites |
| I-01 (trait breaking change) | — | AC-10 | unimatrix-observe | full `cargo test -p unimatrix-observe` |
| I-02 (block_sync context) | — | NFR-01 | observation.rs | multi-window test inside `#[tokio::test(flavor="multi_thread")]` |
| I-03 (registry race) | — | AC-05 | listener.rs | `enrich_topic_signal_no_registry_entry` (unregistered session → `None`) |
| I-04 (insert_cycle_event API contract) | — | AC-11 | observation.rs | round-trip in `no_cycle_events` and `single_window` tests |
| E-01 (valid window, zero observations) | — | AC-15 | observation.rs | `rows_exist_no_signal_match` |
| E-02 (cycle_phase_end between start/stop) | — | — | observation.rs | `phase_end_events_ignored` |
| E-03 (duplicate cycle_start) | — | — | observation.rs | `malformed_double_start` |
| E-05 (saturating_mul overflow) | — | — | observation.rs | `saturating_mul_overflow_guard` |
| E-06 (empty cycle_id) | — | — | observation.rs | `empty_cycle_id` |
| S-01/S-02 (SQL injection, topic_signal length) | — | — | code review | parameterized queries only; no `format!` near cycle_id |
| FM-01 (SQL error propagates, not falls back) | — | — | observation.rs | mock returning `Err` causes propagation (tools.rs) |

---

## Cross-Component Test Dependencies

| Dependency | Consumer | What must be true |
|------------|----------|-------------------|
| `insert_cycle_event` API | AC-01/02/03/11/15 tests | Must insert rows; test verifies round-trip via subsequent `load_cycle_observations` |
| `insert_observation` test helper (with `topic_signal`) | AC-01/02/15 | Existing helper must be extended to bind `topic_signal` column |
| `block_sync` present in test runtime | R-05, I-02 | All `load_cycle_observations` tests must use `#[tokio::test(flavor="multi_thread")]` |
| `SessionRegistry::get_state` | AC-05/06/07/08 | Tests that call `enrich_topic_signal` directly construct a `SessionRegistry` and call `register_session` first |
| `tracing_test` or log-capture | AC-08, AC-14 | Tests asserting debug log require a log-capture mechanism; confirm `tracing-test` is in dev-dependencies |

---

## Integration Harness Plan (infra-001)

### Suite Selection

col-024 modifies:
- `unimatrix-observe/src/source.rs` — trait; no MCP-visible interface change
- `unimatrix-server/src/services/observation.rs` — internal observation loading; not directly testable through MCP JSON-RPC
- `unimatrix-server/src/mcp/tools.rs` — `context_cycle_review` tool; MCP-visible but existing behavior preserved
- `unimatrix-server/src/uds/listener.rs` — UDS write path; `topic_signal` enrichment is internal

Per the suite selection table, col-024 touches server tool logic (`context_cycle_review`) and
lifecycle behavior (multi-step observation attribution). Run:

| Suite | Rationale | Required? |
|-------|-----------|-----------|
| `smoke` | Mandatory minimum gate — confirms server starts, basic tools work | Yes |
| `tools` | `context_cycle_review` is tool #12; existing tests validate its MCP interface is not broken (AC-09/12) | Yes |
| `lifecycle` | Multi-step flows; context_cycle → context_cycle_review end-to-end path | Yes |

Additional suites (`protocol`, `confidence`, `contradiction`, `security`, `edge_cases`, `volume`,
`adaptation`) are not required — col-024 does not modify those subsystems.

### Gap Analysis: Existing Suites vs. col-024 Behavior

The primary attribution behavior change (cycle_events-first lookup) is **not visible through MCP
JSON-RPC** in isolation because:

1. `context_cycle_review` returns the same report structure regardless of which path produced the
   observations. The response format is unchanged.
2. The UDS write-path enrichment (`topic_signal`) happens over the Unix domain socket, which the
   infra-001 harness does not exercise (it uses MCP stdio only).

Therefore, the col-024-specific correctness guarantees come entirely from **unit tests** in
`services/observation.rs` and `uds/listener.rs`. The infra-001 harness validates:
- The MCP interface of `context_cycle_review` is not broken
- Backward compatibility for pre-col-024 features

### New Integration Tests Needed

No new integration tests are required for the infra-001 harness. The behavioral changes are not
observable through MCP JSON-RPC, and the existing `lifecycle` and `tools` suites provide adequate
regression coverage for backward compatibility (AC-09/12).

If a future feature adds end-to-end UDS + MCP test infrastructure, a new `lifecycle` test
`test_cycle_review_uses_cycle_events_primary_path` should be planned at that time.

### Running Order for Stage 3c

```bash
# 1. Unit tests first
cargo test --workspace 2>&1 | tail -30

# 2. Mandatory smoke gate
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# 3. Relevant suites
python -m pytest suites/test_tools.py suites/test_lifecycle.py -v --timeout=60
```

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "col-024 architectural decisions" (category: decision, topic: col-024) — found ADR-001 through ADR-005 (#3371–#3375), all directly relevant
- Queried: /uni-knowledge-search for "SqlObservationSource integration test patterns insert_cycle_event fixtures" — found #3040 (infra-001 cycle_events seeding pattern), #3367 (topic_signal enrichment pattern), #2936 (SqlObservationSource wiring pattern)
