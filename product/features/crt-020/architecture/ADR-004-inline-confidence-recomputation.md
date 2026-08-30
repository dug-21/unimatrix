## ADR-004: Inline vs Deferred Confidence Recomputation

### Context

After applying implicit votes via `record_usage_with_confidence`, the `confidence_fn` parameter
enables inline confidence recomputation for each affected entry within the same transaction. The
alternative is to skip `confidence_fn` in the implicit vote step and let the subsequent tick's
confidence refresh batch pick up the updated `helpful_count` / `unhelpful_count` values.

SR-03 from the risk assessment flags that inline recomputation may extend tick duration when
processing the full batch cap (500 sessions × N entries). The existing confidence refresh step
(`run_maintenance` step 2) already uses inline recomputation for up to
`MAX_CONFIDENCE_REFRESH_BATCH = 500` entries, gated by a 200ms duration guard.

**Analysis**:

Inline recomputation in `record_usage_with_confidence` processes each entry_id in the `all_ids`
slice. For each: one `SELECT` (read back the updated entry), one tag load, one confidence
computation (pure math, no I/O), one `UPDATE confidence`. At ~0.2ms per entry in a `BEGIN
IMMEDIATE` transaction with WAL mode enabled, 2,500 entries (500 sessions × 5 avg injections)
takes approximately 500ms.

This is within the tick budget (120s total, run_maintenance takes ~2–10s). The risk is
non-trivial variance: if entry updates are slower due to write contention from concurrent MCP
calls or if the batch happens to include many large entries, the step could approach 5–10s.

The existing `record_usage_with_confidence` call from `run_confidence_consumer` (real-time path)
already uses inline recomputation — so the pattern is established.

**Alternative: deferred recomputation**

Skip `confidence_fn = None` in the implicit vote step. The confidence refresh batch in the
subsequent tick will pick up the updated vote counts. Trade-off: a ~15-minute delay before the
new votes influence confidence scores. During the cold-start drain, entries that receive votes
in tick N don't get their confidence scores updated until tick N+1.

**Decision basis**: The purpose of applying votes inline with recomputation is that the
confidence refresh batch in `run_maintenance` runs before the implicit vote step (it is step 1 in
`run_maintenance`, while implicit votes run after `run_maintenance` returns). If we defer, the
votes applied in tick N will influence confidence in tick N+1's refresh. This is a 15-minute lag
— acceptable given that the feature is background and complementary.

However, crt-019's design intent is that confidence differentiates entries through vote signals.
Applying votes without recomputing confidence in the same tick creates a window where the vote
counters are updated but confidence doesn't reflect them — which could cause the confidence
refresh batch to start from stale values.

The safer approach is to recompute inline, accepting the bounded extra duration. If this proves
too slow in practice (observable via tracing logs), a follow-up can switch to deferred mode
by passing `None` as `confidence_fn`.

### Decision

Use **inline confidence recomputation** (pass `confidence_fn = Some(...)` to
`record_usage_with_confidence` in the implicit vote step).

The confidence function closure uses `alpha0`/`beta0` snapshotted from `ConfidenceStateHandle`
before entering `spawn_blocking`, following the established pattern from `UsageService::record_mcp_usage`
(ADR-001 of crt-019, entry #1543 in Unimatrix).

```rust
let (alpha0, beta0) = {
    let guard = confidence_state.read().unwrap_or_else(|e| e.into_inner());
    (guard.alpha0, guard.beta0)
};
let confidence_fn: Box<dyn Fn(&EntryRecord, u64) -> f64 + Send> =
    Box::new(move |entry, now| compute_confidence(entry, now, alpha0, beta0));
```

This snapshot is taken once before the `spawn_blocking` call. The single snapshot is reused for
both the helpful and the unhelpful `record_usage_with_confidence` calls within the same tick body,
which is correct: both calls should use the same prior parameters snapshot for consistency.

**Duration guard (SR-03 mitigation)**: If `apply_implicit_votes` consistently exceeds 10s in
production (observable via tracing::debug timing), the `confidence_fn` parameter can be changed
to `None` without any further design changes. This is a configuration-level knob, not an
architectural change.

### Consequences

**Easier**:
- Confidence scores reflect new implicit votes within the same tick they are applied.
- The approach is consistent with the existing `UsageService::record_mcp_usage` pattern.
- No 15-minute lag on confidence signal activation after cold-start drain.

**Harder**:
- Tick duration includes the confidence recomputation overhead (~0.1–0.2ms per entry).
  At 500 sessions × 5 entries = 2,500 entries this is ~250–500ms of overhead.
- If the implicit vote step is extended to larger batch sizes in future, the inline
  recomputation overhead grows proportionally. Switching to deferred mode would be necessary.
