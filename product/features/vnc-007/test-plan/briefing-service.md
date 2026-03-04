# Test Plan: BriefingService (services/briefing.rs)

## Test Infrastructure

BriefingService unit tests use real AsyncEntryStore (backed by tempdir redb) + SecurityGateway::new_permissive(). SearchService is the real implementation but embed_service is not started (triggers EmbedNotReady for semantic tests).

For tests that need to verify include_semantic=false isolation (R-04), a custom approach is needed: verify that when include_semantic=false, the code path never calls SearchService. This can be tested by providing a task but setting include_semantic=false and confirming no embedding error occurs.

## Test Scenarios

### T-BS-01: Convention lookup with role (AC-02)
```
Setup: Store 2 convention entries with topic="architect"
Params: role=Some("architect"), include_conventions=true, include_semantic=false
Assert: result.conventions contains both entries
Assert: result.injection_sections is empty (all vecs)
Assert: result.relevant_context is empty
Assert: result.entry_ids contains both entry IDs
```

### T-BS-02: Convention lookup skipped when role=None
```
Setup: Store convention entries
Params: role=None, include_conventions=true, include_semantic=false
Assert: result.conventions is empty (no topic to query)
```

### T-BS-03: Convention lookup skipped when include_conventions=false
```
Setup: Store convention entries with topic="dev"
Params: role=Some("dev"), include_conventions=false, include_semantic=false
Assert: result.conventions is empty
```

### T-BS-04: Semantic search isolation when include_semantic=false (R-04, AC-03)
```
Setup: Store entries in knowledge base. Do NOT start embed service.
Params: role=None, task=Some("test query"), include_semantic=false, injection_history=None
Assert: No error (SearchService never called, embed never invoked)
Assert: result.relevant_context is empty
Assert: result.search_available is true (default — not attempted, not failed)
```

### T-BS-05: Semantic search when include_semantic=true but embed not ready (R-08)
```
Setup: Store entries. Embed service not started.
Params: task=Some("test"), include_semantic=true
Assert: result.search_available is false
Assert: result.relevant_context is empty
Assert: No error returned (graceful degradation)
Assert: result.conventions still populated if include_conventions=true
```

### T-BS-06: Injection history — basic processing (AC-04)
```
Setup: Store 3 entries — 1 decision, 1 convention, 1 pattern (other)
       All Active status.
Params: injection_history=Some([
    InjectionEntry { entry_id: 1, confidence: 0.8 },
    InjectionEntry { entry_id: 2, confidence: 0.7 },
    InjectionEntry { entry_id: 3, confidence: 0.9 },
])
Assert: result.injection_sections.decisions has entry 1
Assert: result.injection_sections.conventions has entry 2
Assert: result.injection_sections.injections has entry 3
Assert: all entry IDs in result.entry_ids
```

### T-BS-07: Injection history — deduplication (keeps highest confidence)
```
Setup: Store 1 entry (id=10)
Params: injection_history=Some([
    InjectionEntry { entry_id: 10, confidence: 0.3 },
    InjectionEntry { entry_id: 10, confidence: 0.9 },
    InjectionEntry { entry_id: 10, confidence: 0.5 },
])
Assert: result contains entry 10 exactly once
Assert: confidence associated is 0.9
```

### T-BS-08: Injection history — quarantine exclusion (R-05, AC-06)
```
Setup: Store entry A (Active), entry B (Quarantined)
Params: injection_history=Some([A, B])
Assert: result contains entry A
Assert: result does NOT contain entry B
Assert: entry B's ID not in result.entry_ids
```

### T-BS-09: Injection history — deleted entry skipped
```
Setup: Store entry id=1. Do NOT store id=99.
Params: injection_history=Some([
    InjectionEntry { entry_id: 1, confidence: 0.8 },
    InjectionEntry { entry_id: 99, confidence: 0.5 },
])
Assert: result contains entry 1
Assert: no error for missing entry 99
```

### T-BS-10: Injection history — deprecated entries included
```
Setup: Store entry (Deprecated status)
Params: injection_history=Some([that entry])
Assert: entry IS included in results (deprecated != quarantined)
```

### T-BS-11: Token budget — entries truncated (AC-05)
```
Setup: Store 3 large convention entries (each ~300 chars title+content)
Params: max_tokens=500, include_conventions=true, role=Some("dev")
Assert: char_budget = 500 * 4 = 2000
Assert: not all 3 entries fit within budget
Assert: result.conventions has fewer than 3 entries
```

### T-BS-12: Token budget — proportional allocation with injection history (AC-05)
```
Setup: Store 5 decision entries, 5 injection entries, 5 convention entries (each ~200 chars)
Params: max_tokens=500, injection_history=Some([all 15 entries])
Assert: decisions section gets 40% of budget
Assert: injections section gets 30% of budget
Assert: conventions section gets 20% of budget
Assert: total entries <= what fits in budget
```

### T-BS-13: Token budget — max_tokens=500 (minimum, R-06 boundary)
```
Setup: Store 1 entry per category
Params: max_tokens=500, injection_history=Some([all])
Assert: no panic, result has some entries (or empty if entries exceed budget)
```

### T-BS-14: Input validation — role too long (AC-07)
```
Params: role=Some("x".repeat(501)), max_tokens=3000
Assert: Err(ServiceError::ValidationFailed) containing "role"
```

### T-BS-15: Input validation — task too long (AC-07)
```
Params: task=Some("x".repeat(10001)), max_tokens=3000
Assert: Err(ServiceError::ValidationFailed) containing "task"
```

### T-BS-16: Input validation — max_tokens out of range (AC-07)
```
Params: max_tokens=100 (below minimum 500)
Assert: Err(ServiceError::ValidationFailed) containing "max_tokens"

Params: max_tokens=20000 (above maximum 10000)
Assert: Err(ServiceError::ValidationFailed) containing "max_tokens"
```

### T-BS-17: Input validation — control characters in task
```
Params: task=Some("test\x01query"), max_tokens=3000
Assert: Err(ServiceError::ValidationFailed) containing "control"
```

### T-BS-18: Empty knowledge base
```
Setup: Empty store (no entries)
Params: role=Some("dev"), include_conventions=true, include_semantic=false
Assert: result.conventions is empty
Assert: result.injection_sections is empty
Assert: no error
```

### T-BS-19: Feature sort — feature-tagged conventions first
```
Setup: Store convention A (no feature tag), convention B (tagged "vnc-007")
Params: role=Some(topic), feature=Some("vnc-007"), include_conventions=true
Assert: result.conventions[0] is B (feature-tagged first)
```

### T-BS-20: All injection entries quarantined
```
Setup: Store 3 entries, all Quarantined
Params: injection_history=Some([all 3])
Assert: result.injection_sections all empty
Assert: result.entry_ids empty
```

## Risk Coverage Mapping

| Risk | Test(s) | Status |
|------|---------|--------|
| R-04 (SearchService isolation) | T-BS-04 | Covered |
| R-05 (Quarantine exclusion) | T-BS-08, T-BS-20 | Covered |
| R-06 (Budget overflow) | T-BS-11, T-BS-12, T-BS-13 | Covered |
| R-08 (EmbedNotReady) | T-BS-05 | Covered |
