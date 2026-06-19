# Agent Report — vnc-040-agent-1-pseudocode

## Deliverables
- product/features/vnc-040/pseudocode/OVERVIEW.md
- product/features/vnc-040/pseudocode/slug_config_classification.md
- product/features/vnc-040/pseudocode/resolve_slug_config.md
- product/features/vnc-040/pseudocode/per_slug_loop.md

## Components covered (3, per architecture Component Map)
1. slug_config_classification — ADR-004 declarative registry, DATA-ONLY (infra/config.rs)
2. resolve_slug_config — NEW overlay helper (http_provision.rs)
3. per_slug_loop — MODIFY main.rs:1089-1110 + relocate main.rs:687

## resolve_slug_config file-home decision
EXISTING `http_provision.rs` (NOT a new slug_config.rs). Avoids a `mod` declaration collision at
the main.rs crate root (only `mod http_provision;` exists at line 6) with the per_slug_loop agent's
edits; http_provision.rs already imports ProjectSlug/UnimatrixConfig/ServerError/Arc/Path and owns
the base_dir.join(slug.as_str()) derivation. ≤500-line budget flagged for Gate 3b re-confirm.

## Open questions / gaps flagged
1. SIGNATURE GAP (must reach rust-dev + architect): brief + ARCHITECTURE §9 show
   `merge_configs(global: &UnimatrixConfig, project: &UnimatrixConfig)`. LIVE signature takes OWNED
   values: `fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig`
   (config.rs:3825). resolve_slug_config holds `global` by ref → must `global.clone()` once to feed
   the merge. One clone per slug-with-a-file, startup-only. NOT a merge_configs rewrite; reuse intact.
2. SR-02/R-02 re-audit (no change expected, flag for implementer): merge_configs' inline
   InferenceConfig {…} literal (#4070) is confirmed to list every field explicitly with NO
   ..Default() tail (config.rs:3895-4260). rust-dev must re-confirm the global→per-slug call shape
   exercises the SAME arm before trusting reuse.
3. Registry key-string EXHAUSTIVENESS (carry-item 9 / architect OQ): the literal key strings in
   PER_SLUG_CONFIG_CLASSIFICATION must be reconciled against real struct field paths (InferenceConfig
   weight + PPR field names, ServerConfig transport field names) at impl time. The disposition map is
   fixed; the exact identifiers are bound by the AC-11 drift-guard. Left as enumerated rows + a
   reconcile note, not invented field names.
4. main.rs:687 `server_instructions` may be used beyond the per-slug fan-out (e.g. daemon's own
   ServiceLayer). rust-dev must grep usages and relocate ONLY the per-slug source (flagged in
   per_slug_loop.md).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern: config merge overlay) — surfaced #2395
  (two-level merge replace semantics), #4655 (security-critical global-wins), #5090 (cross-component
  seam wave-handoff). context_search (decision, topic vnc-040) — surfaced all four ADRs #5209/#5206/
  #5199/#5210; read the ADR files directly to confirm.
- Deviations from established patterns: none. Pseudocode reuses merge_configs/load_single_config/
  validate_config unchanged (#2395/#4655 patterns preserved), data-only ADR-004 registry, no new
  merge model. The owned-vs-ref merge_configs signature is an upstream-doc gap, not a pattern
  deviation.
