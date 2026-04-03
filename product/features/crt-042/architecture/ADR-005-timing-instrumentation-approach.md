## ADR-005: Phase 0 Timing Instrumentation via `debug!` — Wall-Clock Duration in Milliseconds

### Context

SR-01 from SCOPE-RISK-ASSESSMENT.md requires that the architecture wire timing instrumentation
into Phase 0 before the feature is considered architecturally complete. The instrumentation
enables the A/B eval to capture latency data alongside MRR/P@5, so the post-measurement gate
condition (P95 latency ceiling) can be evaluated.

Four instrumentation options were considered:

**Option A — No instrumentation**: Rely on external profiling or system-level timing. Provides
no per-request data for the eval framework.

**Option B — Dedicated metrics counter** (e.g., Prometheus histogram, internal counter):
Requires a metrics infrastructure that does not exist in the codebase. Adding it would be
significant scope creep.

**Option C — Structured log with elapsed duration (`debug!` trace)**: Use `std::time::Instant`
before Phase 0, compute elapsed after Phase 0 ends, emit a single `debug!` log line with:
- `expanded_count` — how many entries graph_expand returned
- `fetched_count` — how many passed quarantine and embedding checks
- `elapsed_ms` — wall-clock duration of Phase 0 in milliseconds
- `depth` — configured expansion depth
- `max_candidates` — configured max_expansion_candidates

**Option D — Structured log at `info!` level**: Same as C but at `info!` level, emitting on
every search request when the expander is enabled.

**Option B** is out of scope — no metrics infrastructure exists. Adding it would be a separate
feature, not a timing tracing concern.

**Option D** (`info!`) would produce high-volume log output on every search request (potentially
hundreds of lines per minute in production). `info!` is visible in default log levels. The eval
harness and production deployments would be flooded.

**Option C** (`debug!`) is the correct level for per-request hot-path instrumentation. `debug!`
is disabled by default in production builds (RUST_LOG=info). It is enabled selectively during
eval runs or latency profiling via `RUST_LOG=unimatrix_server::services::search=debug`. This
matches the existing `debug!` instrumentation pattern in search.rs (multiple debug! calls in
the Phase 1–5 block).

**Option A** is excluded by SR-01: the scope risk assessment explicitly requires the timing
to be wired into Phase 0 so the A/B eval can capture it.

The `debug!` macro overhead when disabled is a single pointer comparison — negligible on the
hot path. The `Instant::now()` call (one allocation at Phase 0 entry when expander is enabled)
is similarly negligible.

### Decision

Phase 0 emits a single `debug!` trace event on completion (after the fetch loop):

```rust
let phase0_start = std::time::Instant::now();
// ... BFS traversal and fetch loop ...
tracing::debug!(
    expanded_count = expanded_ids.len(),
    fetched_count = results_added,
    elapsed_ms = phase0_start.elapsed().as_millis(),
    expansion_depth = self.expansion_depth,
    max_expansion_candidates = self.max_expansion_candidates,
    "Phase 0 (graph_expand) complete"
);
```

`Instant::now()` is called only inside the `if self.ppr_expander_enabled` branch — zero overhead
on the default (disabled) path.

**Latency gate condition** (post-measurement, not a pre-commitment):

Before `ppr_expander_enabled` can be set to `true` as a default in any configuration, the
following gate must be satisfied based on measured eval data:

> P95 Phase 0 `elapsed_ms` across the ASS-039 scenario set must be < 50ms.

The 50ms ceiling is the architecture's pre-measurement estimate. It may be revised upward or
downward based on actual measurements. If the P95 exceeds 50ms, the O(1) embedding lookup
investigation (ADR-003) must be completed before default enablement. The feature flag remains
the enforcement mechanism — the ceiling is a documented decision criterion, not a hard compile-time
constraint.

### Consequences

- Eval runs with `RUST_LOG=..search=debug` capture per-scenario Phase 0 latency.
- The A/B eval framework can aggregate `elapsed_ms` from debug output to compute P95.
- No metrics infrastructure is required; no new dependencies.
- `debug!` overhead on disabled path is zero.
- The 50ms gate condition is documented here and must be referenced in the delivery brief's
  eval gate section so the delivery agent and eval runner know what to measure.
- If O(1) embedding lookup is implemented (ADR-003), the 50ms ceiling is likely unnecessary
  and can be removed in a follow-up ADR.
