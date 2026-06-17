# Test Plan — register CLI (`projects.rs`)

> Component: `crates/unimatrix-server/src/projects.rs` (`register`, State A/B/C) · Surface: `src/infra/projects_config_tests.rs` + project-lifecycle fixtures · Risks: R-05 (High), R-06 (High) · AC-02, AC-03, AC-04

## Scope
`register <slug>` writes `[[projects]]` routing intent atomically (temp+fsync+rename), is idempotent and re-attach-safe, and prints NO config.toml instructions (the `projects.rs:302-304`/`:335` State B/C prints removed). Same one command for project 1 and project N. Distroless/no-shell — Rust-binary only.

## Unit Test Expectations

### Routing-intent write, no instruction print (AC-02/AC-03)
- `test_register_writes_projects_stanza` — `register <slug>` from clean state writes a `[[projects]] slug = "<slug>"` entry to `config.toml` AND creates the per-slug data dir + genesis store.
- `test_register_prints_no_instructions` — assert the State B/C `eprintln!("...add to config.toml...")` strings are gone; nothing instructional is printed (AC-03).
- `test_nth_register_identical_command` — registering a 2nd slug uses the IDENTICAL command path and writes a 2nd `[[projects]]` entry with no manual edit (AC-04).

### Genesis-clobber guard (R-05 — hash chain is sacred)
- `test_re_register_re_attaches_no_clobber` — `register <slug>` against an EXISTING per-slug store OPENS it; assert the genesis block / chain-head hash is UNCHANGED (hash equality before == after). State B precedent.
- `test_re_register_idempotent_single_stanza` — running `register <slug>` twice yields exactly ONE `[[projects]]` entry and ONE untouched store (no duplicate stanza, no second genesis).
- `test_no_genesis_creation_when_dir_exists` — assert no genesis-creation code path runs when the per-slug data dir already exists.

### Atomic / additive config write (R-06)
- `test_config_write_atomic` — simulate interruption mid-write (temp+fsync+rename); assert the on-disk `config.toml` is ALWAYS the complete old OR complete new file, never partial/malformed.
- `test_config_write_preserves_existing_stanzas` — register into a config with N existing stanzas; assert all N+1 are intact and well-formed (read-modify-write preserves prior routing intent).
- `test_register_then_boot_reread` — write `[[projects]]`, re-read via `load_config_and_build_allowlist`; assert the slug is in `project_slugs` (full write→restart→resolve loop; pairs with boot-wiring.md).

## Edge Cases
- Re-register an already-registered slug (R-05) → re-attach, no clobber.
- Register a reserved name → rejected at the parse edge (delegated to reserved-slugs.md; assert `register` surfaces the rejection).
- Register into a non-empty config (R-06) → additive.

## Security (`config.toml` write trust boundary)
- `test_slug_regex_constrained_pre_write` — the slug is `ProjectSlug`-newtype regex-constrained BEFORE the write; assert no TOML metacharacter (newline, quote) can break the `[[projects]]` stanza (TOML-injection guard). A slug with `\n` or `"` is rejected, never written.

## Coverage Requirement
Re-register is provably re-attach (open), never genesis-clobber — chain-head hash invariant before == after; the write is atomic and additive — an interrupted register never yields a malformed config or loses an existing project's routing intent; the slug is regex-constrained pre-write (no TOML injection).
