# Test Plan — main.rs (clap `--slug` wiring + help text)

Change: add the `--slug <name>` clap arg to the `Export`/`Import` subcommands and thread it into the
`run_export`/`run_import` calls (`main.rs:556-567`). Help text per FR-15. Risks: R-08 (edge wiring),
R-12 (help discoverability), R-09 (no-slug default).

The clap layer is thin; most behavior is proven in export.md / import.md through the `run_*` entry
points. These tests prove the CLI edge is wired and the help contract holds.

## Wiring

- `test_cli_export_slug_threads_into_run_export` / `test_cli_import_slug_threads_into_run_import` —
  drive the compiled binary (or clap-parse unit) with `export --slug foo` / `import --slug foo`;
  assert the value reaches `run_export`/`run_import` as `Some("foo")`, and absence yields `None`
  (no-slug default, AC-05). If a binary-level test is heavy, a clap-parse assertion on the parsed
  struct suffices for wiring; the behavioral proof is in the integration files.
- `test_cli_slug_invalid_rejected_at_edge` — `export --slug 'Foo!'` (and one reserved, one traversal)
  → non-zero exit, error before any FS/DB (AC-04). Confirms validation is reached via the real CLI
  dispatch, not only the library entry point.

## Help text (AC-07, FR-15) — R-12

- `test_cli_export_slug_help_states_contract` — `export --help` (or `--slug` help) contains: (a) base
  derived from `--project-dir`, (b) in-container posture is the expected invocation, (c) `--slug`
  means "a store dir under the base," not "a registered project."
- `test_cli_import_slug_help_carries_readme_pointer` — import's `--slug` help contains all of the
  above **plus** the one-line pointer to the README restore procedure.
- Snapshot/assertion style: substring assertions on `--help` output (not a brittle full-snapshot) so
  wording tweaks that preserve the contract don't false-fail.

## Notes

- No shared runtime path modified (C-9): the only `main.rs` edit is the two subcommand arg additions
  + call-site threading at `556-567`.
- Both commands stay sync pre-tokio subcommands (C-8) — the clap arg addition does not change the
  dispatch model.
</content>
