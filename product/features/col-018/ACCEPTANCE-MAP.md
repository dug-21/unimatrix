# col-018: Acceptance Map

## Acceptance Criteria to Test Mapping

| AC | Description | Tests | Risk |
|----|-------------|-------|------|
| AC-01 | Observation created for ContextSearch with valid session_id | T-01 | R-01 |
| AC-02 | topic_signal populated from feature ID in prompt | T-03, T-05 | R-02 |
| AC-03 | Topic signal accumulated in session registry | T-12 | R-06 |
| AC-04 | Search results returned unchanged | T-10, T-11 | R-05 |
| AC-05 | No latency impact (fire-and-forget) | T-01 (structural) | R-01 |
| AC-06 | session_id=None skips observation | T-08 | R-04 |
| AC-07 | Empty query skips observation | T-09 | R-04 |
| AC-08 | Input truncated to 4096 chars | T-06, T-07 | R-03 |
| AC-09 | No topic signal for generic prompts | T-04 | R-02 |
| AC-10 | Empty prompt path unchanged | T-10 (existing) | -- |

## Implementation Checklist

- [ ] Observation write added to ContextSearch dispatch arm
- [ ] Topic signal extracted server-side via `extract_topic_signal(&query)`
- [ ] Topic signal accumulated via `record_topic_signal()`
- [ ] Input truncated to 4096 characters
- [ ] session_id=None guard skips observation
- [ ] Empty query guard skips observation
- [ ] Fire-and-forget via `spawn_blocking_fire_and_forget`
- [ ] Tests: observation created (T-01)
- [ ] Tests: topic signal populated (T-03, T-04, T-05)
- [ ] Tests: truncation (T-06, T-07)
- [ ] Tests: guards (T-08, T-09)
- [ ] Tests: search results unchanged (T-10, T-11)
- [ ] Tests: topic accumulation (T-12)
- [ ] All existing tests pass (no regressions)

## Files Modified

| File | Change Type | Lines (est.) |
|------|-------------|--------------|
| `crates/unimatrix-server/src/uds/listener.rs` | Modified | ~20 production, ~150 test |

## Verification

```
cargo test -p unimatrix-server
cargo clippy -p unimatrix-server
```
