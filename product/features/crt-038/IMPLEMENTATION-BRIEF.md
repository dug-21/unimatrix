# crt-038 Implementation Brief — conf-boost-c Formula and NLI Dead-Code Removal

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-038/SCOPE.md |
| Architecture | product/features/crt-038/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-038/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-038/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-038/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| FusionWeights::effective() short-circuit | pseudocode/effective-short-circuit.md | test-plan/effective-short-circuit.md |
| config.rs default weight constants | pseudocode/config-defaults.md | test-plan/config-defaults.md |
| run_post_store_nli removal | pseudocode/post-store-nli-removal.md | test-plan/post-store-nli-removal.md |
| maybe_run_bootstrap_promotion removal | pseudocode/bootstrap-promotion-removal.md | test-plan/bootstrap-promotion-removal.md |
| NLI auto-quarantine guard removal | pseudocode/auto-quarantine-removal.md | test-plan/auto-quarantine-removal.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture — actual file paths are filled during delivery.

---

## Goal

Change the production scoring formula defaults from the NLI-dominant configuration
(`w_nli=0.35`) to the empirically validated conf-boost-c profile (`w_sim=0.50,
w_conf=0.35`, all others 0.0), and surgically remove three NLI code paths that are
operationally dead: post-store edge detection, bootstrap promotion, and the NLI
auto-quarantine guard. The formula change requires a correctness fix to
`FusionWeights::effective()` (AC-02 short-circuit) before the eval gate is run,
ensuring the effective weights match the ASS-039 evaluated formula that produced
MRR=0.2913.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| `FusionWeights::effective()` re-normalization when `w_nli==0.0` produces skewed weights (`w_sim'≈0.588, w_conf'≈0.412`) | Add short-circuit guard as first branch: `if self.w_nli == 0.0 { return *self; }`. Re-normalization is semantically meaningful only when `w_nli > 0.0`. Zero-NLI re-normalization is a correctness error, not an optimization. | SR-01, AC-02, ADR-001 | architecture/ADR-001-effective-zero-nli-short-circuit.md |
| `NliStoreConfig` field retention — SCOPE.md Background implied partial retention while AC-14 requires full deletion | AC-14 is authoritative: delete `NliStoreConfig` entirely from `store_ops.rs`. `InferenceConfig` retains same-named fields independently for `run_graph_inference_tick` and operator use. No partial retention. | SR-04, ADR-002 | architecture/ADR-002-nlistoreconfig-complete-deletion.md |
| Implementation ordering — formula change vs. eval gate vs. dead-code removal | Mandatory sequence: Step 1 = AC-02 (effective() short-circuit), Step 2 = AC-01 (config defaults), Step 3 = AC-12 (eval gate, blocking), Step 4 = Group B dead-code removals (any order), Step 5 = cargo test + clippy. Eval run before Step 1 produces invalid MRR comparison. | SR-02, ADR-003 | architecture/ADR-003-implementation-ordering.md |
| Module merge of `nli_detection.rs` shared helpers into `nli_detection_tick.rs` | Deferred to Group 2 tick decomposition (separate feature). File name and path unchanged. `write_edges_with_cap` deleted (no callers after removal). Three retained `pub(crate)` symbols remain at current locations. | ADR-004 | architecture/ADR-004-nli-detection-module-merge-deferred.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/services/search.rs` | Modify | Add `w_nli==0.0` short-circuit to `FusionWeights::effective()` as first branch; add two new unit tests (AC-02); update `test_fusion_weights_default_sum_unchanged_by_crt030` assertion message |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Change six `default_w_*()` functions and `default_nli_enabled()`; update `test_inference_config_weight_defaults_when_absent` and `test_inference_config_default_weights_sum_within_headroom` |
| `crates/unimatrix-server/src/services/nli_detection.rs` | Modify (deletions) | Delete `run_post_store_nli`, `maybe_run_bootstrap_promotion`, `run_bootstrap_promotion`, `write_edges_with_cap`; retain `write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`; remove 13 test functions; update module-level doc comment |
| `crates/unimatrix-server/src/services/store_ops.rs` | Modify (deletions) | Delete `NliStoreConfig` struct + `impl Default`; remove `nli_cfg` field from store ops context; remove `run_post_store_nli` import; remove `tokio::spawn` NLI block (~20 lines) |
| `crates/unimatrix-server/src/services/mod.rs` | Modify (deletions) | Remove `use crate::services::store_ops::NliStoreConfig` import; remove `nli_store_cfg` construction block and its argument to `StoreService::new` |
| `crates/unimatrix-server/src/background.rs` | Modify (deletions) | Remove `maybe_run_bootstrap_promotion` import + two-line call site; delete `NliQuarantineCheck` enum; delete `nli_auto_quarantine_allowed` fn; remove `nli_enabled` and `nli_auto_quarantine_threshold` params from `process_auto_quarantine`; update call site in `maintenance_tick`; remove stale sequencing comment; remove 4 integration tests |

---

## Data Structures

### FusionWeights (search.rs — modified behavior, not structure)

```rust
// Unchanged struct fields. Changed: effective() method behavior when w_nli == 0.0.
pub struct FusionWeights {
    pub w_sim:             f64,
    pub w_nli:             f64,
    pub w_conf:            f64,
    pub w_coac:            f64,
    pub w_util:            f64,
    pub w_prov:            f64,
    pub w_phase_histogram: f64,
    pub w_phase_explicit:  f64,
}
```

### InferenceConfig defaults (config.rs — changed values only)

| Field | Old default | New default |
|-------|-------------|-------------|
| `w_sim` | 0.25 | 0.50 |
| `w_nli` | 0.35 | 0.00 |
| `w_conf` | 0.15 | 0.35 |
| `w_util` | 0.05 | 0.00 |
| `w_prov` | 0.05 | 0.00 |
| `w_coac` | 0.00 | 0.00 (unchanged) |
| `nli_enabled` | true | false |

Sum of six core weights: 0.85. Total with additive phase terms: 0.92. `validate()` constraint `sum ≤ 1.0` continues to hold.

### process_auto_quarantine signature (background.rs — changed)

```rust
// Before:
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    nli_enabled: bool,                      // REMOVED
    nli_auto_quarantine_threshold: f32,     // REMOVED
) -> Vec<u64>

// After:
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
) -> Vec<u64>
```

---

## Function Signatures

### FusionWeights::effective() — modified (search.rs)

```rust
// New short-circuit as the first branch:
pub fn effective(&self, nli_available: bool) -> FusionWeights {
    if self.w_nli == 0.0 {
        return *self;   // No weight to redistribute; return unchanged
    }
    // Existing nli_available=true fast-path and re-normalization follow unchanged
    if nli_available {
        return *self;
    }
    // ... existing re-normalization logic (w_nli > 0.0 case only) ...
}
```

### Symbols deleted — grep-verify each to zero before marking ACs complete

```
pub async fn run_post_store_nli        (nli_detection.rs)
pub async fn maybe_run_bootstrap_promotion  (nli_detection.rs)
async fn run_bootstrap_promotion       (nli_detection.rs)
async fn write_edges_with_cap          (nli_detection.rs — dead after run_post_store_nli removal)
pub(crate) struct NliStoreConfig       (store_ops.rs)
impl Default for NliStoreConfig        (store_ops.rs)
async fn nli_auto_quarantine_allowed   (background.rs)
enum NliQuarantineCheck                (background.rs)
```

### Symbols retained — must NOT be deleted (AC-13)

```
pub(crate) async fn write_nli_edge         (nli_detection.rs:532)
pub(crate) fn format_nli_metadata          (nli_detection.rs:628)
pub(crate) fn current_timestamp_secs       (nli_detection.rs:639)
```

These three symbols are imported by `nli_detection_tick.rs` line 34:
```rust
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};
```

---

## Constraints

1. `nli_enabled` field on `InferenceConfig` is retained — only the default changes to `false`. Operators can still override to `true` in a config file.
2. No changes to `contradiction.rs`, `nli_handle.rs`, `nli_detection_tick.rs`, or `try_nli_rerank` in `search.rs`.
3. No schema changes, data migrations, or `Cargo.toml` changes.
4. 500-line file limit applies to files newly created by this feature. `background.rs` (4,229 lines) and `nli_detection.rs` (1,373 lines) are pre-existing over-limit violations — this feature removes lines from both but does not resolve the pre-existing violation. NFR-05 explicitly exempts pre-existing over-limit files from gate failure (SR-07 accepted).
5. `InferenceConfig` fields `nli_post_store_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`, `max_contradicts_per_tick`, `nli_auto_quarantine_threshold`, `nli_model_name`, `nli_model_path`, `nli_model_sha256`, `nli_top_k` are all retained for operator use and `run_graph_inference_tick`.
6. `w_nli == 0.0` short-circuit uses exact f64 equality. Safe because `default_w_nli()` returns a constant literal `0.0`, not a computed value.
7. `cargo audit` must pass; no new dependencies introduced.

---

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| `crates/unimatrix-server/src/services/search.rs` | In-crate | `FusionWeights::effective()` modification |
| `crates/unimatrix-server/src/infra/config.rs` | In-crate | Six `default_w_*()` functions + `default_nli_enabled()` |
| `crates/unimatrix-server/src/services/nli_detection.rs` | In-crate | Deletions of three functions + `write_edges_with_cap`; three shared helpers retained |
| `crates/unimatrix-server/src/services/store_ops.rs` | In-crate | `NliStoreConfig` deletion + spawn block removal |
| `crates/unimatrix-server/src/services/mod.rs` | In-crate | Import + constructor site cleanup |
| `crates/unimatrix-server/src/background.rs` | In-crate | Import, call site, enum, fn, signature removal |
| `product/research/ass-039/harness/scenarios.jsonl` | Eval input | 1,585 behavioral scenarios for AC-12 blocking gate |

No new crate dependencies. No `Cargo.toml` changes.

---

## NOT in Scope

- Removal of `run_graph_inference_tick` or restructuring of the NLI gate (Group 2 tick decomposition).
- Removal of `NliServiceHandle`, `NliConfig`, or NLI model loading infrastructure.
- Removal of the `nli_enabled` config field (default value changes only).
- Cosine Supports detection replacement (Group 3 feature).
- Any change to `try_nli_rerank` in `search.rs` beyond what falls out of `nli_enabled=false` default.
- Any schema changes or data migrations.
- Changes to `contradiction.rs`.
- Merging or renaming `nli_detection.rs` (deferred to Group 2).
- Resolving pre-existing 500-line violations in `background.rs` or `nli_detection.rs`.
- Removal of any `InferenceConfig` fields (`nli_model_name`, `nli_auto_quarantine_threshold`, etc.).

---

## Critical Delivery Constraints

### Mandatory Implementation Order (ADR-003)

AC-02 before AC-12. Do not run the eval gate until the `effective()` short-circuit is
implemented and `cargo test --workspace` passes.

```
Step 1: Implement effective() short-circuit (AC-02) → Step 2: Change config defaults
(AC-01) → Step 3: Run eval gate (AC-12, blocking pre-merge) → Step 4: Dead-code
removals in any order (AC-03–AC-09, AC-13, AC-14) → Step 5: cargo test + clippy (AC-10,
AC-11)
```

### Vision Guardian WARN: write_edges_with_cap

`write_edges_with_cap` deletion is mandated by R-05 and AC-11 (clippy gate) but is
absent from the ARCHITECTURE.md Integration Surface table. Delivery must treat its
deletion as required: after `run_post_store_nli` is removed, `write_edges_with_cap` has
zero callers and `cargo clippy --workspace -- -D warnings` will fail. Delete it
alongside `run_post_store_nli`. Grep-verify: `grep -r "write_edges_with_cap" crates/`
returns zero matches after deletion.

### Vision Guardian WARN: Third AC-02 Unit Test

RISK-TEST-STRATEGY.md R-01 scenario 3 adds a third unit test not named in SCOPE.md or
SPECIFICATION.md:

```
test_effective_renormalization_still_fires_when_w_nli_positive
```

This test constructs `FusionWeights` with `w_nli=0.20`, calls `effective(false)`, and
asserts re-normalization occurs — verifying the short-circuit guard does not suppress
the `w_nli > 0.0` path. Delivery must include this test. AC-02 coverage is incomplete
without it.

### Required Test Additions (AC-02)

Three unit tests required in `search.rs` (two from spec, one from risk strategy):

1. `test_effective_short_circuit_w_nli_zero_nli_available_false` — `effective(false)` with `w_nli=0.0` returns weights unchanged.
2. `test_effective_short_circuit_w_nli_zero_nli_available_true` — `effective(true)` with `w_nli=0.0` returns weights unchanged.
3. `test_effective_renormalization_still_fires_when_w_nli_positive` — `effective(false)` with `w_nli=0.20` applies re-normalization (guard must not fire).

### Required Test Updates (not deletions)

| Test | File | Change |
|------|------|--------|
| `test_inference_config_weight_defaults_when_absent` | config.rs | Assert new defaults: `w_sim=0.50, w_nli=0.00, w_conf=0.35, w_util=0.00, w_prov=0.00, nli_enabled=false` |
| `test_inference_config_default_weights_sum_within_headroom` | config.rs | Update if it asserts an exact old value; `sum ≤ 0.95` still holds (0.85 ≤ 0.95) |
| `test_fusion_weights_default_sum_unchanged_by_crt030` | search.rs | Update assertion message to reference crt-038; expected sum 0.92 is unchanged |

### Deleted Test Symbols (grep-verify to zero — AC-09)

**nli_detection.rs (13 functions):**
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
- (2 additional test functions covering `run_post_store_nli` — confirm all 13 present in spec deleted)

**background.rs (4 integration tests):**
- `test_nli_edges_below_auto_quarantine_threshold_no_quarantine`
- `test_nli_edges_above_threshold_allow_quarantine`
- `test_nli_auto_quarantine_mixed_penalty_allowed`
- `test_nli_auto_quarantine_no_edges_allowed`

---

## Eval Gate (AC-12 — Blocking Pre-Merge)

- Harness: `product/research/ass-039/harness/scenarios.jsonl` (1,585 scenarios)
- Gate: MRR ≥ 0.2913
- Precondition: AC-01 and AC-02 must be implemented and their tests passing before eval is run (ADR-003)
- PR description must include: exact eval command used, full terminal output including MRR value, git commit hash at time of run (confirms hash is post-AC-02), confirmation that the run used the production server with new defaults active (not a test instance with overridden weights)
- Merge is blocked until all four items above are in the PR description

**ASS-039 baseline path validation (R-03):** ADR-001 documents that ASS-039 was run
with `nli_enabled=true` and `w_nli=0.0` (effective(true) path, weights unchanged). The
PR description must state which scoring path the baseline was measured on and include
supporting evidence (harness config or commit hash). If delivery cannot confirm the
baseline was on the effective(true) path, a new baseline eval must be run before AC-12
can gate anything.

---

## Alignment Status

**Overall: PASS. Two WARNs for delivery awareness — no blockers, no variances requiring approval.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly corrects WA-0 formula using the eval harness (W1-3) that the vision mandates as the intelligence gate |
| Milestone Fit | PASS | Cortical phase Wave 1A correction; no Wave 2 or Wave 3 work included |
| Architecture Consistency | PASS | ADR-001 through ADR-004 cross-reference cleanly across all artifacts |
| Scope Additions | PASS | No out-of-scope work in any source document |
| write_edges_with_cap | WARN | Deletion mandated by R-05/AC-11 but absent from ARCHITECTURE.md Integration Surface table — called out explicitly in this brief; delivery must delete it |
| Third AC-02 test | WARN | RISK-TEST-STRATEGY.md R-01 adds `test_effective_renormalization_still_fires_when_w_nli_positive` not named in SCOPE.md or SPECIFICATION.md — included in this brief's test requirements; delivery must add it |
| R-03 resolution | WARN | Eval baseline validity (ASS-039 scoring path) is a procedural gate only — no automated backstop; PR description evidence is mandatory and reviewer must not approve without it |
