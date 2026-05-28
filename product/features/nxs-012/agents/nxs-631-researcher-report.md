# nxs-631-researcher Report

## Summary

Explored the export/import pipeline (export.rs, format.rs, import/mod.rs, inserters.rs) and the three missing table schemas (graph_edges, observations, cycle_events). Analyzed the existing 8-table pattern, FK constraints, BLOB handling, bootstrap_only semantics, and format_version contract. Produced SCOPE.md with 21 acceptance criteria and 3 open questions for human review.

## Key Findings

1. **graph_edges has no FK constraints** -- source_id/target_id reference entry IDs but without FOREIGN KEY. No cascade ordering concern, but import should still follow entries.
2. **bootstrap_only edges should NOT be filtered** -- Migration only re-derives them when schema_version is behind. Same-version export/import skips migration, so bootstrap edges would be lost if excluded.
3. **cycle_events.goal_embedding is bincode BLOB** -- Model-version-specific. Must be excluded from export to avoid silent incompatibility.
4. **format_version bump is required** -- Old binaries reject unknown `_table` values via serde. Bumping to 2 ensures old import code fails cleanly on version check rather than on parse error.
5. **Observations table is bounded by retention GC** -- No filtering needed; exported set is already the retained set.
6. **All three tables use AUTOINCREMENT id** -- graph_edges.id is unreferenced (can omit). observations.id and cycle_events.id serve as watermarks/ordering keys (should preserve).

## Files Read

- `/workspaces/unimatrix/crates/unimatrix-server/src/export.rs` -- full export pipeline
- `/workspaces/unimatrix/crates/unimatrix-server/src/format.rs` -- ExportRow enum, row structs
- `/workspaces/unimatrix/crates/unimatrix-server/src/import/mod.rs` -- import pipeline
- `/workspaces/unimatrix/crates/unimatrix-server/src/import/inserters.rs` -- per-table inserters
- `/workspaces/unimatrix/crates/unimatrix-store/src/db.rs` -- DDL for all three tables
- `/workspaces/unimatrix/crates/unimatrix-store/src/migration.rs` -- bootstrap edge creation
- `/workspaces/unimatrix/crates/unimatrix-server/tests/export_integration.rs` -- test patterns
- `/workspaces/unimatrix/product/features/nan-002/SCOPE.md` -- prior import SCOPE for pattern

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- 19 entries returned. Key entries: #343 (JSONL migration pattern), #1161 (shared format module pattern), #1143 (format.rs ADR), #1144 (import Store::open ADR). All confirmed existing patterns match proposed approach.
- Stored: nothing novel to store -- all findings are feature-specific scope details that belong in SCOPE.md, not generalizable patterns.
