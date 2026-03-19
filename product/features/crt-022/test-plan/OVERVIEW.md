# crt-022 Test Plan Overview: Rayon Thread Pool + Embedding Migration

## Test Strategy

Testing for crt-022 operates at three levels:

1. **Unit tests** (`cargo test -p unimatrix-server`) — Pure logic in `RayonPool`, `RayonError`,
   and `InferenceConfig`. No ONNX model required. Panic containment, timeout semantics, pool
   lifecycle, and config boundary validation. These are self-contained and run without the binary.

2. **Static / grep audits** — Structural checks run as shell commands during Stage 3c.
   Verify call-site migration correctness, `AsyncEmbedService` absence, `spawn_blocking`
   elimination from inference paths, `Cargo.toml` dependency constraints.
   These are deterministic and do not depend on runtime behaviour.

3. **Integration tests via infra-001** — End-to-end MCP protocol tests against the compiled
   `unimatrix-server` binary. Verify that embedding-dependent tools (`context_search`,
   `context_store`, `context_correct`, `context_status`) continue to work correctly through
   the rayon bridge. The smoke suite is the mandatory gate; the `tools` and `lifecycle` suites
   are required for this feature.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk | Test Component | Test Type |
|---------|----------|------|---------------|-----------|
| R-01 | High | Panic in rayon closure propagates to tokio runtime | rayon_pool.md §panic-containment | Unit |
| R-02 | Critical | Pool threads silently occupied after timeout; pool capacity degrades | rayon_pool.md §timeout-semantics | Unit |
| R-03 | High | Mutex poison after OrtSession hang converts TimedOut to Cancelled for all callers | rayon_pool.md §mutex-poison | Unit (bridge boundary) |
| R-04 | Critical | MCP call site uses `spawn` instead of `spawn_with_timeout` | call_site_migration.md §method-audit, ci_enforcement.md | Grep/Static |
| R-05 | Med | `AsyncEmbedService` removal breaks workspace consumer | async_embed_removal.md | Shell + grep |
| R-06 | High | Missed `spawn_blocking` inference site post-migration | ci_enforcement.md §spawn-blocking-grep | CI grep step |
| R-07 | Med | Invalid `rayon_pool_size` reaches pool construction | inference_config.md, rayon_pool.md §startup-error | Unit + Integration |
| R-08 | High | Pool exhaustion deadlock under concurrent background + MCP load | rayon_pool.md §concurrency | Unit |
| R-09 | Med | Second `RayonPool` instantiated ad-hoc in W1-4 wiring | call_site_migration.md §single-instantiation | Grep |
| R-10 | Low | `OnnxProvider::new` accidentally migrated to rayon | call_site_migration.md §embed-handle-guard | Grep |
| R-11 | Low | Rayon version drift to 2.x breaks ThreadPoolBuilder | ci_enforcement.md §cargo-toml-check | Cargo.toml inspection |
| R-Security | Med | Adversarial input exhausts all pool threads via spawn_with_timeout | rayon_pool.md §adversarial | Unit (bounded blast radius) |

---

## AC Coverage Map

| AC-ID | Test Plan Component | Test Type |
|-------|---------------------|-----------|
| AC-01 | ci_enforcement.md §crate-boundary | Shell: `cargo tree` |
| AC-02 | rayon_pool.md §file-existence | File check + unit test |
| AC-03 | rayon_pool.md §panic-containment | Unit |
| AC-04 | Integration smoke suite | Integration (infra-001) |
| AC-05 | async_embed_removal.md | Shell + grep + cargo check |
| AC-06 | call_site_migration.md §per-site-audit | Grep: 7 locations |
| AC-07 | ci_enforcement.md §spawn-blocking-grep | CI grep step |
| AC-08 | call_site_migration.md §embed-handle-guard | Grep: embed_handle.rs |
| AC-09 | inference_config.md §validate-unit-tests | Unit (8 boundary values) + integration startup |
| AC-10 | Integration tools + lifecycle suites | Integration (infra-001) |
| AC-11 | rayon_pool.md + inference_config.md | Unit (8 required tests) |

---

## Test Execution Order

Stage 3c executes in this order:

1. `cargo test --workspace 2>&1 | tail -30` — all unit tests
2. Grep/static audits (fast, no binary required)
3. `cargo build --release` — build binary
4. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` — mandatory gate
5. `python -m pytest suites/test_tools.py -v --timeout=60`
6. `python -m pytest suites/test_lifecycle.py -v --timeout=60`

---

## Integration Harness Plan

### Feature Touch Points

crt-022 changes:
- Server tool behaviour: embedding now runs through rayon instead of `spawn_blocking`
- Store, retrieval, correction, status tools all invoke embedding via the new bridge
- No schema changes; no new MCP tools; no confidence or contradiction logic changes

### Suite Selection (from USAGE-PROTOCOL.md table)

| Suite | Run? | Reason |
|-------|------|--------|
| `protocol` | No (smoke covers this) | No protocol changes in crt-022 |
| `tools` | YES | All embedding-dependent tools (search, store, correct, status) changed internally |
| `lifecycle` | YES | Multi-step store→search flows exercise the bridge end-to-end |
| `volume` | No | No scale behaviour changes; volume tests unaffected by bridge internals |
| `security` | No | No security logic changes; content scanning unchanged |
| `confidence` | No | No confidence formula changes |
| `contradiction` | No | Contradiction scan threading changed (rayon instead of spawn_blocking), covered by smoke |
| `edge_cases` | No | Edge-case embedding behaviour unchanged by bridge |
| `adaptation` | No | No allowlist or format changes |
| `smoke` | YES (mandatory gate) | Minimum gate before any further testing |

### Existing Suite Coverage of crt-022

The `tools` suite exercises:
- `context_search` — triggers query embedding through the rayon bridge
- `context_store` — triggers store-path embedding through the rayon bridge
- `context_correct` — triggers correction-path embedding through the rayon bridge
- `context_status` — triggers embedding consistency check through the rayon bridge

The `lifecycle` suite exercises multi-step flows that chain these tools. Any regression
in the rayon bridge (panic propagation, deadlock, incorrect error mapping) would surface
here as a test failure.

### New Integration Tests Needed

One new test for AC-09 / R-07 (startup rejection on invalid config):

**Suite**: `test_tools.py` (or a new `test_startup.py` file — prefer `test_tools.py` to avoid
new suite overhead).

**Test**: `test_server_rejects_invalid_rayon_pool_size`

```python
# Fixture: server with rayon_pool_size = 0 in config
# Assert: server process exits with non-zero status before accepting MCP connections
# Assert: stderr contains a structured error message referencing [inference] rayon_pool_size
```

This test is not coverable by unit tests alone because it requires the full binary startup
path through config loading, validation, and pool construction. It verifies AC-09 end-to-end.

**Fixture to use**: Custom fixture (not `server`); needs to launch the binary with a bad config
and check the exit code without connecting to it. This is a lighter variant of the `server`
fixture that does not wait for MCP readiness.

**Decision**: If adding a custom fixture to `conftest.py` is required, scope it tightly.
If the harness infrastructure cost is high, file a GH Issue and accept a unit-test-only
coverage gap for this scenario with a note in RISK-COVERAGE-REPORT.md.

### Convention for New Tests

```python
# Naming pattern
def test_server_rejects_invalid_rayon_pool_size(bad_config_server): ...
```

Fixture scope: `function` (fresh process, no state leakage). Mark with `@pytest.mark.smoke`
if the server rejection is fast enough (<5s) — it should be.

---

## Cross-Component Dependencies

| Component | Depends On | Test Dependency |
|-----------|-----------|-----------------|
| `RayonPool` | rayon crate, tokio oneshot | Unit tests require tokio runtime (`#[tokio::test]`) |
| `InferenceConfig` | `ConfigError` enum (config.rs) | Must extend existing `ConfigError` pattern |
| Call-site migration | `RayonPool` (must exist) | Grep tests are order-independent |
| `AsyncEmbedService` removal | `async_wrappers.rs` current state | Must be verified after removal |
| CI enforcement | All above | CI step runs after all code changes are complete |

Integration tests depend on: all 5 components above being implemented correctly. The smoke
suite is the integration gate; partial implementation that passes smoke is not acceptable —
all tools used in smoke tests must function correctly through the rayon bridge.

---

## Edge Cases Requiring Test Coverage

From RISK-TEST-STRATEGY.md §Edge Cases:

| Case | Covered In | Test Type |
|------|-----------|-----------|
| Single-core container: `num_cpus = 1` → pool size 4 | rayon_pool.md §pool-init | Unit |
| `rayon_pool_size = 1`: single thread, no deadlock | rayon_pool.md §pool-init | Unit |
| Pool shutdown while closure queued | rayon_pool.md §shutdown | Unit |
| N simultaneous timeouts: pool remains healthy | rayon_pool.md §timeout-semantics | Unit |
| Zero-length input panic containment | rayon_pool.md §panic-containment | Unit (covered by general panic path) |
