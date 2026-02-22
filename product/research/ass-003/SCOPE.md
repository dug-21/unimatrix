# ass-003: redb Storage Patterns Spike

**Phase**: Assimilate (Research)
**Parent**: Pre-Roadmap Spike, Track 1B
**Date**: 2026-02-20
**Status**: In Progress

---

## Objective

Understand the natural operation shapes of redb at the level of detail needed to design the metadata persistence layer for Unimatrix. redb serves as the crash-safe transactional store alongside hnsw_rs (which lacks atomic persistence).

## Research Questions

| # | Question | Why It Matters for Interface Design |
|---|----------|-------------------------------------|
| Q1 | Can redb do range queries on timestamps efficiently? | Determines whether `memory_list` supports "entries since X" natively or needs secondary indexing |
| Q2 | Multiple named tables in one DB vs. separate DB files per table? | Determines physical storage layout for metadata, vectors, lifecycle state |
| Q3 | What's the transaction model? Single writer + multiple readers? | Determines whether concurrent search + insert is safe, which affects tool behavior under load |
| Q4 | How do typed tables work? Can we store structured metadata (not just blobs)? | Determines metadata schema richness — flat key-value or structured types |
| Q5 | What's the practical size limit before performance degrades? | Informs when "project too large" warnings should trigger |

## Deliverable

**D2: redb Storage Pattern Guide** — documenting table layout, transaction patterns, and query capabilities. Written as a reference for persistence layer design (Track 3).

## Tracking

Research findings stored in `research/` subdirectory.
Final storage pattern guide: `research/D2-redb-storage-pattern-guide.md`
