# Gate 3b Report: crt-038

> Gate: 3b (Code Review)
> Date: 2026-04-02
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All deletions and the effective() guard match pseudocode exactly |
| Architecture compliance | FAIL | `impl Default for InferenceConfig` retains old values (w_nli=0.35, w_sim=0.25, w_conf=0.15) — diverges from serde defaults and AC-01 specification |
| Interface implementation | PASS | process_auto_quarantine, all four chain signatures, and main.rs call site all correctly stripped |
| Test case alignment | PASS | All 3 AC-02 tests present; all 13 nli_detection + 4 background NLI tests absent |
| Code quality — no stubs | PASS | No todo!/unimplemented! in modified files |
| Code quality — compilation | PASS | `cargo build --workspace` succeeds; one pre-existing unrelated test failure (col018_topic_signal_from_file_path) |
| Code quality — no .unwrap() in non-test | WARN | Pre-existing; not introduced by this feature |
| Code quality — file line limits | WARN | search.rs (4967), config.rs (7043), background.rs pre-existing violations; all pre-date crt-038 and are explicitly exempted by NFR-05 |
| Security | PASS | No new untrusted input surfaces; dead code removed reduces attack surface |
| Knowledge stewardship | PASS | Agent reports contain Queried/Stored entries |

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Evidence**:

`effective()` short-circuit — `search.rs` lines 161–163:
```rust
if self.w_nli == 0.0 {
    return *self;
}
```
This is the FIRST branch before the `if nli_available` branch at line 165. `FusionWeights` derives `Copy` (line 113: `#[derive(Debug, Clone, Copy, Default)]`), so `return *self` is correct and equivalent to the pseudocode's `return FusionWeights { ..*self }`.

All deletions confirmed absent via grep:
- `run_post_store_nli` — 0 code matches (2 doc-comment references only; see Check 6)
- `maybe_run_bootstrap_promotion` / `run_bootstrap_promotion` — 0 matches
- `write_edges_with_cap` — 0 matches
- `NliStoreConfig` / `nli_store_cfg` — 0 matches
- `NliQuarantineCheck` / `nli_auto_quarantine_allowed` — 0 matches
- `parse_nli_contradiction_from_metadata` — 0 matches

serde default functions (`default_w_*()`) updated correctly in `config.rs` lines 670–704:
- `default_w_sim()` = 0.50
- `default_w_nli()` = 0.00
- `default_w_conf()` = 0.35
- `default_w_util()` = 0.00
- `default_w_prov()` = 0.00
- `default_nli_enabled()` = false

`nli_detection.rs` reduced to 134 lines containing only the 3 retained helpers + 2 tests for `format_nli_metadata`. Module-level doc comment updated per pseudocode template.

---

### Check 2: Architecture Compliance (AC-01 Violation)

**Status**: FAIL

**Evidence**: The architecture specification states:

> **AC-01**: Verification: `InferenceConfig::default()` produces exactly: w_sim=0.50, w_nli=0.00, w_conf=0.35, ...

The `impl Default for InferenceConfig` at `config.rs` lines 576–631 hardcodes the OLD pre-crt-038 values:

```rust
w_sim: 0.25,   // should be 0.50
w_nli: 0.35,   // should be 0.00
w_conf: 0.15,  // should be 0.35
w_coac: 0.0,   // correct
w_util: 0.05,  // should be 0.00
w_prov: 0.05,  // should be 0.00
```

This `Default` impl does NOT call the updated `default_w_*()` serde backing functions. Two code paths now diverge:

- `toml::from_str("[inference]\n")` → serde calls `default_w_*()` → conf-boost-c values ✓
- `InferenceConfig::default()` → manual impl → old NLI-dominant values ✗

**Impact**:

1. `shutdown.rs` lines 314 and 416 use `InferenceConfig::default()` in test server fixtures. Those fixtures pass old scoring weights to `SearchService`, meaning search tests run against the old formula.
2. Config merge tests in `config.rs` (lines 4049, 4062, 4075, 4101, 4123, 4136) use `..InferenceConfig::default()` spread syntax — inheriting old w_nli, w_sim, w_conf values as baselines for those tests.
3. The test `test_inference_config_deserialize_missing_field` at line 4207 calls `InferenceConfig::default()` explicitly. It only checks `rayon_pool_size` so it passes, but any future test comparing other fields against `::default()` will see the old values.

**Why tests still pass**: The key AC-01 test (`test_inference_config_weight_defaults_when_absent`) correctly uses serde deserialization (`toml::from_str("[inference]\n")`), not `::default()`. The sum test (`test_fusion_weights_default_sum_unchanged_by_crt030`) passes coincidentally because both old and new weight profiles sum to 0.92.

**Fix**: Update `impl Default for InferenceConfig` to call the serde backing functions:
```rust
w_sim:  default_w_sim(),   // 0.50
w_nli:  default_w_nli(),   // 0.00
w_conf: default_w_conf(),  // 0.35
w_util: default_w_util(),  // 0.00
w_prov: default_w_prov(),  // 0.00
nli_enabled: default_nli_enabled(),  // false
```

---

### Check 3: Interface Implementation

**Status**: PASS

**Evidence**:

`process_auto_quarantine` signature (`background.rs` line 1064–1071): 6 parameters only — no `nli_enabled`, no `nli_auto_quarantine_threshold`. Matches AC-08 specification exactly.

Full cascade chain verified:
- `spawn_background_tick` (line 231): no nli_enabled/nli_auto_quarantine_threshold
- `background_tick_loop` (line 304): no nli_enabled/nli_auto_quarantine_threshold
- `run_single_tick` (line 411): no nli_enabled/nli_auto_quarantine_threshold
- `maintenance_tick` (line 789): no nli_enabled/nli_auto_quarantine_threshold
- call site in `maintenance_tick` (line 922): 6 arguments, no stripped params
- `main.rs` call to `spawn_background_tick` (line 712): 22 parameters, none being nli_enabled or nli_auto_quarantine_threshold

Retained `nli_enabled` reference at `background.rs` line 760 is the `run_graph_inference_tick` gate — retained code per architecture; correctly present.

Three retained helpers in `nli_detection.rs` (AC-13):
- `pub(crate) async fn write_nli_edge` at line 19
- `pub(crate) fn format_nli_metadata` at line 62
- `pub(crate) fn current_timestamp_secs` at line 73

`nli_detection_tick.rs` line 34 import unchanged; builds without error.

`StoreService::new` (store_ops.rs line 69–95): 10 parameters, no `nli_cfg`. The `#[allow(clippy::too_many_arguments)]` attribute is retained; with 10 arguments it may no longer be needed, but its retention causes no failure.

---

### Check 4: Test Case Alignment

**Status**: PASS

**Evidence**:

Three AC-02 tests present in `search.rs` (lines 4862–4965):
1. `test_effective_short_circuit_w_nli_zero_nli_available_false` — asserts all 8 fields unchanged when effective(false) called with w_nli=0.0; uses exact `assert_eq!` per test plan
2. `test_effective_short_circuit_w_nli_zero_nli_available_true` — asserts same with effective(true)
3. `test_effective_renormalization_still_fires_when_w_nli_positive` — asserts re-normalization occurs with w_nli=0.20, nli_available=false; uses `< 1e-10` approximate equality; confirms `result.w_sim != fw.w_sim`

`test_fusion_weights_default_sum_unchanged_by_crt030` updated: assertion message now references "crt-038: conf-boost-c defaults" (lines 4854–4857). Arithmetic comment block updated at lines 4840–4842.

Deleted tests confirmed absent:
- All 13 nli_detection.rs tests (grep returns 0 matches for all named functions)
- All 4 background.rs NLI integration tests (grep returns 0 matches)
- `parse_nli_contradiction_from_metadata` absent (0 matches)

`nli_detection.rs` retains 2 tests for `format_nli_metadata` (the retained helper) — appropriate.

---

### Check 5: Code Quality

**Status**: PASS (with pre-existing WARNs)

**Evidence**:

`cargo build --workspace` — succeeds with 16 warnings in `unimatrix-server` lib (pre-existing, not introduced by crt-038).

Test run: 2569 passed, 1 failed. The failure is `uds::listener::tests::col018_topic_signal_from_file_path` — embedding model not initialized in test env. This test is in `uds/listener.rs` (not a crt-038 file) and the failure is a pre-existing environment issue unrelated to formula changes or dead-code removal.

No `todo!()`, `unimplemented!()`, or `TODO`/`FIXME` introduced in modified files.

No `.unwrap()` introduced in non-test code in modified files (the `.unwrap_or_default()` in `current_timestamp_secs` is appropriate for `SystemTime` operations).

Pre-existing 500-line violations explicitly exempted by NFR-05:
- `search.rs`: 4,967 lines
- `config.rs`: 7,043 lines
- `background.rs`: large (pre-existing, lines reduced by crt-038)
- `services/mod.rs`: 641 lines (pre-existing)

`nli_detection.rs` now 134 lines — well within limit.

---

### Check 6: Security

**Status**: PASS

**Evidence**:

No new untrusted input surfaces introduced. Dead-code removal eliminates `run_post_store_nli`'s path that processed stored entry content through the NLI cross-encoder — reducing attack surface.

Two stale doc-comment references to deleted functions remain (not functional code):
- `config.rs` line 325: `/// Per-call cap on total edges written during `run_post_store_nli`.` — doc comment on the retained `max_contradicts_per_tick` field
- `nli_detection_tick.rs` line 3: `//! ... is the counterpart to `maybe_run_bootstrap_promotion`` — module doc comment

These are cosmetic; they do not affect security posture. They are flagged as WARN under knowledge stewardship concerns.

---

### Check 7: Knowledge Stewardship

**Status**: PASS

**Evidence**: Agent reports reviewed. Wave 1 and Wave 2 agent reports contain `## Knowledge Stewardship` sections with `Queried:` entries (evidence of `/uni-query-patterns` before implementing) and `Stored:` or "nothing novel to store" entries.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `impl Default for InferenceConfig` still has old weight values (w_nli=0.35, w_sim=0.25, w_conf=0.15, w_util=0.05, w_prov=0.05) — AC-01 requires `InferenceConfig::default()` to return conf-boost-c values | rust-dev (Wave 1b re-run or Wave 2 follow-up) | In `config.rs` lines 601–606, replace the hardcoded literals with calls to the serde backing functions: `w_sim: default_w_sim()`, `w_nli: default_w_nli()`, `w_conf: default_w_conf()`, `w_util: default_w_util()`, `w_prov: default_w_prov()`, and `nli_enabled: default_nli_enabled()`. Verify `test_fusion_weights_default_sum_unchanged_by_crt030` still passes after change (sum remains 0.92). Verify any config test using `..InferenceConfig::default()` spread that previously relied on old weight values is updated. |

## Additional Findings (WARN)

1. **Commit message cosmetic error** (spawn-prompt flagged): Commit `29802055` has `(#476)` in message instead of `(#483)`. Code changes correct; no action needed.

2. **Stale doc comments**: `config.rs` line 325 references the deleted `run_post_store_nli`; `nli_detection_tick.rs` line 3 references the deleted `maybe_run_bootstrap_promotion`. These are in doc comments, not function signatures. Should be cleaned up but do not block correctness.

3. **`#[allow(clippy::too_many_arguments)]` on `StoreService::new`**: With 10 parameters remaining post-deletion, this allow attribute may be unnecessary. Not a clippy failure (clippy threshold is typically 7). Can be cleaned up but does not block.

4. **Pre-existing `cargo clippy --workspace -- -D warnings` failures**: All clippy errors are in `unimatrix-engine/src/auth.rs` and `unimatrix-observe/src/synthesis.rs` — not touched by crt-038. AC-11 (clippy passes) is contingent on pre-existing workspace state; the crt-038 changes themselves introduce no new warnings.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` — searched for "Default impl serde default function divergence config" before assessing Check 2; no existing pattern entry found for `Default` impl vs serde default function mismatches.
- Stored: nothing novel to store — the `impl Default` vs serde backing function divergence is a one-off finding specific to this feature's delivery method; does not rise to a recurring cross-feature pattern.
