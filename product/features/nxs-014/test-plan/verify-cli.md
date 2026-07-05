# Test Plan — verify-cli (`unimatrix-server/src/verify.rs` + `main.rs`, new)

> New `Command::Verify { }` + `run_verify(project_dir) -> Result<(), Box<dyn Error>>`. Covers R-10
> (CLI contract), the R-02 CLI-loader half, and the AC-11/AC-12 grep/file-check ACs. Tests mirror the
> existing `Export`/`Import` CLI subcommand integration tests (direct-DB, temp project dir, no running
> server). Reuse that harness; do not build a new one.

## R-10 / AC-09 — CLI verify contract (both exit-code branches + id-naming)

### `test_verify_cli_clean_corpus_exit_zero_with_summary`
- Arrange: a temp project dir + DB seeded with a clean corrected chain (via `correct_entry` or a
  seeded fixture DB), including at least one legacy entry.
- Act: invoke the verify subcommand (`run_verify(Some(project_dir))` or the `assert_cmd` binary run).
- Assert: exit code `0` AND stdout contains a summary of what was checked (e.g. entries/chains
  checked, `skipped_legacy` count). Not silent.

### `test_verify_cli_tampered_corpus_nonzero_exit_names_id`
- Arrange: seed a DB, then mutate a superseded entry's `content` directly (stale `content_hash`).
- Act: invoke verify.
- Assert: **non-zero** exit code AND output NAMES the offending `entry_id` and the break kind — NOT
  just a count "N problems" (guards #5180 green-on-detect and the AC-04/AC-09 id-naming requirement).
- **Teeth:** a `run_verify` that returns `Ok`/exit 0 on a populated `ChainReport`, or prints only a
  count, fails this test.

## R-02 (CLI half) — loader returns Deprecated predecessors (Critical, gating guard)

### `test_query_all_entries_returns_deprecated_rows` (loader guard — in `unimatrix-store`)
- Direct guard on the loader `run_verify` uses. Insert an entry, correct it (original → `Deprecated`).
- Assert `store.query_all_entries()` returns a row with `status == Deprecated`. If it filters to
  `Active`, this test fails loud and localizes the defect to the loader, not the core (R-02 scenario 2).

### `test_verify_cli_deprecated_predecessor_verifies_clean`
- Seed a DB with a real correction chain (predecessor `Deprecated`, successor `Active` chained).
- Act: invoke verify.
- Assert: exit `0` (clean) AND the summary reflects the Deprecated predecessor was checked (e.g.
  `checked` count includes it) — proving the CLI loader (`query_all_entries`) fed ALL statuses to the
  core. A false `MissingPredecessor` alarm here means the loader filtered to Active.

## R-10 — resolution + read-only open

### `test_verify_cli_opens_readonly`
- Assert `run_verify` opens the DB via `SqlxStore::open_readonly` (read-only). Verification: after a
  verify run, the DB file is unmodified (mtime/content unchanged), OR assert the open path is
  `open_readonly` at the call site (compile/review-level). Guards against a future widening to
  read-write (security risk note in RISK-TEST-STRATEGY §Security).

### `test_verify_cli_missing_project_dir_errors_cleanly`
- Invoke verify against a missing/invalid project dir. Assert it returns an error / non-zero exit
  cleanly (via `ensure_data_directory` resolution) and does NOT panic.

## AC-11 — no MCP tool added (grep/manual)
### `test_no_mcp_verify_tool_registered` (or review grep)
- Assert no new MCP tool for chain-verify is registered in the server tool surface (the tool list is
  unchanged vs baseline). Assert the core signature `verify_entries(&[EntryRecord]) -> ChainReport`
  carries no CLI/MCP/transport types (grep the signature; C-07, D-4, FR-09).

## AC-12 — no schema migration (grep/file-check)
### `test_schema_version_still_30`
- Assert `migration.rs` schema version is unchanged (still 30) and no new migration step was added
  (NFR-02, C-05). Grep/file-check; a diff to the migration list fails.

## Assertions summary (concrete)
- clean corpus → exit 0 + summary naming counts checked/skipped
- tampered corpus → non-zero exit + offending `entry_id` named (not a bare count)
- `query_all_entries()` returns `status = Deprecated` rows
- Deprecated predecessor counted as checked on the CLI path
- DB opened read-only (unmodified after run); invalid project dir → clean error, no panic
- no new MCP tool; core signature transport-free; schema version 30 unchanged
