# Agent Report: nan-001-agent-1-pseudocode

## Task
Produce per-component pseudocode for nan-001 (Knowledge Export): cli-extension, export-module, row-serialization.

## Artifacts Produced

| File | Description |
|------|-------------|
| `product/features/nan-001/pseudocode/OVERVIEW.md` | Component interaction, data flow, shared types, sequencing |
| `product/features/nan-001/pseudocode/cli-extension.md` | Command enum extension and dispatch in main.rs |
| `product/features/nan-001/pseudocode/export-module.md` | run_export orchestration, transaction management, writer setup |
| `product/features/nan-001/pseudocode/row-serialization.md` | Per-table SQL-to-JSON mapping for all 8 tables |

## Components Covered

1. **cli-extension** -- `Command::Export` variant, match arm dispatch, project_dir wiring
2. **export-module** -- `run_export()`, `do_export()`, `write_header()`, writer setup (file vs stdout)
3. **row-serialization** -- 8 table export functions, type conversion rules, nullable helpers, write_row helper

## Key Design Decisions Followed

- ADR-001: BEGIN DEFERRED transaction wraps all 8 table reads
- ADR-002: Explicit column-to-JSON mapping via serde_json::Value, no Rust struct intermediary, JSON-in-TEXT emitted as raw strings
- ADR-003: serde_json `preserve_order` feature for insertion-order key determinism
- Architecture signature: `run_export(project_dir: Option<&Path>, output: Option<&Path>)` (not spec's store-based variant)
- Hook subcommand pattern: synchronous, no tokio runtime

## Open Questions

1. **Module registration location**: Confirmed `pub mod export;` goes in `crates/unimatrix-server/src/lib.rs` alongside existing `pub mod uds;`, `pub mod server;`, etc.

2. **query_map vs query pattern**: Pseudocode uses `query_map` for clarity but notes that the `query` + `while let Some(row) = rows.next()?` pattern may be cleaner for the implementation to avoid double-Result nesting. Implementation agent should choose.

3. **No ADR files found**: The spawn prompt mentioned reading `ADR-*.md` files but none exist as separate files. All ADR decisions are documented inline in ARCHITECTURE.md and IMPLEMENTATION-BRIEF.md. This is not a blocker -- all decisions were extracted from those documents.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names -- every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections -- no gaps found
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/nan-001/pseudocode/`
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: codebase patterns via direct file reads (main.rs Command enum, hook.rs sync pattern, db.rs Store::open/lock_conn, project.rs ensure_data_directory, lib.rs module declarations) -- all patterns confirmed and followed
- Deviations from established patterns: none. Export follows the hook subcommand pattern exactly (sync dispatch from main, no tokio). Module registered in lib.rs like all other server modules.
