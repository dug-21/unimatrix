# Agent Report — vnc-041 C2 (Per-slug seed renderer)

Agent: vnc-041-agent-3-per-slug-seed-renderer
Component: C2 — `render_per_slug_seed_toml() -> String` (ADR-003 #5237)

## Files modified
- `crates/unimatrix-server/src/infra/config.rs`

## What was implemented
- `render_per_slug_seed_toml() -> String` (pub): static per-slug header + a
  classification-derived legend block (one commented line per
  `PER_SLUG_CONFIG_CLASSIFICATION` entry, iterated in order) + reused
  `DEFAULT_CONFIG_TOML` appended verbatim. No new struct→TOML serializer.
- `render_legend_line(entry: &ConfigKeyClass) -> String` (private): the single
  site of the EXHAUSTIVE `match` on `OverlayDisposition` — NO catch-all `_` arm
  (ADR-003 forcing function, R-06; a future variant is an intended compile
  break). Factored out so the flip test exercises the same render path. Keyed on
  `entry.key` string + `disposition` only — never a `UnimatrixConfig` field, so
  field-less locks (`permissive`, `tls`, `http`, `*_sha256`, `rayon_pool_size`)
  render "managed globally" with no editable knob and cannot panic (R-12).
- 10 unit tests in the inline `mod tests` of config.rs (kept in-file per the
  STRICT boundary: edit only config.rs — a sibling `*_tests.rs` file would be a
  new file). Covers R-04 full-registry coverage, overlayable⇒editable /
  locked⇒"managed globally"+IGNORED, disposition-keyed render, the A→B FLIP test
  (C2 half), field-less locks, no-panic, exact dotted-key set, valid-TOML parse,
  and the "overlays nothing" deserialize oracle.

## Tests
- C2 renderer tests (config::tests::test_render*): **10 passed / 0 failed**.
- `cargo build -p unimatrix-server` (lib + tests): passes.
- `cargo clippy -p unimatrix-server --lib --tests`: zero warnings.
- Tests run via `--lib` (per the C5 note, `--bins` matters only for
  tracing-assertion tests; C2 has none — no `tracing` in the renderer).

## Two real findings (test plan corrections, fixed in-component)
1. **Legend-line extraction must be block-scoped.** `DEFAULT_CONFIG_TOML`
   comments also use the em-dash separator (e.g. `# [profile] — …`), so a
   whole-body `# ` + ` — ` filter over-counts (33 vs 22). `legend_lines()` now
   scopes to the block between the classification header and the editable-knobs
   footer.
2. **`deserializes_to_default` oracle is structurally impossible.**
   `KnowledgeConfig.boosted_categories`/`adaptive_categories` have a serde
   default fn returning `["lesson-learned"]` while the programmatic `Default`
   impl returns `[]` (config.rs:415-416). Any `toml::from_str` omitting those
   fields invokes the serde fn, so a parsed seed can never equal
   `UnimatrixConfig::default()`. Renamed to
   `test_render_output_deserializes_same_as_bare_template` and assert equality
   with bare `DEFAULT_CONFIG_TOML` parsed alone — the correct R-14 "overlays
   nothing" proof (the comment-only legend prepend is inert).

## Issues / adjacent breakage
- None caused by this change. One TRANSIENT incremental-build flake on
  `--test export_integration` (an integration target that does not reference the
  renderer) cleared on immediate rebuild — not attributable to C2.
- `infra/config.rs` is 12863 lines (was 12495 pre-C2). It was already far over
  the 500-line guidance before this work (a pre-existing condition for this
  crate's central config module; new test modules already use sibling files).
  Per the STRICT boundary I did NOT split it. **FLAGGED** for leader awareness;
  not actionable within this component's scope.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (pattern/decision) —
  surfaced ADR-003 (#5237, binding), plus the dual-site serde/Default divergence
  pattern (#3817) which directly explained finding #2.
- Stored: entry #5242 "Per-slug seed renderer test traps: legend-block scoping +
  deserialize oracle (vnc-041 C2)" via /uni-store-pattern.
