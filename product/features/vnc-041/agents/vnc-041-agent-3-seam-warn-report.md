# vnc-041 Agent Report — C5 Locked-Key Seam WARN

**Agent:** vnc-041-agent-3-seam-warn
**Component:** C5 — Locked-key seam WARN in `resolve_slug_config`
**Status:** COMPLETE

## What was implemented

Added a WARN pass to `resolve_slug_config`'s file-present arm (ADR-005 #5239). When a per-slug
config file SETS a key whose `is_per_slug_overlayable(key) == false`, the resolver emits ONE
`tracing::warn` naming key + slug. WARN-only: resolution output is byte-identical to the no-WARN
path; the raw parse never converts a parseable file into a new error.

Implementation shape (shape (a) from the pseudocode — lowest risk):
- A separate `std::fs::read_to_string` for the WARN pass, gated on `Ok(text)`. On a read error,
  the WARN is skipped and the canonical loud slug-named error is left to `load_single_config`
  (which re-reads the path). The typed parse remains the SOLE error source.
- `warn_locked_keys(text, slug)` — raw `toml::from_str::<toml::Value>` (degrades to no-warn on
  parse failure), iterates `flatten_present_keys`, warns where `!is_per_slug_overlayable(key)`.
  Content-free: emits `slug` + `key` only, never the operator's value (#4749).
- `flatten_present_keys(&raw)` — top-level leaves → `"key"`; sub-tables → `"section.subkey"`.
  One nesting level covers the entire registry surface. Table-shaped locks (`tls`/`http`) flatten
  to `tls.<field>` which hit the conservative-unknown default, so the WARN still fires.

Dedup (OQ-C / R-08): no dedup structure — the resolver runs once per slug per boot, the raw table
lists each key once, so the loop is naturally once-per-(slug,key)-per-boot. WARN keyed on the
`slug` argument, so per-slug isolation holds by construction. No cross-boot/cross-slug state.

## Files modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/http_provision.rs` (447 lines; added
  `is_per_slug_overlayable` import, the WARN call in the file-present arm, `warn_locked_keys`,
  `flatten_present_keys`)
- `/workspaces/unimatrix/crates/unimatrix-server/src/http_provision/slug_config_tests.rs` (17 new
  C5 tests appended to the existing `TempBase` harness)

## Tests

`cargo test -p unimatrix-server --bins slug_config_tests` → **27 passed, 0 failed** (10 pre-existing
vnc-040 + 17 new C5). Coverage:
- R-04/AC-04: WARN fires for locked key (sha256), silent for overlayable (nli_top_k); names key+slug,
  not value.
- Flip test: nli_top_k (overlayable, no WARN) vs rayon_pool_size (locked, WARN) — proves runtime
  registry binding (C5 half of the A→B flip).
- Conservative-unknown default for typo'd/unknown keys.
- No-hand-list structural assertion via `is_per_slug_overlayable`.
- R-07 WARN-only equivalence: merged value == global pin == `merge_configs` output; locked value
  ignored. Malformed file → sole error is existing `load_single_config` Config error. No-file arm
  unchanged (Cow::Borrowed, no WARN).
- R-08: single call warns once; two slugs same key → distinct per-slug WARN.
- R-12: table-shaped lock `tls.enabled` warns; sha256 divergence → both C5 and merge_configs WARNs
  present (acceptable). Empty file → no WARN.

`cargo build -p unimatrix-server` clean; `cargo clippy -p unimatrix-server --bins --tests` zero
warnings. Files formatted with `rustfmt --edition 2021` (no workspace-wide fmt).

## Issues / adjacent breakage flagged

- NONE blocking. Stayed strictly within `http_provision.rs` (+ its test submodule). Did NOT touch
  `infra/config.rs`, `main.rs`, or `projects.rs`.
- OBSERVED (not mine, not touched): `crates/unimatrix-server/src/{infra/config.rs, main.rs}` show as
  modified and `src/global_serve_seed_tests.rs` as new in the shared checkout — these are other
  vnc-041 agents' work (C1/C2/C4). Flagging only so the leader knows my git status line includes
  their files; I ran no git commands and made no edits to them.
- `#[allow(dead_code)]` on `resolve_slug_config`/`config_err`/`PROJECT_CONFIG_NAME` LEFT IN PLACE —
  the sole caller (main.rs per-slug loop) is still a separate wave; the WARN helpers are only
  reached through that dead-code-allowed fn and produce no dead_code warnings.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get(#5239 ADR-005) --
  surfaced ADR-005 (the binding decision), the two-level TOML merge pattern (#2395), and the
  registry single-consumption-point convention. Applied directly.
- Stored: entry #5241 "Locked-key seam WARN: raw toml present-key flatten + tracing-test runs in
  the BINARY target, not the lib" via /uni-store-pattern (test-target trap, table-shaped-lock
  validate interaction, permissive-field asymmetry, rustfmt edition gotcha).
