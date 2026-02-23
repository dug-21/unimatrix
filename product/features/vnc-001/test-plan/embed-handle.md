# Test Plan: embed_handle.rs

## Risks Covered
- R-10: Embedding model download failure (High)

## Unit Tests

### State machine

```
test_new_starts_loading
  Act: EmbedServiceHandle::new()
  Assert: is_ready() == false

test_ready_after_load (requires model or mock)
  Arrange: handle in Loading state
  Act: transition to Ready (set state directly in test)
  Assert: is_ready() == true, get_adapter() returns Ok

test_failed_state
  Arrange: handle in Loading state
  Act: transition to Failed("error msg")
  Assert: is_ready() == false, get_adapter() returns EmbedFailed

test_get_adapter_loading_returns_embed_not_ready
  Arrange: handle in Loading state (fresh new())
  Act: get_adapter().await
  Assert: Err(ServerError::EmbedNotReady)

test_get_adapter_failed_returns_embed_failed
  Arrange: handle transitioned to Failed
  Act: get_adapter().await
  Assert: Err(ServerError::EmbedFailed(_))
```

Note: Testing the full `start_loading` -> Ready path requires a cached embedding model. These tests are marked `#[ignore]` with a comment explaining the dependency. The state machine itself is tested by directly setting state.

### Helper for testing

Since `EmbedState` is private, tests need a way to set state. Options:
1. Add `#[cfg(test)]` helper method on `EmbedServiceHandle` that sets state directly
2. Test via the public interface only (requires model)

Recommended: Add a `#[cfg(test)]` method `set_state_for_test(state: EmbedState)` to enable isolated state machine testing without model download.
