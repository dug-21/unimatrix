# ass-001: hnsw_rs Capability Spike

**Phase**: Assimilate (Research)
**Parent**: Pre-Roadmap Spike, Track 1A
**Date**: 2026-02-20
**Status**: In Progress

---

## Objective

Understand the natural operation shapes of hnsw_rs at the level of detail needed to design the MCP tool interface confidently. This is not production code — it is throwaway test harness research that answers specific questions.

## Research Questions

| # | Question | Why It Matters for Interface Design |
|---|----------|-------------------------------------|
| Q1 | Does `FilterT` support pre-filtering during search (not post-filter)? | Determines whether `memory_search` takes inline filter params or needs a separate filtered-search flow |
| Q2 | What does `search()` return? (IDs + distances? Ranked? Configurable k?) | Shapes the `memory_search` response schema |
| Q3 | Does `insert_parallel()` work reliably for batch operations? | Determines whether `memory_import` can be fast or needs sequential insertion |
| Q4 | What does `file_dump()` / reload look like? Format? Speed? Atomic? | Determines persistence model and whether we need redb for index state or just metadata |
| Q5 | Can the index handle mixed dimensionality or is it fixed at creation? | Determines whether switching embedding models (OpenAI 1536d -> local 384d) requires index rebuild |
| Q6 | What's the actual memory profile at 1K, 10K, 100K entries? | Informs per-project resource limits and whether we need quantization planning |
| Q7 | How does `DistCosine` vs `DistL2` choice affect retrieval quality for text embeddings? | Determines whether distance metric should be configurable per-project |

## Deliverable

**D1: hnsw_rs Capability Matrix** — documenting each operation, its parameters, return types, and constraints. Written as a reference for interface design (Track 3).

## Tracking

Research findings stored in `research/` subdirectory.
Final capability matrix: `research/D1-hnsw-rs-capability-matrix.md`
