# Test Plan: Bootstrap Edge Promotion (`unimatrix-server/src/services/nli_detection.rs`)

## Component Scope

File: `crates/unimatrix-server/src/services/nli_detection.rs`

Functions: `run_bootstrap_promotion`, `maybe_run_bootstrap_promotion`

Background tick integration: `services/background.rs` (calls `maybe_run_bootstrap_promotion`
on each tick; no-op after marker is set).

## Risks Covered

R-11 (High): Bootstrap promotion partial transaction failure.
R-12 (Med): Bootstrap promotion runs before HNSW warmup.
R-20 (Med): INSERT OR IGNORE silently preserves bootstrap edge.
AC-12, AC-24.

---

## W1-2 Compliance Test

```rust
#[tokio::test]
async fn test_bootstrap_promotion_nli_inference_runs_on_rayon() {
    // W1-2: ALL NLI inference in bootstrap promotion must run via rayon_pool.spawn().
    // Approach: ThreadRecordingProvider records which thread calls score_batch.
    let thread_recorder = Arc::new(ThreadRecordingProvider::new());
    let store = make_store_with_bootstrap_rows(3); // 3 bootstrap_only=1 rows
    let pool = Arc::new(RayonPool::new_with_min_threads(2));

    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle(thread_recorder.clone()),
        Arc::clone(&pool),
        &config_with_contradiction_threshold(0.6),
    ).await.unwrap();

    let call_threads = thread_recorder.all_caller_threads();
    for thread_id in &call_threads {
        assert!(
            rayon::current_thread_index_at(thread_id).is_some(),
            "score_batch in bootstrap promotion must run on rayon thread: {:?}", thread_id
        );
    }
}

#[tokio::test]
async fn test_bootstrap_promotion_single_rayon_spawn_for_all_pairs() {
    // W1-2: All pairs must be collected first, then batched in ONE rayon spawn.
    // NOT one spawn per pair (that would be inline-in-async violation).
    let spawn_counter = Arc::new(RayonSpawnCounter::new());
    let store = make_store_with_bootstrap_rows(10); // 10 rows → 1 batch

    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle_with_spawn_counter(spawn_counter.clone()),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await.unwrap();

    assert_eq!(spawn_counter.spawn_count(), 1,
        "All pairs must be batched in a single rayon spawn, not one per pair");
}
```

## AC-12a: Zero-Row Case Completes and Sets Marker

```rust
#[tokio::test]
async fn test_bootstrap_promotion_zero_rows_sets_marker() {
    // AC-12a (non-negotiable test):
    // When GRAPH_EDGES has no bootstrap_only=1 rows, promotion completes without error
    // and sets the COUNTERS key bootstrap_nli_promotion_done = 1.
    let store = make_store_with_in_memory_db(); // no bootstrap rows
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle_with_confirm_provider(),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await.unwrap();

    let marker = store.read_counter("bootstrap_nli_promotion_done").unwrap();
    assert_eq!(marker, 1,
        "Completion marker must be set even for zero bootstrap rows");
}
```

## AC-24, R-11: Idempotency via COUNTERS Marker

```rust
#[tokio::test]
async fn test_maybe_bootstrap_promotion_skips_if_marker_present() {
    // AC-24 (non-negotiable): marker already set → task is a no-op.
    // Assert: no GRAPH_EDGES queries are made.
    let store = make_store_with_in_memory_db();
    store.set_counter("bootstrap_nli_promotion_done", 1).unwrap();
    let query_counter = Arc::new(QueryCountingStore::wrap(store));

    maybe_run_bootstrap_promotion(
        &query_counter,
        &make_ready_handle_with_confirm_provider(),
        &make_rayon_pool(),
        &InferenceConfig::default(),
    ).await;

    assert_eq!(query_counter.graph_edges_query_count(), 0,
        "Promotion must not query GRAPH_EDGES when marker already present");
}

#[tokio::test]
async fn test_bootstrap_promotion_idempotent_run_twice_no_duplicates() {
    // R-11: Run promotion twice (simulating two ticks before marker is set due to failure).
    // Assert GRAPH_EDGES is identical after both runs (no duplicate rows due to INSERT OR IGNORE).
    let store = make_store_with_bootstrap_rows(2);
    let provider = Arc::new(ConfirmAllProvider::new(NliScores {
        entailment: 0.1, neutral: 0.1, contradiction: 0.9, // confirms Contradicts
    }));

    // First run
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle(provider.clone()),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await.unwrap();

    let edges_after_first = store.count_graph_edges_with_source("nli").unwrap();
    let marker_after_first = store.read_counter("bootstrap_nli_promotion_done").unwrap();
    assert_eq!(marker_after_first, 1);

    // Simulate marker absent for second run (marker cleared by test to simulate partial failure)
    store.set_counter("bootstrap_nli_promotion_done", 0).unwrap();

    // Second run
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle(provider),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await.unwrap();

    let edges_after_second = store.count_graph_edges_with_source("nli").unwrap();
    assert_eq!(edges_after_second, edges_after_first,
        "GRAPH_EDGES must be identical after second run (INSERT OR IGNORE idempotency)");
}
```

## AC-12b: Synthetic Bootstrap Rows — Promotion and Deletion

```rust
#[tokio::test]
async fn test_bootstrap_promotion_confirms_above_threshold() {
    // AC-12b: bootstrap_only=1 Contradicts edge; NLI score above threshold.
    // → DELETE old row, INSERT source='nli', bootstrap_only=0.
    let store = make_store_with_in_memory_db();
    insert_bootstrap_edge(&store, SOURCE_ID, TARGET_ID, "Contradicts");

    let provider = Arc::new(FixedMockProvider::new(NliScores {
        entailment: 0.05, neutral: 0.05, contradiction: 0.9, // above 0.6 threshold
    }));
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle(provider),
        make_rayon_pool(),
        &config_with_contradiction_threshold(0.6),
    ).await.unwrap();

    // Old bootstrap edge must be gone
    let bootstrap_edge = store.get_graph_edge_with_bootstrap_flag(SOURCE_ID, TARGET_ID, "Contradicts", true);
    assert!(bootstrap_edge.is_none(), "bootstrap_only=1 edge must be deleted after promotion");

    // New NLI-confirmed edge must exist
    let nli_edge = store.get_graph_edge(SOURCE_ID, TARGET_ID, "Contradicts").unwrap().unwrap();
    assert_eq!(nli_edge.source, "nli");
    assert_eq!(nli_edge.bootstrap_only, 0);
    assert!(nli_edge.metadata.is_some());
}

#[tokio::test]
async fn test_bootstrap_promotion_refutes_below_threshold() {
    // AC-12: NLI score below threshold → DELETE old row, do NOT insert replacement.
    let store = make_store_with_in_memory_db();
    insert_bootstrap_edge(&store, SOURCE_ID, TARGET_ID, "Contradicts");

    let provider = Arc::new(FixedMockProvider::new(NliScores {
        entailment: 0.8, neutral: 0.1, contradiction: 0.1, // below 0.6 contradiction threshold
    }));
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle(provider),
        make_rayon_pool(),
        &config_with_contradiction_threshold(0.6),
    ).await.unwrap();

    let any_edge = store.get_graph_edge(SOURCE_ID, TARGET_ID, "Contradicts");
    assert!(any_edge.unwrap().is_none(),
        "Refuted bootstrap edge must be deleted with no replacement");
}
```

## R-12: No HNSW Dependency

```rust
#[tokio::test]
async fn test_bootstrap_promotion_does_not_call_hnsw_search() {
    // R-12: bootstrap promotion must not call vector_index.search().
    // Verification: no VectorIndex argument is passed to run_bootstrap_promotion.
    // This is a structural test: function signature must NOT accept VectorIndex.
    // The test verifies the function signature at compile time (no VectorIndex param).
    //
    // If run_bootstrap_promotion is called with a cold/empty VectorIndex,
    // the function must still complete successfully.
    let store = make_store_with_bootstrap_rows(2);
    // No VectorIndex passed — if function requires one, this won't compile.
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle_with_confirm_provider(),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await.unwrap();
    // Completion without error confirms no HNSW dependency.
}

#[tokio::test]
async fn test_bootstrap_promotion_cold_index_completes() {
    // R-12: Even if called in an environment with no HNSW data loaded,
    // promotion reads entry texts from SQL store only.
    let store = make_store_with_bootstrap_rows(3);
    // cold vector_index is not passed (confirmed: no HNSW dependency in signature)
    let result = run_bootstrap_promotion(
        store,
        make_ready_handle_with_confirm_provider(),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await;
    assert!(result.is_ok());
}
```

## R-20: INSERT OR IGNORE and Bootstrap Edge Conflict

```rust
#[tokio::test]
async fn test_post_store_bootstrap_edge_conflict_resolved_by_promotion() {
    // R-20: A bootstrap_only=1 edge exists between A and B.
    // Bootstrap promotion runs first → deletes old, inserts source='nli', bootstrap_only=0.
    // Subsequent post-store NLI for an entry adjacent to A fires INSERT OR IGNORE
    // for the same (A, B, Contradicts) edge → ignored (NLI-confirmed already exists).
    // Assert: no duplicate rows; existing NLI-confirmed edge is preserved.
    let store = make_store_with_in_memory_db();
    insert_bootstrap_edge(&store, ENTRY_A_ID, ENTRY_B_ID, "Contradicts");
    insert_test_entry(&store, ENTRY_A_ID, "text A");
    insert_test_entry(&store, ENTRY_B_ID, "text B");

    // Step 1: run bootstrap promotion (confirms edge)
    let provider = Arc::new(FixedMockProvider::new(NliScores {
        entailment: 0.05, neutral: 0.05, contradiction: 0.9,
    }));
    run_bootstrap_promotion(
        Arc::clone(&store),
        make_ready_handle(provider.clone()),
        make_rayon_pool(),
        &InferenceConfig::default(),
    ).await.unwrap();

    // Step 2: post-store NLI for a new entry adjacent to A and B
    // This triggers an INSERT OR IGNORE for (A, B, Contradicts)
    run_post_store_nli(
        make_embedding(128), NEW_ENTRY_ID, "new text".to_string(),
        make_ready_handle(provider),
        Arc::clone(&store),
        make_vector_index_returning_neighbors(&[ENTRY_A_ID, ENTRY_B_ID], make_embedding(128)),
        make_rayon_pool(),
        2, 0.6, 0.6, 10,
    ).await;

    // Assert: exactly one (A, B, Contradicts) edge exists with source='nli', bootstrap_only=0
    let edges = store.get_all_graph_edges_between(ENTRY_A_ID, ENTRY_B_ID).unwrap();
    let contradicts: Vec<_> = edges.iter().filter(|e| e.relation_type == "Contradicts").collect();
    assert_eq!(contradicts.len(), 1, "Must have exactly one Contradicts edge (no duplicates)");
    assert_eq!(contradicts[0].source, "nli");
    assert_eq!(contradicts[0].bootstrap_only, 0);
}
```

## Deferral When NLI Not Ready

```rust
#[tokio::test]
async fn test_maybe_bootstrap_promotion_defers_when_nli_not_ready() {
    // FR-25: if NLI not ready → deferred (no marker set, tracing::info! logged).
    let store = make_store_with_bootstrap_rows(2);
    let not_ready_handle = NliServiceHandle::new(); // never started

    maybe_run_bootstrap_promotion(
        &store,
        &not_ready_handle,
        &make_rayon_pool(),
        &InferenceConfig::default(),
    ).await;

    // Marker must NOT be set (deferral, not completion)
    let marker = store.read_counter("bootstrap_nli_promotion_done").unwrap();
    assert_eq!(marker, 0, "Marker must not be set when NLI not ready (deferred)");
}
```
