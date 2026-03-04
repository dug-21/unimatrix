## ADR-002: Lazy Eviction for Rate Limiter Sliding Window

### Context

The rate limiter needs a sliding window to track per-caller request timestamps. Window entries older than 1 hour must be evicted to prevent unbounded memory growth. Two strategies:

1. **Lazy eviction**: Evict expired entries on each `check_*_rate()` call. Simple, no background tasks, but stale entries accumulate for inactive callers.
2. **Proactive eviction**: Background timer (e.g., tokio interval) periodically sweeps all windows. Keeps memory tighter but adds a background task, cancellation logic, and timer coordination.

Memory analysis: With N callers and a 1-hour window at 300 requests/hour max, worst case is N * 300 timestamps * 16 bytes = ~5KB per caller. Even with 100 callers, this is 500KB — negligible. Inactive callers accumulate at most their last window of timestamps (300 * 16 = ~5KB) until they make another request.

### Decision

Use lazy eviction. On each `check_*_rate()` call:

1. Acquire `Mutex<HashMap<CallerId, SlidingWindow>>`
2. Get or insert window for this CallerId
3. `window.timestamps.retain(|t| t.elapsed() < Duration::from_secs(window_secs))`
4. Check length against limit
5. If under limit, push `Instant::now()` and return Ok
6. If at limit, compute `retry_after` from oldest remaining timestamp and return Err

No background task, no timer, no cancellation logic. Memory for inactive callers is bounded and negligible.

### Consequences

**Easier**:
- No background task or timer coordination
- No shutdown cleanup needed
- Deterministic behavior — eviction happens exactly when the window is checked
- Testable without async runtime (Mutex + Instant only)

**Harder**:
- Memory for inactive callers persists until they make another request (bounded, negligible at expected scale)
- If a caller stops making requests entirely, their window stays in the HashMap forever. Could add a secondary sweep on `context_status maintain=true` if this becomes a concern.
