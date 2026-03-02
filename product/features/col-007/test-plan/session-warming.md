# Test Plan: session-warming

## Unit Tests

### warm_embedding_model()

| Test | Embed State | Expected | Risk |
|------|-------------|----------|------|
| `test_warming_with_ready_embed` | Ready (adapter available) | embed_entry called, info logged, Ack returned | R-12 |
| `test_warming_with_failed_embed` | Failed("model not found") | Warning logged, Ack returned, no panic | R-12 |
| `test_warming_with_not_ready_embed` | Loading | Warning logged, Ack returned | R-12, R-05 |
| `test_warming_idempotent` | Ready, called twice | Both calls succeed, no double initialization | R-05 |

### SessionRegister handler (integrated warming)

| Test | Scenario | Expected | Risk |
|------|----------|----------|------|
| `test_session_register_returns_ack_after_warming` | SessionRegister with Ready embed | Ack (not Entries, not Error) | R-02 |
| `test_session_register_logs_session_info` | SessionRegister with session_id, cwd | Log contains session_id and cwd | R-02 |

## Integration Tests

| Test | Scenario | Expected | Risk |
|------|----------|----------|------|
| `test_warming_then_search` | SessionRegister followed by ContextSearch | ContextSearch returns non-empty Entries | R-05 |
| `test_search_without_warming` | ContextSearch without prior SessionRegister | Empty Entries (EmbedNotReady) | R-05 |
| `test_multiple_session_registers` | 2 SessionRegister calls, then ContextSearch | Works normally (idempotent) | R-05 |

## Assertions

- SessionRegister always returns Ack (never Error, never Entries)
- Warming failure does not prevent Ack response
- Warming failure does not crash or panic
- After successful warming, ContextSearch returns results
- Without warming, ContextSearch returns empty results (graceful degradation)
- Multiple warming calls are harmless (get_adapter returns immediately when Ready)

## Edge Cases

- SessionRegister with embed service that transitions from Loading to Ready mid-call: get_adapter blocks correctly
- SessionRegister concurrent with another SessionRegister: both complete (RwLock allows concurrent reads once Ready)
- Very slow model loading: SessionRegister blocks until done (acceptable since hook is fire-and-forget)
