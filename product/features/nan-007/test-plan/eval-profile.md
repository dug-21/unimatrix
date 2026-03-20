# Test Plan: `eval/profile.rs`

**Component**: `crates/unimatrix-server/src/eval/profile.rs`
**Types under test**: `EvalProfile`, `EvalServiceLayer`, `AnalyticsMode`, `EvalError`
**Function under test**: `EvalServiceLayer::from_profile(db_path: &Path, profile: &EvalProfile) -> Result<Self, EvalError>`
**AC coverage**: AC-05 (read-only guard), C-14 (model path validation), C-15 (weight invariant)
**Risk coverage**: R-01 (analytics suppression), R-02 (SqlxStore guard), R-09 (ConfigInvariant error), R-10 (ModelNotFound panic), R-11 (block_export_sync)

---

## Unit Tests

Location: `crates/unimatrix-server/src/eval/profile.rs` (inline `#[cfg(test)]`)

### Test: `test_analytics_mode_suppressed_is_default`

**Purpose**: Assert that `EvalServiceLayer::from_profile()` stores `AnalyticsMode::Suppressed` — the drain task is never spawned.
**Arrange**: Prepare a minimal snapshot SQLite file (valid schema, read-only). Build a baseline `EvalProfile` (empty overrides).
**Act**: Call `EvalServiceLayer::from_profile(&snapshot_path, &baseline_profile).await`.
**Assert**:
- Returns `Ok(layer)`.
- Inspect `layer.analytics_mode` — assert it is `AnalyticsMode::Suppressed`.
- No tokio task is spawned for drain (verify by checking no background task handles are returned or by inspecting the `ServiceLayer` construction path).
**Risk**: R-01 (Critical)

### Test: `test_from_profile_does_not_call_sqlx_store_open`

**Purpose**: Ensure the read-only pool path uses raw `SqliteConnectOptions` without calling `SqlxStore::open()`.
**Arrange**: Prepare a snapshot with a known schema version (N). Record the `user_version` PRAGMA value.
**Act**: Call `EvalServiceLayer::from_profile(&snapshot_path, &baseline_profile).await`. Then read `PRAGMA user_version` from the snapshot.
**Assert**:
- Returns `Ok`.
- `user_version` is unchanged (migration did not run).
**Risk**: R-02

### Test: `test_from_profile_opens_read_only_pool`

**Purpose**: Assert that writes to the snapshot via the eval pool fail at the SQLite layer.
**Arrange**: Call `EvalServiceLayer::from_profile(&snapshot_path, &baseline_profile).await`.
**Act**: Attempt to execute a write query against the internal pool (e.g., `INSERT INTO entries ...`).
**Assert**: Write returns `Err` with an SQLite read-only error code (SQLITE_READONLY).
**Risk**: R-01, R-02 (AC-05)

### Test: `test_from_profile_returns_model_not_found`

**Purpose**: Missing inference model path returns structured error, not a panic (R-10, FR-23).
**Arrange**: Build a `EvalProfile` with `config_overrides.inference.nli_model = Some("/nonexistent/model.onnx")`.
**Act**: `EvalServiceLayer::from_profile(&snapshot_path, &profile).await`.
**Assert**: Returns `Err(EvalError::ModelNotFound(path))` where `path == PathBuf::from("/nonexistent/model.onnx")`. No panic.
**Risk**: R-10

### Test: `test_from_profile_returns_model_not_found_unreadable`

**Purpose**: A model file that exists but is unreadable also returns `ModelNotFound` (or an appropriate `Io` error), not a panic.
**Arrange**: Create a file at a temp path; set permissions to `000`.
**Act**: `EvalServiceLayer::from_profile` with that path as the model.
**Assert**: Returns `Err(...)`. Not a panic.
**Risk**: R-10

### Test: `test_confidence_weights_invariant_sum_low`

**Purpose**: C-15 — weights summing below 0.92 yield `EvalError::ConfigInvariant` with user-readable message (R-09, FR-18).
**Arrange**: Build a `EvalProfile` with `[confidence]` weights: e.g., six values each 0.15 (sum = 0.90).
**Act**: `EvalServiceLayer::from_profile(&snapshot_path, &profile).await`.
**Assert**:
- Returns `Err(EvalError::ConfigInvariant(msg))`.
- `msg` contains `"0.92"` (expected) and `"0.90"` or the actual computed sum.
- `msg` does NOT look like a raw serde error.
**Risk**: R-09

### Test: `test_confidence_weights_invariant_sum_high`

**Purpose**: Weights summing above 0.92 + 1e-9 also yield `ConfigInvariant`.
**Arrange**: Six weights summing to 0.93.
**Assert**: Same as above with actual sum `"0.93"`.
**Risk**: R-09

### Test: `test_confidence_weights_invariant_boundary_pass`

**Purpose**: Weights summing to exactly 0.92 (within ±1e-9) succeed.
**Arrange**: Six weights summing exactly to 0.92.
**Act**: `EvalServiceLayer::from_profile`.
**Assert**: Returns `Ok(...)`.
**Risk**: R-09 boundary

### Test: `test_confidence_weights_invariant_boundary_fail`

**Purpose**: Weights summing to 0.92 + 2e-9 fail.
**Arrange**: Six weights summing to `0.92 + 2e-9`.
**Assert**: Returns `Err(EvalError::ConfigInvariant(...))`.
**Risk**: R-09 boundary

### Test: `test_confidence_weights_missing_field`

**Purpose**: A `[confidence]` section with fewer than six fields produces a structured error (not a raw serde `missing field` error).
**Arrange**: Profile TOML with only five of the six confidence weight fields.
**Assert**: Returns `Err(EvalError::ConfigInvariant(msg))`. Message is user-readable and names the invariant.
**Risk**: R-09

### Test: `test_baseline_profile_empty_toml_succeeds`

**Purpose**: An empty TOML file (baseline profile) constructs successfully with compiled defaults.
**Arrange**: Empty profile TOML.
**Act**: `EvalServiceLayer::from_profile(&snapshot_path, &baseline_profile).await`.
**Assert**: Returns `Ok(layer)`. `layer.profile_name` matches the `[profile] name` field from the TOML.
**Risk**: Edge case (from Risk Strategy)

### Test: `test_live_db_path_guard`

**Purpose**: `eval run --db` guard mirrors `snapshot` guard — passing the live DB path returns `EvalError::LiveDbPath`.
**Arrange**: Determine active daemon DB path via `ProjectPaths`. Supply that path as `db_path`.
**Act**: `EvalServiceLayer::from_profile(&live_db_path, &baseline_profile).await`.
**Assert**: Returns `Err(EvalError::LiveDbPath { supplied, active })`. Error message names both paths.
**Risk**: R-06, R-10 (AC-16)

---

## Integration Test Expectations (through eval run pipeline)

The `EvalServiceLayer` is the core of `eval run`. Its read-only enforcement is verified end-to-end in `test_eval_offline.py::test_eval_run_readonly_sha256` (AC-05). Individual construction errors (`ModelNotFound`, `ConfigInvariant`, `LiveDbPath`) surface as non-zero exit codes from `unimatrix eval run`.

For integration-level assertions:
- `ConfigInvariant` → exit code != 0; stderr contains expected/actual sums.
- `ModelNotFound` → exit code != 0; stderr contains the missing path.
- `LiveDbPath` → exit code != 0; stderr contains both resolved paths (AC-16).

---

## Edge Cases from Risk Strategy

- Profile name collision: two profile TOML files with identical `[profile] name` fields. `run_eval` must fail before replay begins with a structured error naming the duplicate profile name.
- Empty TOML (baseline): must succeed; compiled defaults applied.
- Weight sum at boundary: ±1e-9 is pass; ±2e-9 is fail. Floating-point sum must be computed carefully (sum the six values, compare to 0.92 with ε = 1e-9).

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #729 (Intelligence pipeline integration tests), #157 (Test infrastructure is cumulative)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-002 (EvalServiceLayer suppresses analytics at construction via AnalyticsMode::Suppressed), ADR-001 (live-DB path guard, no SqlxStore::open), ADR-003 (test-support feature for kendall_tau)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #129 (Concrete assertions), #157 (Test infrastructure is cumulative)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
