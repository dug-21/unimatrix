# Agent Report: crt-022-agent-1-architect

## Task

Design the architecture for crt-022 (W1-2: Rayon Thread Pool + Embedding Migration).
Resolve SR-01 (OrtSession thread safety), SR-03 (timeout semantics), SR-04
(contradiction scan monopolisation).

## Deliverables

### Files Written

- `/workspaces/unimatrix/product/features/crt-022/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-022/architecture/ADR-001-rayon-in-server-only.md`
- `/workspaces/unimatrix/product/features/crt-022/architecture/ADR-002-timeout-at-bridge-not-call-site.md`
- `/workspaces/unimatrix/product/features/crt-022/architecture/ADR-003-scan-single-task-pool-floor-four.md`
- `/workspaces/unimatrix/product/features/crt-022/architecture/ADR-004-appstate-pool-distribution.md`

### Unimatrix Entries

- #2536 — ADR-001: Rayon Dependency Confined to unimatrix-server Only
- #2537 — ADR-002: Timeout Enforced via spawn_with_timeout on RayonPool
- #2538 — ADR-003: Contradiction Scan as Single Rayon Task; Pool Floor Raised to 4
- #2539 — ADR-004: Arc<RayonPool> Distributed via AppState

## SR Risk Resolutions

### SR-01 (High) — OrtSession Thread Safety

RESOLVED. Source analysis confirmed `OnnxProvider` wraps `Session` in
`Mutex<Session>`. The `test_send_sync` test in `onnx.rs` asserts `OnnxProvider:
Send + Sync` at compile time. `EmbedAdapter` wraps `Arc<dyn EmbeddingProvider>` and
is `Send + 'static`. Under rayon, multiple workers may hold `Arc<EmbedAdapter>`
clones concurrently; all calls to `embed_entry` serialise at `Mutex<Session>`. No
synchronization change is needed. The guarantee is documented in §thread-safety in
ARCHITECTURE.md and extended to the W1-4 NLI design requirement.

### SR-03 (High) — Timeout Semantics (OQ-2)

RESOLVED by ADR-002 (#2537). `RayonPool` exposes two methods: `spawn` (no timeout,
for background tasks) and `spawn_with_timeout(timeout: Duration, f: F)` (for MCP
handler paths). `RayonError` gains `TimedOut(Duration)` alongside `Cancelled`. All 7
MCP handler embedding call sites use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`.
Timeout coverage is preserved exactly; call sites pass the duration explicitly.
Caveat documented: `tokio::time::timeout` cancels the async wait but does not
terminate the rayon worker thread. Pool sizing (ADR-003) accounts for this.

### SR-04 (Medium) — Contradiction Scan Monopolisation

RESOLVED by ADR-003 (#2538). Per-entry decomposition was evaluated and rejected:
`Mutex<Session>` serialises ONNX inference regardless of task count, so
parallelisation adds overhead without throughput benefit. Scan remains as a single
rayon task. Pool floor raised from 2 to 4 (formula: `max(num_cpus / 2, 4).min(8)`)
to guarantee at least 2 threads available for MCP inference even with both background
tasks (scan + quality-gate) running concurrently.

## Key Design Decisions

1. `rayon = "1"` only in `unimatrix-server` — crate boundary enforced (ADR-001)
2. `AsyncEmbedService` removed from `unimatrix-core` — zero consumers confirmed
3. `RayonPool::spawn_with_timeout` is the MCP handler pattern; `spawn` for background
4. Pool named `ml_inference_pool`, floor 4, ceiling 8, config range [1,64]
5. `Arc<RayonPool>` on `AppState` — single distribution point for W1-4/W2-4/W3-1
6. Call-site pattern: replace `spawn_blocking_with_timeout` with
   `rayon_pool.spawn_with_timeout`, double `.map_err` chain unchanged
7. `OnnxProvider::new` stays on `spawn_blocking` — model I/O is not inference

## Open Questions

None. All architectural questions resolved.
