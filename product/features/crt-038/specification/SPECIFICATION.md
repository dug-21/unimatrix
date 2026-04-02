# SPECIFICATION: crt-038 — conf-boost-c Formula and NLI Dead-Code Removal

## Objective

The production scoring formula defaults (`w_nli=0.35`) assign dominant weight to an NLI
cross-encoder that is task-mismatched against Unimatrix's structured knowledge corpus.
Research spikes ASS-035, ASS-037, and ASS-039 established that NLI contributes zero net
MRR lift; the conf-boost-c profile (`w_sim=0.50, w_conf=0.35`, all other weights zeroed)
outperforms production by +0.0031 MRR. This feature changes the formula defaults to
conf-boost-c and removes the three NLI code paths that are now dead: post-store NLI
detection, bootstrap edge promotion, and the NLI auto-quarantine guard.

---

## Functional Requirements

### Item 1 — Scoring Formula Defaults (config.rs)

- **FR-01**: Change `default_w_sim()` to return `0.50`.
- **FR-02**: Change `default_w_nli()` to return `0.00`.
- **FR-03**: Change `default_w_conf()` to return `0.35`.
- **FR-04**: Change `default_w_util()` to return `0.00`.
- **FR-05**: Change `default_w_prov()` to return `0.00`.
- **FR-06**: Change `default_nli_enabled()` to return `false`.
- **FR-07**: The six core weight defaults (`w_sim + w_nli + w_conf + w_coac + w_util + w_prov`)
  must sum to 0.85. The total including additive phase terms (`w_phase_histogram=0.02`,
  `w_phase_explicit=0.05`) must equal 0.92. `InferenceConfig::validate()` requires `sum ≤ 1.0`
  — this constraint continues to hold (0.85 ≤ 1.0).
- **FR-08**: `FusionWeights::effective()` in `search.rs` must be modified to short-circuit when
  `w_nli == 0.0`: return `self` unchanged regardless of `nli_available`. See precise
  specification in the Acceptance Criteria section (AC-02).

### Item 2 — Remove run_post_store_nli (nli_detection.rs / store_ops.rs)

- **FR-09**: Delete the `run_post_store_nli` public async function from `nli_detection.rs`
  (currently lines 39–185, approximately 147 lines including all internal steps and helper
  calls private to that function).
- **FR-10**: Remove from `store_ops.rs`:
  - The `use crate::services::nli_detection::run_post_store_nli` import (line 20).
  - The `tokio::spawn` block that calls `run_post_store_nli` (approximately line 312, ~20 lines).
  - The `NliStoreConfig` struct and its `Default` impl (currently lines 38–60).
  - The `nli_cfg: NliStoreConfig` field from `StoreService` (line 103).
  - The `nli_cfg: NliStoreConfig` parameter from `StoreService::new` (line 119).
  - The `use crate::services::store_ops::NliStoreConfig` import from `mod.rs` (line 26).
  - The `nli_store_cfg` construction block and the argument to `StoreService::new` in `mod.rs`
    (approximately lines 435–444).

### Item 3 — Remove maybe_run_bootstrap_promotion (nli_detection.rs / background.rs)

- **FR-11**: Delete the `maybe_run_bootstrap_promotion` public async function from
  `nli_detection.rs` (currently lines 197–274).
- **FR-12**: Delete the private `run_bootstrap_promotion` function from `nli_detection.rs`
  (called only by `maybe_run_bootstrap_promotion`; approximately 200 lines).
- **FR-13**: Remove from `background.rs`:
  - The `use crate::services::nli_detection::maybe_run_bootstrap_promotion` import (line 49).
  - The two-line call-site block inside the `if inference_config.nli_enabled { ... }` guard
    at approximately line 776.

### Item 4 — Remove NLI Auto-Quarantine Guard (background.rs)

- **FR-14**: Delete the `nli_auto_quarantine_allowed` private async function from `background.rs`
  (lines 1254–1290, approximately 37 lines).
- **FR-15**: Delete the `NliQuarantineCheck` enum from `background.rs` (lines 1233–1241, 3 variants:
  `Allowed`, `BlockedBelowThreshold`, `StoreError`).
- **FR-16**: Remove from `process_auto_quarantine` in `background.rs`:
  - The `nli_enabled: bool` parameter (line 1097).
  - The `nli_auto_quarantine_threshold: f32` parameter (line 1098).
  - The entire `if nli_enabled { ... }` guard block (lines 1124–1145).
- **FR-17**: Update the call site of `process_auto_quarantine` in `maintenance_tick` (line 946)
  to drop the `nli_enabled` and `nli_auto_quarantine_threshold` arguments.

### Item 5 — Test Cleanup

- **FR-18**: Delete all test functions that test the removed code paths. The complete required
  deletion list is specified in the Domain Model section under "Deleted Test Symbols."
- **FR-19**: Update all tests that assert old formula defaults. The required test updates are
  specified in the Domain Model section under "Modified Test Symbols."
- **FR-20**: All remaining tests for retained code paths must pass without modification,
  except for updating expected default values where required.

---

## Non-Functional Requirements

- **NFR-01**: `cargo test --workspace` must pass with zero failures after all changes.
- **NFR-02**: `cargo clippy --workspace -- -D warnings` must pass with zero warnings.
- **NFR-03**: `cargo audit` must pass with no new CVEs. No new dependencies may be introduced.
- **NFR-04**: `cargo fmt` must be applied before commit.
- **NFR-05**: No file introduced or modified by this feature may exceed 500 lines. The 500-line
  limit applies to files touched by this feature. `background.rs` (currently 4,229 lines) and
  `nli_detection.rs` (currently 1,373 lines) are pre-existing over-limit violations — this
  feature removes lines from both but does not resolve the pre-existing violations. No gate
  failure on pre-existing over-limit files.
- **NFR-06 (AC-12 — blocking pre-merge gate)**: An eval run on
  `product/research/ass-039/harness/scenarios.jsonl` must be performed after AC-01 and AC-02
  are implemented and tested. The run must produce MRR ≥ 0.2913. The eval command and its
  full output must be attached to the PR description before merge is permitted.

---

## Acceptance Criteria

### Formula Change

**AC-01** — Default weight values verified.

Verification: `InferenceConfig::default()` produces exactly:
```
w_sim  = 0.50
w_nli  = 0.00
w_conf = 0.35
w_coac = 0.00  (unchanged from crt-032)
w_util = 0.00
w_prov = 0.00
```
Test `test_inference_config_weight_defaults_when_absent` must be updated to assert these values.
Test `test_fusion_weights_default_sum_unchanged_by_crt030` must be updated: the expected sum
changes from `0.92` to `0.92` (unchanged total — `0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 +
0.02 + 0.05 = 0.92`). The assertion message must be updated to reference crt-038.

**AC-02** — `FusionWeights::effective()` short-circuit when `w_nli == 0.0`.

This is a correctness requirement. The current re-normalization logic divides each non-NLI
weight by the non-NLI denominator. When `w_nli == 0.0`, the denominator is
`w_sim + w_conf + w_coac + w_util + w_prov = 0.85` (for conf-boost-c defaults), producing
scaled-up effective weights (`w_sim' ≈ 0.588, w_conf' ≈ 0.412`). This diverges from the
formula evaluated in ASS-039 and is a correctness error — redistributing zero weight is
semantically meaningless.

Required behavior: modify `FusionWeights::effective(nli_available: bool)` in `search.rs` to
add a short-circuit guard before the existing `nli_available` branch:

```
if self.w_nli == 0.0 {
    return FusionWeights { ..*self };  // or equivalent field copy
}
```

This guard must execute before the `if nli_available` branch. When `w_nli == 0.0`:
- `effective(true)` must return weights unchanged (same as existing `nli_available=true` path).
- `effective(false)` must return weights unchanged (new behavior; currently would re-normalize).

When `w_nli > 0.0`, existing behavior is preserved:
- `effective(true)` returns weights unchanged.
- `effective(false)` zeros `w_nli` and re-normalizes the remaining five core weights by their sum.

Two new unit tests are required:
- `test_effective_short_circuit_w_nli_zero_nli_available_true`: assert `effective(true)` on a
  `FusionWeights` with `w_nli=0.0` returns weights identical to input.
- `test_effective_short_circuit_w_nli_zero_nli_available_false`: assert `effective(false)` on
  a `FusionWeights` with `w_nli=0.0` returns weights identical to input (no re-normalization).

**AC-01 and AC-02 must be implemented and their tests passing before the eval gate (AC-12) is
run.** This is an ordering constraint — see Ordering Constraint section.

### Post-Store NLI Removal

**AC-03** — `run_post_store_nli` is deleted.

Verification: `grep -r "run_post_store_nli" crates/` returns zero matches.

**AC-04** — `store_ops.rs` spawn block removed.

Verification: `grep -r "tokio::spawn.*nli\|run_post_store_nli" crates/unimatrix-server/src/services/store_ops.rs` returns zero matches.

**AC-14** — `NliStoreConfig` deleted entirely.

`NliStoreConfig` and all its fields (`enabled`, `nli_post_store_k`, `nli_entailment_threshold`,
`nli_contradiction_threshold`, `max_contradicts_per_tick`) are exclusively consumed by the
removed NLI spawn block. The struct must be deleted from `store_ops.rs` and the
`use crate::services::store_ops::NliStoreConfig` import removed from `mod.rs`. No dead fields
may be retained.

Verification: `grep -r "NliStoreConfig" crates/` returns zero matches.

### Bootstrap Promotion Removal

**AC-05** — `maybe_run_bootstrap_promotion` and `run_bootstrap_promotion` are deleted.

Verification: `grep -r "maybe_run_bootstrap_promotion\|run_bootstrap_promotion" crates/` returns zero matches.

**AC-06** — `background.rs` import and call site removed.

Verification: `grep -n "maybe_run_bootstrap_promotion" crates/unimatrix-server/src/background.rs` returns zero matches.

### Auto-Quarantine NLI Guard Removal

**AC-07** — `nli_auto_quarantine_allowed` and `NliQuarantineCheck` deleted.

Verification: `grep -r "nli_auto_quarantine_allowed\|NliQuarantineCheck" crates/` returns zero matches.

**AC-08** — `process_auto_quarantine` signature updated.

The function signature must be:
```rust
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
) -> Vec<u64>
```

Call site in `maintenance_tick` must be updated to omit `nli_enabled` and
`nli_auto_quarantine_threshold` arguments.

Verification: Function compiles without `nli_enabled` or `nli_auto_quarantine_threshold` parameters.

### Test Coverage

**AC-09** — All tests for removed code paths deleted; all tests for retained paths pass.

Delivery must run `grep` against each removed symbol before marking AC-09 complete. The
complete symbol checklist is in the Domain Model section.

**AC-10** — `cargo test --workspace` passes with zero failures.

**AC-11** — `cargo clippy --workspace -- -D warnings` passes with zero warnings.

### Eval Gate

**AC-12** — MRR ≥ 0.2913 on the behavioral ground truth harness. Blocking pre-merge gate.

Eval harness path: `product/research/ass-039/harness/scenarios.jsonl` (1,585 scenarios).

Precondition: AC-01 and AC-02 must be implemented and their unit tests passing before this
eval is run (ordering constraint).

The PR description must include:
1. The exact eval command used.
2. Full terminal output of the eval run, including the MRR value produced.
3. Confirmation that the run was performed against the production server with the new defaults
   active (not a test instance with overridden weights).

Merge is blocked until this output is attached.

### Compilation Safety

**AC-13** — `nli_detection.rs` shared helpers retained; no cross-module compilation breakage.

The following symbols must remain in `nli_detection.rs` after removal of the three functions,
because `nli_detection_tick.rs` imports them directly (verified at line 34):

```rust
// nli_detection_tick.rs line 34:
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};
```

Required retained symbols:
- `pub(crate) async fn write_nli_edge` (line 532) — used by `nli_detection_tick.rs`
- `pub(crate) fn format_nli_metadata` (line 628) — used by `nli_detection_tick.rs`
- `pub(crate) fn current_timestamp_secs` (line 639) — used by `nli_detection_tick.rs`

These symbols must not be deleted or made private. Verify: `cargo build --workspace` succeeds
after all removals.

---

## Ordering Constraint

**The formula change (FR-01 through FR-08, AC-01, AC-02) must be implemented and all
associated tests passing before the eval gate (AC-12) is executed.**

Rationale (SR-02): If AC-02 (the `effective()` short-circuit) is missing when the eval is run,
the scoring path produces re-normalized weights (`w_sim'≈0.588, w_conf'≈0.412`) rather than
the conf-boost-c formula evaluated in ASS-039. An eval run on the wrong scoring path would
produce an invalid MRR comparison against the 0.2913 baseline.

Delivery sequence:
1. Implement FR-01 through FR-08 (config defaults + `effective()` short-circuit).
2. Pass `cargo test --workspace` (confirms AC-01, AC-02, and no formula regressions).
3. Execute eval run; confirm MRR ≥ 0.2913 (AC-12).
4. Implement FR-09 through FR-20 (dead-code removals and test cleanup) in any order.
5. Pass final `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` (AC-10, AC-11).

---

## Domain Model

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| conf-boost-c | The scoring formula profile with `w_sim=0.50, w_conf=0.35`, all other weights 0.0. Established as the optimal configuration by ASS-039 ablation study (1,585 behavioral scenarios, MRR=0.2913). |
| FusionWeights | Struct in `search.rs` holding the six core scoring weights plus two additive phase terms. Used by `compute_fused_score` for every search candidate. |
| effective() | Method on `FusionWeights` that adjusts weights for NLI availability. When `nli_available=true`, returns weights unchanged. When `nli_available=false`, was intended to redistribute `w_nli` to remaining weights. After this feature: short-circuits when `w_nli == 0.0`. |
| nli_available | Boolean passed to `effective()` indicating whether NLI scores were produced for this query. Set to `nli_scores.is_some()` in `SearchService`. |
| run_post_store_nli | Async function (dead code) that fires NLI inference after `context_store`. To be deleted. |
| maybe_run_bootstrap_promotion | Async function (dead code) that runs one-shot NLI bootstrap promotion. To be deleted. |
| process_auto_quarantine | Async function in `background.rs` that quarantines ineffective entries. Loses its NLI guard parameters in this feature. |
| NliStoreConfig | Struct (dead code) holding NLI config fields for the post-store NLI spawn block. To be deleted entirely. |
| NliQuarantineCheck | Enum (dead code, 3 variants) returned by the NLI auto-quarantine guard. To be deleted. |
| shared helpers | Three `pub(crate)` symbols in `nli_detection.rs` that `nli_detection_tick.rs` depends on: `write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`. Must be retained. |
| MRR | Mean Reciprocal Rank. Eval metric. Baseline from ASS-039: 0.2913 on conf-boost-c profile. |

### Deleted Symbols

The following symbols must be fully removed. Delivery must grep-verify each before marking
AC-09, AC-13, AC-14 complete.

**nli_detection.rs (deleted functions):**
- `pub async fn run_post_store_nli` — and all code paths within it
- `pub async fn maybe_run_bootstrap_promotion`
- private `run_bootstrap_promotion` — called only by `maybe_run_bootstrap_promotion`

**store_ops.rs (deleted struct and fields):**
- `pub(crate) struct NliStoreConfig` — entire struct
- Fields: `enabled`, `nli_post_store_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`, `max_contradicts_per_tick`
- `impl Default for NliStoreConfig`
- `nli_cfg: NliStoreConfig` field on `StoreService`
- `nli_cfg: NliStoreConfig` parameter on `StoreService::new`

**background.rs (deleted functions and enum):**
- `async fn nli_auto_quarantine_allowed`
- `enum NliQuarantineCheck` (variants: `Allowed`, `BlockedBelowThreshold`, `StoreError`)
- `if nli_enabled { ... }` guard block inside `process_auto_quarantine`
- Parameters `nli_enabled: bool` and `nli_auto_quarantine_threshold: f32` from `process_auto_quarantine`

**mod.rs (deleted imports and construction):**
- `use crate::services::store_ops::NliStoreConfig`
- `nli_store_cfg` construction block
- `nli_store_cfg` argument to `StoreService::new`

### Deleted Test Symbols

The following test functions must be removed. Delivery must grep-verify each.

**nli_detection.rs (13 test functions in total; all test removed code paths):**
- `test_empty_embedding_skips_nli`
- `test_nli_not_ready_exits_immediately`
- `test_circuit_breaker_stops_at_cap`
- `test_circuit_breaker_counts_all_edge_types`
- `test_bootstrap_promotion_zero_rows_sets_marker`
- `test_maybe_bootstrap_promotion_skips_if_marker_present`
- `test_maybe_bootstrap_promotion_defers_when_nli_not_ready`
- `test_bootstrap_promotion_confirms_above_threshold`
- `test_bootstrap_promotion_refutes_below_threshold`
- `test_bootstrap_promotion_idempotent_second_run_no_duplicates`
- `test_bootstrap_promotion_nli_inference_runs_on_rayon_thread`

(The remaining 2 of the 13 declared test functions are covered by the above list. Delivery
must confirm all 13 test functions that reference removed symbols are removed.)

**background.rs (4 integration tests):**
- `test_nli_edges_below_auto_quarantine_threshold_no_quarantine`
- `test_nli_edges_above_threshold_allow_quarantine`
- `test_nli_auto_quarantine_mixed_penalty_allowed`
- `test_nli_auto_quarantine_no_edges_allowed`

### Modified Test Symbols

The following test functions must be updated in place (not deleted):

**search.rs:**
- `test_fusion_weights_default_sum_unchanged_by_crt030`: Update the expected sum assertion message
  to reference crt-038. The expected value `0.92` is unchanged
  (`0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 + 0.02 + 0.05 = 0.92`).

**config.rs:**
- `test_inference_config_weight_defaults_when_absent`: Update to assert new defaults
  (`w_sim=0.50, w_nli=0.00, w_conf=0.35, w_util=0.00, w_prov=0.00, nli_enabled=false`).
- `test_inference_config_default_weights_sum_within_headroom`: The assertion `sum ≤ 0.95` still
  holds (0.85 ≤ 0.95); update only if the test asserts an exact old value.

### Retained Symbols (Not Modified by This Feature)

The following symbols are explicitly retained and must not be deleted or altered beyond what
falls out of the changes above:

**nli_detection.rs (retained):**
- `pub(crate) async fn write_nli_edge` — shared with `nli_detection_tick.rs`
- `pub(crate) fn format_nli_metadata` — shared with `nli_detection_tick.rs`
- `pub(crate) fn current_timestamp_secs` — shared with `nli_detection_tick.rs`
- All supporting imports and types required by the above three functions

**nli_detection_tick.rs (untouched):**
- `run_graph_inference_tick` and all supporting code — Group 2 tick decomposition (separate feature)

**config.rs (retained fields, default only changes):**
- `nli_enabled` field on `InferenceConfig` — used by `run_graph_inference_tick` gate in
  `background.rs` and by `SearchService.nli_enabled`; only the default value changes
- `nli_model_name`, `nli_model_path`, `nli_model_sha256`, `nli_top_k`, `nli_post_store_k`,
  `nli_entailment_threshold`, `nli_contradiction_threshold`, `max_contradicts_per_tick`,
  `nli_auto_quarantine_threshold` — all retained on `InferenceConfig` for operator use

**search.rs (retained):**
- `try_nli_rerank` — still valid; gated behind `self.nli_enabled`; no changes to this function

**contradiction.rs (untouched):**
- Heuristic-based contradiction scan; no NLI dependency

**nli_handle.rs (untouched):**
- `NliServiceHandle`, `NliConfig`, and the loading state machine

---

## User Workflows

### Agent Querying Context (context_search / context_briefing)

After this feature, every query issued by an agent is scored with:
- `w_sim=0.50` (cosine similarity with embedding)
- `w_conf=0.35` (confidence composite)
- All other weights 0.0

No NLI inference is invoked during search (`nli_enabled=false` default; `try_nli_rerank`
is gated behind that flag). The `effective()` call in `SearchService` receives
`nli_available = nli_scores.is_some() = false`; the new short-circuit returns weights
unchanged (no re-normalization).

### Background Tick

The maintenance tick continues to call `run_graph_inference_tick` (retained, Group 2).
`maybe_run_bootstrap_promotion` is no longer called. `process_auto_quarantine` runs
without NLI guard parameters; all quarantine candidates that meet the cycle threshold
proceed directly to quarantine.

### Store Operation (context_store)

After storing an entry, `store_ops.rs` no longer spawns a `tokio::spawn` NLI task.
The MCP response is returned as before.

---

## Constraints

1. `nli_enabled` config field is retained on `InferenceConfig`; only the default changes to
   `false`. Operators can still set `nli_enabled=true` in a config file.
2. The 500-line file limit applies to files created or substantially rewritten by this feature.
   `background.rs` (4,229 lines) and `nli_detection.rs` (1,373 lines) are pre-existing
   over-limit violations. Removal of lines from these files does not require resolving the
   pre-existing violation; it is tracked separately.
3. No schema changes, data migrations, or COUNTERS table modifications.
4. No changes to `contradiction.rs`, `nli_handle.rs`, `nli_detection_tick.rs`.
5. No changes to `try_nli_rerank` in `search.rs` beyond what falls out of the
   `nli_enabled=false` default.
6. The eval baseline (MRR=0.2913) was established by ASS-039 on 1,585 behavioral scenarios.
   The eval must be run on the same harness file (`product/research/ass-039/harness/scenarios.jsonl`)
   after AC-02 is implemented, to confirm the scoring path matches the evaluated formula.

---

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| `search.rs` | Modified | `FusionWeights::effective()` short-circuit for `w_nli==0.0` |
| `config.rs` | Modified | Six `default_w_*()` functions and `default_nli_enabled()` |
| `nli_detection.rs` | Modified (deletions) | Remove 3 functions; retain 3 shared helpers |
| `store_ops.rs` | Modified (deletions) | Remove `NliStoreConfig`, spawn block, import |
| `background.rs` | Modified (deletions) | Remove import, call site, guard block, enum, helper |
| `services/mod.rs` | Modified (deletions) | Remove `NliStoreConfig` import and construction |
| `product/research/ass-039/harness/scenarios.jsonl` | Eval input | 1,585 behavioral scenarios for AC-12 gate |

No new crate dependencies. No `Cargo.toml` changes.

---

## NOT in Scope

- Removal of `run_graph_inference_tick` or any restructuring of the NLI gate around it
  (Group 2 tick decomposition — separate feature).
- Removal of `NliServiceHandle`, `NliConfig`, or NLI model loading infrastructure.
- Removal of the `nli_enabled` config field (only the default value changes).
- Cosine Supports detection replacement (Group 3 feature).
- Any change to `try_nli_rerank` in `search.rs` beyond the `nli_enabled=false` default effect.
- Any schema change or data migration.
- Changes to `contradiction.rs`.
- Merging or renaming `nli_detection.rs` (deferred to Group 2 tick decomposition).
- Resolving the pre-existing 500-line violation in `background.rs` or `nli_detection.rs`.
- Removal of any `InferenceConfig` fields (`nli_model_name`, `nli_auto_quarantine_threshold`,
  etc.) — retained for operator use.

---

## Open Questions

1. **ASS-039 eval scoring path assumption**: The risk assessment (SCOPE-RISK-ASSESSMENT.md §Assumptions)
   notes the ASS-039 eval may have been run with `nli_enabled=true, w_nli=0.0` (effective(true) path,
   no re-normalization). If it was run via `nli_enabled=false` on a build without the AC-02
   short-circuit, the baseline MRR=0.2913 was measured on the re-normalized formula
   (`w_sim'≈0.588, w_conf'≈0.412`), not the direct conf-boost-c formula. Delivery must confirm
   which code path the ASS-039 eval used before treating the MRR baseline as valid for AC-12.
   Resolution: check ASS-039 harness configuration or commit hash.

2. **nli_detection.rs post-removal line count**: After removing the three functions (~500+ lines
   of code and tests), the file may approach or drop below 200 lines. At that point, merging
   remaining shared helpers into `nli_detection_tick.rs` may be the cleaner boundary. SCOPE.md
   defers this decision to Group 2. Delivery should note the post-removal line count in the PR.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 16 entries; top matches: entry #3985
  (ASS-037 NLI infrastructure audit verdict table), entry #4003 (FusionWeights::effective(false)
  re-normalization pattern when w_nli zeroed), entry #3986 (background tick decomposition ADR).
  Entry #4003 directly confirmed the re-normalization problem specified in AC-02.
