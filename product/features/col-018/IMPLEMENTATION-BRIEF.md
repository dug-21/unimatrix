# col-018: Implementation Brief

## Summary

Add observation persistence and topic signal accumulation as a side effect in the `ContextSearch` dispatch arm of the UDS listener. When a UserPromptSubmit hook event arrives with a non-empty prompt, the server now records the prompt as an observation AND executes the search pipeline. No wire protocol changes, no hook-side changes, no schema changes.

## What Changes

### 1. ContextSearch dispatch arm gains observation side effect

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Location**: The `HookRequest::ContextSearch` match arm in `dispatch_request()` (currently lines 635-669)
**What**: Before the existing `handle_context_search()` call, add three operations:
1. Extract topic signal from the query text using `unimatrix_observe::extract_topic_signal(&query)`
2. If session_id is present and query is non-empty, accumulate the topic signal in the session registry via `session_registry.record_topic_signal()`
3. Build an `ObservationRow` directly (not via `extract_observation_fields` -- see ADR-018-001) and persist it fire-and-forget via `spawn_blocking_fire_and_forget` + `insert_observation()`

**Why**: UserPromptSubmit with non-empty prompt currently maps to ContextSearch which only searches -- the prompt is never recorded. This is the sole observation gap in the hook pipeline.

### 2. Observation field mapping

**File**: Same location as above
**What**: The `ObservationRow` is constructed with these values:
- `session_id`: from ContextSearch request's `session_id` field
- `ts_millis`: current unix time in milliseconds (use `unix_now_secs() * 1000` -- the helper already exists in this file)
- `hook`: literal `"UserPromptSubmit"`
- `tool`: `None`
- `input`: the `query` string, truncated to 4096 characters
- `response_size`: `None`
- `response_snippet`: `None`
- `topic_signal`: result of `extract_topic_signal(&query)` -- populated server-side, not None

**Why**: Matches the observation schema (8 columns, schema v10). Direct construction avoids creating a synthetic ImplantEvent (ADR-018-001).

### 3. Guard conditions

**File**: Same location
**What**: Two guards before the observation write:
- Skip observation when `session_id` is `None` (ADR-018-002)
- Skip observation when `query` is empty (defense-in-depth; empty prompts never reach ContextSearch in practice)

**Why**: `session_id` is NOT NULL in the observations table. Empty queries have no observational value.

## What Does NOT Change

- `crates/unimatrix-server/src/uds/hook.rs` -- the `build_request()` UserPromptSubmit arm is unchanged
- `crates/unimatrix-engine/src/wire.rs` -- no new HookRequest variants
- `handle_context_search()` function -- no signature or behavior changes
- `extract_observation_fields()` -- not used for this path
- `insert_observation()` -- reused as-is
- Database schema -- remains at v10
- Empty-prompt UserPromptSubmit path -- still goes through `generic_record_event()` on the hook side

## Existing Infrastructure to Reuse

All of these already exist in `listener.rs` and are used by the RecordEvent dispatch path:

| Function/Type | Location | Purpose |
|---------------|----------|---------|
| `ObservationRow` | listener.rs:1790 | Struct for observation data |
| `insert_observation()` | listener.rs:1859 | Single row SQL insert |
| `spawn_blocking_fire_and_forget()` | listener.rs:73 | Fire-and-forget tokio helper |
| `unix_now_secs()` | listener.rs:40 | Current timestamp |
| `unimatrix_observe::extract_topic_signal()` | Already imported at line 1385 | Topic extraction facade |
| `session_registry.record_topic_signal()` | Used at line 584 | Topic signal accumulation |

## Agent Guidance

This is a single-agent implementation task. One Rust developer agent modifies `listener.rs` and writes tests. Estimated scope: ~20 lines of production code, ~150 lines of tests.

### Test expectations

Tests should follow the existing patterns in `listener.rs` (the file already has ~400 lines of tests starting around line 1900). Key test patterns:

- Use `dispatch_request()` directly with constructed `HookRequest::ContextSearch` values
- Verify observation row in DB after dispatch using SQL query on the test store
- Verify topic_signal value in the observation row
- Verify search results are still returned (non-regression)
- Test the session_id=None guard (no observation written)
- Test the empty query guard (no observation written)
- Test input truncation for long prompts

### Integration with col-017

The `record_topic_signal()` call uses the same API already wired for RecordEvent. The test should verify that after a ContextSearch dispatch with a feature-ID prompt, the session registry reflects the accumulated signal.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| context-search-observation | pseudocode/context-search-observation.md | test-plan/context-search-observation.md |

## Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Risk Items for Implementation Attention

1. **Input truncation**: Truncate `query` to 4096 characters before storing in `input` field. Use simple byte-safe truncation (`.chars().take(4096).collect::<String>()` or equivalent).
2. **Timestamp**: Use `unix_now_secs()` (seconds) multiplied by 1000 for `ts_millis` (milliseconds), matching the pattern in `extract_observation_fields()` at line 1805.
3. **Arc clone**: Clone `store` Arc before the `spawn_blocking_fire_and_forget` closure, same pattern as RecordEvent path (line 592).
