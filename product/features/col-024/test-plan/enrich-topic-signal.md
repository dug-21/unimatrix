# col-024 Test Plan: enrich_topic_signal
# File: `crates/unimatrix-server/src/uds/listener.rs`

## Component Summary

A new private free function in `listener.rs`:

```rust
fn enrich_topic_signal(
    extracted: Option<String>,
    session_id: &str,
    session_registry: &SessionRegistry,
) -> Option<String>
```

**Semantics**:
- If `extracted` is `Some(x)`: return `Some(x)` unchanged. If `x` differs from the registry
  feature, emit `tracing::debug!` with both values.
- If `extracted` is `None`: read `session_registry.get_state(session_id)`. If the state has a
  non-None `feature`, return `Some(feature)`. Otherwise return `None`.
- If `session_registry.get_state` returns `None` (unregistered session): return `None` — no panic.

Applied at four write sites in `listener.rs`:

| Site | Line (approx) | What gets enriched |
|------|---------------|-------------------|
| RecordEvent | ~684 | `ObservationRow.topic_signal` overridden after `extract_observation_fields` |
| Rework candidate | ~592 | Same pattern |
| RecordEvents batch | ~784–785 | Per-event call inside map building `obs_batch` |
| ContextSearch | ~842 | Replaces inline `topic_signal.clone()` |

---

## Risk Coverage

| Risk | From RISK-TEST-STRATEGY | Test Below |
|------|------------------------|------------|
| R-02 (enrichment missing at a write site) | Critical | T-ENR-04 through T-ENR-07 (one per site) |
| R-04 (enrichment overrides explicit signal) | High | T-ENR-02, T-ENR-03 |
| I-03 (registry race / unregistered session) | Med | T-ENR-06 |
| R-12 (enrichment applied outside scope) | Low | Code review in Stage 3c |
| FM-04 (registry Mutex poisoning) | Low | Implementation constraint: no `.unwrap()` on registry read |

---

## Unit Test Expectations (pure function tests)

These tests call `enrich_topic_signal` directly in the `#[cfg(test)]` block. They require:
- A `SessionRegistry` created via `SessionRegistry::new()`
- `use crate::infra::session::SessionRegistry;`

### T-ENR-01: `enrich_topic_signal_fallback_from_registry`

**AC**: AC-05, AC-06, AC-07 (at the unit level — per-site tests at integration level below)
**Setup**:
```rust
let registry = SessionRegistry::new();
registry.register_session("sess-1", None, Some("col-024".to_string()));
```

**Assertions**:
- `let result = enrich_topic_signal(None, "sess-1", &registry);`
- `assert_eq!(result, Some("col-024".to_string()))`

---

### T-ENR-02: `enrich_topic_signal_explicit_signal_unchanged`

**AC**: AC-08 (no-mismatch branch)
**Setup**: Registry has feature `"col-024"`.

**Assertions**:
- `let result = enrich_topic_signal(Some("bugfix-342".to_string()), "sess-1", &registry);`
- `assert_eq!(result, Some("bugfix-342".to_string()))` — registry feature not used

---

### T-ENR-03: `enrich_topic_signal_mismatch_debug_log`

**AC**: AC-08 (mismatch debug log branch), R-04 scenario 3
**Setup**: Registry has feature `"col-024"`. Input has `extracted = Some("bugfix-342")`.
**Requires**: `tracing-test` or a log-capture subscriber in dev-dependencies.

```rust
#[cfg(test)]
#[tracing_test::traced_test]  // or equivalent log-capture fixture
async fn enrich_topic_signal_mismatch_debug_log() {
    let registry = SessionRegistry::new();
    registry.register_session("sess-1", None, Some("col-024".to_string()));

    let result = enrich_topic_signal(Some("bugfix-342".to_string()), "sess-1", &registry);

    assert_eq!(result, Some("bugfix-342".to_string()));
    // Assert debug log fired with both values
    assert!(logs_contain("bugfix-342"), "log must contain extracted signal");
    assert!(logs_contain("col-024"), "log must contain registry feature");
}
```

**Notes**:
- Confirm `tracing-test` is in `[dev-dependencies]` of `unimatrix-server/Cargo.toml`. If not,
  add `tracing-test = "0.2"`.
- The `logs_contain` assertion is provided by `tracing_test::traced_test` macro.
- The log must be at `debug` level (not `info` or `warn`) per ADR-003 — assert `RUST_LOG=info`
  does not surface it.

---

### T-ENR-04: `enrich_topic_signal_no_registry_entry`

**AC**: FR-13 (best-effort — unregistered session returns `None`)
**Setup**: Empty `SessionRegistry`, no `register_session` call.

**Assertions**:
- `let result = enrich_topic_signal(None, "sess-unknown", &registry);`
- `assert_eq!(result, None)`

---

### T-ENR-05: `enrich_topic_signal_no_feature_in_state`

**AC**: FR-13 (best-effort — registered session with `feature = None`)
**Setup**: `registry.register_session("sess-1", None, None)` — session exists but no feature.

**Assertions**:
- `let result = enrich_topic_signal(None, "sess-1", &registry);`
- `assert_eq!(result, None)`

---

## Per-Site Integration Test Expectations

These tests exercise each of the four write sites in `listener.rs` end-to-end through the UDS
handler dispatch. They require constructing the full handler context (store, registry, embed
handle, etc.) or using a narrower integration helper if available.

**Note**: If full UDS handler invocation is not feasible in unit tests (e.g., `handle_hook`
requires many dependencies), these tests may be implemented as focused integration tests that
send a real `HookRequest` to the UDS listener. The test plan records the expected behavior;
the implementor chooses the appropriate test harness level.

### T-ENR-06: `enrich_record_event_path`

**AC**: AC-05
**Setup**:
1. Open a test store and a `SessionRegistry`.
2. `registry.register_session("sess-rework", None, Some("col-024".to_string()))`
3. Construct a `HookRequest::RecordEvent` with `session_id = "sess-rework"` and
   `topic_signal = None` in the event (no explicit signal).
4. Invoke the listener handler (or call `enrich_topic_signal` directly, verifying the write
   site calls it).

**Assertions**:
- The observation row written to the database has `topic_signal = "col-024"`.
- Verify by querying: `SELECT topic_signal FROM observations WHERE session_id = 'sess-rework'`.

---

### T-ENR-07: `enrich_context_search_path`

**AC**: AC-06
**Setup**: Register session with feature `"col-024"`. Send `HookRequest::ContextSearch` with
`query = "some general query"` (not a feature ID pattern, so `extract_topic_signal` returns
`None`).

**Assertions**:
- Stored observation has `topic_signal = "col-024"`.

---

### T-ENR-08: `enrich_rework_path`

**AC**: AC-07 (rework path)
**Setup**: Register session with feature `"col-024"`. Trigger the rework candidate handler path
(~line 592 in listener.rs).

**Assertions**:
- Stored observation has `topic_signal = "col-024"`.

---

### T-ENR-09: `enrich_record_events_batch_path`

**AC**: AC-07 (batch path)
**Setup**: Register session with feature `"col-024"`. Send `HookRequest::RecordEvents` with
a batch of 3 events, all with `topic_signal = None`.

**Assertions**:
- All 3 stored observations have `topic_signal = "col-024"`.
- Verify exact count to confirm the enrichment is per-event, not just on the first event.

---

## Code Review Gates (Stage 3c)

**R-12 / Scope constraint 6**: `enrich_topic_signal` is `fn` (not `pub fn`). Only four call
sites exist in `listener.rs`. No call sites in any test helper or other file.

```bash
grep -rn "enrich_topic_signal" crates/unimatrix-server/src/
```

Must show exactly 5 occurrences: 1 definition + 4 call sites, all within `uds/listener.rs`.

**FM-04**: The function does not call `.unwrap()` on `session_registry.get_state(...)`.
Verify via code inspection: `get_state` result is handled via `?` operator, `if let`, or
`unwrap_or_else`.

**AC-08 debug log level**: The `tracing::debug!` is not `tracing::info!` or `tracing::warn!`.

---

## Dependency Check

If `tracing-test` is not already in `unimatrix-server/Cargo.toml` `[dev-dependencies]`, it
must be added for T-ENR-03 and T-CCR-03 (context_cycle_review debug log test). Check:

```bash
grep "tracing-test" crates/unimatrix-server/Cargo.toml
```
