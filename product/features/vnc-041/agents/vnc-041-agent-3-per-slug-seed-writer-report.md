# vnc-041 C3 — Per-slug seed writer — Agent Report

Agent: vnc-041-agent-3-per-slug-seed-writer
Component: C3 (per-slug seed writer in `ProjectRegistry::register`, ADR-002 #5236)

## Summary

Implemented C3: `ProjectRegistry::register` now seeds file (b) — the editable per-slug
`config.toml` — at BOTH success branches (State B re-attach + State C genesis), after
`ensure_project_stanza`. Writes ONLY (b); never touches the shared (a)≡(c) path-hash file.
Best-effort (warn-and-continue via the C1 primitive); no `.unwrap()`, no `?` on the seed.

## Files modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/projects.rs`
  - `use crate::infra::config;` import added.
  - `const PROJECT_CONFIG_NAME: &str = "config.toml";` (byte-identical to the resolver's
    module-private const) — file (b) name.
  - NEW private method `ProjectRegistry::write_per_slug_seed(&self, slug)`:
    `path = per_slug_data_dir(&self.base_dir, slug).join(PROJECT_CONFIG_NAME)` (the SINGLE
    join site, SR-09), `body = config::render_per_slug_seed_toml()` (C2),
    `config::write_if_absent(&path, &body)` (C1 no-clobber, best-effort).
  - Two additive call sites: `self.write_per_slug_seed(&slug);` after `ensure_project_stanza`
    at State B (re-attach) and State C (genesis), before the success `println!`. No signature
    changes to `register` / `ensure_project_stanza` / `per_slug_data_dir` (SR-08/R-09).
  - File is exactly 500 lines (within the max-500 rule).

- `/workspaces/unimatrix/crates/unimatrix-server/src/projects/tests.rs`
  - 14 new C3 tests (section F), all green. Coverage:
    - R-05/AC-02: (b) at exactly `per_slug_data_dir(base, slug).join("config.toml")`; seed
      body == `render_per_slug_seed_toml()` verbatim; (b) is a sibling, not inside the
      path-hash dir; (b) at the resolver's literal probe formula.
    - R-05 isolation: (a)/(c) byte-identical across a seeding register vs a control register
      (the seed targets a different file); per-slug legend never leaks into (a)/(c).
    - R-13: seed on State C genesis AND State B re-attach; State A errors with NO seed.
    - R-03/AC-05: pre-placed operator (b) survives (no-clobber); double-register is a no-op.
    - R-10: seed-write failure (directory at the (b) path) does NOT fail register; store +
      stanza still land.
    - Round-trip (in-scope half): the seeded (b) is resolver-loadable via the pub
      `load_single_config` -> `validate_config` -> `merge_configs` -> `validate_config` path.

## Tests

- `cargo test -p unimatrix-server --lib projects::tests`: 48 passed, 0 failed (14 new C3).
- `cargo test -p unimatrix-server --lib --bins`: 4280 + 123 passed, 0 failed.
- `cargo build --workspace`: passes.
- `cargo clippy -p unimatrix-server --lib --bins`: zero warnings.
- `rustfmt --edition 2024 --check` on both edited files: clean.

## Issues / FLAGs

1. ENVIRONMENTAL (not a code defect): the hardened `cargo test --workspace` gate run FAILS
   to LINK the `import_integration` test binary with `collect2: fatal error: ld terminated
   with signal 9 [Killed]` — the linker was OOM-killed under full-workspace link pressure.
   This is the cross-crate memory pressure the workspace rules warn about, unrelated to C3.
   My code compiles and the scoped per-crate lib+bin run (4403 tests) is fully green. A
   gate/CI run on a higher-memory host (or per-crate) is needed to confirm the full
   workspace suite; per-crate is the authoritative scoped result here.

2. SCOPE-BOUND test-placement FLAG: the EMPIRICAL resolver round-trip
   (`register` -> call `resolve_slug_config` and assert `Cow::Owned`) cannot be implemented
   within my boundary. `resolve_slug_config` is in the BINARY crate (`main.rs` mod), while
   `register` + the test ctor `with_dirs` are in the LIB crate and `with_dirs` is
   `#[cfg(test)]`+private; `run_project_command` hardcodes `base_dir=None` -> real HOME, and
   `#![forbid(unsafe_code)]` + Rust 2024 forbid `std::env::set_var` so HOME can't be
   overridden for isolation. Wiring a new bin-target test module requires a `#[cfg(test)] mod`
   declaration in `main.rs`, which is OUT OF SCOPE (boundary: do NOT touch main.rs). I closed
   the seam in-scope instead: the lib test proves register writes the C2 body at the
   resolver's LITERAL path formula AND that the body is resolver-loadable via the same
   pub load/validate/merge functions the resolver uses. RECOMMENDATION for the leader: if the
   empirical-resolver round-trip is required, add the bin-target test to the existing
   `per_slug_loop_tests.rs` (already registered in main.rs) — a one-line `mod` edit is not
   needed there. See stored pattern #5243.

3. Out-of-scope fmt churn AVOIDED: `cargo fmt -p` reformatted import ordering in
   `http_provision.rs` and `http_provision/slug_config_tests.rs` (a sibling agent's
   uncommitted C5 work). I reverted that fmt churn by hand so only my two files remain
   modified. Those C5 files are byte-restored to their pre-fmt working-tree state.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing / context_search -- found ADR-002 (#5236, binding),
  ADR-004 (#5238), ADR-005 (#5239); patterns #5091 (register inverts delete/purge contract),
  #5212 (resolve_slug_config child test module avoids main.rs mod collision + per-field-replace
  merge), #5175 (config-parity binary-crate provisioner test). Applied #5212's module-layout
  insight directly.
- Stored: entry #5243 "unimatrix-server lib/bin split blocks register->resolve round-trip in
  one test; prove each half against the shared path formula" via /uni-store-pattern (captures
  the crate-split test-placement trap + the load_single_config preset-enrichment gotcha that
  makes `merged == UnimatrixConfig::default()` a false assertion for a pristine seed).
