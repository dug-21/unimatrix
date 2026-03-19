# Gate 3a Report: crt-022

> Gate: 3a (Component Design Review)
> Date: 2026-03-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All component boundaries, interfaces, and ADR decisions correctly reflected in pseudocode |
| Specification coverage | WARN | C-11 text conflict: spec says warmup uses `spawn` (no timeout); ARCHITECTURE.md and pseudocode correctly use `spawn_with_timeout`. ARCHITECTURE.md is authoritative. |
| Risk coverage | WARN | RISK-TEST-STRATEGY.md R-07 scenario 5 carries stale formula `max(2)` instead of `max(4)`. Pseudocode and test plans both use the correct ADR-003 formula `max(4)`. No test gap. |
| Interface consistency | PASS | All shared types, signatures, and data flows are consistent across OVERVIEW.md and component pseudocode files |
| Knowledge stewardship | PASS | All four design-phase agents have compliant stewardship blocks with `Queried:` and `Stored:` or "nothing novel" with reason |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

- `rayon_pool.md` — `RayonPool` struct field `ml_inference_pool: Arc<rayon::ThreadPool>` matches ARCHITECTURE.md §Component Breakdown and Integration Surface table exactly. Both `spawn` and `spawn_with_timeout` methods documented with correct signatures.
- `RayonError` — `Cancelled` and `TimedOut(Duration)` variants match ARCHITECTURE.md §timeout-semantics and ADR-002.
- `inference_config.md` — Default formula `(num_cpus::get() / 2).max(4).min(8)` matches ARCHITECTURE.md §pool-sizing and ADR-003 (floor 4, not 2). Field validates `[1, 64]`.
- `async_embed_removal.md` — Correctly identifies `AsyncEmbedService` as the only removal target; `AsyncVectorStore` retained; `async` feature retained. Matches ADR-001.
- `call_site_migration.md` — Pool distribution pattern (single `Arc<RayonPool>` on `ServiceLayer`, cloned to `SearchService`, `StoreService`, `StatusService`) matches ADR-004 and ARCHITECTURE.md §Pool Distribution. `// TODO(W2-4)` annotation documented.
- `ci_enforcement.md` — Grep step logic covers services/, background.rs, async_wrappers.rs, and embed_handle.rs exactly as required by C-09.
- Crate boundary: pseudocode places `rayon_pool.rs` exclusively in `unimatrix-server/src/infra/`, consistent with ADR-001.

**Pool floor formula**: The spawn prompt flags WARN-2 (ALIGNMENT-REPORT: spec FR-06/NFR-04 carry `max(2)`, architecture authoritative at `max(4)`). All pseudocode — `rayon_pool.md`, `inference_config.md` `Default` impl, and OVERVIEW.md — uses the correct formula `(num_cpus::get() / 2).max(4).min(8)`. No discrepancy in the artifacts being validated.

### Specification Coverage

**Status**: WARN

**Evidence** — All FRs accounted for in pseudocode:

| Requirement | Pseudocode Coverage |
|-------------|-------------------|
| FR-01 `RayonPool::spawn` | `rayon_pool.md` §spawn-dispatch |
| FR-02 panic containment via channel drop | `rayon_pool.md` §RayonPool::spawn algorithm note, module-level rustdoc |
| FR-03 single `Arc<RayonPool>` at startup via ServiceLayer | `call_site_migration.md` §main.rs wiring |
| FR-04 startup abort on `ThreadPoolBuildError` | `rayon_pool.md` §Initialization Sequence, `call_site_migration.md` §main.rs wiring |
| FR-05 `[inference]` section with `#[serde(default)]` | `inference_config.md` §InferenceConfig struct |
| FR-06 default formula `(num_cpus/2).max(4).min(8)` | `inference_config.md` §Default impl — CORRECT formula used |
| FR-07 `AsyncEmbedService` removal, `AsyncVectorStore` retained | `async_embed_removal.md` |
| FR-08 7 call sites migrated to `ml_inference_pool.spawn` | `call_site_migration.md` §Call-Site Inventory (all 7 sites) |
| FR-09 `OnnxProvider::new` stays on `spawn_blocking` | `call_site_migration.md` §Sites That Must NOT Be Migrated |
| FR-10 CI grep step enforcement | `ci_enforcement.md` |
| FR-11 `[inference]` naming accommodates future NLI/GGUF | `inference_config.md` §InferenceConfig struct doc comment |

**WARN — C-11 vs. ARCHITECTURE warmup classification**:

SPECIFICATION.md C-11 states: "The three background call sites (contradiction scan, quality-gate loop, warmup) use `ml_inference_pool.spawn(...)` with no timeout."

However, ARCHITECTURE.md §timeout-semantics (Option C resolution) states: "The 7 MCP call sites (search, store, correct, status, warmup, quality-gate, contradiction scan for MCP-path variants) use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`."

`call_site_migration.md` Site 6 (`uds/listener.rs`) correctly uses `spawn_with_timeout`, aligning with ARCHITECTURE.md. The rationale in the pseudocode is sound: warmup runs on a user-visible startup path, and an indefinitely hung warmup would block the listener startup — so a 30s timeout is appropriate.

The test plan (`call_site_migration.md` §per-site-audit) also correctly marks Site 6 as requiring `spawn_with_timeout`.

**No implementation gap exists** — pseudocode follows the more authoritative ARCHITECTURE.md. However, C-11 in the spec is misleading. This is a WARN (documentation inconsistency) that does not affect implementation correctness.

**NFR coverage**:
- NFR-01 (tokio blocking pool freed): addressed in pseudocode bridge design (rx.await suspends tokio task, rayon thread executes)
- NFR-02 (no throughput regression): architectural concern; pseudocode does not add unnecessary overhead
- NFR-03 (structured startup error): `rayon_pool.md` §Error Handling — `ServerStartupError::InferencePoolInit` documented
- NFR-04 (pool cap 8, floor 4): `inference_config.md` `Default` impl correct
- NFR-05 (`ort` pin unchanged): `async_embed_removal.md` notes no new `unimatrix-core` dependencies; pseudocode does not touch `onnx.rs`
- NFR-06 (`// TODO(W3-1)` comment): `call_site_migration.md` §main.rs wiring includes the TODO annotation
- NFR-07 (`cargo check --workspace` passes): `async_embed_removal.md` §Verification Steps

### Risk Coverage

**Status**: WARN

**Evidence** — Risk-to-test mapping is comprehensive. All 11 risks have corresponding test scenarios:

| Risk | Priority | Test Plan Coverage | Assessment |
|------|----------|-------------------|------------|
| R-01 (panic propagates) | High | `rayon_pool.md` §panic-containment: 4 tests including mutex-held-panic and pool-functional-after-panic | PASS |
| R-02 (threads occupied after timeout) | Critical | `rayon_pool.md` §timeout-semantics: pool-accepts-new-submissions-after-timeout, pool-size-accessor-unchanged | PASS |
| R-03 (mutex poisoning) | High | `rayon_pool.md` §panic-containment test_spawn_panic_with_mutex_held | PASS |
| R-04 (wrong method at call site) | Critical | `call_site_migration.md` §method-audit and §module-rustdoc; `ci_enforcement.md` §spawn-blocking-grep | PASS |
| R-05 (AsyncEmbedService breaks workspace) | Med | `async_embed_removal.md` §workspace-build: cargo check + grep | PASS |
| R-06 (missed spawn_blocking site) | High | `ci_enforcement.md` §spawn-blocking-grep: 5 independent grep checks | PASS |
| R-07 (invalid config reaches pool) | Med | `inference_config.md` §validate-unit-tests: 4 boundary values (0, 1, 64, 65) + default + absent section | PASS |
| R-08 (pool exhaustion deadlock) | High | `rayon_pool.md` §concurrency: test_pool_does_not_deadlock_under_full_occupancy (5-task barrier test), test_two_background_and_two_mcp_concurrent | PASS |
| R-09 (second RayonPool instantiated) | Med | `call_site_migration.md` §single-instantiation: grep for RayonPool::new count | PASS |
| R-10 (OnnxProvider::new migrated) | Low | `call_site_migration.md` §embed-handle-guard | PASS |
| R-11 (rayon version drift) | Low | `ci_enforcement.md` §cargo-toml-check | PASS |
| R-Security (adversarial exhaustion) | Med | `rayon_pool.md` §adversarial: test_adversarial_timeout_does_not_hang_pool | PASS |

**WARN — R-07 scenario 5 stale formula**:

RISK-TEST-STRATEGY.md R-07 scenario 5 states: "default applied; `rayon_pool_size` equals `(num_cpus / 2).max(2).min(8)` (AC-09 default, FR-06)."

This is the old SCOPE.md formula. The correct formula per ADR-003, ARCHITECTURE.md, and all pseudocode is `(num_cpus / 2).max(4).min(8)`.

The test plan (`rayon_pool.md` §pool-init — `test_pool_init_default_formula`) correctly asserts the formula against `(num_cpus::get() / 2).max(4).min(8)`. No test gap exists. However, the risk document carries a stale reference. This is a documentation defect (pre-existing from the risk-strategy phase), not a design gap in the artifacts under review.

### Interface Consistency

**Status**: PASS

**Evidence**:

- `OVERVIEW.md` Integration Surface Summary table lists all interfaces; entries match the corresponding component pseudocode:
  - `RayonPool::new(num_threads, name)` — matches `rayon_pool.md`
  - `RayonPool::spawn` / `spawn_with_timeout` / `pool_size` / `name` — match `rayon_pool.md`
  - `RayonError { Cancelled, TimedOut(Duration) }` — match `rayon_pool.md` and ARCHITECTURE.md
  - `InferenceConfig { rayon_pool_size: usize }` — matches `inference_config.md`
  - `UnimatrixConfig::inference` — matches `inference_config.md`
  - `ConfigError::InferencePoolSizeOutOfRange` — matches `inference_config.md`
  - `AsyncEmbedService` REMOVED — matches `async_embed_removal.md`

- Data flow diagram in `OVERVIEW.md` is consistent with the bridge algorithm in `rayon_pool.md` (MCP handler → `spawn_with_timeout` → oneshot → rayon worker; background tick → `spawn` → oneshot → rayon worker).

- Shared types used consistently: `Arc<RayonPool>` referenced as the distribution mechanism in `OVERVIEW.md`, `call_site_migration.md`, and `rayon_pool.md`. No contradictions.

- `ServiceLayer` field name `ml_inference_pool: Arc<RayonPool>` appears consistently in `OVERVIEW.md` §Modified startup wiring and `call_site_migration.md` §How rayon_pool Reaches Each Call Site.

- Error propagation chains are consistent: `RayonError::Cancelled` / `TimedOut` → `ServiceError::EmbeddingFailed(e.to_string())` documented in both `rayon_pool.md` §Error Handling and `call_site_migration.md` §Error Handling Summary.

- The double `.map_err` pattern in `call_site_migration.md` Pattern A matches the ARCHITECTURE.md §Call-Site Migration Pattern exactly.

### AC-11 Eight Required Unit Tests

**Status**: PASS

All 8 required unit tests are present in the test plans:

| AC-11 Test | Test Plan Location | Test Name |
|------------|-------------------|-----------|
| #1 `RayonPool::spawn` successful dispatch | `rayon_pool.md` §spawn-dispatch | `test_spawn_returns_closure_value` |
| #2 `RayonPool::spawn` panic safety | `rayon_pool.md` §panic-containment | `test_spawn_panic_returns_cancelled` |
| #3 pool init with `num_threads = 1` | `rayon_pool.md` §pool-init | `test_pool_init_single_thread` |
| #4 pool init with `num_threads = 8` | `rayon_pool.md` §pool-init | `test_pool_init_eight_threads` |
| #5 `InferenceConfig` valid lower bound | `inference_config.md` §validate-unit-tests | `test_inference_config_valid_lower_bound` |
| #6 `InferenceConfig` valid upper bound | `inference_config.md` §validate-unit-tests | `test_inference_config_valid_upper_bound` |
| #7 `InferenceConfig` rejects 0 | `inference_config.md` §validate-unit-tests | `test_inference_config_rejects_zero` |
| #8 `InferenceConfig` rejects 65 | `inference_config.md` §validate-unit-tests | `test_inference_config_rejects_sixty_five` |

### R-02 Timeout Test Scenario

**Status**: PASS

`rayon_pool.md` §timeout-semantics covers all three R-02 scenarios:
1. `test_spawn_with_timeout_fires_when_closure_exceeds_timeout` — 2-thread pool; 2 closures sleeping beyond timeout → `TimedOut` (R-02 scenario 1)
2. `test_pool_accepts_new_submissions_after_timeout` — pool still functional for new work with occupied threads (R-02 scenario 2)
3. `test_pool_size_accessor_unchanged_after_timeout` — `pool_size()` returns configured count, not idle count (R-02 scenario 3)

### R-08 Deadlock/Queue Test Scenario

**Status**: PASS

`rayon_pool.md` §concurrency covers both R-08 scenarios:
1. `test_pool_does_not_deadlock_under_full_occupancy` — 4-thread pool; 4 closures held at `Barrier`; 5th enqueues and completes after barrier release
2. `test_two_background_and_two_mcp_concurrent` — 2 slow + 2 fast closures concurrently on 4-thread pool; fast closures complete within 2× uncontested time

### Knowledge Stewardship Compliance

**Status**: PASS

All design-phase agents have compliant stewardship blocks:

| Agent | Role | Stewardship Block Present | Queried | Stored/Declined |
|-------|------|--------------------------|---------|-----------------|
| crt-022-agent-1-architect | Active storage (architect) | YES | Entries #68, #76, #316, #1560, #2491, #2524, #2535 | Stored: entries #2536, #2537, #2538, #2539 (ADR-001 through ADR-004) |
| crt-022-agent-3-risk | Active storage (risk-strategist) | YES | Entries #1688, #735, #1700, #1627, #2491, #2535, #2537 | Nothing novel to store — all patterns already captured by prior agents; reason given |
| crt-022-agent-1-pseudocode | Read-only (pseudocode) | YES | Entries #2491, #2535, #2536–#2539 | Nothing novel to store — feature-specific, already stored; reason given |
| crt-022-agent-2-testplan | Read-only (test plan) | YES | Entries #2535, #2491, #2539, #2540, #748 | Nothing novel to store — conventions follow established patterns; reason given |

All stewardship blocks are compliant: active-storage agents have `Stored:` entries with entry IDs; read-only agents have `Queried:` entries and "nothing novel" with explicit reasons.

---

## Warnings Summary

### WARN-1: C-11 warmup site method conflict

SPECIFICATION.md C-11 incorrectly groups the warmup site (`uds/listener.rs`) with the background tasks that use `spawn` (no timeout). ARCHITECTURE.md groups warmup with MCP handler paths using `spawn_with_timeout`. Pseudocode and test plans correctly follow ARCHITECTURE.md.

**Impact**: None on implementation — pseudocode is correct. The spec text is the only artifact with the error.

**Recommendation**: Human or spec-agent may want to update C-11 text before Gate 3b. Not a blocker for implementation.

### WARN-2: R-07 scenario 5 carries stale pool floor formula

RISK-TEST-STRATEGY.md R-07 scenario 5 references `max(2)` (old SCOPE.md formula). All other references in the risk doc, the pseudocode, and the test plan use the correct `max(4)` (ADR-003). No test gap.

**Impact**: None on implementation — test plans are correct. The risk doc text is a known pre-existing defect (ALIGNMENT-REPORT WARN-2, IMPLEMENTATION-BRIEF.md).

**Recommendation**: Not a blocker. The tester in Stage 3c should be aware: when they encounter R-07 scenario 5, the correct expected formula is `max(4).min(8)`, not `max(2).min(8)`.

---

## Rework Required

None. Gate result is PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — gate-3a findings are feature-specific (the C-11/warmup inconsistency and R-07 stale formula are already documented in ALIGNMENT-REPORT and IMPLEMENTATION-BRIEF). The pool-floor formula conflict was pre-documented by the synthesizer; no new generalizable lesson emerged from this validation pass.
