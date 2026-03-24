# Agent Report: col-024-agent-5-enrich-topic-signal

**Component**: `enrich_topic_signal` helper + four write site applications
**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Feature**: col-024 (#362)

## What Was Implemented

### New Function

Added private free function `enrich_topic_signal` (lines ~124–162 in listener.rs) as specified by ADR-004 and the pseudocode. The function:

- Returns `extracted` unchanged when `Some(_)` — explicit signal always wins (AC-08, FR-14)
- When `extracted` is `None`, reads `session_registry.get_state(session_id)` and returns `state.feature.clone()` if present
- Emits `tracing::debug!` with both values when `extracted` differs from the registry feature (AC-08 forensics)
- No `.unwrap()` — `get_state` uses `unwrap_or_else` internally; `and_then` handles `None` gracefully (FM-04)
- No `await`, no `spawn_blocking` — sync Mutex read only (NFR-04)

### Four Write Sites Applied

| Site | Location | Change |
|------|----------|--------|
| Rework candidate | ~line 638 | `let obs = ...` → `let mut obs = ...; obs.topic_signal = enrich_topic_signal(...)` |
| RecordEvent | ~line 735 | Same pattern |
| RecordEvents batch | ~line 840 | `map(extract_observation_fields)` → closure with per-event enrichment |
| ContextSearch | ~line 889 | `topic_signal.clone()` replaced with `enriched_signal`; `record_topic_signal` updated to use enriched value |

For the ContextSearch site, `record_topic_signal` was also updated to use the enriched value (not just the raw extract result), so registry-enriched signals accumulate in the topic vote tally per FR-12.

### Dev-Dependencies

Added `tracing-test = "0.2"` to `crates/unimatrix-server/Cargo.toml` `[dev-dependencies]`.

### Tests Added (5 unit tests)

All in `uds::listener::tests` module:

| Test | AC/FR | Result |
|------|-------|--------|
| `test_enrich_returns_extracted_when_some` | AC-08 no-mismatch | pass |
| `test_enrich_fallback_from_registry` | AC-05/06/07 | pass |
| `test_enrich_no_registry_entry` | FR-13 | pass |
| `test_enrich_explicit_signal_unchanged` | AC-08 mismatch + debug log | pass |
| `test_enrich_registry_no_feature` | FR-13 | pass |

## Test Results

```
running 5 tests
test uds::listener::tests::test_enrich_no_registry_entry ... ok
test uds::listener::tests::test_enrich_fallback_from_registry ... ok
test uds::listener::tests::test_enrich_returns_extracted_when_some ... ok
test uds::listener::tests::test_enrich_registry_no_feature ... ok
test uds::listener::tests::test_enrich_explicit_signal_unchanged ... ok
test result: ok. 5 passed; 0 failed
```

`cargo build --workspace` passes with zero errors (10 pre-existing warnings in unimatrix-server lib).

Note: `cargo test -p unimatrix-server` (full run) emits one `E0369` compile error at `services/observation.rs:1593` — this is pre-existing, introduced by another agent on this branch (col-024-agent-4). It does not affect the enrich tests, which compile and run cleanly.

## Code Review Gates (R-12)

`grep -n "enrich_topic_signal"` shows exactly:
- 1 function definition
- 4 production call sites
- 5 test call sites

All within `uds/listener.rs`. No call sites in other files.

## Commit

`cb074d6 impl(enrich-topic-signal): add enrich_topic_signal helper and apply at four write sites (#362)`

Files: `Cargo.lock`, `crates/unimatrix-server/Cargo.toml`, `crates/unimatrix-server/src/uds/listener.rs`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `enrich_topic_signal session registry feature write site` (pattern) and `col-024 architectural decisions` (decision) -- MCP returned deserialization errors (k param type issue); no results retrieved
- Stored: see below

## Findings / Gotchas

1. **ContextSearch site: `record_topic_signal` must use enriched value** — The pseudocode explicitly requires updating the `record_topic_signal` call to use `enriched_signal` rather than the raw extract result. This ensures registry-enriched signals enter the topic vote tally for majority vote resolution on session close (FR-12). Easy to miss when reading the site naively.

2. **RecordEvents batch: `map(fn_ptr)` must become a closure** — The original `events.iter().map(extract_observation_fields)` uses a function pointer. Adding enrichment requires converting to a closure `|event| { ... }` because the enrichment needs to borrow `session_registry` per-event. The closure captures `session_registry` by reference, which is fine since enrichment happens before the `spawn_blocking_fire_and_forget` closure (which must be `Send + 'static`).

3. **`obs` must be `mut` at RecordEvent/rework sites** — `extract_observation_fields` returns `ObservationRow` by value. To override `topic_signal` after construction, the binding must be `let mut obs = ...`. Simple but can be missed if reading the pseudocode casually.
