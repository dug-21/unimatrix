# Gate 3b Retry Report: crt-022

> Gate: 3b (Code Review) — RETRY after rework
> Date: 2026-03-19
> Result: PASS
> Previous report: `reports/gate-3b-report.md` (REWORKABLE FAIL — InferenceConfig unit tests AC-11 #5–8 absent)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Previously PASS — unchanged; re-verified |
| Architecture compliance | PASS | Previously PASS — unchanged; re-verified |
| Interface implementation | PASS | Previously PASS — unchanged; re-verified |
| Test case alignment | PASS | 13 InferenceConfig unit tests now present; all 4 AC-11 boundary tests (#5–8) confirmed |
| Code quality — no stubs | PASS | Zero `todo!()`, `unimplemented!()`, blocking stubs |
| Code quality — no `.unwrap()` in prod | PASS | All `.unwrap()` confined to `#[cfg(test)]` blocks |
| Code quality — file line limits | WARN | `rayon_pool.rs` 632 lines (182 prod + 450 test); `config.rs` 2438 lines (pre-existing large file); same WARN as previous report |
| Compilation | PASS | `cargo build --workspace` exits 0, zero errors |
| Security | PASS | Previously PASS — no new code added by rework |
| CI enforcement | PASS | `check-inference-sites.sh` passes; all 5 checks OK |
| `AsyncEmbedService` absent | PASS | `grep -r "AsyncEmbedService" crates/ | wc -l` = 0 |
| Knowledge stewardship | PASS | Previously PASS — agent reports contain stewardship sections |

## Detailed Findings

### Previously-Failed Check: Test Case Alignment (AC-11 #5–8)

**Status**: PASS

**Evidence from rework**:

The rework added 13 InferenceConfig unit tests to the `#[cfg(test)] mod tests` block in
`crates/unimatrix-server/src/infra/config.rs`. The 4 required AC-11 boundary tests are
now present and verified passing:

| Test function | AC-11 ref | Input | Expected | Result |
|---|---|---|---|---|
| `test_inference_config_valid_lower_bound` | #5 | `rayon_pool_size = 1` | `Ok(())` | PASS |
| `test_inference_config_valid_upper_bound` | #6 | `rayon_pool_size = 64` | `Ok(())` | PASS |
| `test_inference_config_rejects_zero` | #7 | `rayon_pool_size = 0` | `Err(InferencePoolSizeOutOfRange { value: 0 })` | PASS |
| `test_inference_config_rejects_sixty_five` | #8 | `rayon_pool_size = 65` | `Err(InferencePoolSizeOutOfRange { value: 65 })` | PASS |

Additional tests added by rework also pass:
- `test_inference_config_valid_eight` (R-07 mid-range value)
- `test_inference_config_valid_four` (ADR-003 floor value)
- `test_inference_config_default_formula_in_range` (default produces [4,8])
- `test_inference_config_absent_section_uses_default` (serde `#[serde(default)]` wiring)
- `test_inference_config_parses_from_toml` (explicit value deserialization)
- `test_inference_config_deserialize_missing_field` (absent field uses Default)
- `test_inference_config_error_message_names_field` (actionable error message)
- `test_display_inference_pool_size_out_of_range` (Display impl correctness)
- `test_unimatrix_config_inference_field` (structural wiring into UnimatrixConfig)

Test run result:
```
test result: ok. 81 passed; 0 failed; 0 ignored; 0 measured; 1415 filtered out; finished in 0.05s
```
(81 config module tests total; all pass.)

### Re-verified: Pseudocode Fidelity

**Status**: PASS

No production code was changed by the rework. All previously-verified items remain correct:
- `RayonPool` struct, `spawn`, `spawn_with_timeout`, `pool_size`, `name` match pseudocode
- `InferenceConfig` struct, `Default`, `validate()` match `inference_config.md`
- All 7 call-site migrations match patterns A and B from `call_site_migration.md`
- `AsyncEmbedService` absent (0 matches via grep)
- `AsyncVectorStore` retained unchanged in `unimatrix-core/src/async_wrappers.rs`

### Re-verified: Compilation

**Status**: PASS

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s
```

Zero errors. Existing 6 dead-code warnings are pre-existing and unrelated to crt-022.

### Re-verified: Full Test Suite

**Status**: PASS

```
test result: ok. 1496 passed; 0 failed; 0 ignored; 0 measured; finished in 5.75s
```

All 21 rayon pool unit tests pass:
- AC-11 #1–4 (dispatch, panic, pool-init-1, pool-init-8): PASS
- Timeout semantics, concurrency, drop behaviour, adversarial: PASS

All 11+ InferenceConfig tests (including AC-11 #5–8): PASS

### Re-verified: CI Enforcement

**Status**: PASS

```
Checking services/ for spawn_blocking at embedding inference sites... OK
Checking services/ for spawn_blocking_with_timeout at embedding inference sites... OK
Checking background.rs for spawn_blocking at embedding inference sites... OK
Checking async_wrappers.rs for AsyncEmbedService... OK
Checking embed_handle.rs for exactly 1 spawn_blocking (OnnxProvider::new)... OK
OK: all spawn_blocking enforcement checks passed (AC-07 / crt-022).
```

### `cargo audit`

`cargo-audit` is not installed in this environment. No CVE assessment possible via this tool.
No new external dependencies were introduced by the rework (test-only additions to `config.rs`).
`rayon = "1"` and `num_cpus = "1"` were already evaluated in the original gate-3b report.

---

## Self-Check

- [x] Correct gate check set used (3b per spawn prompt)
- [x] All checks re-evaluated; focus on previously-failed check (AC-11 #5–8)
- [x] Glass box report written to `reports/gate-3b-retry-report.md`
- [x] Previous gate report read before this retry validation
- [x] Only the failing items were re-checked in depth; all others confirmed unchanged
- [x] Cargo output truncated (tail-5 / tail-40)
- [x] Gate result PASS — all 11 checks now PASS (1 WARN unchanged from original)

## Knowledge Stewardship

- Stored: nothing novel to store — this retry confirms a straightforward test-addition rework. The pattern (validator method written before unit tests covering its boundary values) is a one-off execution gap, not a recurring cross-feature quality failure worth a lesson-learned entry. The original gate-3b report already noted this finding.
