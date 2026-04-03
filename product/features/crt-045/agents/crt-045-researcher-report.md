# crt-045 Researcher Report

## Summary

Investigated the problem space for wiring `[inference]` profile overrides into `EvalServiceLayer`
construction. The root cause is a single missing call: `TypedGraphState::rebuild()` is never
invoked during eval runs.

## Root Cause

The config wiring is correct — `ppr_expander_enabled`, `expansion_depth`, and
`max_expansion_candidates` flow correctly from profile TOML through `parse_profile_toml()` →
`UnimatrixConfig` → `ServiceLayer::with_rate_config()` → `SearchService` fields.

The bug is that `TypedGraphState` starts in cold-start mode (`use_fallback = true`, empty graph)
in `EvalServiceLayer::from_profile()` and is never rebuilt from the snapshot database. The
`if !use_fallback` guard in `search.rs` Step 6d then prevents Phase 0 (`graph_expand`), PPR
(Phase 1), and all graph traversal from executing. Both `baseline.toml` and
`ppr-expander-enabled.toml` run the identical cold-start fallback path — no graph traversal, no
PPR, bit-identical results.

## Secondary Bug

`ppr-expander-enabled.toml` declares `distribution_change = true` but omits the required
`[profile.distribution_targets]` sub-table. `parse_profile_toml()` rejects this with
`EvalError::ConfigInvariant` before any graph issue is observable, so the primary bug was never
reached during harness runs.

## Key Files Read

- `crates/unimatrix-server/src/eval/profile/layer.rs` — `EvalServiceLayer::from_profile()`
  (13 steps; no `TypedGraphState::rebuild()` call)
- `crates/unimatrix-server/src/services/typed_graph.rs` — cold-start state definition,
  `rebuild()` implementation
- `crates/unimatrix-server/src/services/mod.rs` — `with_rate_config()` creates a new cold-start
  handle internally; handle is shared via `Arc`
- `crates/unimatrix-server/src/services/search.rs` — `if !use_fallback` guard at Step 6d;
  `ppr_expander_enabled` used correctly inside that guard
- `crates/unimatrix-server/src/eval/profile/validation.rs` — `parse_profile_toml()` strips
  `[profile]` and deserializes remainder as `UnimatrixConfig`; `[inference]` section flows through
- `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` — missing
  `[profile.distribution_targets]` despite `distribution_change = true`
- `product/research/ass-039/harness/run_eval.py` — Python harness passes `--configs` to
  `unimatrix eval run`; no graph-related logic here

## Proposed Fix Scope

Minimal: one new call in `layer.rs` + a post-construction write + TOML fix + one new test.
No signature changes to `with_rate_config()`, no new config fields, no changes to runner/report.

## SCOPE.md

Written to: `product/features/crt-045/SCOPE.md`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 entries; most relevant were #3582
  (eval-harness sidecar pattern), #3610 (7-component decomposition), #4064 (InferenceConfig
  dual-maintenance). None were about TypedGraphState eval initialization.
- Stored: entry #4096 "EvalServiceLayer must call TypedGraphState::rebuild() to avoid silent
  cold-start graph in eval" via `/uni-store-pattern`
