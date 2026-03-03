# Architecture & Dependency Audit Report

**Date**: 2026-03-03
**Scope**: All 8 crates in the Unimatrix Rust workspace + 1 vendored patch
**Codebase**: 45,424 lines of Rust across 91 source files, 1,628 tests

---

## Executive Summary

The Unimatrix workspace is architecturally sound with clean layering, consistent conventions, and zero `unsafe` code (all 8 crates use `#![forbid(unsafe_code)]`). The crate dependency graph follows a clear bottom-up structure from storage primitives to the MCP server.

Key findings:
1. **unimatrix-server is a monolith** (19,672 lines, 43% of codebase) with 23 public modules and direct dependencies on all 7 other crates plus 15 external crates.
2. **Dependency duplication**: `sha2`, `dirs`, `nix`, `serde_json`, and `tracing` are declared as direct dependencies in multiple crates where they could be funneled through intermediate crates or workspace-level declarations.
3. **unimatrix-server does not use workspace metadata** for edition, rust-version, or license -- inconsistent with all other crates.
4. **No integration test files exist** (`crates/*/tests/` directories are empty or absent). All 1,628 tests are in-module `#[cfg(test)]` blocks.
5. **Significant hardcoded constant surface**: ~90+ tuning constants spread across detection rules, confidence scoring, and UDS listener budgets with no runtime configuration mechanism.
6. **Vendored `anndists` patch** is necessary due to upstream edition 2024 incompatibility, but carries transitive dependencies (rayon, num_cpus, cpu-time, lazy_static, env_logger, log, anyhow) that are unused by Unimatrix.

---

## 1. Dependency Inventory

### Workspace-Level Dependencies

| Dependency | Version | Purpose |
|---|---|---|
| `redb` | 3.1 | Embedded key-value storage engine |
| `serde` | 1 (with `derive`) | Serialization framework |
| `serde_json` | 1 | JSON serialization |
| `bincode` | 2 (with `serde`) | Binary serialization for storage |

### unimatrix-store

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `redb` | workspace | direct | Database backend |
| `serde` | workspace | direct | Entry record serialization |
| `bincode` | workspace | direct | Binary encoding for redb values |
| `sha2` | 0.10 | direct | Content hash computation |
| `tempfile` | 3 | optional/dev | Test database directories |

### unimatrix-vector

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `unimatrix-store` | path | direct | Store for VECTOR_MAP persistence |
| `hnsw_rs` | 0.3 (`simdeez_f`) | direct | HNSW approximate nearest neighbor index |
| `anndists` | 0.1 (patched) | direct | Distance functions for HNSW |
| `tempfile` | 3 | optional/dev | Test directories |
| `rand` | 0.9 | optional/dev | Random embeddings for tests |

### unimatrix-embed

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `ort` | =2.0.0-rc.9 | direct | ONNX Runtime bindings (pinned RC) |
| `ort-sys` | =2.0.0-rc.9 | direct | ONNX Runtime sys bindings (pinned RC) |
| `tokenizers` | 0.21 (`onig`) | direct | HuggingFace tokenizer |
| `hf-hub` | 0.4 | direct | Model download from HuggingFace Hub |
| `dirs` | 6 | direct | Platform cache directory detection |
| `thiserror` | 2 | direct | Error derive macros |
| `approx` | 0.5 | dev | Floating-point comparison in tests |

### unimatrix-core

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `unimatrix-store` | path | direct | Store types + adapter |
| `unimatrix-vector` | path | direct | Vector index types + adapter |
| `unimatrix-embed` | path | direct | Embedding types + adapter |
| `tokio` | 1 (`rt`) | optional | Async wrappers (feature: `async`) |
| `tempfile` | 3 | dev | Tests |

### unimatrix-engine

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `unimatrix-core` | path | direct | Core traits and types |
| `unimatrix-store` | path | direct | Direct store access for confidence/coaccess |
| `serde` | workspace | direct | Wire protocol serialization |
| `serde_json` | 1 | direct | Wire protocol JSON encoding |
| `sha2` | 0.10 | direct | Project hash computation |
| `dirs` | 6 | direct | Home directory for data paths |
| `tracing` | 0.1 | direct | Structured logging |
| `nix` | 0.31 (`socket`, `process`, `user`) | direct | Unix peer credentials, process info |
| `tempfile` | 3 | dev | Tests |

### unimatrix-adapt

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `ndarray` | 0.16 | direct | N-dimensional array operations for LoRA |
| `rand` | 0.9 | direct | Random initialization, reservoir sampling |
| `rand_distr` | 0.5 | direct | Normal distribution for weight init |
| `serde` | workspace | direct | State serialization |
| `bincode` | workspace | direct | Binary state persistence |
| `tracing` | 0.1 | direct | Logging warnings |
| `tempfile` | 3 | dev | Tests |

### unimatrix-observe

| Dependency | Version | Type | Purpose |
|---|---|---|---|
| `serde` | workspace | direct | Type serialization |
| `bincode` | workspace | direct | Metric vector serialization |
| `serde_json` | workspace | direct | JSONL session file parsing |
| `tempfile` | 3 | dev | Tests |

### unimatrix-server

| Dependency | Version | Type | Purpose | Notes |
|---|---|---|---|---|
| `unimatrix-core` | path (`async`) | direct | Core traits + async wrappers | |
| `unimatrix-store` | path | direct | Direct store access | Bypasses core abstractions |
| `unimatrix-engine` | path | direct | Confidence, coaccess, project, wire | |
| `unimatrix-vector` | path | direct | Direct vector index access | Bypasses core abstractions |
| `unimatrix-embed` | path | direct | Direct embed config access | |
| `unimatrix-adapt` | path | direct | Adaptation service | |
| `unimatrix-observe` | path | direct | Retrospective pipeline | |
| `nix` | 0.31 (`user`) | direct | UID retrieval (single call) | Only in main.rs |
| `fs2` | 0.4 | direct | File locking for PID guard | |
| `redb` | workspace | direct | Direct table access | Bypasses store API |
| `rmcp` | =0.16.0 | direct | MCP protocol server framework | Exact pin |
| `tokio` | 1 (`full`) | direct | Async runtime | |
| `schemars` | 1 | direct | JSON Schema generation for tool params | |
| `serde` | 1 (`derive`) | direct | Tool parameter deserialization | Not using workspace |
| `serde_json` | 1 | direct | JSON for MCP responses/tests | Not using workspace |
| `regex` | 1 | direct | Contradiction detection + scanning | |
| `sha2` | 0.10 | direct | Content hash (audit, dedup) | Duplicates store's usage |
| `dirs` | 6 | direct | Not used (hook uses engine::project) | **Potentially unused** |
| `tracing` | 0.1 | direct | Structured logging | |
| `tracing-subscriber` | 0.3 (`env-filter`) | direct | Log output configuration | |
| `clap` | 4 (`derive`) | direct | CLI argument parsing | |
| `bincode` | 2 (`serde`) | direct | Audit + registry serialization | Not using workspace |
| `tempfile` | 3 | dev | Tests | |

### Vendored Patch: anndists

| Dependency | Version | Purpose | Notes |
|---|---|---|---|
| `cfg-if` | 1.0 | Conditional compilation | |
| `rayon` | 1.11 | Parallelism | Likely unused by Unimatrix |
| `num_cpus` | 1.17 | CPU count for rayon | Likely unused by Unimatrix |
| `cpu-time` | 1.0 | CPU timing | Likely unused by Unimatrix |
| `num-traits` | 0.2 | Numeric traits | |
| `lazy_static` | 1.4 | Static initialization | |
| `log` | 0.4 | Logging facade | |
| `env_logger` | 0.11 | Log output | |
| `anyhow` | 1.0 | Error handling | |
| `simdeez` | 2.0 | SIMD abstraction | Optional, enabled via `simdeez_f` |

---

## 2. Crate Dependency Graph

```
                    unimatrix-server (binary + lib)
                   /    |    |    |    \    \    \
                  /     |    |    |     \    \    \
    unimatrix-engine    |    |    |      \    \   unimatrix-observe
         |    |         |    |    |       \    \
         |    |    unimatrix-adapt  unimatrix-embed
         |    |                            |
         | unimatrix-store            [ort, tokenizers, hf-hub]
         |       |
    unimatrix-core
     /      |      \
    /       |       \
unimatrix-store  unimatrix-vector  unimatrix-embed
       |              |
     [redb]    [hnsw_rs, anndists(patched)]
```

**Server bypasses core**: unimatrix-server depends on unimatrix-store, unimatrix-vector, and unimatrix-embed directly (not just through unimatrix-core). This is intentional for performance-critical paths like direct `ReadableTable` access and vector index Arc sharing, but it weakens the abstraction boundary.

**Engine depends on both core and store**: unimatrix-engine needs `EntryRecord` from core but also directly accesses store for co-access operations. This creates a diamond dependency (server -> engine -> core -> store AND server -> store).

---

## 3. Architecture Layering Analysis

### Layer 1: Foundation (unimatrix-store, unimatrix-vector, unimatrix-embed)

Well-isolated crates with clear single responsibilities:
- **store**: 7,452 lines, 234 tests. Owns redb schema, CRUD, indexing, migration. Exports `Store`, `EntryRecord`, all table definitions. Public API is large (32+ re-exports in lib.rs) due to exposing table definitions and serialization functions.
- **vector**: 2,459 lines, 104 tests. HNSW index with store-backed persistence. Clean API: `VectorIndex`, `VectorConfig`, `SearchResult`.
- **embed**: 1,801 lines, 94 tests. ONNX model loading and text embedding. Only crate using `thiserror` (all others hand-implement `Display`/`Error`).

### Layer 2: Abstraction (unimatrix-core)

Thin adapter layer (823 lines, 21 tests). Provides trait-based abstractions (`EntryStore`, `VectorStore`, `EmbedService`) and async wrappers. Also re-exports all domain types from the three foundation crates, acting as a facade.

**Observation**: unimatrix-core re-exports `Store`, `VectorIndex`, and `OnnxProvider` (concrete types), not just the traits. This makes it a convenience re-export layer rather than a true abstraction boundary. The server uses both the core traits (via async wrappers) and the concrete types directly.

### Layer 3: Business Logic (unimatrix-engine, unimatrix-adapt, unimatrix-observe)

- **engine**: 3,354 lines, 170 tests. Extracted from server: confidence scoring, co-access boosting, project detection, wire protocol, transport, auth. Clean separation.
- **adapt**: 2,827 lines, 64 tests. Self-contained MicroLoRA adaptation pipeline. No dependencies on store/server (clean isolation).
- **observe**: 7,036 lines, 264 tests. Standalone observation pipeline. No dependency on store/server (ADR-001). Only needs serde/bincode/serde_json.

### Layer 4: Application (unimatrix-server)

Monolith at 19,672 lines (43% of codebase) with 677 tests (42% of all tests). 23 public modules. This is the primary area of concern:

- **All modules are `pub`**: The entire server internals are public "for integration testing" per the lib.rs doc comment, but there are no integration tests. This exposes implementation details unnecessarily.
- **Direct dependency on all 7 other crates**: The server depends on every crate in the workspace plus 15 external crates. It directly accesses redb tables, bypassing the store/core abstraction for performance.
- **Dual server architecture**: MCP (stdio) + UDS (unix domain socket) listeners handle different request types, sharing state through 9+ `Arc<...>` handles passed individually.

### Error Handling Consistency

| Crate | Error Strategy |
|---|---|
| unimatrix-store | Manual `Display`/`Error` impl, `From` impls |
| unimatrix-vector | Manual `Display`/`Error` impl, `From` impls |
| unimatrix-core | Manual `Display`/`Error` impl, `From` impls |
| unimatrix-embed | `thiserror` derive |
| unimatrix-engine | No crate-level error type (uses `io::Error`, ad-hoc) |
| unimatrix-observe | Custom `ObserveError` (manual) |
| unimatrix-adapt | No crate-level error type (uses String, io::Error) |
| unimatrix-server | Manual `Display`/`Error` impl + `Into<ErrorData>` |

**Inconsistency**: `unimatrix-embed` is the only crate using `thiserror`. The other crates have ~100+ lines of boilerplate `Display`/`From` impls that thiserror would eliminate. Meanwhile, `unimatrix-engine` and `unimatrix-adapt` lack proper crate-level error types.

---

## 4. Testing Coverage Assessment

### Test Distribution

| Crate | Lines | Tests | Tests/kLOC | Style |
|---|---|---|---|---|
| unimatrix-store | 7,452 | 234 | 31.4 | Unit (in-module) |
| unimatrix-vector | 2,459 | 104 | 42.3 | Unit (in-module) |
| unimatrix-embed | 1,801 | 94 | 52.2 | Unit (in-module) |
| unimatrix-core | 823 | 21 | 25.5 | Unit + async |
| unimatrix-engine | 3,354 | 170 | 50.7 | Unit (in-module) |
| unimatrix-adapt | 2,827 | 64 | 22.6 | Unit (in-module) |
| unimatrix-observe | 7,036 | 264 | 37.5 | Unit (in-module) |
| unimatrix-server | 19,672 | 677 | 34.4 | Unit + tokio::test |
| **Total** | **45,424** | **1,628** | **35.8** | |

### Test Infrastructure

- **test-support feature flags**: Well-designed cascading system. `unimatrix-store/test-support` exposes `TestDb`; `unimatrix-vector/test-support` enables store's test-support transitively plus adds `TestVectorIndex`.
- **Test helpers**: `test_helpers.rs` modules in store, vector, and embed with builders (`TestEntry`, `TestDb`, `TestVectorIndex`), seed functions, and assertion helpers. Follows the "cumulative test infrastructure" principle.
- **No integration test directory**: Zero files under `crates/*/tests/`. All tests are `#[cfg(test)] mod tests` inside source files. This means there are no tests that exercise the crate's public API from the outside.
- **Async tests**: 48 `#[tokio::test]` tests, all in unimatrix-server and unimatrix-core.
- **No property-based testing**: No proptest/quickcheck usage despite mathematical functions (confidence scoring, normalization, distance computation).

### Test Isolation Concerns

- Server tests mock the embedding service via `EmbedServiceHandle` in test mode but use real redb databases (tempfile-backed). This means server tests are I/O-bound.
- The `#[cfg(test)]` blocks inside `embed_handle.rs` and `scanning.rs` provide test-only alternative implementations, which is good for isolation but means test behavior diverges from production.

---

## 5. Configuration Review

### Hardcoded Constants (Partial Inventory)

**Confidence Scoring** (unimatrix-engine/src/confidence.rs):
- 7 weight constants (W_BASE=0.18, W_USAGE=0.14, W_FRESH=0.18, W_HELP=0.14, W_CORR=0.14, W_TRUST=0.14, W_COAC=0.08)
- MAX_MEANINGFUL_ACCESS=50.0, FRESHNESS_HALF_LIFE_HOURS=168.0
- MINIMUM_SAMPLE_SIZE=5, WILSON_Z=1.96
- SEARCH_SIMILARITY_WEIGHT=0.85, PROVENANCE_BOOST=0.02

**Co-Access** (unimatrix-engine/src/coaccess.rs):
- MAX_CO_ACCESS_ENTRIES=10, CO_ACCESS_STALENESS_SECONDS=30 days
- MAX_CO_ACCESS_BOOST=0.03, MAX_BRIEFING_CO_ACCESS_BOOST=0.01

**UDS Listener Budgets** (unimatrix-server/src/uds_listener.rs):
- SIMILARITY_FLOOR=0.5, CONFIDENCE_FLOOR=0.3
- INJECTION_K=5, EF_SEARCH=32
- MAX_COMPACTION_BYTES=8000
- DECISION_BUDGET_BYTES=1600, INJECTION_BUDGET_BYTES=2400
- CONVENTION_BUDGET_BYTES=1600, CONTEXT_BUDGET_BYTES=800

**Validation Limits** (unimatrix-server/src/validation.rs):
- 20 constants for field length limits, defaults, etc.

**Detection Thresholds** (unimatrix-observe/src/detection/*.rs):
- 15+ threshold constants across 4 detection modules

**Contradiction** (unimatrix-server/src/contradiction.rs):
- 7 hardcoded thresholds for similarity, sensitivity, weights

**Sessions** (unimatrix-store/src/sessions.rs):
- TIMED_OUT_THRESHOLD_SECS=24h, DELETE_THRESHOLD_SECS=30d

**Total**: ~90+ hardcoded numeric constants with no runtime override mechanism. Many of these are tuning parameters that may need adjustment without recompilation.

### Environment Variables

Minimal usage:
- `HOME` via `std::env::var_os` in observe/files.rs
- `CARGO_PKG_VERSION` via `env!()` in server/main.rs
- Tracing filter: hardcoded `"info"` / `"debug"` toggle via `--verbose` flag

No `.env` file exists. No environment-variable-based configuration for any of the 90+ tuning constants.

### CLI Configuration

`clap`-based CLI with only two options:
- `--project-dir`: Override project root
- `--verbose`: Enable debug logging
- `hook` subcommand

No configuration file support. All other parameters are compile-time constants.

---

## 6. Detailed Findings

### F1: Server Workspace Metadata Inconsistency

`unimatrix-server/Cargo.toml` hardcodes `edition = "2024"` and `rust-version = "1.89"` instead of using `edition.workspace = true` and `rust-version.workspace = true`. It also omits `license.workspace = true`. All other 7 crates use workspace inheritance. Additionally, `serde`, `serde_json`, and `bincode` are declared directly instead of using `workspace = true`.

**Impact**: If the workspace edition or license changes, the server would be out of sync.

### F2: Potentially Unused `dirs` in Server

`unimatrix-server/Cargo.toml` declares `dirs = "6"` as a direct dependency, but grep shows zero `use dirs` statements in server source files. The server uses `unimatrix_engine::project` and `unimatrix_server::project` (which is a re-export of engine's project module) for path resolution. The `dirs` crate is used in the hook module but through `unimatrix_engine` -- wait, actually `crates/unimatrix-server/src/hook.rs:47` calls `dirs::home_dir()` directly. So it is used, but only in one location, and it duplicates the same call pattern already in unimatrix-engine.

### F3: Duplicate `sha2` Dependency

`sha2 = "0.10"` appears in three crates:
- `unimatrix-store`: `compute_content_hash()` for entry dedup
- `unimatrix-engine`: `compute_project_hash()` for project identification
- `unimatrix-server`: Used in audit and dedup operations

The store and engine usages are legitimate (different domains). The server's usage could potentially be routed through the existing store/engine functions.

### F4: `nix` Declared in Server for Single Call

`unimatrix-server` declares `nix = { version = "0.31", features = ["user"] }` but only uses it in one line: `nix::unistd::getuid().as_raw()` in `main.rs`. This could be replaced with a `libc::getuid()` call (though that would require `unsafe`), or the UID could be obtained through `unimatrix-engine` which already depends on `nix` with more features (`socket`, `process`, `user`).

### F5: Exact Version Pins on Pre-Release Dependencies

- `ort = "=2.0.0-rc.9"` and `ort-sys = "=2.0.0-rc.9"`: Pinned to release candidates. This is appropriate for pre-release crates but creates upgrade friction.
- `rmcp = "=0.16.0"`: Exact pin on MCP protocol library. Reasonable for protocol stability but should be reviewed periodically.

### F6: Vendored `anndists` Patch Carries Heavy Transitive Dependencies

The patched `anndists` brings in `rayon`, `num_cpus`, `cpu-time`, `lazy_static`, `env_logger`, `log`, and `anyhow`. Most of these are likely unused by Unimatrix's usage of anndists (only distance computations for HNSW). The patch exists because the upstream crate declared `edition = "2024"` but has incompatible code.

### F7: `thiserror` Inconsistency

Only `unimatrix-embed` uses `thiserror` for error derives. The remaining 7 crates manually implement `Display`, `Error`, and `From` for their error types, resulting in significant boilerplate (unimatrix-store/src/error.rs is 117 lines, unimatrix-server/src/error.rs is 567 lines). Standardizing on `thiserror` across all crates would reduce boilerplate by an estimated 400+ lines.

### F8: `#[cfg]` Usage is Minimal and Correct

Conditional compilation is used for:
- `#[cfg(test)]` / `#[cfg(any(test, feature = "test-support"))]`: Standard test gating
- `#[cfg(feature = "async")]` in unimatrix-core: Guards tokio-dependent code
- `#[cfg(unix)]`, `#[cfg(target_os = "linux")]`, `#[cfg(target_os = "macos")]` in unimatrix-engine/auth.rs: Platform-specific peer credential extraction

No Windows support exists (UDS, pidfile, auth all assume Unix).

### F9: No Unsafe Code

All 8 crates enforce `#![forbid(unsafe_code)]`. The vendored `anndists` patch does not use `unsafe` in its modified files. The `hnsw_rs` and `ort` dependencies contain unsafe code internally but this is expected for SIMD and FFI operations respectively.

---

## 7. Prioritized Recommendations

### Priority 1 (Low effort, immediate value)

**R1**: Fix server workspace metadata. Change `unimatrix-server/Cargo.toml` to use `edition.workspace = true`, `rust-version.workspace = true`, `license.workspace = true`, and `{ workspace = true }` for serde, serde_json, bincode.

**R2**: Route server's `dirs` usage through unimatrix-engine. The `hook.rs` call to `dirs::home_dir()` duplicates logic already in `unimatrix_engine::project`. If this is consolidated, the `dirs` direct dependency can be removed from the server.

**R3**: Route server's `nix` usage through unimatrix-engine. The single `getuid()` call in main.rs can use engine's auth module which already depends on nix. This removes a direct dependency from the server.

### Priority 2 (Medium effort, consistency)

**R4**: Standardize error handling on `thiserror` across all crates. Currently only unimatrix-embed uses it. Adopting it in store, vector, core, engine, observe, and server would eliminate ~500+ lines of boilerplate `Display`/`Error`/`From` implementations.

**R5**: Add proper crate-level error types to `unimatrix-engine` and `unimatrix-adapt`. Both crates currently return ad-hoc `io::Error` and `String` errors. A typed error enum would improve error handling at call sites.

**R6**: Add workspace-level dependency declarations for commonly used crates: `sha2`, `dirs`, `tracing`, `nix`, `tempfile`, `tokio`, `rand`. This ensures version consistency and makes upgrades easier.

### Priority 3 (Higher effort, architecture)

**R7**: Introduce a configuration file or environment variable system for tuning parameters. The 90+ hardcoded constants (confidence weights, detection thresholds, budget limits, staleness timeouts) should be configurable without recompilation. A `Config` struct loaded from a TOML/JSON file or env vars would be appropriate.

**R8**: Add integration tests. The `crates/*/tests/` directories contain no files. Integration tests that exercise public APIs from outside the crate boundary would catch re-export issues, API ergonomics problems, and ensure the abstraction layers work as intended.

**R9**: Consider splitting unimatrix-server. At 19,672 lines with 23 modules, the server crate handles: MCP tool dispatch, UDS listener, contradiction detection, coherence analysis, session management, hook dispatch, PID management, scanning, validation, categories, response formatting, and shutdown. Candidates for extraction: `contradiction` + `scanning` into a content-analysis crate, `session` + `hook` + `uds_listener` into a hook/IPC crate.

### Priority 4 (Long-term, dependency hygiene)

**R10**: Monitor `anndists` upstream for edition 2024 fix. When upstream publishes a compatible version, remove the vendored patch and its transitive dependency baggage (rayon, num_cpus, cpu-time, lazy_static, env_logger, log, anyhow).

**R11**: Monitor `ort` for stable 2.0 release. The current `=2.0.0-rc.9` exact pin should be upgraded to a stable release when available, allowing semver-compatible updates.

**R12**: Evaluate whether unimatrix-server needs direct `redb` access. Currently the server uses `redb::ReadableTable` directly in audit.rs, registry.rs, server.rs, and tools.rs. If these patterns were absorbed into unimatrix-store's API, the server could drop its direct redb dependency, strengthening the abstraction layer.

---

## Appendix A: Version Pinning Strategy

| Strategy | Dependencies |
|---|---|
| Exact pin (`=x.y.z`) | `ort`, `ort-sys`, `rmcp` |
| Minor range (`x.y`) | `redb` (3.1), `sha2` (0.10), `tracing` (0.1), `tracing-subscriber` (0.3), `fs2` (0.4), `hf-hub` (0.4) |
| Major range (`x`) | `serde` (1), `serde_json` (1), `bincode` (2), `tokio` (1), `clap` (4), `regex` (1), `schemars` (1), `thiserror` (2), `dirs` (6), `tempfile` (3), `rand` (0.9), `ndarray` (0.16) |

The project uses a reasonable mix: exact pins for volatile pre-release dependencies, minor pins for stable critical dependencies, and caret-compatible ranges for well-established crates.

## Appendix B: Unsafe Code Audit

**Result: Zero unsafe code in Unimatrix source.**

All 8 crates use `#![forbid(unsafe_code)]` at the crate root. Unsafe code exists only in third-party dependencies:
- `ort` / `ort-sys`: ONNX Runtime C++ FFI bindings
- `hnsw_rs` / `anndists`: SIMD-optimized distance computations
- `redb`: Memory-mapped file I/O
- `nix`: Unix system call wrappers
- `tokenizers`: Oniguruma regex engine FFI
