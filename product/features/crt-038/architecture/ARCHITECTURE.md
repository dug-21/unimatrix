# crt-038: conf-boost-c Formula and NLI Dead-Code Removal — Architecture

## System Overview

Unimatrix's `context_search` and `context_briefing` tools rank candidates via a
fused scoring formula implemented in `FusionWeights` (search.rs). The formula
combines six weighted signals: similarity, NLI entailment, confidence, co-access,
utilization, and provenance.

Research spikes ASS-035/037/039 established that the NLI cross-encoder signal
(`w_nli`) contributes zero net MRR lift on the Unimatrix corpus. The production
default of `w_nli=0.35` is the dominant weight despite having no measured value.
The conf-boost-c configuration (`w_sim=0.50, w_conf=0.35, all others 0.00`)
outperforms production by +0.0031 MRR.

This feature changes the formula defaults and surgically removes three NLI code
paths that have been operationally dead: post-store edge detection, bootstrap
promotion, and the NLI auto-quarantine guard. The retained NLI infrastructure
(`run_graph_inference_tick`, `NliServiceHandle`, `try_nli_rerank`) is not
touched.

## Component Breakdown

### Component 1: FusionWeights::effective() — search.rs

**Current behaviour**: when `nli_available=false`, the function zeros `w_nli` and
re-normalizes the remaining five weights by dividing each by their sum. With the
new defaults (`w_nli=0.00`, others summing to 0.85) and `nli_enabled=false`, the
re-normalization path produces `w_sim'≈0.588, w_conf'≈0.412` — NOT the intended
conf-boost-c formula.

**Required change (AC-02, SR-01)**: Add a short-circuit before the re-normalization
branch: if `self.w_nli == 0.0`, return `self` unchanged regardless of
`nli_available`. Re-normalization is semantically meaningful only when `w_nli > 0.0`
(redistributing a real weight budget because NLI is absent); re-normalizing zero is
a correctness error that silently inflates sim and conf.

**Responsibility**: Correctness gate for the entire formula change. Must be
implemented before any eval run (SR-02 ordering constraint).

### Component 2: Default weight constants — config.rs

Six `default_w_*()` functions and `default_nli_enabled()`. These are the
`#[serde(default = "...")]` backing functions for `InferenceConfig`. The production
deployment has no config file overrides, so these constants are the effective
production weights.

**Required changes (AC-01, AC-02)**:
- `default_w_sim()`: 0.25 → 0.50
- `default_w_nli()`: 0.35 → 0.00
- `default_w_conf()`: 0.15 → 0.35
- `default_w_util()`: 0.05 → 0.00
- `default_w_prov()`: 0.05 → 0.00
- `default_w_coac()`: 0.00 → 0.00 (unchanged)
- `default_nli_enabled()`: true → false

**Sum constraint**: 0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 = 0.85 ≤ 1.0. The
`validate()` check passes. The `FusionWeights::effective()` short-circuit ensures
the formula is applied exactly as specified regardless of the `nli_available` path.

**Test impact**: `test_inference_config_weight_defaults_when_absent` asserts the old
values. The config merge test `test_project_config_merge_weight_conflict_rejected`
uses hardcoded values that reference `w_nli=0.35` as the default detection
heuristic; this test must be reviewed and updated to reflect the new default.

### Component 3: run_post_store_nli removal — nli_detection.rs + store_ops.rs

**nli_detection.rs**: Delete `run_post_store_nli` (lines ~39–185 per SCOPE.md
background). This is a `pub async fn` — its pub visibility is only exercised by
the `store_ops.rs` import. Tests to remove: `test_empty_embedding_skips_nli`,
`test_nli_not_ready_exits_immediately`, plus `test_circuit_breaker_stops_at_cap`,
`test_circuit_breaker_counts_all_edge_types` (and any others referencing
`run_post_store_nli`).

**store_ops.rs**: Remove `use crate::services::nli_detection::run_post_store_nli`
import. Remove the `tokio::spawn` NLI block (~20 lines around line 313). Delete
`NliStoreConfig` struct entirely (AC-14): all five fields (`enabled`,
`nli_post_store_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`,
`max_contradicts_per_tick`) are exclusively consumed by the removed spawn block.
Remove the `nli_cfg: NliStoreConfig` field from the store ops context struct and
its constructor parameter. Remove the `NliStoreConfig` import from `services/mod.rs`
(line 26) and the construction site (~line 435).

### Component 4: maybe_run_bootstrap_promotion removal — nli_detection.rs + background.rs

**nli_detection.rs**: Delete `maybe_run_bootstrap_promotion` (pub async fn,
lines ~197–274) and the private `run_bootstrap_promotion` (~200 lines). Tests to
remove: 7 functions listed in SCOPE.md background section.

**background.rs**: Remove `use crate::services::nli_detection::maybe_run_bootstrap_promotion`
import (line 49). Remove the two-line call site block at line 776. The sequencing
comment on line 781 ("Must remain after maybe_run_bootstrap_promotion") should also
be removed or updated as it no longer applies.

### Component 5: NLI auto-quarantine removal — background.rs

**background.rs**: Delete `enum NliQuarantineCheck` (~line 1233, 3 variants). Delete
`nli_auto_quarantine_allowed` private async fn (~line 1254, ~40 lines). Remove the
`if nli_enabled { ... }` block from `process_auto_quarantine` (~lines 1124–1145).
Remove `nli_enabled: bool` and `nli_auto_quarantine_threshold: f32` parameters from
`process_auto_quarantine` signature. Update the call site in `maintenance_tick`
(~line 946) to drop those two arguments. Tests to remove: 4 integration tests
named in SCOPE.md background section.

### Component 6: Eval gate — AC-12

The eval harness at `product/research/ass-039/harness/scenarios.jsonl` must be run
after AC-02 (effective() short-circuit) is in place. The gate requires MRR ≥ 0.2913
on 1,585 scenarios. This is a blocking pre-merge check — the output must be attached
to the PR description.

## Component Interactions

```
config.rs (default_w_*) ──► FusionWeights::from_config ──► FusionWeights::effective
                                                                      │
                                              nli_available=false ────┤
                                              w_nli==0.0 short-circuit┘
                                                    │
                                              fused_score computation (search.rs)
                                                    │
                                              context_search / context_briefing ranking
```

The NLI dead-code paths (Components 3, 4, 5) are independent of each other and of
the formula change (Components 1 and 2). They share no data flow with the scoring
pipeline.

## Technology Decisions

- ADR-001: effective() zero-NLI short-circuit (correctness, not optimization)
- ADR-002: NliStoreConfig complete deletion (no partial retention)
- ADR-003: Implementation ordering — formula + effective() before eval gate
- ADR-004: Shared helpers in nli_detection.rs — deferred to Group 2

## Integration Points

### Retained NLI infrastructure (not touched)

- `crates/unimatrix-server/src/infra/nli_handle.rs` — NliServiceHandle, NliConfig
- `crates/unimatrix-server/src/services/nli_detection_tick.rs` — run_graph_inference_tick
- `crates/unimatrix-server/src/infra/contradiction.rs` — heuristic contradiction scan
- `crates/unimatrix-server/src/services/search.rs:try_nli_rerank` — gated by nli_enabled
- `InferenceConfig.nli_enabled` field — retained, default changes to false

### Cross-module import dependencies for retained code

`nli_detection_tick.rs` line 34 imports three symbols from `nli_detection.rs`:

```rust
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};
```

These three symbols must remain in `nli_detection.rs` after removal. They are
currently `pub(crate)` functions. No changes to their signatures or visibility.
See ADR-004 for the deferred module merge question.

### InferenceConfig fields retained

The following `InferenceConfig` fields are retained even though the struct that
previously consumed them (`NliStoreConfig`) is deleted. They continue to be used
by `run_graph_inference_tick` and operator tooling:
- `nli_post_store_k`
- `nli_entailment_threshold`
- `nli_contradiction_threshold`
- `max_contradicts_per_tick`
- `nli_auto_quarantine_threshold`

**Distinction**: `NliStoreConfig` in `store_ops.rs` is deleted entirely (AC-14).
`InferenceConfig` fields of the same names are retained. These are separate structs;
the former was a localized config copy constructed from the latter.

## Integration Surface

| Integration Point | Type/Signature | Source | Action |
|-------------------|----------------|--------|--------|
| `FusionWeights::effective` | `fn effective(&self, nli_available: bool) -> FusionWeights` | search.rs:151 | Add w_nli==0.0 short-circuit before re-normalization |
| `default_w_sim` | `fn() -> f64` | config.rs:669 | 0.25 → 0.50 |
| `default_w_nli` | `fn() -> f64` | config.rs:673 | 0.35 → 0.00 |
| `default_w_conf` | `fn() -> f64` | config.rs:677 | 0.15 → 0.35 |
| `default_w_util` | `fn() -> f64` | config.rs:685 | 0.05 → 0.00 |
| `default_w_prov` | `fn() -> f64` | config.rs:689 | 0.05 → 0.00 |
| `default_nli_enabled` | `fn() -> bool` | config.rs | true → false |
| `run_post_store_nli` | `pub async fn` | nli_detection.rs:39 | Delete |
| `NliStoreConfig` | `pub(crate) struct` | store_ops.rs:38 | Delete entirely |
| `nli_cfg` field | store ops context | store_ops.rs:103 | Remove field + ctor param |
| `maybe_run_bootstrap_promotion` | `pub async fn` | nli_detection.rs:197 | Delete |
| `run_bootstrap_promotion` | `private async fn` | nli_detection.rs | Delete |
| `NliQuarantineCheck` | `enum` (3 variants) | background.rs:1233 | Delete |
| `nli_auto_quarantine_allowed` | `private async fn` | background.rs:1254 | Delete |
| `process_auto_quarantine` signature | `fn(..., nli_enabled: bool, nli_auto_quarantine_threshold: f32)` | background.rs | Drop two params; update call site |
| `current_timestamp_secs` | `pub(crate) fn() -> u64` | nli_detection.rs:639 | Retain — imported by nli_detection_tick.rs |
| `format_nli_metadata` | `pub(crate) fn(&NliScores) -> String` | nli_detection.rs:628 | Retain — imported by nli_detection_tick.rs |
| `write_nli_edge` | `pub(crate) async fn` | nli_detection.rs:532 | Retain — imported by nli_detection_tick.rs |
| `write_edges_with_cap` | `pub(crate) async fn` | nli_detection.rs | Delete — callerless after `run_post_store_nli` is removed; clippy dead-code warning if retained (AC-11) |

## Implementation Ordering Constraints

The following ordering is mandatory (SR-01, SR-02):

1. **Step 1 — effective() short-circuit (AC-02)**: Implement the `w_nli==0.0`
   guard in `FusionWeights::effective`. This must be first because all subsequent
   correctness depends on the scoring path being correct.

2. **Step 2 — Formula defaults (AC-01)**: Change `default_w_*()` values and
   `default_nli_enabled()`. Update config tests.

3. **Step 3 — eval gate (AC-12)**: Run eval harness. Steps 1 and 2 must be
   complete. MRR ≥ 0.2913 is blocking.

4. **Step 4 — Dead-code removal (AC-03 through AC-09, AC-13, AC-14)**: Components
   3, 4, 5 may be done in any internal order, but all in the same PR as Steps 1–3.
   Each removal is fully independent.

5. **Step 5 — Build + test verification (AC-10, AC-11)**: `cargo test --workspace`
   and `cargo clippy --workspace -- -D warnings` pass clean.

## Pre-existing Constraint Acknowledgement

`background.rs` is 4,229 lines — a pre-existing violation of the 500-line workspace
rule (SR-07). Removing ~60 lines of NLI dead-code does not resolve this. The 500-line
limit applies to new files created by this feature. The pre-existing violation is
tracked separately and is not a gate condition for crt-038.

## Symbol Checklist for Delivery (SR-03 / SR-04)

After completing removal, delivery must grep-verify these symbols return zero
results in compiled source (excluding test files and comments):

**Deleted from nli_detection.rs:**
- `pub async fn run_post_store_nli`
- `pub async fn maybe_run_bootstrap_promotion`
- `async fn run_bootstrap_promotion`
- `pub(crate) async fn write_edges_with_cap` — only caller was `run_post_store_nli`; callerless after removal; must be deleted to satisfy clippy -D warnings (AC-11)

**Deleted from store_ops.rs:**
- `struct NliStoreConfig`
- `impl Default for NliStoreConfig`
- `nli_cfg:` (field reference)
- `run_post_store_nli` (import and call)

**Deleted from background.rs:**
- `enum NliQuarantineCheck`
- `fn nli_auto_quarantine_allowed`
- `maybe_run_bootstrap_promotion` (import and call)
- `nli_enabled:` parameter in `process_auto_quarantine` signature
- `nli_auto_quarantine_threshold:` parameter in `process_auto_quarantine` signature

**Retained in nli_detection.rs (must NOT be deleted):**
- `pub(crate) fn format_nli_metadata`
- `pub(crate) async fn write_nli_edge`
- `pub(crate) fn current_timestamp_secs`

## Open Questions

None blocking delivery. The following are noted for completeness:

1. **Module fate of nli_detection.rs**: After removal, the file will contain only
   the three retained helpers (`format_nli_metadata`, `write_nli_edge`,
   `current_timestamp_secs`). `write_edges_with_cap` is deleted (callerless after
   `run_post_store_nli` is removed — see Symbol Checklist). The module merge question
   is deferred to Group 2 tick decomposition (ADR-004).

2. **w_util / w_prov signal zeroing**: Setting `w_util=0.00` and `w_prov=0.00`
   silently eliminates utilization and provenance signals for all queries. Both were
   low-weight (0.05) and no operator configs override them. Delivery should confirm
   in the PR description that no production overrides exist (SR-05).
