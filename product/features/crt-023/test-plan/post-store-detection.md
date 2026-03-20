# Test Plan: Post-Store NLI Detection (`unimatrix-server/src/services/nli_detection.rs`)

## Component Scope

File: `crates/unimatrix-server/src/services/nli_detection.rs`

Function: `run_post_store_nli` (async, fire-and-forget)

Also covers: `StoreService` fire-and-forget spawn in `store_ops.rs`.

## Risks Covered

R-07 (High): Embedding consumed before NLI hand-off point.
R-08 (Med): HNSW insert failure — orphaned entry, no NLI edges.
R-09 (Critical): Circuit breaker counts only Contradicts (wrong) vs all edges (correct).
R-16 (High): Post-store NLI write contention on SQLite write pool.
AC-10, AC-11, AC-13, AC-19, AC-23.

---

## W1-2 Compliance Test

```rust
#[tokio::test]
async fn test_post_store_nli_inference_runs_on_rayon_not_tokio() {
    // W1-2 contract: NLI inference must run on rayon pool, never inline in async task.
    // Verification: mock CrossEncoderProvider records the thread it is called from.
    let thread_recorder = Arc::new(ThreadRecordingProvider::new());
    let pool = Arc::new(RayonPool::new_with_min_threads(2));

    run_post_store_nli(
        vec![0.1f32; 128],
        ENTRY_ID,
        "entry text".to_string(),
        make_ready_handle(thread_recorder.clone()),
        make_store_with_neighbors(),
        make_vector_index_with_neighbors(),
        Arc::clone(&pool),
        10, 0.6, 0.6, 10,
    ).await;

    // Assert: provider was called from a rayon thread, not the current tokio thread.
    let call_thread = thread_recorder.last_caller_thread();
    assert!(
        rayon::current_thread_index_at(&call_thread).is_some(),
        "NLI inference must run on rayon thread, not tokio thread: {:?}", call_thread
    );
}
```

## R-07: Embedding Handed Off Correctly

```rust
#[tokio::test]
async fn test_post_store_embedding_non_empty_reaches_nli_task() {
    // R-07: The embedding Vec<f32> inserted into HNSW must reach the NLI task non-empty.
    // Mock: VectorIndex.search returns ENTRY_A as neighbor;
    //       EmbeddingRecordingProvider records the query embedding used.
    let embed_recorder = Arc::new(EmbeddingRecordingProvider::new());
    let query_embedding: Vec<f32> = (0..128).map(|i| i as f32 / 128.0).collect();
    let embedding_clone = query_embedding.clone();

    run_post_store_nli(
        query_embedding,
        NEW_ENTRY_ID,
        "new entry text".to_string(),
        make_ready_handle(embed_recorder.clone()),
        make_store_with_entry(ENTRY_A_ID, "entry A text"),
        make_vector_index_returning_neighbor(ENTRY_A_ID, embedding_clone),
        make_rayon_pool(),
        10, 0.6, 0.6, 10,
    ).await;

    // Assert: the NLI task ran and the HNSW search used the provided embedding.
    assert!(embed_recorder.was_called(), "NLI provider must be called");
    assert_eq!(
        embed_recorder.search_embedding_used(),
        (0..128).map(|i| i as f32 / 128.0).collect::<Vec<_>>(),
        "HNSW search must use the moved embedding, not a recomputed one"
    );
}

#[tokio::test]
async fn test_post_store_empty_embedding_skips_nli() {
    // R-07: if embedding is empty, NLI task must skip gracefully (log warn, no crash).
    let call_recorder = Arc::new(CallCountingProvider::new());
    run_post_store_nli(
        vec![], // empty embedding
        NEW_ENTRY_ID,
        "text".to_string(),
        make_ready_handle(call_recorder.clone()),
        make_store(),
        make_vector_index(),
        make_rayon_pool(),
        10, 0.6, 0.6, 10,
    ).await;
    assert_eq!(call_recorder.call_count(), 0,
        "NLI provider must not be called when embedding is empty");
}
```

## R-09: Circuit Breaker Counts All Edge Types (Critical)

```rust
#[tokio::test]
async fn test_circuit_breaker_counts_supports_and_contradicts_combined() {
    // R-09 (Critical, non-negotiable):
    // max_edges_per_call=2; 5 neighbors all above both thresholds.
    // Both Supports AND Contradicts count toward cap.
    // Assert: exactly 2 edges written to GRAPH_EDGES total (not 2+2=4).
    let mock_provider = Arc::new(UniformMockProvider::new(NliScores {
        entailment: 0.9,     // above 0.6 threshold → Supports
        neutral: 0.0,
        contradiction: 0.9,  // above 0.6 threshold → Contradicts
    }));
    let store = make_store_with_in_memory_db();
    let vector_index = make_vector_index_with_n_neighbors(5);

    run_post_store_nli(
        make_embedding(128),
        NEW_ENTRY_ID,
        "text".to_string(),
        make_ready_handle(mock_provider),
        Arc::clone(&store),
        vector_index,
        make_rayon_pool(),
        5,   // nli_post_store_k
        0.6, // nli_entailment_threshold
        0.6, // nli_contradiction_threshold
        2,   // max_edges_per_call = 2 (cap)
    ).await;

    let edge_count = store.count_graph_edges_with_source("nli").unwrap();
    assert_eq!(edge_count, 2,
        "Circuit breaker must limit TOTAL edges (Supports+Contradicts) to cap=2, got: {edge_count}");
}

#[tokio::test]
async fn test_circuit_breaker_stops_at_cap_mixed_types() {
    // R-09: 2 Supports + 2 Contradicts neighbors; cap=3.
    // Assert: exactly 3 edges written (first 3 processed, regardless of type).
    let mock_provider = Arc::new(AlternatingMockProvider::new(
        NliScores { entailment: 0.9, neutral: 0.0, contradiction: 0.0 }, // Supports only
        NliScores { entailment: 0.0, neutral: 0.0, contradiction: 0.9 }, // Contradicts only
    ));
    let store = make_store_with_in_memory_db();
    let vector_index = make_vector_index_with_n_neighbors(4);

    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "text".to_string(),
        make_ready_handle(mock_provider), Arc::clone(&store),
        vector_index, make_rayon_pool(),
        4, 0.6, 0.6, 3, // cap=3
    ).await;

    let edge_count = store.count_graph_edges_with_source("nli").unwrap();
    assert_eq!(edge_count, 3, "Cap=3 must stop at 3 edges across mixed types");
}

#[tokio::test]
async fn test_circuit_breaker_debug_log_on_dropped_edges() {
    // R-09: Dropped edges must be logged at debug level.
    // Verification: tracing subscriber captures debug events; assert edge drop is logged.
    // (Implementation detail: use tracing_test or similar subscriber capture.)
}
```

## AC-10, AC-11: Edge Written With Correct Fields

```rust
#[tokio::test]
async fn test_contradicts_edge_written_with_nli_source() {
    // AC-10: Contradicts edge must have source='nli', bootstrap_only=0.
    let mock_provider = Arc::new(FixedMockProvider::new(NliScores {
        entailment: 0.1,
        neutral: 0.1,
        contradiction: 0.8, // above 0.6 → Contradicts
    }));
    let store = make_store_with_in_memory_db();
    insert_test_entry(&store, NEIGHBOR_ID, "contradictory text");

    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "new text".to_string(),
        make_ready_handle(mock_provider), Arc::clone(&store),
        make_vector_index_returning_neighbor(NEIGHBOR_ID, make_embedding(128)),
        make_rayon_pool(), 1, 0.6, 0.6, 10,
    ).await;

    let edge = store.get_graph_edge(NEW_ENTRY_ID, NEIGHBOR_ID, "Contradicts").unwrap().unwrap();
    assert_eq!(edge.created_by, "nli");
    assert_eq!(edge.source, "nli");
    assert_eq!(edge.bootstrap_only, 0);
}

#[tokio::test]
async fn test_edge_metadata_contains_nli_scores() {
    // AC-11: metadata must parse as JSON with nli_entailment and nli_contradiction keys.
    let mock_provider = Arc::new(FixedMockProvider::new(NliScores {
        entailment: 0.7,
        neutral: 0.1,
        contradiction: 0.2,
    }));
    let store = make_store_with_in_memory_db();
    insert_test_entry(&store, NEIGHBOR_ID, "text");

    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "new text".to_string(),
        make_ready_handle(mock_provider), Arc::clone(&store),
        make_vector_index_returning_neighbor(NEIGHBOR_ID, make_embedding(128)),
        make_rayon_pool(), 1, 0.6, 0.6, 10,
    ).await;

    let edge = store.get_graph_edge(NEW_ENTRY_ID, NEIGHBOR_ID, "Supports").unwrap().unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&edge.metadata.unwrap()).unwrap();
    assert!(metadata["nli_entailment"].is_number(), "metadata must have nli_entailment");
    assert!(metadata["nli_contradiction"].is_number(), "metadata must have nli_contradiction");
    assert!((metadata["nli_entailment"].as_f64().unwrap() - 0.7f64).abs() < 1e-4);
}
```

## R-08: HNSW Insert Failure — Silent Degradation

```rust
#[tokio::test]
async fn test_hnsw_failure_store_returns_ok_and_task_spawned() {
    // R-08: HNSW insert failure is non-fatal; store returns Ok.
    // The NLI task is still spawned but finds 0 HNSW neighbors.
    let call_recorder = Arc::new(CallCountingProvider::new());
    let store = make_store_with_in_memory_db();
    let failing_vector_index = make_vector_index_with_hnsw_insert_failure();

    // Call StoreService::insert (not just run_post_store_nli) to test the full path.
    let store_service = make_store_service_with_failing_hnsw(
        Arc::clone(&store),
        failing_vector_index,
        make_ready_handle(call_recorder.clone()),
    );
    let result = store_service.insert(make_test_entry()).await;
    assert!(result.is_ok(), "HNSW failure must not propagate to store result");
}

#[tokio::test]
async fn test_post_store_zero_neighbors_task_exits_cleanly() {
    // R-08: vector_index.search returns empty list (HNSW has no neighbors).
    // Task exits without writing edges, without error.
    let call_recorder = Arc::new(CallCountingProvider::new());
    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "text".to_string(),
        make_ready_handle(call_recorder.clone()),
        make_store_with_in_memory_db(),
        make_vector_index_returning_empty(), // no neighbors
        make_rayon_pool(),
        10, 0.6, 0.6, 10,
    ).await;
    assert_eq!(call_recorder.call_count(), 0,
        "NLI provider must not be called when no neighbors found");
}
```

## R-16: Write Pool Contention Under Burst Stores

```rust
#[tokio::test]
async fn test_burst_stores_all_nli_edges_written() {
    // R-16: 5 concurrent context_store calls → 5 concurrent fire-and-forget NLI tasks.
    // Assert all expected GRAPH_EDGES rows written despite write pool contention.
    let store = make_store_with_real_wal_db();
    let mock_provider = Arc::new(FixedMockProvider::new(NliScores {
        entailment: 0.8, neutral: 0.1, contradiction: 0.1, // Supports
    }));
    let handle = make_ready_handle(mock_provider);
    let vector_index = make_vector_index_with_one_neighbor();

    let tasks: Vec<_> = (0..5).map(|i| {
        let store = Arc::clone(&store);
        let handle = Arc::clone(&handle);
        let vi = Arc::clone(&vector_index);
        tokio::spawn(async move {
            run_post_store_nli(
                make_embedding(128), i as u64, format!("entry {i}"),
                handle, store, vi, make_rayon_pool(),
                1, 0.6, 0.6, 10,
            ).await;
        })
    }).collect();
    futures::future::join_all(tasks).await;

    let edge_count = store.count_graph_edges_with_source("nli").unwrap();
    // 5 stores × 1 neighbor each = up to 5 edges (capped at 1 each by max_edges=10>1)
    assert_eq!(edge_count, 5, "All 5 NLI edge writes must succeed despite write pool contention");
}

#[tokio::test]
async fn test_write_pool_error_not_propagated_to_mcp_response() {
    // R-16: Write pool timeout in fire-and-forget task must not surface as MCP error.
    // The store_service.insert() future must resolve to Ok even if NLI task later fails.
    // (Fire-and-forget: the store response is returned before NLI task completes.)
    let store_service = make_store_service_with_slow_write_pool();
    let result = store_service.insert(make_test_entry()).await;
    assert!(result.is_ok(), "MCP store response must not be affected by NLI task write pool errors");
}
```

## AC-15: Panic Containment in Fire-and-Forget Task

```rust
#[tokio::test]
async fn test_post_store_nli_panic_in_rayon_does_not_crash_tokio() {
    // AC-15: A panic inside the rayon NLI call must produce RayonError::Cancelled,
    // not propagate as a panic to the tokio runtime.
    let panicking_provider = Arc::new(PanicOnCallProvider);
    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "text".to_string(),
        make_ready_handle(panicking_provider),
        make_store_with_in_memory_db(),
        make_vector_index_with_one_neighbor(),
        make_rayon_pool(),
        1, 0.6, 0.6, 10,
    ).await; // Must complete without propagating panic
    // If we reach here, tokio thread is alive — test passes
}
```

## AC-19: StoreService Uses nli_post_store_k (Not nli_top_k)

```rust
#[test]
fn test_store_service_reads_nli_post_store_k_not_top_k() {
    // AC-19: StoreService must pass nli_post_store_k for neighbor count, not nli_top_k.
    let config = InferenceConfig {
        nli_top_k: 50,
        nli_post_store_k: 3,
        ..InferenceConfig::default()
    };
    let service = StoreService::new_with_config(config);
    assert_eq!(service.nli_neighbor_count(), 3,
        "StoreService must use nli_post_store_k=3 for neighbor count, not nli_top_k=50");
}
```

## NLI Not Ready — Task Exits Immediately

```rust
#[tokio::test]
async fn test_post_store_nli_not_ready_exits_immediately() {
    // When NliServiceHandle returns Err(NliNotReady), task must exit without calling anything.
    let failing_handle = NliServiceHandle::new(); // never started, stays in initial not-ready state
    let call_recorder = Arc::new(CallCountingProvider::new());
    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "text".to_string(),
        failing_handle, // not ready
        make_store_with_in_memory_db(),
        make_vector_index_with_one_neighbor(),
        make_rayon_pool(),
        10, 0.6, 0.6, 10,
    ).await;
    assert_eq!(call_recorder.call_count(), 0);
}
```
