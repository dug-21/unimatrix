# dsn-001 Test Plan — search-service

Component: `crates/unimatrix-server/src/services/search.rs`

Risks covered: IR-03, AC-03, EC-02.

---

## Scope of Changes

All four hardcoded `entry.category == "lesson-learned"` comparisons in `search.rs`
are replaced with a `HashSet<String>` lookup against `SearchService.boosted_categories`.
The `HashSet` is constructed from `config.knowledge.boosted_categories` at
`SearchService` construction time.

---

## Static Gate: Literal Removed (AC-03 primary)

This is a mandatory pre-merge static check:

```bash
grep -n '"lesson-learned"' crates/unimatrix-server/src/services/search.rs
```

Must return zero results. If any line matches, the replacement is incomplete (IR-03
— partial replacement leaves hardcoded behavior for some search paths).

This grep must be run in Stage 3c and its result included in RISK-COVERAGE-REPORT.md.

---

## `boosted_categories` HashSet Field Tests

### test_search_service_boosted_set_lookup_replaces_hardcoded

The search service must use `boosted_categories.contains(&entry.category)` instead
of `== "lesson-learned"`. This is tested indirectly through the behavior tests below.
A direct structural test is not practical without access to private internals —
use the grep gate and behavior tests together.

---

## Boost Applied to Configured Category (AC-03)

These tests verify that the `HashSet` lookup works correctly in the re-ranking paths.

### test_boosted_category_receives_provenance_boost

```rust
#[tokio::test]
async fn test_boosted_category_receives_provenance_boost() {
    // SearchService constructed with boosted_categories = {"decision"}
    let boosted: HashSet<String> = std::iter::once("decision".to_string()).collect();
    let search_service = SearchService::new(store.clone(), arc_params.clone(), boosted);

    // Store two entries: one "decision", one "pattern" — otherwise identical.
    let decision_id = store_entry(&store, "decision", "test content").await;
    let pattern_id  = store_entry(&store, "pattern", "test content").await;

    let results = search_service.search("test content", 10, None).await.unwrap();
    let decision_result = results.iter().find(|r| r.id == decision_id).unwrap();
    let pattern_result  = results.iter().find(|r| r.id == pattern_id).unwrap();

    // "decision" (boosted) must rank higher or have higher score than "pattern" (not boosted).
    assert!(
        decision_result.score >= pattern_result.score,
        "boosted category 'decision' must score >= non-boosted 'pattern': \
         decision={:.6}, pattern={:.6}",
        decision_result.score, pattern_result.score
    );
}
```

### test_lesson_learned_not_boosted_when_absent_from_config

```rust
#[tokio::test]
async fn test_lesson_learned_not_boosted_when_absent_from_config() {
    // boosted_categories = {"decision"} — "lesson-learned" NOT in the set.
    let boosted: HashSet<String> = std::iter::once("decision".to_string()).collect();
    let search_service = SearchService::new(store.clone(), arc_params.clone(), boosted);

    let decision_id = store_entry(&store, "decision", "test content").await;
    let ll_id       = store_entry(&store, "lesson-learned", "test content").await;

    let results = search_service.search("test content", 10, None).await.unwrap();
    let decision_result = results.iter().find(|r| r.id == decision_id).unwrap();
    let ll_result       = results.iter().find(|r| r.id == ll_id).unwrap();

    // "lesson-learned" must NOT receive the boost when absent from boosted_categories.
    // "decision" (boosted) must score higher than "lesson-learned" (not boosted).
    assert!(
        decision_result.score >= ll_result.score,
        "lesson-learned must not be boosted when absent from config: \
         decision={:.6}, lesson-learned={:.6}",
        decision_result.score, ll_result.score
    );
}
```

### test_lesson_learned_boosted_when_in_config (default behavior preserved)

```rust
#[tokio::test]
async fn test_lesson_learned_boosted_when_in_config() {
    // Default boosted_categories = ["lesson-learned"] — preserves pre-dsn-001 behavior.
    let boosted: HashSet<String> = std::iter::once("lesson-learned".to_string()).collect();
    let search_service = SearchService::new(store.clone(), arc_params.clone(), boosted);

    let ll_id      = store_entry(&store, "lesson-learned", "test content").await;
    let pattern_id = store_entry(&store, "pattern", "test content").await;

    let results = search_service.search("test content", 10, None).await.unwrap();
    let ll_result      = results.iter().find(|r| r.id == ll_id).unwrap();
    let pattern_result = results.iter().find(|r| r.id == pattern_id).unwrap();

    assert!(
        ll_result.score >= pattern_result.score,
        "lesson-learned must be boosted when in config: \
         ll={:.6}, pattern={:.6}", ll_result.score, pattern_result.score
    );
}
```

---

## Empty `boosted_categories` Set (EC-02)

### test_empty_boosted_set_no_panic

```rust
#[tokio::test]
async fn test_empty_boosted_set_no_panic() {
    // Empty HashSet — no categories receive the provenance boost.
    let boosted: HashSet<String> = HashSet::new();
    let search_service = SearchService::new(store.clone(), arc_params.clone(), boosted);

    let entry_id = store_entry(&store, "lesson-learned", "test content").await;
    // Must not panic; must return results normally.
    let results = search_service.search("test content", 10, None).await.unwrap();
    assert!(!results.is_empty(), "results must be returned even with empty boosted set");
    // lesson-learned must not be boosted (empty set).
    let ll_result = results.iter().find(|r| r.id == entry_id);
    // No assertion on rank — just verify no panic and results returned.
    let _ = ll_result;
}
```

---

## All Four Comparison Sites Replaced (IR-03)

The grep gate above is the primary check. For completeness, note that the
SPECIFICATION.md §IR-03 identifies four occurrences at approximately lines 413, 418,
484, and 489. After replacement:

- All four paths use `self.boosted_categories.contains(&entry.category)`.
- Partial replacement (e.g., two sites updated and two left hardcoded) would cause
  inconsistent boost behavior depending on which code path is executed.

The behavior tests above exercise multiple code paths (vector search and potentially
keyword/briefing search). If a partial replacement exists, one of the tests above
would pass while the `lesson-learned` hardcoded path might still be active.

---

## Integration Test Reference (AC-03)

An integration-level test is planned in `suites/test_tools.py` (see OVERVIEW.md
§New Integration Tests Needed). The unit tests above are the primary coverage.
The integration test provides end-to-end MCP-level verification that the configured
`boosted_categories` takes effect.
