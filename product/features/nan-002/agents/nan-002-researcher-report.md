# nan-002-researcher Report

## Summary

Explored the full problem space for Knowledge Import (nan-002). Read the export implementation, CLI structure, database schema, write paths, migration patterns, embedding pipeline, vector index construction, and hash chain system. Produced SCOPE.md with 24 acceptance criteria covering the complete import pipeline.

## Key Findings

1. **Import cannot use Store::insert()** -- The Store API auto-generates IDs, timestamps, content hashes, and confidence. Import must use direct SQL INSERT to preserve original values, following the v5-to-v6 migration pattern in `migration.rs`.

2. **Re-embedding is synchronous** -- `OnnxProvider::new()` and `embed_batch()` are both synchronous. Import can follow the hook/export sync pattern with no tokio runtime.

3. **Dependency-ordered export simplifies import** -- nan-001 emits tables in FK-compatible order (entries before entry_tags). Streaming line-by-line insertion within a single transaction works without buffering.

4. **Content hash validation is straightforward** -- `compute_content_hash(title, content)` in `hash.rs` can be called during import to verify each entry's stored hash. Chain validation (previous_hash -> content_hash linkage) requires a post-insertion pass.

5. **Vector index must be persisted** -- After building the HNSW index, it needs to be saved to the `vector/` directory. The existing VectorIndex persistence API handles this.

6. **Empty-database guard is essential** -- Without ID remapping logic (out of scope), importing into a populated database would cause primary key conflicts. The scope explicitly requires an empty target.

## Files Read

- `/workspaces/unimatrix/product/PRODUCT-VISION.md` (nan-002 requirements)
- `/workspaces/unimatrix/product/features/nan-001/SCOPE.md` (export contract)
- `/workspaces/unimatrix/product/features/nan-001/architecture/ARCHITECTURE.md` (JSONL format spec)
- `/workspaces/unimatrix/crates/unimatrix-server/src/export.rs` (export implementation)
- `/workspaces/unimatrix/crates/unimatrix-server/src/main.rs` (CLI structure)
- `/workspaces/unimatrix/crates/unimatrix-store/src/schema.rs` (EntryRecord, types)
- `/workspaces/unimatrix/crates/unimatrix-store/src/hash.rs` (content hash)
- `/workspaces/unimatrix/crates/unimatrix-store/src/db.rs` (Store::open, lock_conn)
- `/workspaces/unimatrix/crates/unimatrix-store/src/write.rs` (insert pattern)
- `/workspaces/unimatrix/crates/unimatrix-store/src/write_ext.rs` (vector_map, co_access writes)
- `/workspaces/unimatrix/crates/unimatrix-store/src/migration.rs` (migration patterns, schema version)
- `/workspaces/unimatrix/crates/unimatrix-embed/src/lib.rs` (embed crate API)
- `/workspaces/unimatrix/crates/unimatrix-embed/src/text.rs` (embed_entry, embed_entries)
- `/workspaces/unimatrix/crates/unimatrix-embed/src/provider.rs` (EmbeddingProvider trait)
- `/workspaces/unimatrix/crates/unimatrix-vector/src/index.rs` (VectorIndex construction)
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/embed_handle.rs` (async embed pattern)

## Scope Boundaries

- **In**: Full restore from nan-001 JSONL, re-embedding, hash validation, schema check, CLI subcommand
- **Out**: Merge/append, incremental import, MCP tool, remote import, adaptation state, format negotiation

## Open Questions for Human

1. Should stdin (`--input -`) be supported, or file-only?
2. Should a `--force` flag exist to clear existing data before import?
3. Is progress reporting to stderr needed for large imports?
4. Should the import operation itself be recorded in the audit log?
5. Is it acceptable that adaptation state (MicroLoRA) resets after import?

## Knowledge Stewardship

- Queried: /query-patterns for "knowledge import restore backup schema compatibility embedding" -- 5 results, mostly schema migration patterns. No direct import patterns found.
- Stored: nothing novel to store -- findings are feature-specific (import pipeline design). The direct-SQL-for-bulk-write pattern and dependency-ordered emission are already captured in nan-001 and migration code.
