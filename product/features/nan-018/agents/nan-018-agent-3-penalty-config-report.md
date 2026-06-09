# Agent Report — nan-018-agent-3-penalty-config (Wave 2: penalty-config)

## Scope
Implement `GraphPenaltyConfig` + `UnimatrixConfig.graph_penalty` in
`crates/unimatrix-server/src/infra/config.rs` per the Integration Surface, with
dual-default discipline, `resolve_params()` multiplier overlay, and range validation.

## Files Modified / Created
- `crates/unimatrix-server/src/infra/config.rs` (modified)
- `crates/unimatrix-server/src/infra/graph_penalty_config_tests.rs` (new — `#[path]` test submodule)

## What Was Implemented (5 atomic sites, per #4070)
1. `GraphPenaltyConfig` struct (`#[serde(default)]`, plain `f64` fields + `multiplier: Option<f64>`),
   `Serialize + Deserialize` (Serialize added only on this new struct to enable the #3557
   round-trip test; the existing Deserialize-only convention for the wider tree is unchanged).
2. Seven dual-default `default_*()` fns — each references the engine const
   (`unimatrix_engine::graph::*`), never an inlined literal — and a `Default` impl resolving
   to the same consts (single numeric source of truth, #4064).
3. `UnimatrixConfig.graph_penalty` field with `#[serde(default)]`.
4. `GraphPenaltyConfig::resolve_params() -> GraphPenaltyParams`: multiplier scales the FIVE
   severities only (orphan, clean_replacement, partial_supersession, dead_end, fallback),
   NEVER hop_decay or max_traversal_depth (ADR-001 §3); per-field override wins via the
   equals-default heuristic (documented caveat: deliberate-set-to-default reads as unset).
5. `ConfigError::GraphPenaltyFieldOutOfRange` variant + Display arm; `validate_graph_penalty`
   wired UNCONDITIONALLY into `validate_config`; `graph_penalty` added to the hidden
   `merge_configs` literal (section-level replace, mirroring http/tls).

Range validation: severities finite in `[0,1]`; `max_traversal_depth >= 1`; `multiplier`
finite in `(0,1]`. Out-of-range aborts config load — never silently used or clamped. Does
NOT re-implement the engine's depth clamp (carry-flag coordinated, not duplicated).

## C-02 default-equivalence (LOAD-BEARING)
`test_config_omits_graph_penalty_section_deserializes_to_defaults` and the empty-table test
assert that an absent / empty `[graph_penalty]` ⇒ `GraphPenaltyConfig::default()` and
`resolve_params() == GraphPenaltyParams::default()` — behavior bit-for-bit unchanged.

## Tests
- New: 24 unit tests in `graph_penalty_config_tests.rs` — all PASS.
  (7 dual-default triangulations, 3 empty/partial deserialization, 2 multiplier-field,
  3 resolve_params semantics, 8 range-validation, 1 serde round-trip.)
- Regression: `cargo test -p unimatrix-server --lib infra::config` → 465 passed, 0 failed
  (merge_configs + validate_config suites intact).
- `cargo build -p unimatrix-server --tests`: clean. `cargo clippy` (lib+tests): no
  graph_penalty warnings. `cargo fmt --check`: clean.

## Issues / Blockers
- Swarm file race: my `graph_penalty_config_tests.rs` was twice overwritten with an empty
  stub by the trust-metric agent, and config.rs was once reverted to pristine by a
  concurrent process. Re-applied all edits and re-wrote the test file atomically; verified
  intact (32 marker hits in config.rs, 326-line test file, 465+24 green). Captured as a
  reusable pattern (entry #4904).
- Files-≤500 note: config.rs is a pre-existing 11.4k-line file (out of scope to split);
  my new test file is the modular split for the new tests.

## Knowledge Stewardship
- Queried: context_search (pattern: serde dual-default/range-validation) → #4070 (five-site
  atomic + hidden merge_configs literal — applied exactly), #3817 (dual-site divergence),
  #646, #3774; context_search (decision, nan-018) → #4897 (ADR-001), #4894 (ADR-006);
  context_get #4070 (full).
- Stored: entry #4904 "Swarm agents racing on a shared #[path] test file" via
  /uni-store-pattern (the compile-green stub-overwrite trap — a genuine cross-feature
  gotcha invisible in source). The penalty-config implementation itself is a single-feature
  instance of already-recorded ADR-001/#4064/#4070 — nothing novel there to store.
