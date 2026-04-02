# crt-038: conf-boost-c Formula and NLI Dead-Code Removal

## Problem Statement

The production scoring formula (defaults set in crt-024) assigns `w_nli=0.35` as the
dominant ranking signal. Research spikes ASS-035, ASS-037, and ASS-039 established that
the NLI cross-encoder (SNLI-trained MiniLM) produces task-mismatched scores against
Unimatrix's structured knowledge entries, contributing zero net MRR lift and −0.0031 MRR
relative to the conf-boost-c configuration. The scoring formula defaults are wrong and the
three NLI code paths they enabled are now dead: no Contradicts edges have ever been
written, the bootstrap promotion marker is unset but would find zero rows, and the
auto-quarantine NLI guard returns `Allowed` on every invocation.

Agents retrieving context via `context_search` and `context_briefing` receive
sub-optimal rankings every query. The fix is a weight change and targeted dead-code
removal — both are low risk, fully isolated, and have no architectural dependencies.

## Goals

1. Change the default fusion weight constants to conf-boost-c: `w_sim=0.50, w_conf=0.35,
   w_nli=0.00, w_util=0.00, w_prov=0.00`.
2. Default `nli_enabled=false` so the NLI model is not invoked during search when
   `w_nli=0.00` (avoids dead inference overhead).
3. Remove `run_post_store_nli` from `nli_detection.rs` and its call site in
   `store_ops.rs`, including `NliStoreConfig.enabled` gate logic.
4. Remove the NLI guard block (`nli_auto_quarantine_allowed`, `NliQuarantineCheck`) from
   `process_auto_quarantine` in `background.rs` along with the `nli_enabled` and
   `nli_auto_quarantine_threshold` parameters to that function.
5. Remove `maybe_run_bootstrap_promotion` from `nli_detection.rs` and its call site in
   `background.rs`.
6. Update or remove all tests that assert the old defaults or test the removed code paths.
7. Confirm no MRR regression: post-ship eval run on
   `product/research/ass-039/harness/scenarios.jsonl` must show MRR ≥ 0.2913.

## Non-Goals

- Removal of `run_graph_inference_tick` or restructuring of the NLI gate around it
  (Group 2 tick decomposition — separate feature).
- Removal of `NliServiceHandle`, `NliConfig`, or the NLI model loading infrastructure.
  These remain for the contradiction scan and future domain-adapted model work.
- Removal of `nli_enabled` config field itself. The field stays; its default changes to
  `false`.
- Cosine Supports detection replacement (Group 3 feature). This feature removes the NLI
  Supports path; the cosine replacement ships separately.
- Any change to `try_nli_rerank` in `search.rs` beyond what falls out of the
  `nli_enabled=false` default (the function itself is still valid for future use).
- Any schema changes or data migrations.
- Changes to `contradiction.rs` (heuristic-based, not NLI-dependent).

## Background Research

### Codebase State

**Scoring formula — current defaults (config.rs)**
```
default_w_sim()  → 0.25
default_w_nli()  → 0.35
default_w_conf() → 0.15
default_w_coac() → 0.00   (zeroed in crt-032)
default_w_util() → 0.05
default_w_prov() → 0.05
default_nli_enabled → true
```

**Scoring formula — conf-boost-c target**
```
default_w_sim()  → 0.50
default_w_nli()  → 0.00
default_w_conf() → 0.35
default_w_coac() → 0.00
default_w_util() → 0.00
default_w_prov() → 0.00
default_nli_enabled → false
```

Sum = 0.85. Headroom for phase signals (w_phase_histogram=0.02, w_phase_explicit=0.05)
unchanged. Validate constraint (`sum ≤ 1.0`) still holds.

**Formula is a code change, not config-only.** The defaults live in `default_w_*()` fns
in `config.rs`. Operators cannot override default code without config file entries, and
the production deployment does not set weight overrides. The research statement "config
change" refers to the conceptual scope (no new logic), not the implementation mechanism.

**`FusionWeights::effective()` interaction.** When `nli_available=true` (NLI enabled and
model loaded), `effective()` returns weights unchanged — including `w_nli=0.0` if that is
the configured value. The NLI model would still be invoked in `try_nli_rerank` (wasted
inference). Setting `nli_enabled=false` skips the `try_nli_rerank` call entirely
(`nli_scores=None`), triggering the `effective(false)` re-normalization path. With
`w_nli=0.00` as the configured value, the denominator for re-normalization is
`w_sim + w_conf + w_coac + w_util + w_prov = 0.50 + 0.35 + 0.00 + 0.00 + 0.00 = 0.85`,
producing `w_sim'=0.50/0.85 ≈ 0.588, w_conf'=0.35/0.85 ≈ 0.412`. This is NOT the
intended formula. The correct approach: set `nli_enabled=false` default, which skips NLI
inference, then the `effective(false)` path re-normalizes to that skewed result.

**Open question (see below):** Should `w_nli=0.00` skip re-normalization entirely? The
`effective(false)` re-normalization was designed for NLI being temporarily unavailable
with positive `w_nli`. With `w_nli=0.0`, the denominator already excludes NLI weight,
so re-normalization produces scaled-up sim/conf. The intended behavior is `effective(true)`
with `w_nli=0.0` — NLI enabled (available) but contributing nothing. This requires either:
(a) keeping `nli_enabled=true` in default (NLI model still invoked wastefully), or
(b) adding a no-rerank path when `w_nli=0.0` regardless of `nli_enabled`. The simplest
correct approach: set `nli_enabled=false` AND document that the re-normalization
produces a slightly scaled-up formula — or set `nli_enabled=false` and also set
`w_sim=0.50, w_conf=0.35` as the post-normalization target, accepting the re-normalization
effect at the default `nli_enabled=false` path. Delivery must resolve this.

**`run_post_store_nli` (nli_detection.rs, lines 39–185)**
- Spawned fire-and-forget from `store_ops.rs` line 312 via `tokio::spawn`
- Guarded by `self.nli_cfg.enabled && self.nli_handle.is_ready_or_loading()`
- Writes Supports/Contradicts edges to GRAPH_EDGES
- Production effect: 30 Supports edges total; 27 source/target endpoints now quarantined
- 0 Contradicts edges ever written
- Tests in `nli_detection.rs`: `test_empty_embedding_skips_nli`,
  `test_nli_not_ready_exits_immediately`, plus `test_circuit_breaker_stops_at_cap`,
  `test_circuit_breaker_counts_all_edge_types` (13 test functions total in nli_detection.rs)
- Removal requires: removing `use crate::services::nli_detection::run_post_store_nli`
  import from `store_ops.rs`, removing spawn block (~20 lines), simplifying or removing
  `NliStoreConfig` struct (fields `enabled`, `nli_post_store_k`, `nli_entailment_threshold`,
  `nli_contradiction_threshold`, `max_contradicts_per_tick`)

**`maybe_run_bootstrap_promotion` (nli_detection.rs, lines 197–274)**
- Called in `background.rs` line 776 inside `if inference_config.nli_enabled { ... }`
- Idempotency marker `bootstrap_nli_promotion_done` not set in production (would no-op)
- 0 bootstrap Contradicts rows in DB — `run_bootstrap_promotion` would return immediately
- Private function `run_bootstrap_promotion` (~200 lines) is also fully removable
- Tests: `test_bootstrap_promotion_zero_rows_sets_marker`,
  `test_maybe_bootstrap_promotion_skips_if_marker_present`,
  `test_maybe_bootstrap_promotion_defers_when_nli_not_ready`,
  `test_bootstrap_promotion_confirms_above_threshold`,
  `test_bootstrap_promotion_refutes_below_threshold`,
  `test_bootstrap_promotion_idempotent_second_run_no_duplicates`,
  `test_bootstrap_promotion_nli_inference_runs_on_rayon_thread` (7 test functions)
- Removal requires: removing import `use crate::services::nli_detection::maybe_run_bootstrap_promotion`
  from `background.rs`, removing the two-line call site block

**`process_auto_quarantine` NLI guard (background.rs, lines 1124–1145)**
- The `if nli_enabled { ... }` block checks `nli_auto_quarantine_allowed()` before each
  quarantine candidate
- `nli_auto_quarantine_allowed` (private, ~40 lines) queries GRAPH_EDGES for Contradicts
  edges; always returns `Allowed` when zero Contradicts edges exist
- `NliQuarantineCheck` enum (3 variants) used only by this function pair
- Function signature of `process_auto_quarantine` loses `nli_enabled: bool` and
  `nli_auto_quarantine_threshold: f32` parameters
- Call site in `maintenance_tick` (background.rs ~line 946) must drop those two args
- Tests in `background.rs`: 4 integration tests named
  `test_nli_edges_below_auto_quarantine_threshold_no_quarantine`,
  `test_nli_edges_above_threshold_allow_quarantine`,
  `test_nli_auto_quarantine_mixed_penalty_allowed`,
  `test_nli_auto_quarantine_no_edges_allowed`

**What NLI infrastructure remains (not in scope)**
- `crates/unimatrix-server/src/infra/nli_handle.rs` — NliServiceHandle, NliConfig,
  state machine (Loading/Ready/Failed/Retrying)
- `run_graph_inference_tick` in `nli_detection_tick.rs` — tick-based Informs edge inference
- `contradiction.rs` — heuristic-based contradiction scan (no NLI dependency)
- `try_nli_rerank` in `search.rs` — still valid; gated behind `self.nli_enabled`
- `nli_enabled` config field — retained; default changes to `false`
- All NLI-related `InferenceConfig` fields (`nli_model_name`, `nli_model_path`,
  `nli_model_sha256`, `nli_top_k`, `nli_post_store_k`, `nli_entailment_threshold`,
  `nli_contradiction_threshold`, `max_contradicts_per_tick`, `nli_auto_quarantine_threshold`)
  — retained for operator use and Group 2 tick decomposition

**Test impact in config.rs**
- `test_inference_config_weight_defaults_when_absent` — asserts `w_nli=0.35`; must update
- `test_inference_config_default_weights_sum_within_headroom` — asserts sum `≤ 0.95`; still
  holds (0.85 ≤ 0.95) but value check must update if present
- Search.rs tests: several tests use `w_nli: 0.35` in `default_weights()` helper; these
  are unit tests of formula mechanics, not of defaults — may remain or update to reflect
  new defaults. Tests asserting NLI dominance (AC-11, Constraint 9/10) test the formula
  structure, not the production defaults, and can remain with their explicit weight values.

### Research Findings

| Spike | Finding relevant to this feature |
|-------|----------------------------------|
| ASS-035 | NLI task mismatch on SNLI model vs Unimatrix corpus confirmed |
| ASS-037 | conf-boost-c formula confirmed; NLI dead (zero contribution); 0 Contradicts ever written |
| ASS-039 | Behavioral ground truth (1,585 scenarios): conf-boost-c MRR=0.2913, production MRR=0.2882; +0.0031 delta |

### Eval Gate

Gate metric from ROADMAP.md: MRR ≥ 0.2913 on `product/research/ass-039/harness/scenarios.jsonl`.
P@5 is formula-invariant — not a gating metric for this feature.

## Proposed Approach

**Item 1 — Formula change (config.rs defaults):**
Change the six `default_w_*()` functions in `config.rs`. Change `default_nli_enabled()` to
return `false`. Update doc comments on `InferenceConfig` fields. Update tests asserting old
defaults. Confirm `validate()` still passes with new defaults (sum = 0.85 ≤ 1.0).

Delivery must resolve whether `nli_enabled=false` with re-normalization produces the
correct effective formula. The cleanest resolution: set `nli_enabled=false` as default
and treat the conf-boost-c formula as the *direct* weights, which means the
`effective(false)` re-normalization path will scale them. To avoid this, the default
could keep `nli_enabled=false` and document that the intended effective weights are
produced only when w_nli=0.0 (since re-normalization with a zero w_nli term is identity
on the remaining terms — denom = w_sim + w_conf + w_coac + w_util + w_prov, which for
conf-boost-c = 0.85, so post-normalization w_sim=0.588, w_conf=0.412). If the intent is
exact w_sim=0.50, w_conf=0.35, delivery should confirm the scoring path taken.

**Items 2–4 — Dead-code removal (surgical):**
Remove function bodies, their tests, and call sites. Each removal is independent and
can be done in any order. No new logic introduced anywhere. No schema changes.

**Ordering:** Formula change first (immediately affects production ranking). Dead-code
removals can follow in the same PR or as a single batch.

## Acceptance Criteria

- AC-01: Default `w_sim=0.50`, `w_conf=0.35`, `w_nli=0.00`, `w_util=0.00`,
  `w_prov=0.00` in `InferenceConfig::default()` (validated by updated config tests).
- AC-02: Default `nli_enabled=false` in `InferenceConfig::default()`. `FusionWeights::effective()` short-circuits when `w_nli == 0.0`: return weights unchanged regardless of `nli_available`. Re-normalization only applies when `w_nli > 0.0` and NLI is unavailable — redistributing that weight is semantically meaningful; redistributing zero weight is a correctness error.
- AC-03: `run_post_store_nli` function is deleted from `nli_detection.rs`.
- AC-04: `store_ops.rs` no longer imports or calls `run_post_store_nli`; the
  `tokio::spawn` NLI block is removed.
- AC-05: `maybe_run_bootstrap_promotion` and `run_bootstrap_promotion` are deleted from
  `nli_detection.rs`.
- AC-06: `background.rs` no longer imports or calls `maybe_run_bootstrap_promotion`.
- AC-07: `nli_auto_quarantine_allowed` and `NliQuarantineCheck` are deleted from
  `background.rs`.
- AC-08: `process_auto_quarantine` signature no longer includes `nli_enabled` or
  `nli_auto_quarantine_threshold` parameters; its call site is updated accordingly.
- AC-09: All tests for removed code paths are removed; all tests for retained code paths
  pass without modification (or with minimal updates to expected default values).
- AC-10: `cargo test --workspace` passes with zero failures.
- AC-11: `cargo clippy --workspace -- -D warnings` passes with zero warnings.
- AC-12: Eval run on `product/research/ass-039/harness/scenarios.jsonl` produces
  MRR ≥ 0.2913 (confirms no regression from formula change). **Blocking pre-merge gate.**
  Eval output must be attached to the PR description before merge is permitted.
- AC-13: `nli_detection.rs` retains `NliServiceHandle` imports and any shared helpers
  used by `run_graph_inference_tick`; no cross-module compilation breakage.
- AC-14: `NliStoreConfig` struct is deleted entirely from `store_ops.rs`. All fields
  (`enabled`, `nli_post_store_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`,
  `max_contradicts_per_tick`) are exclusively consumed by the removed NLI spawn block.
  No dead fields retained.

## Constraints

- `nli_detection.rs` is 1,373 lines; ~700+ lines are tests. After removing the three
  functions and their tests, the file will shrink substantially. If remaining code
  (`run_graph_inference_tick` helpers, shared helpers) drops below 200 lines, consider
  merging into `nli_detection_tick.rs` — but do not do so unless the resulting file
  length and module boundary are clean. Not required for this feature.
- `NliStoreConfig` in `store_ops.rs` — if `run_post_store_nli` is the only consumer of
  the `enabled` field, the struct can be simplified or removed. Fields
  `nli_entailment_threshold`, `nli_contradiction_threshold`, `max_contradicts_per_tick`,
  `nli_post_store_k` are only used in the NLI spawn block. Removal is required for clean
  code; retain no dead fields.
- The `nli_enabled` field on `InferenceConfig` must remain (used by `run_graph_inference_tick`
  gate in `background.rs` and by `SearchService.nli_enabled`). Only the default changes.
- Max 500 lines per file (Rust workspace rule). No file should exceed this after removal.
  Current `background.rs` is 4,229 lines — pre-existing. No new lines added.
- `cargo audit` must pass; no new dependencies introduced.
- Eval harness is at `product/research/ass-039/harness/scenarios.jsonl`. Delivery must
  document the eval run command and output in the PR description.

## Open Questions

1. **`nli_detection.rs` module fate**: After removing the three functions, the file contains
   only helpers used by `nli_detection_tick.rs` (shared types, `write_edges_with_cap`,
   `format_nli_metadata`). Should the module be renamed or merged? Defer to Group 2 tick
   decomposition (separate feature); this feature leaves the module name unchanged.

## Tracking

https://github.com/dug-21/unimatrix/issues/483
