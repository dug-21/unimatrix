# Agent Report: nan-002-agent-2-spec

## Status: COMPLETE

## Artifact Produced

- `/workspaces/unimatrix/product/features/nan-002/specification/SPECIFICATION.md`

## Key Decisions

1. **Transaction boundary placement**: The database transaction wraps JSONL ingestion and hash validation through COMMIT. Re-embedding occurs after commit because vector index writes are separate from SQLite transactions. If re-embedding fails post-commit, the database has valid data but no vector index — the server can rebuild on startup. This avoids holding a long transaction during CPU-intensive embedding work.

2. **Shared format types recommended**: Per SR-08 and SR-09, the specification recommends (but does not mandate) that the architect define shared row struct definitions between export and import modules. This reduces format drift risk when columns are added. The architect decides the implementation approach.

3. **--force safety level**: SCOPE.md defines --force as a single flag. SR-04 recommends double-opt-in (--force --yes). The specification follows SCOPE.md (single flag) but adds stderr logging of the entry count being dropped, and notes double-opt-in as a future iteration in NOT in scope.

4. **No --skip-embedding mode**: SR-01 suggests a dry-run mode that skips embedding for air-gapped environments. This is not in SCOPE.md acceptance criteria, so it is explicitly listed in NOT in scope. The architect can choose to add it if the risk warrants.

5. **PID check is warning-only**: Per SCOPE.md and SR-07, detecting a running server via PID file results in a stderr warning, not a blocking error. The import proceeds. This matches the scope's "warn if a server is running" language.

## Open Questions

1. **Vector index rebuild after embedding failure**: If the database transaction commits successfully but re-embedding fails (e.g., ONNX model unavailable), should the user be able to re-run import without --force (since entries exist)? Or should re-embedding be retriable independently? The current spec requires --force to re-run, which would re-insert all data. The architect may want a separate re-embed-only path.

2. **Shared format module location**: SR-08 recommends shared types between export and import. Should these live in `unimatrix-server/src/format.rs` (shared within the server crate) or in `unimatrix-store` (shared across crates)? The architect should decide based on dependency direction.

3. **Counter completeness**: The export may contain counters beyond `next_entry_id`, `next_signal_id`, and `schema_version`. The spec says "restore all counters present in the export." The architect should verify the full counter set from export.rs to ensure none are missed.

## Knowledge Stewardship

- Queried: SCOPE.md and SCOPE-RISK-ASSESSMENT.md directly for nan-002 domain -- primary inputs for all requirements and risk mitigations
- Queried: export.rs for format contract -- confirmed 8 tables, header structure, emission order
- Queried: migration.rs for CURRENT_SCHEMA_VERSION -- confirmed 11
- Queried: main.rs for CLI pattern -- confirmed synchronous subcommand model
