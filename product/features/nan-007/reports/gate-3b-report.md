# Gate 3b Report: nan-007

> Gate: 3b (Code Review)
> Date: 2026-03-20
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | FAIL | `SqlxStore::open()` called on snapshot in `profile.rs` (FR-24, C-02); embed model wait loop missing from `runner.rs` |
| Architecture compliance | WARN | `SqlxStore::open()` deviation documented by implementer as OQ-A resolution; analytics drain task not spawned so primary invariant holds |
| Interface implementation | PASS | All public signatures match pseudocode; EvalError variants complete; Python clients implement all specified methods |
| Test case alignment | PASS | All risk-to-test scenario mappings from test plans are implemented; AC coverage present across all modules |
| Code quality — compiles | PASS | `cargo build --workspace` compiles clean; no errors; 6 pre-existing warnings in unimatrix-server |
| Code quality — no stubs | PASS | No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` in production code |
| Code quality — no unwrap in non-test code | PASS | All `.unwrap()` calls confined to `#[cfg(test)]` blocks |
| Code quality — file line limits | FAIL | `profile.rs` (1031 lines), `runner.rs` (1084 lines) exceed 500-line limit; `scenarios.rs` (900 lines, test module starts at line 342) also over 500 |
| Security — path traversal | PASS | `canonicalize()` used correctly in both `snapshot.rs` and `profile.rs` |
| Security — input validation | PASS | SQL source filter uses static literals only; `limit` uses `usize`; payload size guard fires before send |
| Security — no secrets | PASS | No hardcoded credentials or API keys |
| cargo audit | WARN | `cargo-audit` not installed in this environment; cannot verify CVE status |
| Knowledge stewardship | PASS | No agent report to check — gate validator operates as single-agent |

---

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: FAIL

**Finding 1a — `SqlxStore::open()` on snapshot (FR-24 / C-02 violation)**

`profile.rs` lines 324–330 call `unimatrix_store::SqlxStore::open(db_path, PoolConfig::default()).await` on the snapshot database. This is explicitly prohibited by:

- FR-24: "`SqlxStore::open()` (which triggers schema migration) shall not be called on a snapshot database."
- C-02: "Raw `sqlx::SqlitePool` with `read_only(true)` — never `SqlxStore::open()` on snapshot."

The implementer documented the deviation as an OQ-A resolution (lines 313–323): `VectorIndex::new()` requires `Arc<Store>` (= `Arc<SqlxStore>`), and the raw pool cannot satisfy this trait. The note claims the migration is a no-op and the drain task is not spawned. This reasoning is partially valid — the analytics suppression invariant is still satisfied at runtime — but:

1. The spec prohibits `SqlxStore::open()` categorically (not conditionally).
2. Even a no-op migration write touches the snapshot file's schema version row, potentially altering the file's bytes and violating NFR-04 (SHA-256 integrity).
3. R-02 test scenario 1 in the risk strategy requires: "grep for `SqlxStore::open` calls inside `eval/` — assert zero occurrences." This test would fail.

The correct resolution requires one of:
  - Implementing a minimal `Arc<Store>`-compatible wrapper around the raw `SqlitePool` that satisfies only the `VectorIndex::new()` trait requirements (the pseudocode Path B at eval-profile.md line 185).
  - Adding a `VectorIndex::open_readonly(pool: SqlitePool)` constructor that accepts a raw pool directly.
  - Verifying that the migration is truly a no-op and that it does not modify the snapshot bytes, then updating the spec's FR-24 to allow `SqlxStore::open()` when schema is current (requires spec change, not implementer decision).

**Finding 1b — embed model wait loop missing from `runner.rs`**

The pseudocode `run_eval_async` (eval-runner.md lines 148–158) specifies a wait loop after each `EvalServiceLayer::from_profile()` call:

```
let mut attempts = 0
loop:
  match layer.inner.embed_handle().get_adapter().await:
    Ok(_)  → break
    Err(e) if attempts < 30: sleep 100ms, attempts += 1
    Err(e): return Err(...)
```

The implementation (`runner.rs` line 178) calls `EvalServiceLayer::from_profile()` and immediately proceeds to scenario replay with no model readiness check. If the embed model has not finished loading (a background task started in `profile.rs` step 6), the first scenario's `layer.inner.search.search(...)` call will fail with an embed error. This is a semantic gap from the pseudocode, not merely a style difference.

---

### Check 2: Architecture Compliance

**Status**: WARN

The `SqlxStore::open()` deviation in `profile.rs` was flagged as a FAIL in Check 1 (spec/pseudocode violation). From a pure architecture standpoint, the key ADR-002 invariant (analytics drain task never spawned) is satisfied: `ServiceLayer::with_rate_config()` is called without wiring an analytics channel, so `enqueue_analytics` is no-op'd by absence of a channel receiver. The `AnalyticsMode::Suppressed` field is stored and verifiable. ADR-003 (`test-support` feature) is correctly implemented in `Cargo.toml` line 20. ADR-005 (nested clap subcommand) is correctly implemented in `main.rs` and `eval/mod.rs`. ADR-004 (no new workspace crate) is satisfied.

The `run_report` submodule structure deviates from the single-file design in ARCHITECTURE.md (`eval/report.rs`) by splitting into `eval/report/mod.rs`, `aggregate.rs`, `render.rs`, and `tests.rs`. This is a positive change (under 500 lines each) that preserves module semantics without changing the public API.

---

### Check 3: Interface Implementation

**Status**: PASS

All public function signatures match the pseudocode specifications:

| Function | Pseudocode Signature | Implemented Signature | Match |
|----------|---------------------|----------------------|-------|
| `run_snapshot` | `fn(project_dir: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>` | Matches | PASS |
| `EvalServiceLayer::from_profile` | `async fn(db_path: &Path, profile: &EvalProfile, project_dir: Option<&Path>) -> Result<Self, EvalError>` | Matches (3-arg form from WARN-A in 3a) | PASS |
| `run_scenarios` | `fn(db: &Path, source: ScenarioSource, limit: Option<usize>, out: &Path) -> Result<(), Box<dyn Error>>` | Matches | PASS |
| `run_eval` | `fn(db: &Path, scenarios: &Path, configs: &[PathBuf], k: usize, out: &Path) -> Result<(), Box<dyn Error>>` | Matches | PASS |
| `run_report` | `fn(results: &Path, scenarios: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>` | Matches | PASS |
| `run_eval_command` | `fn(cmd: EvalCommand, project_dir: Option<&Path>) -> Result<(), Box<dyn Error>>` | Matches | PASS |

`EvalError` variants: all 6 variants (`ModelNotFound`, `ConfigInvariant`, `LiveDbPath`, `Io`, `Store`, `ProfileNameCollision`, `InvalidK`) are implemented with `Display` and `std::error::Error`. `HookResponse`, `UnimatrixUdsClient`, `UnimatrixHookClient` match their Python pseudocode fully.

`UdsConnectionError` constructor wraps `cause: Exception` — spec says raise `ConnectionError` with socket path; implementation raises `UdsConnectionError` (a subclass of `UdsClientError`) which is correct. The `HookPayloadTooLargeError` raises `ValueError` as per AC-14 is implemented as a separate exception class, not Python's built-in `ValueError`. This is a minor deviation from AC-14 which specifies `ValueError` specifically, but `HookPayloadTooLargeError` inherits from `HookClientError` not `ValueError`. The test plan tests for `HookPayloadTooLargeError` not `ValueError`, so the test coverage aligns with the implementation.

---

### Check 4: Test Case Alignment

**Status**: PASS

All major risk scenarios from the Risk Strategy are covered:

| Risk | Required Coverage | Implemented | Status |
|------|------------------|-------------|--------|
| R-01 (analytics suppression) | SHA-256 integrity + unit test on AnalyticsMode | `test_from_profile_analytics_mode_is_suppressed`, `test_run_scenarios_does_not_write_to_snapshot` | PASS |
| R-02 (`SqlxStore::open` guard) | grep assertion | `test_snapshot_no_sqlx_store_open_in_snapshot` (structural doc test) — but `eval/` has no equivalent | WARN |
| R-03 (test-support feature) | Direct `kendall_tau` call in runner | `test_kendall_tau_reachable_from_eval_runner` | PASS |
| R-04 (UDS framing) | Raw byte capture | `test_uds_framing_no_length_prefix` (in test_eval_uds.py) | PASS |
| R-06 (path canonicalization) | Symlink test | `test_snapshot_path_guard_symlink` | PASS |
| R-08 (P@K dual-mode) | Both branches tested | `test_pak_hard_labels_not_confused_with_baseline`, `test_pak_soft_ground_truth_query_log_scenario` | PASS |
| R-09 (ConfigInvariant message) | Message content assertion | `test_confidence_weights_invariant_sum_low_fails`, boundary tests | PASS |
| R-12 (OR semantics regression) | MRR-only and P@K-only cases | `test_zero_regression_check_mrr_regression_only`, `test_zero_regression_check_pak_regression_only` (in report/tests.rs) | PASS |
| R-13 (payload size guard before send) | Pre-send assertion | `test_payload_too_large_raises_before_send` (in test_eval_hooks.py) | PASS |
| R-14 (UDS path length) | 103/104 byte boundary | `test_uds_path_too_long_rejected`, `test_uds_path_exactly_103_bytes_ok` | PASS |
| R-16 (length parity) | Mismatch truncation | `test_run_scenarios_length_parity` | PASS |
| R-17 (section headers) | All 5 headers present | `test_report_contains_all_five_sections` | PASS |

AC-08 and AC-09 coverage verified in `eval/report/tests.rs`. AC-15 (help text CLI registration) verified in `main_tests.rs`. Note: AC-14 specifies `ValueError` but `HookPayloadTooLargeError` is not a `ValueError` subclass — this is a test plan alignment deviation (WARN).

---

### Check 5: Code Quality — Compilation

**Status**: PASS

```
cargo build --workspace 2>&1 | tail -3:
  warning: `unimatrix-server` (lib) generated 6 warnings (run `cargo fix --lib -p unimatrix-server` to apply 1 suggestion)
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
```

Zero errors. The 6 warnings in `unimatrix-server` are pre-existing dead code warnings (fields `vector_store`, `usage_dedup`, `effectiveness_state`, `confidence_state`; methods `correct_with_audit`, `record_usage_for_entries`; function `new`). None are in nan-007 code.

**Doc-test failure** (pre-existing): `crates/unimatrix-server/src/infra/config.rs - infra::config (line 21)` fails because a file path starting with `~` is parsed as a Rust doctest. This is pre-existing and unrelated to nan-007.

All 2474 unit/integration tests pass (excluding the pre-existing doc-test and ignored tests).

---

### Check 6: Code Quality — No Stubs or Placeholders

**Status**: PASS

No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or placeholder functions found in production code paths. All modules are fully implemented.

---

### Check 7: Code Quality — No `.unwrap()` in Non-Test Code

**Status**: PASS

All `.unwrap()` calls appear inside `#[cfg(test)]` blocks. Production paths use proper error propagation via `?`, `map_err`, and structured `EvalError` variants.

---

### Check 8: Code Quality — File Line Limit (500 lines)

**Status**: FAIL

Three source files exceed the 500-line limit:

| File | Total Lines | Production Lines (before `#[cfg(test)]`) | Status |
|------|------------|------------------------------------------|--------|
| `eval/profile.rs` | 1031 | 547 (test module at line 548) | FAIL |
| `eval/runner.rs` | 1084 | 562 (test module at line 563) | FAIL |
| `eval/scenarios.rs` | 900 | 341 (test module at line 342) | FAIL (total) |

Note: `eval/report/` was correctly split into a submodule tree, keeping each file under 500 lines. The same refactoring should be applied to the three oversized files.

---

### Check 9: Security

**Status**: PASS

- **Path traversal**: Both `snapshot.rs` and `profile.rs` use `std::fs::canonicalize()` before comparing paths. The `canonicalize_or_parent()` helper handles non-existent output paths correctly.
- **SQL injection**: `scenarios.rs` builds a dynamic SQL string by interpolating the source filter (`ScenarioSource::to_sql_filter()` returns only static literals `"mcp"` or `"uds"`) and the limit (typed `usize`). The comment at line 199 correctly documents the reasoning. No user-controlled string reaches the SQL query body. This is safe but uses string formatting rather than parameterized queries — a WARN-level observation, not a security failure.
- **No hardcoded secrets**: No API keys, passwords, or credentials in any file.
- **Python clients**: `HookPayloadTooLargeError` fires before `sendall()` — verified at line 169 of `hook_client.py` that the size check precedes the `header + payload` send.
- **Serialization**: `serde_json::from_str` is used throughout; malformed JSON produces `Err` not panic.

---

### Check 10: cargo audit

**Status**: WARN

`cargo-audit` is not installed in this environment. CVE status cannot be verified automatically. This should be run in CI.

---

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| **FR-24 / C-02**: `SqlxStore::open()` called on snapshot in `profile.rs` (lines 324–330) | rust-dev | Replace with a minimal `Arc<Store>`-compatible read-only wrapper around the raw `SqlitePool`, OR verify that the migration does not alter snapshot bytes and file a spec change. Pseudocode eval-profile.md documents the "Path B" approach (build a minimal Store-compatible adapter). |
| **Pseudocode fidelity**: Embed model wait loop missing in `run_eval_async` (`runner.rs`) | rust-dev | After each `EvalServiceLayer::from_profile()` call, poll `layer.inner.embed_handle().get_adapter().await` with up to 30 × 100ms retries before proceeding to scenario replay (per eval-runner.md lines 148–158). |
| **File line limit**: `profile.rs` (1031), `runner.rs` (1084), `scenarios.rs` (900) all exceed 500 lines | rust-dev | Refactor each oversized file into a submodule tree (same pattern as `eval/report/`). For example: `eval/profile/` with `mod.rs`, `layer.rs`, `error.rs`, `validation.rs`, `tests.rs`. |
| **AC-14 exact type**: `HookPayloadTooLargeError` is not a subclass of Python's `ValueError` | rust-dev (Python) | Either inherit `HookPayloadTooLargeError` from `ValueError`, or update the test plan and spec to reference `HookPayloadTooLargeError` specifically. AC-14 says "raises `ValueError`." |

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "gate failure patterns file line limit eval harness" — no direct matches; the 500-line limit FAIL is a recurring gate pattern per entry #1203 (missing boundary tests) and #1204 (pseudocode cross-reference failures). The `SqlxStore::open()` on snapshot is a new pattern: implementer resolved a pseudocode Open Question by using a prohibited call. Worth storing if seen in a second feature.
- Queried: `/uni-query-patterns` for "SqlxStore open snapshot migration eval read-only" — entry #2060 (migration connection sequencing) and entry #2125 (analytics drain unsuitable for immediate-visibility reads) are relevant. Neither directly covers the trait-incompatibility motivation for the OQ-A choice.
- Stored: nothing novel to store — the "Open Question resolved via prohibited API call" pattern is feature-specific to nan-007's OQ-A. If the resolution approach (Path B store wrapper) is implemented, that technique warrants storage. Will re-evaluate after rework.
