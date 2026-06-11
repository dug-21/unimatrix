# Agent Report — vnc-034 Wave 2, Agent 3 (projects-config)

Issue #727 · Stage 3b Wave 1 of 3 (config foundation) · branch `feature/vnc-034-wave2`
Commit `f1e63678` — `impl(projects-config): [[projects]] parse + D1/D5 slug validation (#727)`

## Scope delivered

`[[projects]]` config parsing + reserved-slug refusal in `infra/config.rs`, per
`wave2/pseudocode/projects-config.md` and `wave2/test-plan/projects-config.md`.

## Files modified / created

- `crates/unimatrix-server/src/infra/config.rs` (modified)
- `crates/unimatrix-server/src/http/router/tests.rs` (modified — added the 2 D1 discriminators)
- `crates/unimatrix-server/src/infra/projects_config_tests.rs` (new — focused sibling test file,
  wired via `#[cfg(test)] #[path = ...] mod`, mirroring the existing `graph_penalty_config_tests.rs`
  pattern since `config.rs` is already 11.6k lines)

## Tests

- New: 18 config-side tests (`projects_config_tests`) — all pass.
- New: 3 seam discriminators in `http/router/tests.rs` — all pass (`test_slug_reject_underscore_discriminator`,
  `test_slug_reject_64_char_discriminator`, `test_slug_accept_63_char_boundary`).
- Regression: full `--lib` suite 3966 passed, 1 ignored, plus ONE PRE-EXISTING FLAKE unrelated to this
  work — `http::token::tests::test_concurrent_creation_no_corruption` (a filesystem race in
  `load_or_generate_token`; passes in isolation, touches no config/slug code). Net new failures: 0.
- `cargo clippy -p unimatrix-server --all-targets`: zero warnings on new code.
- `cargo fmt --check`: clean.

## Decisions honored

- **D1**: charset validation reuses the merged Wave-1 `ProjectSlug::TryFrom`
  (`^[a-z0-9][a-z0-9-]{0,62}$`). No second validator, regex not re-spelled.
- **D5**: `RESERVED_SLUGS`/`is_reserved_slug` defined here (canonical owner). Reserved check is
  SEPARATE from and AFTER the charset check; `tools` is charset-valid yet rejected (proven by
  `test_reserved_check_is_separate_from_charset`).
- **D2**: no config-overlay — `ProjectConfigEntry` is `slug`-only; negative test asserts no overlay merge.
- **OQ-CFG-1**: resolved WITHOUT extraction. `http` and `infra` are sibling modules in ONE crate, so
  `infra/config.rs` uses `crate::http::ProjectSlug` directly — an intra-crate module reference graph
  may contain cycles and compiles fine (the no-cycle rule is between crates only). Regex NOT duplicated.
- **OQ-CFG-2**: validated slugs carried on `ConfigLoadResult.projects: Vec<ProjectSlug>` (validate-once
  at load; downstream consumes the typed list).

## Note for later waves — exact names/locations

All in `crates/unimatrix-server/src/infra/config.rs`, re-exportable via `crate::infra::config::…`:

- `pub const RESERVED_SLUGS: [&str; 4] = ["v1", "health", "observe", "tools"];`
- `pub fn is_reserved_slug(slug: &ProjectSlug) -> bool` — exact-match only (no prefix/substring).
- `pub fn validate_projects_config(entries: &[ProjectConfigEntry], path: &Path) -> Result<Vec<ProjectSlug>, ConfigError>`
  — runs D1 charset → D5 reserved → duplicate, in that order; returns validated/deduped `Vec<ProjectSlug>`.
- `pub struct ProjectConfigEntry { pub slug: String }` — the `[[projects]]` stanza type.
- `UnimatrixConfig.projects: Vec<ProjectConfigEntry>` (`#[serde(default)]`; absent ⇒ empty).
- Validated-slug accessor: `ConfigLoadResult.projects: Vec<ProjectSlug>` (populated by `load_config`).
- New `ConfigError` variants: `ProjectSlugInvalid { path, value }`, `ProjectSlugReserved { path, value }`,
  `ProjectSlugDuplicate { path, value }`. The `ProjectSlugInvalid` Display text contains the literal
  `^[a-z0-9][a-z0-9-]{0,62}$` (gate/PR-review assertion anchor).

The CLI/registry wave should IMPORT `RESERVED_SLUGS`/`is_reserved_slug` from here (D5 single source —
no second list). The ProjectRouter wave consumes `ConfigLoadResult.projects` (already-validated slugs).

## Issues / blockers

None. The one red line in the full-suite run is a pre-existing token-concurrency flake, not introduced here.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) — surfaced #2395 (two-level TOML merge,
  dsn-001 — confirms config precedence is split out per D2), #3928 (nested-section toml test convention),
  and the vnc-034 ADR set (#4949 ADR-005 default alias, #4952 ADR-006 wave mapping). Applied the
  nested-section TOML test convention and the ADR-005 `tools`-shadow rationale.
- Stored: entry #4971 "OQ-CFG-1 resolved: intra-crate module 'cycle' is not a cycle — config can use
  crate::http::ProjectSlug directly" via context_store (pattern, topic unimatrix-server).
