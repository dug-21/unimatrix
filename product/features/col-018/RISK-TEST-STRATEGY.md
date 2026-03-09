# col-018: Risk Test Strategy

## Risk Registry

### R-01: Observation write fails silently, prompt data lost

**Severity**: Medium
**Likelihood**: Low
**Category**: Data loss
**Source**: SR-01

The fire-and-forget `spawn_blocking_fire_and_forget` pattern means `insert_observation()` failures are logged but not surfaced. If SQLite is locked or disk is full, prompt observations are silently dropped.

**Test strategy**: Verify `tracing::error!` is emitted on write failure. Verify observation row exists in DB after successful dispatch. No new mitigation needed -- this is the established col-012 pattern for all observation writes.

**Tests**:
- T-01: ContextSearch dispatch with valid session_id produces observation row in DB
- T-02: Observation write failure logs error (mock/simulate write failure if feasible)

### R-02: Topic signal false positives from prompt text

**Severity**: Low
**Likelihood**: Medium
**Category**: Signal quality
**Source**: SR-02

`extract_topic_signal()` may match incidental feature-ID-like patterns in user prompts (e.g., "use the rust-2024 edition"). These false positives feed into session-level topic attribution via majority vote.

**Test strategy**: Verify topic_signal is populated correctly for prompts containing real feature IDs. Verify topic_signal is NULL for prompts without feature-ID patterns. Majority vote (col-017) handles residual false positives.

**Tests**:
- T-03: Prompt "implement col-018 feature" produces topic_signal = "col-018"
- T-04: Prompt "help me fix the bug" produces topic_signal = NULL
- T-05: Prompt "work on product/features/col-018/SCOPE.md" produces topic_signal = "col-018" (path extraction)

### R-03: Input field unbounded for long prompts

**Severity**: Low
**Likelihood**: Low
**Category**: Storage
**Source**: SR-03

Without truncation, extremely long prompts (up to 1 MiB per MAX_PAYLOAD_SIZE) would be stored verbatim in the `input` column, wasting storage and slowing queries.

**Test strategy**: Verify truncation to 4096 characters for long prompts.

**Tests**:
- T-06: Prompt longer than 4096 chars is truncated in observation input field
- T-07: Prompt exactly 4096 chars is stored without truncation

### R-04: Session ID None skips observation

**Severity**: Low
**Likelihood**: Very Low
**Category**: Edge case
**Source**: SR-04

When session_id is None, observation write is skipped per ADR-018-002. In practice this never happens for hook-originated requests.

**Test strategy**: Verify no observation is written when session_id is None. Verify search pipeline still executes.

**Tests**:
- T-08: ContextSearch with session_id=None produces search results but no observation row
- T-09: ContextSearch with session_id=Some("") -- decide whether empty string is treated as present or absent

### R-05: Search pipeline regression

**Severity**: High
**Likelihood**: Very Low
**Category**: Functional regression

Adding observation logic before `handle_context_search()` could theoretically break the search response if the observation code panics or corrupts shared state.

**Test strategy**: Verify search results are identical with and without the observation side effect. The observation code uses only `Arc::clone(store)` and `spawn_blocking_fire_and_forget` -- no shared mutable state.

**Tests**:
- T-10: ContextSearch response contains expected entries (existing test coverage applies)
- T-11: ContextSearch with observation side effect returns same results as before

### R-06: Topic signal accumulation missed

**Severity**: Medium
**Likelihood**: Low
**Category**: Integration

If the `record_topic_signal()` call is omitted or mis-wired, UserPromptSubmit topic signals will not contribute to session-level attribution.

**Test strategy**: Verify that after a ContextSearch dispatch with a feature-ID-containing prompt, the session registry has the accumulated topic signal.

**Tests**:
- T-12: After ContextSearch dispatch, session_registry contains topic signal for the session

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Test Coverage |
|------------|-------------------|---------------|
| SR-01: Silent write failure | R-01 | T-01, T-02 |
| SR-02: Topic noise from short prompts | R-02 | T-03, T-04, T-05 |
| SR-03: Input truncation divergence | R-03 | T-06, T-07 |
| SR-04: Session ID None edge case | R-04 | T-08, T-09 |
| SR-05: Duplicate observation risk | (no arch risk -- paths mutually exclusive) | T-10 (existing) |

## Test Summary

| Test ID | Description | Type | Risk |
|---------|-------------|------|------|
| T-01 | ContextSearch produces observation row | Integration | R-01 |
| T-02 | Write failure logs error | Unit | R-01 |
| T-03 | Topic signal extracted from feature ID prompt | Unit | R-02 |
| T-04 | Topic signal NULL for generic prompt | Unit | R-02 |
| T-05 | Topic signal from file path in prompt | Unit | R-02 |
| T-06 | Long prompt truncated to 4096 chars | Unit | R-03 |
| T-07 | Prompt at limit stored without truncation | Unit | R-03 |
| T-08 | session_id=None skips observation | Unit | R-04 |
| T-09 | Empty session_id handling | Unit | R-04 |
| T-10 | Search results unchanged (existing coverage) | Integration | R-05 |
| T-11 | Search results with observation side effect | Integration | R-05 |
| T-12 | Topic signal accumulated in session registry | Integration | R-06 |
