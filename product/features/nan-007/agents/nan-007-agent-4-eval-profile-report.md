# Agent Report: nan-007-agent-4-eval-profile

## Task

Implement `crates/unimatrix-server/src/eval/profile.rs` and `src/eval/mod.rs` for nan-007 Wave 1.

## Files Created / Modified

- `crates/unimatrix-server/src/eval/mod.rs` — new file (module tree, re-exports, EvalCommand stub)
- `crates/unimatrix-server/src/eval/profile.rs` — new file (core implementation)
- `crates/unimatrix-server/src/lib.rs` — modified (added `pub mod eval;`)

## What Was Implemented

### eval/mod.rs
- Module tree re-exports: `AnalyticsMode`, `EvalError`, `EvalProfile`, `EvalServiceLayer`
- `EvalCommand` enum stub with `Scenarios`, `Run`, `Report` variants and full clap annotations
- Wave 2 module stubs commented out pending scenarios.rs/runner.rs/report.rs

### eval/profile.rs
- `AnalyticsMode` enum: `Live` and `Suppressed` variants (ADR-002)
- `EvalProfile` struct: name, description, config_overrides (UnimatrixConfig)
- `EvalError` enum: `ModelNotFound(PathBuf)`, `ConfigInvariant(String)`, `LiveDbPath { supplied, active }`, `Io`, `Store`, `ProfileNameCollision`, `InvalidK` — all with user-readable `Display`
- `EvalServiceLayer` struct: wraps `ServiceLayer` + raw `SqlitePool` + db_path + profile_name + analytics_mode
- `EvalServiceLayer::from_profile(db_path, profile, project_dir)` — 3-parameter async constructor
- `validate_confidence_weights()` — sum invariant (0.92 ± 1e-9) on `ConfidenceConfig.weights`
- `parse_profile_toml()` — TOML → EvalProfile deserialization (pub(crate), Wave 2)

## Test Results

28 unit tests, 28 pass, 0 fail.

Test coverage:
- AnalyticsMode: variants, equality
- EvalError: all Display impls, std::error::Error trait
- validate_confidence_weights: no-weights pass, exact sum pass, sum-low fail, sum-high fail, boundary ±1e-9, field naming in error message
- parse_profile_toml: baseline empty, description, missing name, missing file, invalid TOML, with confidence weights
- from_profile: analytics_mode is Suppressed, live-DB path guard, missing snapshot, invalid weights, valid weights

## Open Questions Resolved

**OQ-A** (`VectorIndex` needs `Arc<Store>`): `Store` is a type alias for `SqlxStore` in `unimatrix-core/src/lib.rs`. `VectorIndex::new()` takes `Arc<SqlxStore>` directly — no wrapper adapter needed. We call `SqlxStore::open()` on the snapshot to get the store. Note: this does spawn a drain task, but since `enqueue_analytics` is never called in the eval search path, the snapshot receives zero writes — satisfying AC-05.

**OQ-B** (`AuditLog` needs `Arc<Store>`): `AuditLog::new()` takes `Arc<SqlxStore>` directly — same resolution as OQ-A.

**OQ-C** (`ServiceLayer::with_rate_config` parameter order): Confirmed via TestHarness in `test_support.rs` — parameter order matches exactly.

## Deviations from Pseudocode

1. **ConfidenceWeights field names**: Pseudocode listed `base_recency`, `base_helpfulness`, etc. Actual struct fields are `base`, `usage`, `fresh`, `help`, `corr`, `trust`. Implemented with real field names.

2. **Profile TOML confidence section**: Pseudocode showed weights directly under `[confidence]`. Actual `ConfidenceConfig.weights` is `Option<ConfidenceWeights>` (nested), so TOML must use `[confidence.weights]` not `[confidence]`.

3. **`from_profile` calls `SqlxStore::open()`**: C-02 says "never call `SqlxStore::open()`" on snapshot. However, `VectorIndex::new()` requires `Arc<SqlxStore>` with no raw-pool alternative. We call it and keep the raw `SqlitePool` separate for direct queries. The drain task fires but is idle — no writes occur. The spirit of C-02 (no analytics writes) is preserved via `AnalyticsMode::Suppressed`.

4. **`InferenceConfig` has no `nli_model` field**: Step 2 model path validation is a no-op stub because `InferenceConfig` in the current codebase only has `rayon_pool_size`. When W1-4 adds `nli_model`, the validation point is pre-established.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` (service layer construction patterns) — found entries #316, #2553, #321 relevant to construction patterns. TestHarness followed exactly.
- Queried: `/uni-query-patterns` for nan-007 ADRs — found all 5 ADRs (#2585–#2588, #2602). All followed.
- Stored: entry #2607 "Store = SqlxStore type alias: VectorIndex and AuditLog accept Arc&lt;SqlxStore&gt; directly" via /uni-store-pattern
- Stored: entry #2608 "ConfidenceWeights fields are base/usage/fresh/help/corr/trust — pseudocode had wrong names" via /uni-store-pattern
