# Gate 3c Report: crt-022

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 11 risks + security risk covered by RISK-COVERAGE-REPORT; tests verified |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; 21 rayon_pool + 12 InferenceConfig unit tests; 116 integration tests including smoke gate |
| Specification compliance | PASS | All AC-01–AC-11 verified; AC-09 PARTIAL (integration startup abort deferred to GH #319) |
| Architecture compliance | PASS | Pool in server-only crate, single instance per process, ServiceLayer distribution, embed_handle.rs unchanged |
| Knowledge stewardship | WARN | RISK-COVERAGE-REPORT inaccurately states lifecycle xfail has "no GH Issue filed" — GH#291 exists; stewardship content otherwise complete |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**:

RISK-COVERAGE-REPORT.md maps all 11 risks (R-01 through R-11) plus R-Security to passing tests. Independently verified:

- **R-01 (panic propagation)**: `test_spawn_panic_returns_cancelled`, `test_spawn_with_timeout_panic_returns_cancelled_not_timeout`, `test_pool_functional_after_panic` — all pass. Runtime does not abort.
- **R-02 (threads silently occupied after timeout)**: `test_spawn_with_timeout_fires_when_closure_exceeds_timeout`, `test_spawn_timeout_duration_preserved`, `test_pool_accepts_new_submissions_after_timeout`, `test_pool_size_accessor_unchanged_after_timeout` — all pass. Confirmed pool continues accepting new work after timeouts.
- **R-03 (mutex poisoning)**: `test_spawn_panic_with_mutex_held` — passes. Bridge correctly returns `Cancelled`; mutex is confirmed poisoned, demonstrating the recovery boundary is `EmbedServiceHandle`, not the bridge.
- **R-04 (MCP call site uses `spawn` instead of `spawn_with_timeout`)**: Static grep confirms all 5 MCP-path sites use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`: search.rs:235, store_ops.rs:120, store_correct.rs:52, status.rs:549, uds/listener.rs:1393. Background paths (background.rs:550, :1169) correctly use `spawn` (no timeout). Module rustdoc documents the convention. CI script enforces it.
- **R-05 (AsyncEmbedService removal breaks consumer)**: `grep -r "AsyncEmbedService" crates/ | wc -l` → 0. `cargo test --workspace --lib` passes all 2583 tests. `AsyncVectorStore` retained in `async_wrappers.rs`.
- **R-06 (missed spawn_blocking site)**: `bash scripts/check-inference-sites.sh` exits 0. Five-check script verifies: no `spawn_blocking` with `embed` in services/, no `spawn_blocking_with_timeout` with `embed` in services/, no `spawn_blocking` with `embed` in background.rs, `AsyncEmbedService` absent, `embed_handle.rs` has exactly 1 `spawn_blocking`. The remaining `spawn_blocking_with_timeout` at search.rs:462 is for `compute_search_boost` (graph-traversal DB operation) and is correctly excluded by the `grep "embed"` filter.
- **R-07 (invalid rayon_pool_size)**: 4 boundary unit tests pass (values 0, 1, 64, 65). Integration startup abort test deferred to GH #319 (confirmed OPEN). Mitigation documented: unit tests cover the validation logic; startup path has no additional swallowing logic.
- **R-08 (pool exhaustion deadlock)**: `test_pool_does_not_deadlock_under_full_occupancy` (barrier-based, 4 threads + queued 5th), `test_two_background_and_two_mcp_concurrent` (4-thread, 2 slow + 2 fast, completes in under 500ms) — both pass.
- **R-09 (second pool instantiation)**: `RayonPool::new` appears at main.rs:489 (`tokio_main_daemon`) and main.rs:813 (`tokio_main_stdio`) — confirmed mutually exclusive startup functions (single pool per process). `ServiceLayer.ml_inference_pool: Arc<RayonPool>` is the single distribution field. `// TODO(W2-4): add gguf_rayon_pool` comment present at both sites.
- **R-10 (OnnxProvider::new migrated to rayon)**: `grep -n "spawn_blocking" embed_handle.rs` returns exactly line 76 (OnnxProvider::new). Zero rayon calls in that file.
- **R-11 (rayon version drift)**: `unimatrix-server/Cargo.toml` specifies `rayon = "1"` (semver-pinned). No rayon direct dependency in other workspace crates.
- **R-Security (adversarial pool exhaustion)**: `test_adversarial_timeout_does_not_hang_pool` passes — 4 adversarial slow closures all timeout promptly; pool accepts subsequent short closure.

---

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**:

Unit tests (cargo test --workspace --lib):
- Total: 2583 passed, 0 failed, 18 ignored — matches RISK-COVERAGE-REPORT
- `infra::rayon_pool::tests`: 21 tests, all pass
- `infra::config::tests` (InferenceConfig group): 12 tests, all pass (confirmed by `cargo test --package unimatrix-server --lib -- "infra::config"`: 81 passed, 0 failed)
- Exceeds AC-11 minimum of 8 new tests

Integration tests (infra-001):
- Smoke gate: PASS (report: 20/20 passed, 0 xfailed) — mandatory gate satisfied
- Tools suite: 72 passed, 0 failed, 1 xfailed (GH#305, pre-existing)
- Lifecycle suite: 24 passed, 0 failed, 1 xfailed (GH#291, pre-existing)
- Feature-relevant suites run: tools (embedding-dependent tool ops), lifecycle (store→search, persistence)
- 116 integration tests total

Pre-existing doctest failure in config.rs line 21 (markdown `~` in path string misinterpreted as Rust syntax): confirmed pre-existing, not caused by crt-022. Does not affect `--lib` test run.

All risk-to-scenario mappings from RISK-TEST-STRATEGY.md are exercised (R-01 scenarios 1–3, R-02 scenarios 1–3, R-03 scenarios 1–2, R-04 static+CI, R-05 scenarios 1–3, R-06 CI grep, R-07 boundary values, R-08 scenarios 1–2, R-09 structural, R-10 grep, R-11 Cargo.toml).

---

### 3. Specification Compliance

**Status**: PASS

**Evidence**:

All 11 acceptance criteria verified:

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `rayon = "1"` in unimatrix-server/Cargo.toml; no rayon in other crates (grep confirms) |
| AC-02 | PASS | `rayon_pool.rs` exists; `RayonPool` and `RayonError` exported; async `spawn` signature correct |
| AC-03 | PASS | `test_spawn_panic_returns_cancelled` passes; tokio runtime does not abort |
| AC-04 | PASS | Smoke suite 20/20 passed; pool constructed from `[inference] rayon_pool_size`; ServiceLayer holds `ml_inference_pool` |
| AC-05 | PASS | `AsyncEmbedService` count = 0 (grep confirmed); `AsyncVectorStore` retained; `cargo test --workspace --lib` exits 0 |
| AC-06 | PASS | All 7 migration sites verified: 5 `spawn_with_timeout`, 2 `spawn`; zero remaining `spawn_blocking` at inference sites |
| AC-07 | PASS | `check-inference-sites.sh` exits 0; `.github/workflows/ci.yml` job `enforce-inference-sites` present, triggered on `pull_request` |
| AC-08 | PASS | `embed_handle.rs` line 76: exactly 1 `spawn_blocking` (OnnxProvider::new); zero rayon calls |
| AC-09 | PARTIAL | Unit tests cover all 4 boundary values + default + serde; integration startup abort deferred GH #319 (OPEN). Mitigation: validation logic is fully covered; startup path has no swallow path for ConfigError |
| AC-10 | PASS | All embedding-dependent tools function through rayon bridge; 116 integration tests pass |
| AC-11 | PASS | 21 rayon_pool tests + 12 InferenceConfig tests = 33 new tests (≥8 minimum); all 8 required minimum tests present |

Functional requirements FR-01 through FR-11: all implemented. Non-functional requirements NFR-01 through NFR-07 addressed:
- NFR-01 (tokio blocking pool free during inference): satisfied by rayon bridge; tokio task suspends on `rx.await`
- NFR-02 (throughput no regression): integration tests pass; performance regression detectable via W1-3 eval harness
- NFR-03 (structured startup error): `ServerError::InferencePoolInit(e.to_string())` used at both startup paths
- NFR-04 (pool floor 4): `InferenceConfig::default()` formula `(num_cpus/2).max(4).min(8)` confirmed
- NFR-05 (ort version unchanged): not touched; ONNX session calls same as before, only execution context changed
- NFR-06 (TODO comment): `// TODO(W3-1): add unimatrix-onnx crate extraction here` at main.rs:21
- NFR-07 (cargo check): `cargo test --workspace --lib` exits 0; zero compilation errors

---

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- **ADR-001 (rayon in server-only)**: `rayon = "1"` absent from unimatrix-core, unimatrix-embed, unimatrix-vector, unimatrix-store — confirmed by grep.
- **ADR-002 (spawn_with_timeout for MCP)**: 5 MCP-path sites use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`; 2 background sites use `spawn`. Module rustdoc documents the convention. No hard-coded duration literals at any site.
- **ADR-003 (contradiction scan single task, pool floor 4)**: background.rs:550 dispatches the entire `scan_contradictions` closure as a single `spawn`. Pool default formula yields floor 4.
- **ADR-004 (AppState owns single pool)**: `ServiceLayer.ml_inference_pool: Arc<RayonPool>` is the single field; `RayonPool::new` called exactly once per startup path (two mutually exclusive startup functions).
- **Component boundaries**: `rayon_pool.rs` and `config.rs` in `infra/` as designed. `AsyncEmbedService` removed from `unimatrix-core`. `EmbedServiceHandle` unchanged. `ServiceLayer` wiring confirmed.
- **Error handling**: `RayonError::Cancelled` and `RayonError::TimedOut` both map to `ServiceError::EmbeddingFailed` at all call sites. Background paths emit `tracing::error!` on `Cancelled` (lines 580 and 1218 in background.rs).

No architectural drift detected.

---

### 5. Knowledge Stewardship Compliance

**Status**: WARN

**Evidence**:

RISK-COVERAGE-REPORT.md (the tester's deliverable) contains a `## Knowledge Stewardship` section with:
- `Queried:` entry: `/uni-knowledge-search` for testing procedures, gate verification, rayon tokio
- `Stored:` entry: "nothing novel to store — the harness infrastructure gap pattern is a product gap filed as GH #319, not a novel testing technique"

The stewardship block is present and has reasoning after "nothing novel to store." Content is compliant.

**WARN (minor)**: The RISK-COVERAGE-REPORT describes the lifecycle xfail as "no GH Issue filed (already documented in test)." However the actual test file at line 566 references GH#291, which is confirmed OPEN. The issue exists; the report description is inaccurate but the underlying compliance (xfail has a GH issue) is satisfied. No remediation required.

---

## xfail Marker Compliance

Both xfailed integration tests have corresponding GH issues:

| Test | xfail Reason | GH Issue | Status |
|------|--------------|----------|--------|
| `test_lifecycle.py::test_auto_quarantine_after_consecutive_bad_ticks` | Tick interval not overridable at integration level | GH#291 | OPEN |
| `test_tools.py` (1 test) | baseline_comparison null when synthetic features lack delivery counter | GH#305 | pre-existing |

Neither failure is caused by crt-022. No integration tests were deleted or commented out. RISK-COVERAGE-REPORT includes integration test counts (smoke 20, tools 72+1xfail, lifecycle 24+1xfail, total 116).

---

## R-07 Gap Assessment

The integration startup abort test for invalid `rayon_pool_size` is deferred (GH #319). This is classified as PARTIAL coverage on AC-09, not a FAIL, for the following reasons:

1. The validation path is fully unit-tested at all boundary values (0, 1, 64, 65, default)
2. The startup wiring at main.rs:481-490 is straightforward: `config.inference.validate(path)?` then `RayonPool::new`. There is no additional logic that could swallow a `ConfigError` between validation and startup abort.
3. The gap is an infrastructure limitation (harness cannot place a config file at the SHA-256-hashed path) filed with a clear remediation path (GH #319 requests `--base-dir` CLI flag or `UNIMATRIX_BASE_DIR` env var).
4. Both startup functions (daemon and stdio) implement identical validation-then-construction wiring.

This gap does not block PASS at Gate 3c.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for "validation gate final risk coverage rayon thread pool pattern" — no novel patterns found beyond what is already documented in crt-022 feature artifacts.
- Stored: nothing novel to store — gate-3c validation of an infrastructure feature with clear pass/warn split; no systemic pattern distinguishing this from prior gates. The R-07 integration gap pattern (harness cannot place config at hash-addressed path) is already filed as GH #319 with a concrete remediation path. No novel lesson learned for the pattern store.
