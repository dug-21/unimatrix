# Test Plan: goal-embedding

Component covers: `handle_cycle_event` extension in `crates/unimatrix-server/src/uds/listener.rs` and the new `Store::update_cycle_start_goal_embedding` method.

Test file: new `#[cfg(test)]` module inside `listener.rs` (or `crates/unimatrix-server/tests/goal_embedding.rs`).

---

## Component Responsibilities Under Test

1. `handle_cycle_event` spawns an embedding task after the INSERT spawn when lifecycle == `CycleLifecycle::Start` and goal is non-empty.
2. The embedding task calls `embed_service.get_adapter()`, handles `EmbedNotReady` with `tracing::warn!`, calls `adapter.embed_entry`, encodes the result, and calls `store.update_cycle_start_goal_embedding`.
3. On empty or absent goal, no embedding task is spawned and no warn is emitted.
4. `handle_cycle_event` returns synchronously (fire-and-forget; embedding runs asynchronously).
5. `dispatch_request` passes `embed_service` at all three `handle_cycle_event` call sites (Start, PhaseEnd, Stop).

---

## Test Infrastructure Requirements

### Stub EmbedServiceHandle

Tests require an injectable stub for `EmbedServiceHandle` that can be configured to:
- Return a fixed `Vec<f32>` embedding on success
- Return `EmbedNotReady`
- Return an error during `embed_entry`
- Record whether `get_adapter()` was called (call counter)

The stub must satisfy whatever interface `handle_cycle_event` uses to call the embed service. If `EmbedServiceHandle` is not directly mockable in tests, a trait-based seam or test-feature-gated constructor is needed. Delivery agent must provide this.

### tracing Capture

Tests that assert `tracing::warn!` is emitted use `tracing_test::traced_test` or an equivalent approach already established in the codebase. Check for existing tracing test helpers before introducing a new dependency.

### DB Access

Tests that assert `goal_embedding` is written read back via raw sqlx query on the store's `read_pool_test()` pool (same pattern as migration tests). An in-process `SqlxStore` instance in a `TempDir` is the fixture.

---

## Unit Test Expectations

### EMBED-SRV-01: goal_embedding written after cycle start (R-01, AC-02, AC-03, AC-13)

**This is the mandatory R-01 integration test.**

```rust
#[tokio::test]
async fn test_goal_embedding_written_after_cycle_start() {
    // Arrange: fresh store, stub embed service returning a 384-float Vec.
    // Arrange: construct a CycleStart ImplantEvent with goal = "design a query pipeline".
    // Act: call handle_cycle_event(event, CycleLifecycle::Start, session_registry, store, embed_service).
    // Synchronize: await both spawned tasks (store the JoinHandles or use a small tokio::time::sleep).
    // Assert: SELECT goal_embedding FROM cycle_events WHERE topic = ? AND event_type = 'cycle_start'
    //         returns a non-NULL blob.
    // Assert: decode_goal_embedding(&blob) == stub_embedding_vector.
}
```

**Synchronization note:** The test must await task completion before reading the DB. Options:
- Have `handle_cycle_event` return the JoinHandles in test builds (test-feature-gated)
- Use `tokio::time::sleep(Duration::from_millis(200))` as a pragmatic fallback (documented as an accepted timing assumption in the test)
- Use a oneshot channel in the stub embed handle to signal completion

The chosen approach must be documented in the test body comment.

---

### EMBED-SRV-02: Concurrent cycle starts — all embeddings written (R-01 stress scenario)

```rust
#[tokio::test]
#[ignore] // Slow test — run with `cargo test -- --ignored` in Stage 3c
async fn test_goal_embedding_concurrent_cycle_starts() {
    // Arrange: 20 distinct cycle_ids, each with a non-empty goal.
    // Act: fire 20 handle_cycle_event calls concurrently (join_all or spawn each).
    // Synchronize: wait for all embedding tasks to complete.
    // Assert: all 20 goal_embedding columns in cycle_events are non-NULL.
    // Assert: decoded embeddings have the expected dimension (384).
}
```

Marked `#[ignore]` as a slow/stress test. Must be explicitly invoked in Stage 3c to provide evidence for R-01 Coverage.

---

### EMBED-SRV-03: Empty goal → no task spawned, no warn, goal_embedding NULL (R-09, AC-04b)

```rust
#[tokio::test]
async fn test_no_embed_task_on_empty_goal() {
    // Arrange: stub embed handle with call counter starting at 0.
    // Arrange: CycleStart event with goal = "".
    // Act: call handle_cycle_event.
    // Wait briefly (no spawn should occur; sleep is safe here).
    // Assert: stub call_count == 0 (get_adapter was never called).
    // Assert: no tracing::warn! captured.
    // Assert: cycle_events row has goal_embedding IS NULL.
}
```

---

### EMBED-SRV-04: Absent goal → no task spawned, no warn (R-09, AC-04b)

```rust
#[tokio::test]
async fn test_no_embed_task_on_absent_goal() {
    // Same as EMBED-SRV-03 but goal = None in the event payload.
    // Identical assertions.
}
```

---

### EMBED-SRV-05: Embed service unavailable → warn emitted, cycle start not blocked, goal_embedding NULL (R-10, AC-04a)

```rust
#[tokio::test]
async fn test_goal_embedding_unavailable_service_warn() {
    // Arrange: stub embed handle configured to return EmbedNotReady.
    // Arrange: CycleStart event with goal = "test goal".
    // Act: call handle_cycle_event. Record wall-clock duration.
    // Assert: handle_cycle_event returns in < 5ms (call is not blocked by embed).
    // Synchronize: wait for the spawned embed task to complete.
    // Assert: tracing::warn! with expected message was emitted.
    // Assert: goal_embedding IS NULL in the cycle_events row.
}
```

---

### EMBED-SRV-06: Embed error during embed_entry → warn emitted, goal_embedding NULL (R-10, AC-04a)

```rust
#[tokio::test]
async fn test_goal_embedding_error_during_embed() {
    // Same structure as EMBED-SRV-05 but stub returns Ok(adapter) from get_adapter()
    // and the adapter returns Err from embed_entry.
    // Same assertions: warn captured, NULL blob, no panic.
}
```

---

### EMBED-SRV-07: handle_cycle_event returns before embedding completes (R-07, R-10 timing, NFR-01)

```rust
#[tokio::test]
async fn test_handle_cycle_event_returns_before_embedding() {
    // Arrange: stub embed handle that introduces a 200ms artificial delay.
    // Act: record start time, call handle_cycle_event, record end time.
    // Assert: elapsed < 10ms (well within 50ms UDS hook budget).
    // Note: no assertion on goal_embedding (may or may not be written depending on timing).
}
```

This validates the fire-and-forget contract: the UDS handler does not block on embed computation.

---

### EMBED-SRV-08: PhaseEnd and Stop events do not spawn embed task

```rust
#[tokio::test]
async fn test_no_embed_task_on_phase_end_event() {
    // Arrange: CycleLifecycle::PhaseEnd event with goal field set.
    // Act: call handle_cycle_event.
    // Assert: stub call_count == 0 (embedding only fires for Start).
}

#[tokio::test]
async fn test_no_embed_task_on_stop_event() {
    // Same for CycleLifecycle::Stop.
}
```

---

### EMBED-SRV-09: context_cycle MCP response text unchanged (R-12, AC-06)

```rust
#[tokio::test]
async fn test_context_cycle_response_text_unchanged() {
    // Arrange: call context_cycle(type=start, ...) through the MCP tool handler
    //          (not through UDS; use the MCP test harness if available).
    // Assert: returned text matches the expected pre-crt-043 response string byte-for-byte.
    // Note: this is a regression check. If the pre-crt-043 response text is parameterized,
    //       assert the template variables are unchanged.
}
```

If an MCP-level test harness is not available in the server unit test module, this test may use the infra-001 tools suite instead (see integration harness plan in OVERVIEW.md).

---

## Integration Test Expectation

The new lifecycle scenario `test_cycle_start_goal_does_not_block_response` in `product/test/infra-001/suites/test_lifecycle.py` validates NFR-01 through the actual MCP JSON-RPC interface:

```python
@pytest.mark.smoke
def test_cycle_start_goal_does_not_block_response(server):
    """crt-043: context_cycle start with non-empty goal returns promptly (fire-and-forget embed)."""
    import time
    start = time.monotonic()
    resp = server.call_tool("context_cycle", {
        "type": "start",
        "session_id": "test-session-goal-latency",
        "goal": "design a query pipeline for the Unimatrix cortical layer"
    })
    elapsed = time.monotonic() - start
    assert elapsed < 2.0, f"context_cycle start with goal must return within 2s, got {elapsed:.2f}s"
    assert resp.is_success(), f"expected success, got {resp}"
```

Note: 2s wall-clock budget is generous and accounts for binary startup in the fixture. The fire-and-forget contract is validated at unit level with tighter budgets (< 5ms for handle_cycle_event itself).

---

## Edge Cases

- **Goal = whitespace only**: Spec is silent on whitespace-only goals. Delivery agent must decide: treat as non-empty (embed whitespace string) or trim-then-check. The test must verify whichever behavior is implemented, consistently with the delivery note.
- **Multiple concurrent CycleStart for same cycle_id**: `update_cycle_start_goal_embedding` updates all matching rows. Test that a second embedding for the same cycle_id overwrites the first (last write wins — acceptable since it's an anomalous condition).
- **encode_goal_embedding failure**: If `encode_goal_embedding` returns an error (should be unreachable for valid Vec<f32>), the embed task must emit `tracing::warn!` and exit gracefully without panicking.

---

## Code Review Assertions (Static)

1. In `handle_cycle_event`, the embed `tokio::spawn` appears **after** the INSERT `tokio::spawn`, with no conditional path that could reorder them (R-01, ADR-002).
2. `adapter.embed_entry()` dispatches through `ml_inference_pool` (rayon pool), not directly on the tokio thread (R-07).
3. The embed task does **not** acquire the Store mutex independently — it calls `store.update_cycle_start_goal_embedding` which is the designated async store method (NFR-03, C-07).
4. `dispatch_request` passes `embed_service` at all three `handle_cycle_event` call sites (PhaseEnd and Stop paths receive it but the function ignores it for those lifecycle values).
5. `goal_for_event.is_some()` check (or equivalent non-empty check) guards the embed spawn — empty string must not enter the spawn path.
