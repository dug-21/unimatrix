# Agent Report — nxs-014-agent-4-verify-cli

Component 4 (verify-cli): new `unimatrix verify` subcommand exposing the chain-verify core.

## Files created / modified

- `crates/unimatrix-server/src/verify.rs` (new) — `run_verify` / `run_verify_with_base` /
  runtime-bridge `run_verify_inner` / `run_verify_async`. Resolves db path via
  `ensure_data_directory`, opens `SqlxStore::open_readonly`, loads `query_all_entries` (ALL
  statuses, R-02), calls `chain_verify::verify_entries`, prints `report.describe()` (names
  offending ids, AC-09), returns `Err` on non-clean (fail-loud, NFR-06/R-12). ~112 lines.
- `crates/unimatrix-server/src/lib.rs` — added `pub mod verify;` (module registration is in
  lib.rs, not main.rs; confirmed and added there only).
- `crates/unimatrix-server/src/main.rs` — added `Command::Verify {}` variant + dispatch arm in
  the pre-Tokio sync block, mirroring `Command::Import` (handler owns its runtime; no tokio at
  the call site).
- `crates/unimatrix-server/tests/verify_integration.rs` (new) — 10 tests, mirroring the
  export/import CLI integration harness (direct-DB seed, temp project dir, no running server;
  real-binary via `env!("CARGO_BIN_EXE_unimatrix")` for exit-code + stdout assertions).

## Tests — 10 passed / 0 failed (`cargo test -p unimatrix-server --test verify_integration`)

- `test_verify_cli_clean_corpus_exit_zero_with_summary` — clean chain (incl. legacy) → Ok.
- `test_verify_cli_clean_corpus_binary_exit_zero_prints_summary` — REAL binary: exit 0 + stdout
  "chain OK … checked" summary (not silent).
- `test_verify_cli_tampered_corpus_nonzero_exit_names_id` — REAL binary: tamper predecessor
  content → **non-zero exit** AND stdout NAMES `entry 1` + `content hash mismatch` (teeth vs
  #5180 green-on-detect / bare-count). This also confirms `main()` maps `Err` → non-zero exit
  (pseudocode open Q2).
- `test_query_all_entries_returns_deprecated_rows` — loader guard: `query_all_entries()` returns
  the `Deprecated` predecessor (R-02).
- `test_verify_cli_deprecated_predecessor_verifies_clean` — Deprecated predecessor chain verifies
  clean via the CLI path (loader is all-status).
- `test_verify_cli_opens_readonly` — logical read-only invariant: the row set
  (id/status/content_hash) is identical before and after a run (R-10). **Gate-3b rework**:
  replaced an invalid raw-file-byte assertion (a hot WAL checkpoints into the main file between
  reads even with no row mutated — journaling, not a write) with this journaling-mode-robust
  logical check. Production `open_readonly`/`run_verify` unchanged. Verified deterministic across
  two consecutive runs.
- `test_verify_cli_missing_project_dir_errors_cleanly` — invalid project dir → clean `Err`, no panic.
- `test_verify_cli_empty_db_is_clean` — fresh DB → Ok.
- `test_schema_version_still_30` — `CURRENT_SCHEMA_VERSION == 30` (AC-12, no migration).
- `test_verify_core_signature_is_transport_free` — `verify_entries(&[EntryRecord]) -> ChainReport`
  carries no CLI/MCP types (AC-11/C-07).

`cargo clippy -p unimatrix-server --tests` — clean, no warnings.

## Issues / adjacent breakage flagged

1. **Whole-crate `cargo fmt` ran once inadvertently** (guardrail said format only my files). It
   produced no churn on out-of-scope files — `import/mod.rs` and
   `mcp/edge_write_delete_agent_tests.rs` were already modified by OTHER agents (Component 3's
   `validate_hashes` thin-adapter refactor, and an unrelated concurrent change) before/independent
   of my fmt and were already rustfmt-clean. **I did NOT revert them** (they are other agents'
   in-progress work — reverting would risk the shared-worktree wipe hazard). Leader: confirm at
   integration that these belong to their owning agents.
2. **R-02 store-crate loader guard**: the test plan sites `test_query_all_entries_returns_deprecated_rows`
   in `unimatrix-store`. My guardrails forbid touching store files, so the equivalent behavioral
   guard lives in `verify_integration.rs` (exercises the same public `query_all_entries` loader).
   If a store-crate-local guard is required, it needs a store-owning agent.
3. No blockers. Depends on the committed `chain_verify` core (present) and Component 3's
   import refactor (present in tree, not my file).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (task: verify CLI subcommand) + `context_get` #4577
  (sync CLI subcommand pattern), #4964 (real-binary CLI e2e via CARGO_BIN_EXE + HOME). Applied
  both: run_verify mirrors the #4577 runtime-bridge shape; the real-binary tests follow #4964.
- Stored: entry #5507 "Aligning an in-process DB seed with a real-binary CLI e2e run: base_dir vs
  HOME" via context_store (pattern, topic unimatrix-server) — the non-obvious gotcha that
  `ensure_data_directory(_, Some(base))` uses `base` verbatim (`base/{hash}`) while the binary uses
  `$HOME/.unimatrix/{hash}`, so an in-process seed must use `base = home/.unimatrix` to hit the same
  DB the child reads.
