# Test Plan: Context Store Phase Capture (Component 8)

Files: `crates/unimatrix-server/src/server.rs`, `services/usage.rs`
Risks: R-01 (Critical — primary causal test site), R-02 (partial), R-14, AC-05, AC-07, AC-09

---

## Unit Test Expectations

### `UsageContext` struct (R-14)

**`test_usage_context_has_current_phase_field`** (compile-time / R-14)
- Verify: `UsageContext` struct includes `current_phase: Option<String>` field
- Verify: construction without `current_phase` fails to compile

**`test_usage_context_current_phase_propagates_to_feature_entry`** (R-14, FR-06.1)
- Arrange: `UsageContext { current_phase: Some("scope"), ... }`
- Verify: when `UsageContext` is used to create `AnalyticsWrite::FeatureEntry`, the `phase`
  field on the variant equals `Some("scope")` — not `None`

### Phase snapshot at call site (FR-06.1, ADR-001)

**`test_context_store_snapshots_phase_at_call_time`** (unit — if extractable as pure function)
- If the phase-snapshot logic is extracted to a helper: verify that the helper reads
  `session_state.current_phase` at call time and produces a local `Option<String>` that
  is passed to both write paths
- Key assertion: the local variable is created before any `await` or `spawn_blocking`

---

## Integration Test Expectations

These are the primary R-01 causal integration tests. They require a full server stack
(in-process or via test binary) with a real `SessionRegistry` and store.

### R-01 Critical: Phase tagging after `context_cycle`

**`test_phase_end_then_store_phase_tagged`** (R-01 Critical, AC-05, AC-09)
- Arrange: fresh server, registered session `s1`
- Act (sequential, no yields between steps):
  1. `context_cycle(type="start", topic="t", next_phase="scope")` via MCP handler
  2. `context_store(content="decision content", topic="t", category="decision", agent_id="s1")`
- Assert: query `feature_entries WHERE entry_id = <stored_id>`
  → `phase = "scope"` (NOT NULL)

This test FAILS if there is any async delay between the UDS listener's `set_current_phase`
call and the `context_store` handler reading `session_state.current_phase`. The synchronous
mutation design (ADR-001) makes this test pass by construction.

**`test_start_without_next_phase_store_phase_null`** (AC-09 NULL case)
- Arrange: `context_cycle(type="start", topic="t")` — no `next_phase`
- Act: `context_store(...)`
- Assert: `feature_entries.phase IS NULL`

**`test_stop_then_store_phase_null`** (R-01, AC-07)
- Arrange: set phase to "testing" via start event
- Act: `context_cycle(type="stop", topic="t")` then `context_store(...)`
- Assert: `feature_entries.phase IS NULL`

**`test_phase_transition_affects_subsequent_stores`** (R-01, end-to-end)
- Arrange: session with phase sequence: scope → design → implementation
- Act:
  1. `start` with `next_phase="scope"` → `store entry A`
  2. `phase-end` with `next_phase="design"` → `store entry B`
  3. `phase-end` with `next_phase="implementation"` → `store entry C`
- Assert: `feature_entries.phase` for A = "scope", B = "design", C = "implementation"

### R-02 Critical: Enqueue-time snapshot, not drain-time

**`test_context_store_drain_path_uses_enqueue_phase`** (R-02 Critical, FR-06.2)

This test exercises the analytics drain path specifically:
- Arrange: a server where the analytics drain is paused (or delayed)
- Act:
  1. Session phase = "implementation"
  2. Call `context_store(...)` → enqueues `AnalyticsWrite::FeatureEntry { phase: Some("implementation") }`
  3. Advance session phase to "testing" (via `context_cycle(type="phase-end", next_phase="testing")`)
  4. Allow drain to run
- Assert: `feature_entries.phase = "implementation"` — drain used enqueue-time value, not drain-time state

Implementation note: if the drain cannot be paused in a test context, this test can be structured
as a direct unit test on the `AnalyticsWrite` queue (see store-layer.md). The server-level test
validates the full path (that `phase` is correctly baked into the `FeatureEntry` variant at
enqueue in `server.rs`/`usage.rs`).

### R-14: Call site signature (compile check)

**`test_record_feature_entries_call_sites_pass_phase`** (R-14)
- Verify: the `record_feature_entries` call in `server.rs` passes `phase.as_deref()` as third arg
- Verify: the `record_feature_entries` call in `services/usage.rs` passes `usage_ctx.current_phase.as_deref()`
- These are compile-time verifications; a wrong arity causes a build failure

---

## Critical Assertions

- `session_state.current_phase` is read into a local variable **before** any `await` in the
  `context_store` handler — the snapshot happens synchronously, not after async dispatch
- Both write paths (direct and drain) receive the same snapshot value
- `UsageContext.current_phase` is set from the session snapshot, not from a re-read of
  `SessionState` after async operations
- The test `test_phase_end_then_store_phase_tagged` is the single most important test in the
  feature: it verifies R-01 by proving the causal ordering guarantee is real

---

## Test Naming Summary

| Test Name | Risk | AC |
|-----------|------|----|
| `test_phase_end_then_store_phase_tagged` | R-01 | AC-05 |
| `test_start_without_next_phase_store_phase_null` | R-01 | AC-09 |
| `test_stop_then_store_phase_null` | R-01 | AC-07 |
| `test_phase_transition_affects_subsequent_stores` | R-01 | AC-09 |
| `test_context_store_drain_path_uses_enqueue_phase` | R-02 | AC-09 |
| `test_usage_context_has_current_phase_field` | R-14 | — |
| `test_usage_context_current_phase_propagates_to_feature_entry` | R-14 | — |
