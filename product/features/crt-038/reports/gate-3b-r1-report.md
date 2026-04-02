# Gate 3b-r1 Report: crt-038

> Gate: 3b (Code Review — rework iteration 1)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Rework verification: Default impl calls backing functions | PASS | `impl Default` now calls `default_w_*()` at lines 591, 601–606 |
| Rework verification: No new hardcoded weight literals | PASS | Commit a06c39a2 adds only calls to `default_w_*()`, no raw literals |
| effective() short-circuit is first branch | PASS | `if self.w_nli == 0.0` guard at search.rs line 161, before `nli_available` branch at line 165 |
| AC-02 tests: all 3 present and passing | PASS | All 3 tests present at lines 4862–4965; 4/4 AC-02 + AC-01 tests pass |
| write_edges_with_cap absent | PASS | 0 grep matches in crates/ |
| parse_nli_contradiction_from_metadata absent | PASS | 0 grep matches in crates/ |
| Three retained helpers present in nli_detection.rs | PASS | write_nli_edge at line 19, format_nli_metadata at line 62, current_timestamp_secs at line 73 |
| NliStoreConfig absent | PASS | 0 grep matches in crates/ |
| nli_enabled/nli_auto_quarantine_threshold absent from 5 signatures + main.rs | PASS | All 5 function chain signatures clean; main.rs call clean |
| cargo build --workspace | PASS | Builds with 0 errors; pre-existing 16 warnings in unimatrix-server lib (unchanged) |
| cargo clippy --workspace -- -D warnings | WARN | All clippy errors are in unimatrix-engine and unimatrix-observe — pre-existing, not introduced by crt-038. Count unchanged (139 errors) before and after crt-038 |
| cargo test --workspace | PASS | 2570+ passing across all crates; 0 failures |
| Knowledge stewardship | PASS | Agent reports contain Queried/Stored sections |

## Detailed Findings

### Rework Verification: InferenceConfig::default() Now Produces Correct Values

**Status**: PASS

**Evidence**: Commit a06c39a2 updated `impl Default for InferenceConfig` in `config.rs` lines 589–630.
The six weight fields and `nli_enabled` now call the serde backing functions:

```rust
nli_enabled: default_nli_enabled(),   // false
w_sim: default_w_sim(),               // 0.50
w_nli: default_w_nli(),               // 0.00
w_conf: default_w_conf(),             // 0.35
w_util: default_w_util(),             // 0.00
w_prov: default_w_prov(),             // 0.00
```

The diff for commit a06c39a2 shows only calls to `default_w_*()` functions in the `Default` impl — no new
hardcoded weight literals introduced. The `test_merge_configs_post_merge_fusion_weight_sum_exceeded` test was
updated in the same commit to use a scenario valid under the new defaults (global w_sim=0.7, project w_nli=0.4).

Test `test_inference_config_weight_defaults_when_absent` passes with the new values:
```
infra::config::tests::test_inference_config_weight_defaults_when_absent ... ok
```

### effective() Short-Circuit Position

**Status**: PASS

**Evidence**: `search.rs` lines 161–163 contain the short-circuit guard:
```rust
if self.w_nli == 0.0 {
    return *self;
}
```
This is the FIRST branch in `FusionWeights::effective()`. The `if nli_available` branch appears at line 165.
`FusionWeights` derives `Copy` (confirmed: `#[derive(Debug, Clone, Copy, Default)]`), so `return *self` is
equivalent to `return FusionWeights { ..*self }`.

### AC-02 Tests

**Status**: PASS

All three required tests pass:
```
services::search::tests::step_6d::test_effective_short_circuit_w_nli_zero_nli_available_false ... ok
services::search::tests::step_6d::test_effective_short_circuit_w_nli_zero_nli_available_true ... ok
services::search::tests::step_6d::test_effective_renormalization_still_fires_when_w_nli_positive ... ok
```

Tests are present at `search.rs` lines 4862–4965 and match the test plan specifications exactly (field-by-field
`assert_eq!` for exact equality in short-circuit tests; `< 1e-10` approximate equality for re-normalization test).

### Dead Symbol Verification

**Status**: PASS

All required deletions confirmed absent via grep:
- `run_post_store_nli` — 2 doc-comment references only (in nli_detection.rs module doc and nli_detection_tick.rs); 0 code matches (pre-existing from gate-3b-report.md, not introduced by rework)
- `write_edges_with_cap` — 0 matches in crates/
- `parse_nli_contradiction_from_metadata` — 0 matches in crates/
- `NliStoreConfig` / `nli_store_cfg` — 0 matches in crates/
- `NliQuarantineCheck` / `nli_auto_quarantine_allowed` — 0 matches in crates/
- `maybe_run_bootstrap_promotion` / `run_bootstrap_promotion` — 2 doc-comment references only (not code)

Three retained helpers confirmed present in `nli_detection.rs`:
- `pub(crate) async fn write_nli_edge` at line 19
- `pub(crate) fn format_nli_metadata` at line 62
- `pub(crate) fn current_timestamp_secs` at line 73

### Function Signature Chain

**Status**: PASS

None of the five signatures in the propagation chain carry `nli_enabled: bool` or `nli_auto_quarantine_threshold: f32`:
- `spawn_background_tick` (line 231): clean
- `background_tick_loop` (line 304): clean
- `run_single_tick` (line 411): clean
- `maintenance_tick` (line 789): clean
- `process_auto_quarantine` (line 1064): 6 parameters, matches AC-08 specification exactly
- Call site at maintenance_tick line 922: 6 arguments
- main.rs `spawn_background_tick` call: no nli_enabled or nli_auto_quarantine_threshold arguments

The `nli_enabled` reference at background.rs line 760 is the retained `run_graph_inference_tick` gate —
correct retained code per architecture.

### Cargo Build

**Status**: PASS

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
```
Zero build errors. 16 pre-existing warnings in unimatrix-server lib (unchanged from before crt-038).

### Cargo Clippy

**Status**: WARN (pre-existing failures, not introduced by rework)

`cargo clippy --workspace -- -D warnings` reports errors in `crates/unimatrix-engine` and
`crates/unimatrix-observe` — neither file is touched by crt-038. Baseline verification confirmed:
stash/restore shows 139 clippy errors before crt-038 and 139 errors after — count unchanged.

No new clippy errors introduced by commit a06c39a2 or any prior crt-038 commit.

`unimatrix-server` itself generates no clippy errors under `-D warnings` (confirmed via
`--package unimatrix-server` targeting only that package and its transitive deps within the workspace).

Note: A `field 'nli_handle' is never read` warning exists in `store_ops.rs:66` — this was pre-existing
(baseline confirms it was present before the rework commit).

### Cargo Test

**Status**: PASS

```
test result: ok. 2570 passed; 0 failed; 0 ignored
```
Zero failures across workspace. The previously-failing `col018_topic_signal_from_file_path` test now passes.

### Knowledge Stewardship

**Status**: PASS

Wave 1 and Wave 2 agent reports (reviewed in gate-3b-report.md) contain `## Knowledge Stewardship` sections
with `Queried:` entries and `Stored:` or "nothing novel" entries.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the `impl Default` vs serde backing function divergence was a one-off
  delivery error specific to this feature. The pattern was correctly identified in gate-3b and resolved
  cleanly in a single rework commit. Does not rise to a recurring cross-feature pattern warranting a
  lesson-learned entry.
