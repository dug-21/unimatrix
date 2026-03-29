## ADR-008: Step 6d Latency Budget and RayonPool Offload (DEFERRED)

### Context

SR-01 in the risk assessment identifies that PPR power iteration is O(I × E_pos) per search
call. At large scale (100K entries, 500K edges) without offload, PPR could take 10–50 ms
in the Tokio async context, starving other async tasks and approaching the `MCP_HANDLER_TIMEOUT`
deadline.

SR-02 identifies that sequential store fetches for up to `ppr_max_expand = 50` PPR-only
entries add latency inside the search hot path.

Two questions were raised during design:
1. What is the acceptable latency ceiling for Step 6d as a whole?
2. At what scale should the PPR computation be offloaded to `RayonPool`?

**Current `context_search` timeout**: `MCP_HANDLER_TIMEOUT` (defined in
`infra/timeout.rs`). PPR must leave sufficient headroom for Steps 6c, 7, and 10b, which
include co-access prefetch (async with `spawn_blocking_with_timeout`) and NLI scoring
(rayon offload).

**RayonPool pattern**: `rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` is the
established pattern for CPU-bound work in MCP handlers (ADR-002 rayon_pool). PPR computation
is CPU-bound (pure in-memory HashMap operations) and fits this pattern — but this path
is not needed at current production scale.

**Store fetch for PPR entries**: Async `entry_store.get()` calls cannot be rayon-offloaded.
They remain sequential async operations regardless of whether the PPR computation is
offloaded. At `ppr_max_expand = 50` and sub-millisecond SQLite latency, 50 sequential
fetches add < 10 ms.

### Decision

**Rayon offload is DEFERRED.** crt-030 ships the inline synchronous path only.
`PPR_RAYON_OFFLOAD_THRESHOLD` is not defined in this feature. A follow-up issue will
scope the offload path when 100K+ scale is reached.

**Latency budget for Step 6d (informational — informs the deferral rationale):**
- PPR computation (power iteration + sorted accumulation): ≤ 5 ms budget at the default
  scale (< 10K entries). Beyond this, it is a regression.
- PPR-only entry store fetches: ≤ 10 ms added latency (50 entries × sub-ms SQLite).
- Total Step 6d budget: ≤ 15 ms at the 10K scale.
- At 100K scale (~10–50 ms PPR computation), offload to RayonPool would be warranted —
  this is the trigger condition for the follow-up issue.

**crt-030 ships**: the inline synchronous call only:

```rust
let ppr_scores = personalized_pagerank(&typed_graph, &seed_scores, alpha, iterations);
```

No threshold constant, no conditional branch. The follow-up issue will add
`PPR_RAYON_OFFLOAD_THRESHOLD` and the offload branch when 100K+ scale is reached.

**Sequential store fetch policy**: Sequential async `entry_store.get()` calls are the
correct v1 implementation for PPR-only entry fetches. The 50-entry × sub-ms latency
budget is acceptable for in-memory SQLite. A batch fetch optimization is deferred to a
follow-up issue if storage layer changes (e.g., remote storage in a future W2-1 container
packaging change).

### Consequences

- The latency budget is defined and measurable: a regression test or benchmark can verify
  that Step 6d stays within 15 ms at 10K entries.
- Implementation is simpler — no conditional branch, no threshold constant to maintain.
- At current production scale (< 10K entries), the inline path adds < 1 ms. No rayon pool
  pressure is added by PPR.
- The `MCP_HANDLER_TIMEOUT` remains the outer bound. If PPR cannot complete within the
  timeout (extremely unlikely at < 10K scale), it returns an empty map and the search
  proceeds without PPR expansion.
- When 100K+ scale is reached, a follow-up issue will define `PPR_RAYON_OFFLOAD_THRESHOLD`
  and add the offload branch. The latency analysis in this ADR provides the justification.
