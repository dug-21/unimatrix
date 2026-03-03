# Component 3: Lesson-Learned — Test Plan

## Unit Tests (tools.rs)

### T-LL-01: build_lesson_learned_content with narratives (structured path)
- Report with narratives=Some([narrative1, narrative2]), recommendations=[rec1].
- Assert content includes narrative summaries.
- Assert content includes recommendation text.
- Assert content is non-empty.

### T-LL-02: build_lesson_learned_content without narratives (JSONL path)
- Report with narratives=None, hotspots with claims, recommendations=[rec1].
- Assert content includes hotspot claims (not narrative summaries).
- Assert content includes recommendation text.
- Assert content is non-empty.

### T-LL-03: build_lesson_learned_content empty fallback (R-09)
- Report with narratives=None, hotspots with empty claims, no recommendations.
- Assert content is non-empty (fallback text).

## Unit Tests (server.rs)

### T-LL-08: insert_with_audit sets embedding_dim from embedding length
- Call insert_with_audit with a 384-element embedding.
- Assert returned EntryRecord has embedding_dim == 384.

### T-LL-09: correct_with_audit sets embedding_dim from embedding length
- Call correct_with_audit with a 384-element embedding.
- Assert returned correction EntryRecord has embedding_dim == 384.

### T-LL-10: insert_with_audit with empty embedding sets embedding_dim = 0
- Call insert_with_audit with an empty vec![].
- Assert returned EntryRecord has embedding_dim == 0.

## Integration Tests

### T-LL-04: Embedding failure path (R-03)
- Mock embed service to return an error.
- Trigger context_retrospective with >= 1 hotspot.
- Assert: tool returns valid report (fire-and-forget does not block).
- Assert: lesson-learned entry exists with embedding_dim = 0.
- Assert: entry retrievable via context_lookup(category: "lesson-learned").

### T-LL-05: CategoryAllowlist guard (R-07)
- Remove "lesson-learned" from allowlist.
- Trigger context_retrospective with >= 1 hotspot.
- Assert: retrospective report returned successfully.
- Assert: no lesson-learned entry written.

### AC-06: Lesson-learned auto-persistence
- Call context_retrospective with >= 1 hotspot.
- Wait for fire-and-forget task to complete.
- context_lookup(category: "lesson-learned", topic: "retrospective/{fc}").
- Assert: entry exists with trust_source="system", embedding_dim > 0,
  non-empty content, tags contain "source:retrospective".

### AC-07: Supersede on second call
- Call context_retrospective twice for same feature_cycle.
- Wait for both fire-and-forget tasks.
- Assert: exactly 1 active entry with topic "retrospective/{fc}".
- Assert: prior entry has status=Deprecated, superseded_by set.

### AC-08: HNSW searchability
- After lesson-learned entry created (AC-06):
- context_search("permission retry patterns") or similar query.
- Assert: lesson-learned entry appears in search results.
- This verifies the HNSW vector was actually inserted (the critical fix).

### T-LL-06: Concurrent supersede race (R-04)
- Spawn two concurrent context_retrospective calls for same feature_cycle.
- Assert: at most 2 active entries (known tolerated limitation).
- Third single call reduces to exactly 1.

### T-LL-07: No write when zero hotspots and zero recommendations
- Report with hotspots=[], recommendations=[].
- Assert: no lesson-learned entry written (FR-06.8).
