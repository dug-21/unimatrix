# Agent Report — vnc-040-agent-3-slug_config_classification

Component: ADR-004 canonical per-slug-vs-global classification registry (DATA-ONLY).

## Files modified
- `crates/unimatrix-server/src/infra/config.rs` — added `enum OverlayDisposition`,
  `struct ConfigKeyClass`, `const PER_SLUG_CONFIG_CLASSIFICATION`, `fn is_per_slug_overlayable`
  (colocated immediately after `merge_configs`); registered the sibling test module via
  `#[path = "slug_config_classification_tests.rs"]`.
- `crates/unimatrix-server/src/infra/slug_config_classification_tests.rs` — NEW sibling test
  file (follows the existing `graph_penalty_config_tests` / `projects_config_tests` convention).

`merge_configs` logic UNCHANGED (data-only, per ADR-004 / brief).

## Tests — component
11 passed / 0 failed.
- `test_classification_drift_guard_every_entry_matches_merge_configs` (AC-11/R-14, mandatory)
- `test_classification_registry_exhaustive_vs_seam_field_set` (carry-item 9, R-07 closed-set)
- `test_is_per_slug_overlayable_matches_registry_disposition`
- `test_is_per_slug_overlayable_unknown_key_returns_false` (unknown ⇒ false, pinned)
- `test_is_per_slug_overlayable_sampled_dispositions`
- `test_sha256_pins_global_wins_under_per_slug_pairing` (R-05, warn captured via tracing_test)
- `test_no_global_pin_plus_per_slug_pin_does_not_silently_lock`
- `test_inference_overlayable_fields_overlay_siblings_fall_through` (R-02)
- `test_option_field_set_global_unset_per_slug_retains_global` (R-02 `.or()` edge)
- `test_list_field_override_replaces_not_appends` (#2286 replace, AC-03)
- `test_nli_runtime_params_overlay_and_are_classified_overlayable` (R-08)

Build: `cargo build -p unimatrix-server` clean. `cargo clippy -p unimatrix-server --tests`
introduces ZERO new warnings in the registry region or the test file. `cargo fmt` applied.

Full `cargo test -p unimatrix-server --lib`: 4249 passed, 1 failed. The single failure
(`eval::corpus::fixtures_tests::test_ac14_scenario_search_returns_non_empty_ranked_list`) is a
PRE-EXISTING ranking-order flake under parallel `--lib` execution — zero references to
`merge_configs`/classification/config, passes in isolation. NOT introduced by this component.

## SR-02 / #4070 inference-arm audit (RESULT)
PASS. Mechanically diffed the `InferenceConfig` struct field set against the inline
`InferenceConfig {…}` literal inside `merge_configs`: **47 struct fields == 47 literal fields,
no field missing, no `..Default()` tail.** Every inference field is handled explicitly for the
global→per-slug call shape (same arm as global→project — no project-only assumption). Reuse of
`merge_configs` for the C6 shape is safe. No rewrite made.

## Registry exhaustiveness approach
`test_classification_registry_exhaustive_vs_seam_field_set` compares the registry key set against
a closed `EXPECTED_CLASSIFIED_KEYS` checklist (the §9 verdict rows as stable ids), asserting set
equality both directions (no missing seam key, no extra/unclassified key), no duplicate keys, and
that every non-construction key has drift-guard coverage (a value-setter + accessor). A future
`build_project_server`-relevant key added without a registry row fails the test.

## CRITICAL implementation note (load-bearing — flag to tester/validator)
`GlobalLocked` is TWO mechanisms, and the drift-guard must distinguish them:
1. **Merge-locked** (`*_sha256` hash pins): locked INSIDE `merge_configs` (global-wins carve-out).
   Drift-guard asserts `merged == global` directly. ✅ tested.
2. **Construction-locked** (`inference.rayon_pool_size`, `tls`, `http`, `permissive`):
   `merge_configs` does NOT lock these — they win project-wins in the merge; their lock is held
   BY CONSTRUCTION in `per_slug_loop` (component 3), which never sources pool/transport/process
   posture from the merged config (FR-15, ADR-002 §1). Asserting `merged == global` for them would
   be a FALSE claim about `merge_configs`. The test lists them in `CONSTRUCTION_LOCKED_KEYS`,
   asserts only that they are classified `GlobalLocked`, and skips the merge-value assertion.
   `permissive` additionally has NO `UnimatrixConfig` field (daemon flag) — its pair-builder
   returns `None`.

The pseudocode/test-plan phrasing "GlobalLocked ⇒ global wins in merge_configs (incl. *_sha256)"
is only literally true for the hash pins; the registry honors the verdict table, but the merge
binding for construction-locked keys is `per_slug_loop`'s responsibility, not the merge's. The
§9 table's GlobalLocked rows still render correctly from the registry.

## Other notes
- `merge_configs` is now `pub fn` (changed from private by the parallel `resolve_slug_config`
  agent, which calls it cross-module). My test accesses it via `super::merge_configs` either way —
  no impact. Flag only.
- Registry key strings use LIVE struct paths: top-level `tls` and `http` (not `server.tls`/
  `server.http` — those are top-level `UnimatrixConfig` fields).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern) — surfaced #4070 (hidden merge_configs
  literal), #4655 (security-critical global-wins merge), #4044 (InferenceConfig hidden sites),
  #3771 (KnowledgeConfig parallel-list defaults). Applied #4070/#4655 directly.
- Stored: entry #5211 "Drift-guard registry: split GlobalLocked into merge-locked vs
  construction-locked or the test lies" via /uni-store-pattern (topic `unimatrix-server`).

## Issues / blockers
None. Component compiles in isolation, all 11 component tests pass, clippy clean.
The one full-lib failure is a pre-existing unrelated eval flake (documented above).
