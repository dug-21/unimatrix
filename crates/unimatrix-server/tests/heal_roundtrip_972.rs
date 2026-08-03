//! Server-level round-trip test for bugfix #972, Leg 2.
//!
//! Bug #972: after a DB-only copy (`unimatrix.db` moved WITHOUT the HNSW index
//! dir), `vector_map` records more mappings than the loaded graph holds. The fix
//! (crates/unimatrix-vector/src/persistence.rs) membership-filters the rebuilt
//! IdMap in `VectorIndex::load` to graph-present origin_ids, so `contains()` is
//! truthful (Leg 1). This test proves Leg 2 — the DoD headline outcome — that the
//! EXISTING capped maintenance heal (services/status.rs Sub-case B, guarded by
//! `embedding_dim > 0 && !contains`) then FIRES and retrieval self-repopulates the
//! previously-missing entry, WITHOUT any manual `DELETE FROM vector_map`.
//!
//! Requires the ONNX model (the heal re-embeds from stored content). Skips
//! gracefully when the model is absent — same `skip_if_no_model()` gate as the
//! other embed-dependent server tests (pipeline_e2e.rs). In a container without
//! libonnxruntime this test self-skips; it executes fully under Docker.

use std::sync::Arc;

use unimatrix_core::{EmbedService, VectorConfig, VectorIndex};
use unimatrix_server::test_support::{TestHarness, skip_if_no_model};
use unimatrix_store::{NewEntry, PoolConfig, SqlxStore, Status};

/// data_id deliberately absent from the authored graph (present ids are 0..N-1),
/// so the load-time membership filter drops the injected mapping.
const ABSENT_DATA_ID: u64 = 9_000_000;

fn new_entry(title: &str, content: &str) -> NewEntry {
    NewEntry {
        title: title.to_string(),
        content: content.to_string(),
        topic: "test".to_string(),
        category: "vector".to_string(),
        tags: vec![],
        source: "test".to_string(),
        status: Status::Active,
        created_by: "test".to_string(),
        feature_cycle: "bugfix-972".to_string(),
        trust_source: "human".to_string(),
    }
}

/// GH#972 Leg 2: a DB-only-copy under-count load drops the graph-absent mapping
/// (membership filter), then the maintenance heal (Sub-case B) re-embeds the entry
/// from its stored content, re-inserts the HNSW point, and the entry becomes
/// retrievable again — end-to-end, through the real server maintenance path.
#[tokio::test]
async fn test_maintenance_heal_repopulates_graph_undercount_after_db_only_copy_load() {
    if skip_if_no_model() {
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let dump_dir = dir.path().join("index");

    // ONE embed handle shared across author → dump → load → heal (the graph must be
    // authored before the harness, which wraps the *loaded* index, is constructed).
    let embed_handle = match TestHarness::load_embed_handle().await {
        Some(h) => h,
        None => return, // model unavailable — skip
    };
    let adapter = embed_handle
        .get_adapter()
        .await
        .expect("embed adapter must be ready after load_embed_handle");

    let store = Arc::new(
        SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("open store"),
    );

    // ---- Phase A: author a graph with PRESENT entries, then dump it ----
    // These land in the HNSW graph (data_ids 0..N-1) and in vector_map; the dump
    // captures them. Set embedding_dim > 0 so they are NOT swept by heal Sub-case A.
    let vi = Arc::new(
        VectorIndex::new(Arc::clone(&store), VectorConfig::default()).expect("create index"),
    );
    let mut present_ids: Vec<u64> = Vec::new();
    for i in 0..20 {
        let eid = store
            .insert(new_entry(
                &format!("Generic filler entry {i}"),
                &format!("Routine background content number {i} with no distinctive terms"),
            ))
            .await
            .expect("insert present entry");
        let emb = adapter
            .embed_entry(
                &format!("Generic filler entry {i}"),
                &format!("Routine background content number {i} with no distinctive terms"),
            )
            .expect("embed present entry");
        vi.insert(eid, &emb).await.expect("hnsw insert");
        store
            .update_embedding_dim(eid, emb.len() as u16)
            .await
            .expect("set embedding_dim");
        present_ids.push(eid);
    }
    vi.dump(&dump_dir).expect("dump graph");
    let dumped_point_count = vi.point_count();
    drop(vi); // release the authoring index before the load

    // ---- Phase B: inject the DB-only-copy divergence ----
    // A NEW active entry with a distinctive body, embedding_dim > 0, and a vector_map
    // row pointing at a data_id that is NOT in the dumped graph — exactly the state a
    // DB-only copy leaves: vector_map over-counts what the graph holds. NOTE: no HNSW
    // point is authored for it, and no `DELETE FROM vector_map` is performed anywhere.
    let absent_title = "Peculiar tungsten spectroscopy of deep-sea bioluminescence";
    let absent_content =
        "unique-marker-xyzzy quantum widget calibration for abyssal photophore telemetry";
    let absent_id = store
        .insert(new_entry(absent_title, absent_content))
        .await
        .expect("insert absent entry");
    store
        .put_vector_mapping(absent_id, ABSENT_DATA_ID)
        .await
        .expect("inject graph-absent vector_map row");
    store
        .update_embedding_dim(absent_id, 384)
        .await
        .expect("set embedding_dim > 0 for absent entry");

    // ---- Phase C: LOAD (the round-trip) — membership filter drops the absent mapping ----
    let loaded = Arc::new(
        VectorIndex::load(Arc::clone(&store), VectorConfig::default(), &dump_dir)
            .await
            .expect("load index"),
    );
    // meta claims the dumped count (>= 1) — NOT the empty-first-boot short-circuit.
    assert_eq!(loaded.point_count(), dumped_point_count);

    // Leg 1 precondition: contains() is truthful after the filter.
    for &id in &present_ids {
        assert!(
            loaded.contains(id),
            "graph-present entry {id} must be retained"
        );
    }
    assert!(
        !loaded.contains(absent_id),
        "graph-absent entry {absent_id} must be filtered out at load (Leg 1)"
    );

    // ---- Phase D: wire the server around the LOADED index and drive the heal ----
    let harness = TestHarness::from_parts(
        Arc::clone(&store),
        Arc::clone(&loaded),
        Arc::clone(&embed_handle),
    )
    .await;
    // Sanity: the entry is not retrievable before the heal (no graph point yet).
    let before = harness
        .search(absent_content, 10)
        .await
        .expect("pre-heal search");
    assert!(
        before.iter().all(|r| r.entry.id != absent_id),
        "absent entry must NOT be retrievable before the heal"
    );

    // Same path the running server uses (maintenance_tick → run_maintenance heal Sub-case B).
    harness.run_maintenance_heal().await;

    // ---- Phase E: Leg 2 — the heal fired and retrieval self-repopulated ----
    assert!(
        loaded.contains(absent_id),
        "heal (Sub-case B) must repopulate the graph-absent entry {absent_id}"
    );
    let after = harness
        .search(absent_content, 10)
        .await
        .expect("post-heal search");
    assert!(
        after.iter().any(|r| r.entry.id == absent_id),
        "previously-absent entry {absent_id} must be retrievable after the heal (Leg 2)"
    );
}
