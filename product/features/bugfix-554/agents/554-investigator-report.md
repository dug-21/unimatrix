# Investigator Report — GH #554

**Agent ID**: 554-investigator

## Root Cause

`import/mod.rs:72` creates a `new_current_thread` Tokio runtime in the `Err` arm of `Handle::try_current()`. `embed_reconstruct.rs:140` calls `tokio::task::block_in_place`, which panics on current-thread runtimes. The insert phase completes; the panic fires at the start of embedding reconstruction.

## Affected Files

- `crates/unimatrix-server/src/import/mod.rs:72` — fix site (wrong runtime flavor)
- `crates/unimatrix-server/src/embed_reconstruct.rs:117,140` — panic site
- `crates/unimatrix-server/src/embed_reconstruct.rs:142` — latent same bug in block_sync_raw Err arm

## Proposed Fix

Change `new_current_thread` to `new_multi_thread` at both Err-arm locations. Add a plain `#[test]` (no tokio attribute) exercising the Err arm directly.

## Risk Assessment

Low. Isolated to CLI import path. Multi-thread is a superset of current-thread capabilities. No shared state with the running server.

## Missing Test

A `#[test]` (sync, no tokio attribute) calling `run_import` from a sync context — exercises the exact code path that panics in production.

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — returned entries #2126 (block_in_place pattern), #1146 (ADR-004 re-embed after commit), #2380 (prior tokio nesting fix procedure). Searched lesson-learned category — no prior instances found.
- **Stored**: Entry #4286 "CLI self-created runtime must be multi_thread when async body calls block_in_place" via `/uni-store-lesson`. Tagged `caused_by_feature:nan-002`.
