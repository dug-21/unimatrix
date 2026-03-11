# nan-001-researcher Report

## Task
Research the problem space for nan-001 (Knowledge Export) and produce SCOPE.md.

## Key Findings

### Database Schema (v11)
- 20 tables total in SQLite. 19 should be exported (vector_map excluded as derived data).
- Only 2 actual foreign key constraints: `entry_tags -> entries` and `observation_phase_metrics -> observation_metrics`. Both have ON DELETE CASCADE.
- The `shadow_evaluations` table has one BLOB column (`digest`) -- the only non-text column in the entire schema. Requires base64 encoding for text-only export.
- `entries` table has 26 columns including hash chain fields (`content_hash`, `previous_hash`) critical for correction chain integrity.
- Tags are stored in a separate junction table (`entry_tags`), NOT in the entries table.

### CLI Architecture
- Binary uses clap with `#[derive(Subcommand)]`. Currently one subcommand: `Hook`.
- Hook subcommand runs synchronously (no tokio) for latency. Export should follow same pattern.
- `--project-dir` flag exists on root CLI struct and is available to all subcommands.
- `Store::open()` handles migration on startup. Export can reuse this -- database will always be at current schema version.

### Prior Art: JSON-Lines Format
- The nxs-008 redb-to-SQLite migration (Unimatrix #333, #335, #336, #343) established JSON-Lines as the intermediate format for data migration.
- ADR-003 (#335) chose direct database access over Store API for export operations.
- ADR-004 (#336) chose Store::open() then raw SQL for import operations.
- These decisions from nxs-008 directly inform nan-001/nan-002 design.

### Format Design
- Single JSONL file with `_table` discriminator per line.
- Header line with `format_version`, `schema_version`, `exported_at`, `entry_count`.
- Dependency-ordered table emission enables streaming import.
- Deterministic output (sorted by primary key) enables diffing.

## Artifacts Produced
- `/workspaces/unimatrix/product/features/nan-001/SCOPE.md` -- 18 ACs, full scope definition

## Open Questions for Human
1. Should operational tables (observations, sessions, injection_log, query_log) be included by default? They can be large. Consider a `--knowledge-only` flag.
2. Should `shadow_evaluations` (with its BLOB column) be included?
3. Should counters be exported verbatim or should nan-002 recompute them?

## Risks
- **Format lock-in**: Once nan-002 is built against format_version 1, changing the format requires migration logic in the importer. Getting the format right now is high-leverage.
- **Large operational tables**: A busy project could have 50k+ observation rows. Export might be slow or produce large files without a filtering option.
- **base64 dependency**: The `base64` crate is not currently in the server's dependency tree. Need to add it or find an alternative for the single BLOB column.

## Knowledge Stewardship
- Queried: /query-patterns for "storage schema tables data model export backup" -- found 4 relevant ADRs from nxs-008 migration (#333, #335, #336, #343) confirming JSON-Lines format and direct SQL access patterns
- Stored: nothing novel to store -- agent lacks Write capability (permission error). The schema dependency ordering pattern would be useful for future reference but could not be persisted.
