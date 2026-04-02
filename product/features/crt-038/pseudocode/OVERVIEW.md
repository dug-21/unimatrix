# crt-038 Pseudocode Overview

## Feature Summary

Change production scoring formula defaults from the NLI-dominant configuration
(`w_nli=0.35`) to the empirically validated conf-boost-c profile (`w_sim=0.50,
w_conf=0.35`, all others 0.0). Add a correctness guard to `FusionWeights::effective()`
that prevents re-normalization when `w_nli == 0.0`. Surgically remove three dead NLI
code paths: post-store edge detection, bootstrap promotion, and the NLI auto-quarantine
guard.

## Component Map

| Wave | Component | Agent | File(s) |
|------|-----------|-------|---------|
| 1 | FusionWeights::effective() short-circuit | Wave 1a | `search.rs` |
| 1 | Config default constants | Wave 1b | `config.rs` |
| 2 | Dead-code removal (Components 3+4+5) | Wave 2 | `nli_detection.rs`, `store_ops.rs`, `mod.rs`, `background.rs` |

Wave 1 components are parallel (no shared files). Wave 2 must be a single agent
because components 3+4 both touch `nli_detection.rs` and components 4+5 both touch
`background.rs`.

## Implementation Ordering (mandatory, ADR-003)

```
Step 1: Wave 1a — effective() short-circuit (AC-02)
Step 2: Wave 1b — config defaults (AC-01)
Step 3: Eval gate — MRR >= 0.2913 on ass-039 harness (AC-12, blocking pre-merge)
Step 4: Wave 2 — dead-code removal (AC-03 through AC-09, AC-13, AC-14)
Step 5: cargo test --workspace && cargo clippy --workspace -- -D warnings
```

Steps 1 and 2 must be complete and `cargo test --workspace` passing BEFORE the eval
gate is run. An eval run on a build without the short-circuit produces invalid MRR.

## Data Flow After This Feature

```
InferenceConfig defaults (config.rs)
  w_sim=0.50, w_nli=0.00, w_conf=0.35, others=0.0
       |
       v
FusionWeights::from_config() --> FusionWeights { w_sim=0.50, w_nli=0.00, w_conf=0.35, ... }
       |
       v
FusionWeights::effective(nli_available=false)
       |
       +-- w_nli == 0.0? YES --> return self unchanged (new short-circuit)
       |                         w_sim=0.50, w_conf=0.35 exact (conf-boost-c formula)
       |
       v
compute_fused_score(inputs, weights)
  score = 0.50*sim + 0.00*nli + 0.35*conf + 0.02*phase_histogram + 0.05*phase_explicit
       |
       v
context_search / context_briefing ranking
```

## Shared Types — Modified Behavior, Unchanged Structure

### FusionWeights (search.rs)

Structure is unchanged. `effective()` method gains one new early-return branch.

```
struct FusionWeights {
    w_sim:             f64,   // 0.25 -> 0.50 (default via InferenceConfig)
    w_nli:             f64,   // 0.35 -> 0.00 (default via InferenceConfig)
    w_conf:            f64,   // 0.15 -> 0.35 (default via InferenceConfig)
    w_coac:            f64,   // 0.00 unchanged
    w_util:            f64,   // 0.05 -> 0.00 (default via InferenceConfig)
    w_prov:            f64,   // 0.05 -> 0.00 (default via InferenceConfig)
    w_phase_histogram: f64,   // 0.02 unchanged (additive, not in six-weight sum)
    w_phase_explicit:  f64,   // 0.05 unchanged (additive, not in six-weight sum)
}
```

### InferenceConfig (config.rs)

Field set is unchanged. Only the values returned by `default_w_*()` backing functions
change.

| Backing function | Old return | New return |
|-----------------|------------|------------|
| `default_w_sim()` | 0.25 | 0.50 |
| `default_w_nli()` | 0.35 | 0.00 |
| `default_w_conf()` | 0.15 | 0.35 |
| `default_w_util()` | 0.05 | 0.00 |
| `default_w_prov()` | 0.05 | 0.00 |
| `default_nli_enabled()` | true | false |

### process_auto_quarantine (background.rs)

Signature loses two parameters; internal NLI guard block is deleted.

```
// Before:
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    nli_enabled: bool,                      // DELETED
    nli_auto_quarantine_threshold: f32,     // DELETED
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

### Deleted Types

- `pub(crate) struct NliStoreConfig` (store_ops.rs) — entire struct + `impl Default`
- `enum NliQuarantineCheck` (background.rs) — 3 variants

## Symbols Deleted (grep-verify to zero before claiming ACs)

```
nli_detection.rs:
  pub async fn run_post_store_nli
  pub async fn maybe_run_bootstrap_promotion
  async fn run_bootstrap_promotion
  pub(crate) async fn write_edges_with_cap

store_ops.rs:
  pub(crate) struct NliStoreConfig
  impl Default for NliStoreConfig
  nli_cfg field on StoreService
  nli_cfg parameter on StoreService::new

mod.rs:
  use crate::services::store_ops::NliStoreConfig (import)
  nli_store_cfg construction block
  nli_store_cfg argument to StoreService::new

background.rs:
  enum NliQuarantineCheck
  async fn nli_auto_quarantine_allowed
  if nli_enabled { ... } guard block in process_auto_quarantine
  nli_enabled parameter from process_auto_quarantine signature
  nli_auto_quarantine_threshold parameter from process_auto_quarantine signature
  maybe_run_bootstrap_promotion import (line 49)
  two-line call site block at line 776
  stale sequencing comment at line 781
```

## Symbols Retained (must NOT be deleted — AC-13)

```
nli_detection.rs:
  pub(crate) async fn write_nli_edge        (line 532, imported by nli_detection_tick.rs:34)
  pub(crate) fn format_nli_metadata         (line 628, imported by nli_detection_tick.rs:34)
  pub(crate) fn current_timestamp_secs      (line 639, imported by nli_detection_tick.rs:34)
```

These three symbols are imported by the single cross-module use statement at
`nli_detection_tick.rs:34`:
```rust
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};
```

## Sequencing Constraints

- Wave 1a must complete before eval gate (AC-02 is a correctness precondition for AC-12)
- Wave 1b must complete before eval gate (AC-01 activates the new defaults)
- Wave 2 has no ordering dependency on Wave 1 functionally, but must be in the same PR
- Wave 2 is a single agent because `nli_detection.rs` and `background.rs` are each
  touched by multiple Wave 2 components

## Files Not Touched

- `crates/unimatrix-server/src/infra/nli_handle.rs`
- `crates/unimatrix-server/src/services/nli_detection_tick.rs`
- `crates/unimatrix-server/src/infra/contradiction.rs`
- `Cargo.toml` files (no new dependencies)
