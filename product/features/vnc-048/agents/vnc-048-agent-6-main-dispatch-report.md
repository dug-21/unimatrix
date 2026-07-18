# Agent Report — vnc-048-agent-6-main-dispatch

Component 4: CLI wiring (`--slug` clap arg) + C-9 test green-up.

## Files modified

- `crates/unimatrix-server/src/main.rs` — added `--slug` clap field to `Export` and `Import`
  subcommands; threaded parsed value into `run_export`/`run_import` dispatch arms.
- `crates/unimatrix-server/src/main_tests.rs` — added `slug` to the `Export`/`Import` match
  patterns (asserting no-slug default is `None`, AC-05); added 4 vnc-048 tests (2 wiring, 2 help
  contract).
- `crates/unimatrix-server/tests/export_integration.rs` — threaded `None` into the 8 `run_export*`
  call sites (C-9 signature fallout); existing default-path assertions unchanged.

Committed as `e7a591a4  impl(main-dispatch): wire --slug clap arg into export/import (#953)`.
Staged only the three owned files — Wave 1-2 uncommitted work (export.rs, import/mod.rs,
projects.rs, import_integration.rs) left untouched for its owners.

## Clap arg names + threading

- Both subcommands get a subcommand-level `#[arg(long)] slug: Option<String>` (NOT the global
  `--project-dir`).
- Export dispatch: `slug.as_deref()` passed as arg 3 of `run_export(project_dir, output, slug,
  skip_quarantined, confirm)`.
- Import dispatch: `slug.as_deref()` passed as arg 3 of `run_import(project_dir, input, slug,
  skip_hash_validation, force)`.
- No validation in `main` — raw `Option<&str>` forwarded; validation is the downstream funnel's
  `validate_slug` edge (Component 1).
- Help text (AC-07/FR-15): both state base-derived-from-`--project-dir`, in-container posture,
  and "store dir under the base, not a registered [[projects]] entry"; import's help additionally
  carries the README restore-procedure pointer. Verified by 2 substring-assertion tests (whitespace
  collapsed to survive clap line-wrapping).

## Full-crate build/test/clippy status

- **Build**: `cargo build -p unimatrix-server --tests` — clean (0 errors, 0 warnings). The 8
  arity errors in export_integration.rs and the 2 E0027 pattern errors in main_tests.rs are
  resolved.
- **Test**: `cargo test -p unimatrix-server` — PASS. Full crate: 4563 + 137 + others, 0 failed.
  The 4 new CLI tests pass. bin+export_integration rerun after fmt: 137 + 21, 0 failed.
  - NOTE: one **flake** observed once in `import_integration` under the full-crate parallel run;
    it passed in isolation (RC=0) and on the immediate full re-run (RC=0). This is the known
    shared-state parallelism flake class (`.claude/rules/rust-workspace.md`), not caused by this
    change (which touches only clap parsing + test arity, no shared DB state).
- **Clippy**: `cargo clippy -p unimatrix-server --tests -- -D warnings` reports **2 pre-existing
  errors**, both in `crates/unimatrix-server/src/mcp/response/verbosity.rs:192,208`
  (`repeat().take()` → `repeat_n`, test code committed in vnc-044 #920). NOT in my diff, NOT a
  file I own, NOT introduced by this change. My three files are clippy-clean. Flagging as
  pre-existing out-of-scope tech debt for the leader (newer clippy toolchain surfaced it).

## Discipline

- Did NOT run crate-wide `cargo fmt`. Formatted only owned files via
  `rustfmt --edition 2024 --config skip_children=true <files>` so `mod`-child source files were
  not reflowed. `git status --short` confirms no out-of-scope file churn.
- No file exceeds 500 lines that wasn't already (main_tests.rs is a test module; main.rs unchanged
  in size class).

## Issues / blockers

- Pre-existing clippy failure in `verbosity.rs` blocks a clean `clippy --tests -- -D warnings` at
  the crate level. Recommend a separate one-line fix (`repeat_n`) outside this component's scope.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced #4059 (verify prerequisite signature
  before writing call sites), #4956 (`pub use` re-export of `pub(crate)` fns for integration
  tests), #335/#336 (export/import subcommand DB-open decisions). Confirmed the wiring approach;
  no gotchas contradicted the pseudocode.
- Stored: nothing novel — this was mechanical clap arg addition + C-9 arity green-up, a
  well-trodden pattern already covered by existing knowledge (signature-first call-site threading
  #4059, and the fmt-churn/skip_children discipline already in session memory). No runtime-invisible
  trap discovered.
