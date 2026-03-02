# Test Plan: lesson-learned

Component: Lesson-Learned Auto-Persistence + Provenance Boost (P1)
Covers: AC-20, AC-21, AC-22, AC-23
Risks: R-06, R-07

---

## Unit Tests

### PROVENANCE_BOOST constant (AC-23)

```
test_provenance_boost_constant_value
  - Assert: PROVENANCE_BOOST == 0.02

test_provenance_boost_applied_for_lesson_learned
  - Compute rerank_score(sim=0.8, conf=0.6, coac=0.01, category="lesson-learned")
  - Compute rerank_score(sim=0.8, conf=0.6, coac=0.01, category="convention")
  - Assert: difference == 0.02

test_provenance_boost_not_applied_for_other_categories
  - For category in ["decision", "convention", "outcome", "pattern"]:
    - Assert: rerank_score(... category=X) == rerank_score(... category="convention")
  - (All non-lesson-learned categories get 0.0 boost)

test_provenance_boost_does_not_exceed_max_score
  - sim=1.0, conf=1.0, coac=0.03 (max co-access), category="lesson-learned"
  - Compute score; verify it doesn't panic or produce NaN
```

### build_lesson_learned_content

```
test_lesson_learned_content_includes_hotspot
  - Report with 1 hotspot { rule_name="permission_retries", claim="5 retries" }
  - Assert: content contains "permission_retries"
  - Assert: content contains "5 retries" or "5 retries"
  - Assert: content is non-empty

test_lesson_learned_content_includes_recommendations
  - Report with 1 recommendation { action="Add to Allow list" }
  - Assert: content contains "Add to Allow list"

test_lesson_learned_content_with_narratives
  - Report with narrative { summary="3 permission retries in 30s" }
  - Assert: content contains "3 permission retries"

test_lesson_learned_content_empty_report_safe
  - Empty hotspots, empty recommendations, narratives=None
  - Assert: content is a string (may be empty or minimal)
```

---

## Integration Tests

### Auto-persist after retrospective (AC-20)

```
test_lesson_learned_persisted_after_retrospective
  - Populate SESSIONS for feature_cycle="fc-ll-test" with hotspot-triggering data
  - Call context_retrospective("fc-ll-test")
  - Wait for fire-and-forget embed task (give it up to 2 seconds)
  - context_lookup(category:"lesson-learned", topic:"retrospective/fc-ll-test")
  - Assert: entry found
  - Assert: entry.trust_source == "system"
  - Assert: entry.category == "lesson-learned"
  - Assert: entry.content is non-empty (contains hotspot info)
  - Assert: entry.embedding_dim > 0 (embedding completed)

test_lesson_learned_not_persisted_for_empty_report
  - Call context_retrospective for feature with no hotspots or recommendations
  - Wait 2 seconds
  - Assert: no lesson-learned entry written for this feature_cycle

test_lesson_learned_onnx_failure_writes_entry_without_embedding  (R-06)
  - Mock ONNX adapter to return error
  - Call context_retrospective with hotspot data
  - Wait for fire-and-forget task
  - Assert: lesson-learned entry EXISTS in store
  - Assert: entry.embedding_dim == 0
  - (Entry queryable by context_lookup but not by context_search)
```

### Supersede de-duplication (AC-21)

```
test_lesson_learned_second_call_supersedes_first
  - Call context_retrospective for "fc-supersede-test"; wait for embed
  - Call context_retrospective for same feature again; wait for embed
  - context_lookup(category:"lesson-learned", topic:"retrospective/fc-supersede-test")
  - Assert: exactly 1 Active entry returned
  - Assert: prior entry has status=Deprecated and superseded_by set to new entry id

test_lesson_learned_supersede_chain_is_correct
  - First call creates entry A
  - Second call creates entry B, supersedes A
  - Assert: A.superseded_by == B.id
  - Assert: B.supersedes == A.id (or supersedes field is set per existing correction chain pattern)
```

### Searchability (AC-22)

```
test_lesson_learned_searchable_after_embedding
  - Retrospective produces lesson-learned with "permission retry patterns" in content
  - Wait for embedding
  - context_search("permission retry patterns")
  - Assert: lesson-learned entry appears in results (within top 5)

test_lesson_learned_ranks_above_same_confidence_entry  (AC-23)
  - Insert convention entry with sim=0.8, conf=0.6 (expected)
  - Insert lesson-learned entry with same expected sim and conf
  - context_search with a query that returns both
  - Assert: lesson-learned entry ranks higher
```

### Provenance boost in both search paths (R-07)

```
test_provenance_boost_applied_in_mcp_context_search
  - Verify via context_search tool: lesson-learned ranks above same-score conventional entry

test_provenance_boost_applied_in_hook_context_search
  - Verify via UDS ContextSearch hook path: lesson-learned ranks above same-score entry
  - (Integration test or unit test of the re-ranking function)

test_provenance_boost_constant_same_in_both_paths
  - Assert: both uds_listener.rs and tools.rs import PROVENANCE_BOOST from same constant
  - (Code review check: no duplicate literal "0.02" anywhere)
```

---

## Fire-and-Forget Timing Assertions

```
test_retrospective_returns_before_embed_completes
  - Time context_retrospective call
  - Assert: response received in < 500ms (before ONNX completes)
  - Then wait up to 2 seconds for lesson-learned to appear
  - (Demonstrates fire-and-forget pattern works correctly)
```
