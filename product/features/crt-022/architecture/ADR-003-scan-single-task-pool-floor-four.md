## ADR-003: Contradiction Scan as Single Rayon Task; Pool Floor Raised to 4

### Context

The contradiction scan (`background.rs:543`) iterates over all entries in the
knowledge base, calling `adapter.embed_entry` for each entry in a loop inside a
single `rayon_pool.spawn()` closure. On a large knowledge base this scan can take
minutes. It occupies one rayon thread for its full duration.

SCOPE.md proposed the default pool formula as `max(num_cpus / 2, 2).min(8)`, which
on a dual-core machine yields 2 threads. If the contradiction scan occupies one
thread, only one thread remains for all concurrent MCP embedding calls (search,
store, correct, status, warmup). This can produce visible queuing latency.

Two options were evaluated:

**Option A — Decompose scan into per-entry rayon tasks**
Each entry's `embed_entry` call becomes a separate `rayon_pool.spawn`. Results are
collected via a channel or `Arc<Mutex<Vec<...>>>`. The scan is embarrassingly
parallel at the entry level.

Problems with Option A:
1. `Mutex<Session>` in `OnnxProvider` serialises all concurrent `embed_entry` calls
   regardless of how many rayon tasks are submitted. Parallelising the task
   submissions does not increase ONNX throughput — it just adds overhead.
2. `scan_contradictions` is a stateful integrated loop; decomposing it requires
   refactoring the loop internals to collect partial results asynchronously.
3. Rayon submission overhead for `N` per-entry tasks adds `N` oneshot channel
   round-trips compared to 1 for the current design.
4. The net effect: more complexity, more overhead, same ONNX throughput (still
   serial through the session mutex). Option A is rejected.

**Option B — Single rayon task; raise pool floor**
Keep the scan as a single closure. Raise the pool floor so the remaining threads are
sufficient for concurrent MCP inference.

Pool floor derivation under Option B:
- Contradiction scan: occupies at most 1 thread at a time
- Quality-gate embedding loop: occupies at most 1 thread at a time
- Both background tasks can run concurrently in theory (they are on different tick
  intervals): worst case is 2 threads consumed by background work simultaneously
- MCP embedding calls under moderate load: at least 2 concurrent calls need threads
  (2 concurrent MCP sessions each running embedding)
- Minimum: 2 (background) + 2 (MCP) = 4 threads

**For W1-4 (NLI)**: NLI post-store inference is a fire-and-forget task that runs
after each `context_store`. It consumes 1 thread per call. Under concurrent store
operations, multiple NLI tasks can queue. With the pool at 4+, NLI tasks queue
behind ongoing work rather than starvation. Accounting for W1-4: the 4-thread floor
remains defensible — NLI tasks are short (50–200ms) and the pool sizes upward on
more capable hardware.

### Decision

**Option B is adopted.** The contradiction scan remains as a single rayon task.
The pool size default formula is `max(num_cpus / 2, 4).min(8)`.

The floor is 4, not 2. This supersedes the SCOPE.md floor of 2. On single-core
containers, the formula yields `max(0, 4) = 4` — deliberately higher than the
available CPU, because ONNX inference is I/O-bound in terms of data movement and
the tokio runtime competes for CPU separately from the rayon pool.

Config range `[1, 64]` allows operators to tune downward on resource-constrained
deployments. The config validation rejects values outside this range with a
structured error at startup. The floor of 4 is the compiled default, not a hard
minimum.

The pool is named `ml_inference_pool` (human-approved, OQ-1 resolution) to convey
its purpose without ONNX specificity, accommodating W1-4 NLI and W3-1 GNN as
future consumers.

### Consequences

Easier:
- Contradiction scan implementation is unchanged; migration is a one-line change
  (replace `spawn_blocking` with `rayon_pool.spawn`)
- Pool floor of 4 provides headroom for W1-4 NLI tasks without pool redesign
- No per-entry channel overhead or result collection complexity

Harder:
- A very long-running contradiction scan (minutes on a large knowledge base) still
  occupies one rayon thread throughout. Under pool size 4, this leaves 3 threads for
  MCP inference — adequate but not generous. Operators with large knowledge bases
  should increase `rayon_pool_size` in config.
- The lack of yield points inside the scan means a scan cannot be preempted by
  higher-priority MCP calls. This is a known limitation, documented here, and
  accepted as the correct tradeoff versus the complexity of per-entry decomposition.
