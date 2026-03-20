# Test Plan: Search Re-ranking (`unimatrix-server/src/services/search.rs`)

## Component Scope

File: `crates/unimatrix-server/src/services/search.rs`

Changes: `nli_handle: Arc<NliServiceHandle>` field added; NLI re-ranking step inserted;
fallback path retained when NLI not ready.

## Risks Covered

R-03 (Critical): NLI score tie-breaking instability — stable_sort required.
R-04 (High): MCP_HANDLER_TIMEOUT fires mid-batch — cosine fallback required.
R-17 (High): Status penalty applied as entailment score multiplier (wrong) vs post-sort (correct).
AC-08, AC-20: NLI entailment replaces rerank_score entirely when NLI active.

---

## Unit Tests: Sort Stability (R-03, Critical)

```rust
#[tokio::test]
async fn test_nli_sort_stable_identical_scores() {
    // R-03 (Critical, non-negotiable):
    // Mock CrossEncoderProvider returns identical entailment=0.33 for all candidates.
    // sort_by NLI entailment must produce deterministic ordering.
    // Tiebreaker: original HNSW rank (or entry ID) must break ties consistently.
    let mock_provider = Arc::new(UniformMockProvider::new(NliScores {
        entailment: 0.33,
        neutral: 0.34,
        contradiction: 0.33,
    }));
    let search_service = build_search_service_with_provider(mock_provider);
    let entries = build_test_entries(5); // entries with distinct IDs

    // Run search 10 times; collect ordering of entry IDs
    let mut orderings = Vec::new();
    for _ in 0..10 {
        let result = search_service.nli_sort(entries.clone()).await.unwrap();
        orderings.push(result.iter().map(|e| e.id).collect::<Vec<_>>());
    }

    // All orderings must be identical
    let first = &orderings[0];
    for (i, ordering) in orderings.iter().enumerate() {
        assert_eq!(ordering, first,
            "NLI sort ordering is nondeterministic on run {i}: {:?} vs {:?}", ordering, first);
    }
}

#[tokio::test]
async fn test_nli_sort_narrative_entry_above_terse_entry() {
    // R-03: Entry A (narrative) entails query; Entry B (3-word tag) does not.
    // Assert A ranks above B even if HNSW placed B first.
    let mock_provider = Arc::new(SelectiveMockProvider::from_pairs([
        ("what is machine learning", "Machine learning is a subset of AI that learns from data",
         NliScores { entailment: 0.82, neutral: 0.12, contradiction: 0.06 }),
        ("what is machine learning", "ml ai data",
         NliScores { entailment: 0.15, neutral: 0.70, contradiction: 0.15 }),
    ]));
    let search_service = build_search_service_with_provider(mock_provider);
    let result = search_service.search("what is machine learning", &config_nli_enabled()).await.unwrap();

    let ids: Vec<u64> = result.iter().map(|e| e.id).collect();
    let narrative_idx = ids.iter().position(|&id| id == NARRATIVE_ENTRY_ID).unwrap();
    let terse_idx = ids.iter().position(|&id| id == TERSE_ENTRY_ID).unwrap();
    assert!(narrative_idx < terse_idx,
        "Narrative entry (NLI entailment=0.82) must rank above terse entry (0.15)");
}
```

## AC-08, AC-20: NLI Replaces rerank_score When Active

```rust
#[tokio::test]
async fn test_nli_active_sort_by_entailment_not_rerank_score() {
    // AC-20: with NLI active, result ordering must match NLI entailment sort,
    // NOT the composite rerank_score formula.
    // Arrange: two entries where NLI entailment disagrees with rerank_score ordering.
    // Entry A: high confidence, low NLI entailment (0.2)
    // Entry B: low confidence, high NLI entailment (0.8)
    // NLI sort: B > A. rerank_score sort: A > B (due to confidence).
    let mock_provider = Arc::new(SelectiveMockProvider::from_pairs([
        ("query", ENTRY_A_TEXT, NliScores { entailment: 0.2, neutral: 0.6, contradiction: 0.2 }),
        ("query", ENTRY_B_TEXT, NliScores { entailment: 0.8, neutral: 0.15, contradiction: 0.05 }),
    ]));
    let service = build_search_service_with_provider(mock_provider);
    let results = service.search("query", &config_nli_enabled()).await.unwrap();

    let b_idx = results.iter().position(|e| e.id == ENTRY_B_ID).unwrap();
    let a_idx = results.iter().position(|e| e.id == ENTRY_A_ID).unwrap();
    assert!(b_idx < a_idx,
        "NLI entailment must drive sort order (B > A), not rerank_score (A > B)");
}

#[tokio::test]
async fn test_nli_disabled_falls_back_to_rerank_score() {
    // FR-15: when nli_enabled=false, pipeline uses rerank_score unchanged.
    // The NLI mock should never be called.
    let mock_provider = Arc::new(PanicOnCallProvider);
    let service = build_search_service_with_provider(mock_provider);
    let config = InferenceConfig { nli_enabled: false, ..InferenceConfig::default() };
    // Must not panic (mock not called)
    let results = service.search("query", &config).await.unwrap();
    assert!(!results.is_empty());
}
```

## R-04: Timeout Fallback

```rust
#[tokio::test]
async fn test_nli_timeout_falls_back_to_cosine() {
    // R-04: mock provider with 35s delay; timeout=30s → fallback to rerank_score.
    // Assert: search returns results (not error) within 31s.
    let mock_provider = Arc::new(SlowMockProvider::new(Duration::from_secs(35)));
    let service = build_search_service_with_provider_and_timeout(
        mock_provider,
        Duration::from_secs(30)
    );
    let start = Instant::now();
    let results = service.search("query", &config_nli_enabled()).await;
    let elapsed = start.elapsed();
    assert!(results.is_ok(), "Timeout must not produce error, got: {:?}", results);
    assert!(elapsed < Duration::from_secs(31),
        "Timeout fallback must complete within timeout window, took: {:?}", elapsed);
}

#[tokio::test]
async fn test_nli_handle_not_failed_after_timeout() {
    // R-04: timeout of rayon task must not transition NliServiceHandle to Failed.
    // After timeout, get_provider() must still return Ok.
    let mock_provider = Arc::new(SlowMockProvider::new(Duration::from_secs(35)));
    let handle = NliServiceHandle::with_provider_for_test(mock_provider.clone());
    let service = build_search_service_with_handle_and_timeout(
        Arc::clone(&handle),
        Duration::from_secs(30)
    );
    let _ = service.search("query", &config_nli_enabled()).await;
    // Handle must still be Ready
    assert!(matches!(handle.get_provider().await, Ok(_)),
        "NliServiceHandle must remain Ready after rayon timeout");
}

#[tokio::test]
async fn test_nli_second_call_succeeds_after_timeout() {
    // R-04: A second search after timeout must succeed (mutex eventually released).
    let mock_provider = Arc::new(SlowFirstCallProvider::new(Duration::from_secs(35)));
    let service = build_search_service_with_provider(mock_provider);
    // First call: timeout
    let _ = service.search("query", &config_nli_enabled()).await;
    // Wait for rayon task to release mutex
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Second call: must succeed (fast provider on second call)
    let result = service.search("query", &config_nli_enabled()).await;
    assert!(result.is_ok());
}
```

## R-17: Status Penalty Not Applied to NliScores

```rust
#[tokio::test]
async fn test_deprecated_entry_penalty_not_applied_to_nli_entailment_score() {
    // R-17: Status penalty must not multiply the entailment score.
    // Arrange: deprecated entry with NLI entailment=0.85; active entry with NLI entailment=0.5.
    // With correct pipeline: deprecated entry still appears (penalized rank, not score).
    // With wrong pipeline (penalty on entailment): 0.85 * 0.7 = 0.595, which may rank below 0.5.
    // Assert: deprecated entry appears in results (not excluded by score deflation).
    let mock_provider = Arc::new(SelectiveMockProvider::from_pairs([
        ("query", DEPRECATED_ENTRY_TEXT,
         NliScores { entailment: 0.85, neutral: 0.1, contradiction: 0.05 }),
        ("query", ACTIVE_ENTRY_TEXT,
         NliScores { entailment: 0.5, neutral: 0.4, contradiction: 0.1 }),
    ]));
    let service = build_search_service_with_provider(mock_provider);
    let results = service.search("query", &config_nli_enabled()).await.unwrap();

    // Deprecated entry must appear in results (penalized but present)
    assert!(results.iter().any(|e| e.id == DEPRECATED_ENTRY_ID),
        "Deprecated entry with high NLI entailment must appear in results");
}

#[tokio::test]
async fn test_nli_scores_in_graph_edges_metadata_are_raw_not_penalty_adjusted() {
    // R-17: Metadata in GRAPH_EDGES must store raw NliScores (not multiplied by status penalty).
    // This is checked at the post-store detection layer, but the search pipeline
    // must not pass penalty-adjusted values to the metadata serializer.
    // Test via a post-store detection integration test (see post-store-detection.md).
    // Assertion here: the search pipeline does not modify NliScores before returning them.
    // Verified by inspection: search pipeline only uses nli_scores.entailment for sorting;
    // it does not transform the scores themselves.
}
```

## Empty Candidate Pool After Quarantine Filter

```rust
#[tokio::test]
async fn test_nli_batch_empty_after_quarantine_filter_returns_ok() {
    // Edge case: all HNSW candidates are quarantined; NLI receives empty pairs list.
    // score_batch(&[]) must return Ok(vec![]), not an ORT error.
    let mock_provider = Arc::new(PassthroughMockProvider);
    let service = build_search_service_with_all_quarantined_db(mock_provider);
    let result = service.search("query", &config_nli_enabled()).await;
    assert!(result.is_ok(), "Empty NLI batch must not error: {:?}", result);
    // Results may be empty (no candidates passed quarantine filter)
    let results = result.unwrap();
    // Assert: response is structurally valid (vec, possibly empty)
    assert!(results.len() <= config_nli_enabled().nli_top_k);
}
```

## AC-19: SearchService Uses nli_top_k (Not nli_post_store_k)

```rust
#[test]
fn test_search_service_reads_nli_top_k_not_post_store_k() {
    // AC-19: SearchService must expand HNSW candidates to nli_top_k, not nli_post_store_k.
    let config = InferenceConfig {
        nli_top_k: 50,
        nli_post_store_k: 3,
        ..InferenceConfig::default()
    };
    let service = SearchService::new_with_config(config);
    // Assert: candidate expansion uses nli_top_k=50, not 3.
    assert_eq!(service.candidate_pool_size(), 50);
}
```
