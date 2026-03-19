# Risk Coverage Report: crt-022

Rayon Thread Pool + Embedding Migration (W1-2)

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Panic in rayon closure propagates to tokio runtime | `test_spawn_panic_returns_cancelled`, `test_spawn_with_timeout_panic_returns_cancelled_not_timeout`, `test_pool_functional_after_panic` | PASS | Full |
| R-02 | Pool threads silently occupied after timeout; pool capacity degrades | `test_spawn_with_timeout_fires_when_closure_exceeds_timeout`, `test_spawn_timeout_duration_preserved`, `test_pool_accepts_new_submissions_after_timeout`, `test_pool_size_accessor_unchanged_after_timeout` | PASS | Full |
| R-03 | Mutex poisoning after OrtSession hang converts TimedOut to Cancelled | `test_spawn_panic_with_mutex_held` | PASS | Full |
| R-04 | MCP call site uses `spawn` instead of `spawn_with_timeout` | Static grep: all 5 MCP sites use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`; background sites use `spawn`; module rustdoc documents convention; CI grep step passes | PASS | Full |
| R-05 | `AsyncEmbedService` removal breaks workspace consumer | `grep -r "AsyncEmbedService" crates/ → 0 results`; `cargo build --release` exits 0; `AsyncVectorStore` retained | PASS | Full |
| R-06 | Missed `spawn_blocking` ONNX inference site post-migration | CI script `scripts/check-inference-sites.sh` exits 0; grep of services/ returns 0 inference-site results | PASS | Full |
| R-07 | Invalid `rayon_pool_size` reaches pool construction | `test_inference_config_rejects_zero`, `test_inference_config_rejects_sixty_five`, `test_inference_config_valid_lower_bound`, `test_inference_config_valid_upper_bound` | PASS | Partial (see Gaps) |
| R-08 | Pool exhaustion deadlock under concurrent background + MCP load | `test_pool_does_not_deadlock_under_full_occupancy`, `test_two_background_and_two_mcp_concurrent` | PASS | Full |
| R-09 | Second `RayonPool` instantiated ad-hoc in W1-4 wiring | `grep "RayonPool::new" src/main.rs` → 2 results in mutually exclusive startup functions (`tokio_main_daemon`, `tokio_main_stdio`); single `ml_inference_pool` field definition | PASS | Full |
| R-10 | `OnnxProvider::new` accidentally migrated to rayon | `grep -n "spawn_blocking" embed_handle.rs` → exactly 1 result (line 76, `OnnxProvider::new`); no rayon call in file | PASS | Full |
| R-11 | Rayon version drift to 2.x breaks ThreadPoolBuilder | `grep "rayon" Cargo.toml` → `rayon = "1"` (semver-pinned to 1.x) | PASS | Full |
| R-Security | Adversarial input exhausts pool threads | `test_adversarial_timeout_does_not_hang_pool` | PASS | Full |

---

## Test Results

### Unit Tests

- Total: 2583
- Passed: 2583
- Failed: 0
- Ignored: 18

**Breakdown by crate:**

| Crate | Passed | Failed |
|-------|--------|--------|
| unimatrix-server | 1496 | 0 |
| unimatrix-store | 353 | 0 |
| unimatrix-vector | 291 | 0 |
| unimatrix-core | 76 | 0 |
| unimatrix-embed | 73 | 0 |
| unimatrix-server (bin/integration) | 129 | 0 |
| unimatrix-engine (or other) | 165 | 0 |

**crt-022-specific unit tests:**

| Test Module | Tests | Passed |
|------------|-------|--------|
| `infra::rayon_pool::tests` | 21 | 21 |
| `infra::config::tests` (InferenceConfig group) | 12 | 12 |

**Pre-existing doctest failure (not caused by crt-022):**

```
crates/unimatrix-server/src/infra/config.rs - infra::config (line 21) -- FAILED
```

This doctest failure existed on the main branch before crt-022. The doc comment
on line 21 contains `~/.unimatrix/config.toml` which the doctest runner
misinterprets as Rust syntax. Excluded from `--lib` run (which is what passes gates).
No action required in this feature; pre-existing issue.

### Integration Tests (infra-001)

| Suite | Run | Passed | Failed | XFailed |
|-------|-----|--------|--------|---------|
| smoke (mandatory gate) | YES | 20 | 0 | 0 |
| tools | YES | 72 | 0 | 1 |
| lifecycle | YES | 24 | 0 | 1 |
| **Total** | | **116** | **0** | **2** |

**XFailed tests (pre-existing, not caused by crt-022):**

1. `test_lifecycle.py::test_auto_quarantine_after_consecutive_bad_ticks` — requires `UNIMATRIX_TICK_INTERVAL_SECONDS` env var to drive ticks in test; pre-existing xfail; no GH Issue filed (already documented in test).

2. `test_tools.py` — 1 xfailed test (pre-existing xfail, already marked before crt-022).

Both xfails are pre-existing and have no relation to the rayon pool migration.

---

## Static Audit Results (AC-06, AC-07, AC-08, AC-01, R-11)

### spawn_blocking Elimination (AC-07)

```
bash scripts/check-inference-sites.sh → OK: all spawn_blocking enforcement checks passed
```

- `services/search.rs` embedding site: migrated to `spawn_with_timeout` (line 235)
- `services/store_ops.rs` embedding site: migrated to `spawn_with_timeout` (line 120)
- `services/store_correct.rs` embedding site: migrated to `spawn_with_timeout` (line 52)
- `services/status.rs` embedding site: migrated to `spawn_with_timeout` (line 549)
- `uds/listener.rs` warmup site: migrated to `spawn_with_timeout` (line 1393)
- `background.rs` contradiction scan: migrated to `spawn` (line 550)
- `background.rs` quality-gate loop: migrated to `spawn` (line 1169)

Note: `search.rs` retains `spawn_blocking_with_timeout` at line 462 for `compute_search_boost`
(graph-traversal DB operation, not ONNX inference). The CI script correctly excludes this site
because it checks for `spawn_blocking` adjacent to `embed_entry`, not the entire file.

### MCP_HANDLER_TIMEOUT Propagation (R-04 Integration Risk)

All 5 `spawn_with_timeout` MCP call sites pass `MCP_HANDLER_TIMEOUT` by name — no
hard-coded duration literals at any migrated site.

### Background Error Handling (R-04, Integration Risk)

Both background `spawn` sites emit `tracing::error!` on `RayonError::Cancelled`:
- Contradiction scan (line 577): `error!("contradiction scan rayon task cancelled; cache retained")`
- Quality-gate (line 1216): `error!("quality-gate embedding rayon task cancelled; skipping store step")`

### embed_handle.rs Guard (AC-08, R-10)

```
grep -n "spawn_blocking" embed_handle.rs → line 76: OnnxProvider::new (1 result)
grep -n "rayon_pool|RayonPool" embed_handle.rs → 0 results
```

`OnnxProvider::new` correctly remains on `tokio::task::spawn_blocking`.

### AsyncEmbedService Removal (AC-05, R-05)

```
grep -r "AsyncEmbedService" crates/ | wc -l → 0
```

`AsyncVectorStore` retained: 3 references in `unimatrix-core/src/async_wrappers.rs`.
Workspace build (`cargo build --release`): exit 0.

### Crate Boundary (AC-01, ADR-001)

- `unimatrix-server/Cargo.toml` has `rayon = "1"` (semver-pinned to 1.x — R-11 covered)
- `rayon` is NOT a direct dependency in `unimatrix-core`, `unimatrix-embed`, `unimatrix-vector`, or `unimatrix-store`
- Transitive rayon entries in `cargo tree` for those crates come from other dep chains, not from crt-022 additions

### Single Instantiation (R-09, ADR-004)

`RayonPool::new` appears in `main.rs` at lines 489 (`tokio_main_daemon`) and 813
(`tokio_main_stdio`). These two functions are mutually exclusive server startup modes —
only one runs per process invocation. This is structurally correct and not a violation
of ADR-004 (single pool per process). A `// TODO(W2-4): add gguf_rayon_pool` comment
is present at both sites per the IMPLEMENTATION-BRIEF specification.

### Module Rustdoc Convention (R-04)

`rayon_pool.rs` module-level `//!` doc comment documents:
- MCP handler paths MUST use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, f)`
- Background tasks MUST use `spawn(f)` with no timeout
- Panic containment semantics
- Mutex poisoning recovery boundary

### CI Enforcement (AC-07)

`.github/workflows/ci.yml` contains job `enforce-inference-sites` triggered on
`pull_request` (line 4), running `bash scripts/check-inference-sites.sh`.

---

## Gaps

### R-07: Integration Startup Abort Test (Partial Coverage)

**Risk**: `rayon_pool_size = 0` or out-of-range config reaches pool construction
without a structured error.

**Covered**: 4 unit tests verify `InferenceConfig::validate()` returns structured
`ConfigError` at all boundary values (0, 1, 64, 65).

**Gap**: No integration test verifies the full startup abort path (binary receives
bad config → exits with non-zero status → structured error in stderr). This was
planned as `test_server_rejects_invalid_rayon_pool_size` in `test_tools.py`.

**Reason**: The harness infrastructure does not support placing a config file at
the server's data directory without replicating SHA-256 hash computation in Python
and writing to `~/.unimatrix/{hash}/config.toml`. No CLI flag or env var exposes
the base directory override that exists in Rust tests.

**Mitigation**: Unit tests cover the validation logic. The startup wiring in
`main.rs` is straightforward: `config.inference.validate(path)?` then `RayonPool::new`.
The unit test for `validate()` proves the error is generated; the startup path has
no additional logic that could swallow it.

**Filed**: GH Issue #319 — "[infra-001] test_server_rejects_invalid_rayon_pool_size:
integration coverage gap for R-07 startup abort" requesting a `--base-dir` CLI flag
or `UNIMATRIX_BASE_DIR` env var to enable this test class.

### Pre-existing Doctest Failure

`config.rs` line 21 doctest fails due to `~` in path string. Pre-existing. Not caused
by crt-022. No GH Issue filed — issue predates this feature.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `rayon = "1"` in `unimatrix-server/Cargo.toml`; no direct rayon dep in other crates |
| AC-02 | PASS | `rayon_pool.rs` exists; `RayonPool` and `RayonError` exported; `test_spawn_returns_closure_value` passes |
| AC-03 | PASS | `test_spawn_panic_returns_cancelled` passes; test runtime does not abort |
| AC-04 | PASS | Smoke suite: 20/20 passed; search request triggering embedding completes via rayon bridge |
| AC-05 | PASS | `AsyncEmbedService` count = 0; `AsyncVectorStore` retained; `cargo build --release` exits 0 |
| AC-06 | PASS | All 7 sites verified: 5 use `spawn_with_timeout`, 2 background use `spawn`; grep audit clean |
| AC-07 | PASS | `check-inference-sites.sh` exits 0; CI workflow job present, triggered on `pull_request` |
| AC-08 | PASS | `embed_handle.rs` line 76: 1 `spawn_blocking` (OnnxProvider::new); 0 rayon calls |
| AC-09 | PARTIAL | Unit tests cover all 4 boundary values; integration startup abort test deferred (GH #319) |
| AC-10 | PASS | tools suite: 72/73 passed, 1 xfail (pre-existing); lifecycle suite: 24/25 passed, 1 xfail (pre-existing); all embedding-dependent tools function correctly through rayon bridge |
| AC-11 | PASS | 21 unit tests in `rayon_pool.rs` (exceeds minimum of 4); 12 InferenceConfig tests in `config.rs` (exceeds minimum of 4); all 8 required minimum tests present and passing |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "testing procedures gate verification integration test triage rayon tokio" (category: procedure) — found entry #840 (integration harness how-to), #487 (workspace test procedure), #2326 (async pattern verification). None were novel relative to what USAGE-PROTOCOL.md and the test plans already specified.
- Stored: nothing novel to store — the harness infrastructure gap pattern (no base-dir CLI flag → config placement requires hash replication) is a product gap filed as GH #319, not a novel testing technique worth storing as a pattern.
