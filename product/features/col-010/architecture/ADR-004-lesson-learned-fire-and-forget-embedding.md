# ADR-004: Lesson-Learned Entry Embedding via Fire-and-Forget

**Feature**: col-010
**Status**: Accepted
**Date**: 2026-03-02

## Context

AC-20 requires that `context_retrospective` auto-writes a `lesson-learned` entry with full ONNX embedding when ≥1 hotspot or recommendation is found. SR-07 identified that ONNX embedding on a 500-1000 token narrative adds 100-500ms to the `context_retrospective` response — an unbounded blocking step in what was previously a fast computation.

This is distinct from auto-outcome entries (§4 of ARCHITECTURE.md), which correctly skip embedding because they are structured metadata. Retrospective narratives are genuine semantic knowledge and must be embedded for `context_search` to surface them.

## Decision

The `lesson-learned` write (including ONNX embedding) is launched as a **fire-and-forget async task** via `tokio::spawn`. The `context_retrospective` tool returns its `RetrospectiveReport` to the caller immediately, before embedding completes.

```rust
// After building the RetrospectiveReport in context_retrospective handler:
if report.hotspots.len() > 0 || report.recommendations.len() > 0 {
    let narrative_content = build_lesson_learned_content(&report);
    let title = format!("Retrospective: {}", feature_cycle);
    let topic = format!("retrospective/{}", feature_cycle);
    let store_clone = Arc::clone(&store);
    let embed_clone = Arc::clone(&embed_service);
    // Fire-and-forget: does not block the retrospective response
    tokio::spawn(async move {
        write_lesson_learned_entry(
            &store_clone,
            &embed_clone,
            &title,
            &topic,
            &narrative_content,
            &feature_cycle,
        ).await;
    });
}
// Return report to caller immediately
```

If embedding fails, the entry is written without a vector embedding (`embedding_dim = 0`), logged at `warn` level, but the failure does not propagate to the caller. The entry remains queryable via `context_lookup` by topic/category even without an embedding.

AC-20 is satisfied when the entry *exists in Unimatrix* after the retrospective call — not that embedding completes synchronously. Integration tests for AC-20 should wait for the embedding task to complete (e.g., via a small delay or polling) rather than asserting synchronously.

## Rationale

The `context_retrospective` tool is called by agents during active development cycles. Adding 100-500ms of ONNX latency to every retrospective call that produces results would noticeably degrade agent responsiveness. The knowledge value of the lesson-learned entry is not time-critical — it matters for the *next* call, not the current one.

The fire-and-forget pattern is already established in the codebase: signal recording, co-access pair recording, and usage tracking all use `spawn_blocking` / `tokio::spawn` with fire-and-forget semantics.

Embedding failure degrades gracefully: the entry is still written and queryable by metadata filters. The `PROVENANCE_BOOST` still applies at search time (entry exists with `category = "lesson-learned"`). Only vector similarity queries miss it until a future retrospective re-embeds it via the supersede path.

## Consequences

- `context_retrospective` response latency is unaffected by ONNX embedding time.
- The lesson-learned entry appears in `context_search` results after the embedding task completes (~100-500ms after response).
- AC-20 integration tests must account for async embedding completion.
- Embedding failures are logged but do not degrade retrospective functionality.
- The supersede de-duplication check (AC-21) runs synchronously before spawning the write task — the check completes before the response is returned.
