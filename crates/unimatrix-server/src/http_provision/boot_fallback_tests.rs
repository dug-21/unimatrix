//! GH-824 Item 1 boot-resilience test: a torn per-slug vector dump must NOT
//! hard-abort `build_project_server`; the slug boots with an EMPTY index and
//! self-heals via the existing maintenance heal pass.
//!
//! Drives the REAL `build_project_server` boot path (the F1 acceptance site
//! `http_provision.rs`) over a slug whose `{slug}/vector/` dir carries a
//! `point_count > 0` meta but a DELETED `.hnsw.graph` — the exact on-disk state
//! a SIGKILL mid-dump leaves. Asserts `Ok` with an empty index, not `Err`.

use std::collections::HashSet;
use std::sync::Arc;

use unimatrix_core::{VectorConfig, VectorIndex};
use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::domain::DomainPackRegistry;
use unimatrix_server::http::ProjectSlug;
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::config::InferenceConfig;
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_store::{NewEntry, PoolConfig, SqlxStore, Status};

use super::{PROJECT_DB_NAME, PROJECT_VECTOR_DIR, build_project_server};

/// Deterministic 384-dim normalized embedding (no ONNX) — for seeding the dump.
fn det_embedding(text: &str) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let seed = hasher.finish();
    let mut v = vec![0.0f32; 384];
    for (i, val) in v.iter_mut().enumerate() {
        let h = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(i as u64);
        *val = (h as f32) / (u64::MAX as f32) * 2.0 - 1.0;
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    } else {
        v[0] = 1.0;
    }
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_build_project_server_torn_dump_boots_empty_not_err() {
    let base = tempfile::TempDir::new().unwrap();
    let base_path = base.path();
    let slug = ProjectSlug::try_from("alpha").expect("valid slug");

    // The slug's store must already exist (build_project_server never creates it).
    let slug_dir = base_path.join(slug.as_str());
    let db_path = slug_dir.join(PROJECT_DB_NAME);
    let vector_dir = slug_dir.join(PROJECT_VECTOR_DIR);
    std::fs::create_dir_all(&vector_dir).unwrap();

    let store = Arc::new(
        SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("open slug store"),
    );

    // Seed a NON-EMPTY index and dump it to the slug's vector dir.
    let index = VectorIndex::new(Arc::clone(&store), VectorConfig::default()).expect("vi");
    for i in 0..4 {
        let entry_id = store
            .insert(NewEntry {
                title: format!("entry {i}"),
                content: format!("content {i}"),
                topic: "test".to_string(),
                category: "convention".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "bugfix-824".to_string(),
                trust_source: "agent".to_string(),
            })
            .await
            .expect("insert");
        index
            .insert(entry_id, &det_embedding(&format!("entry {i}content {i}")))
            .await
            .expect("insert vector");
    }
    index.dump(&vector_dir).expect("dump");
    drop(index);

    // TORN DUMP: meta claims point_count > 0 but the graph file is gone (the
    // exact SIGKILL-mid-dump artifact). Load would Err on this set.
    let meta = std::fs::read_to_string(vector_dir.join("unimatrix-vector.meta")).unwrap();
    assert!(
        meta.contains("point_count=4"),
        "precondition: meta claims a non-empty index"
    );
    std::fs::remove_file(vector_dir.join("unimatrix.hnsw.graph")).expect("delete graph");
    assert!(vector_dir.join("unimatrix-vector.meta").exists());

    // Drive the real boot path. With Item 1, this returns Ok (empty index +
    // warn), NOT Err — graceful degradation (Architectural Principle 5).
    let embed_handle = EmbedServiceHandle::new();
    let rayon_pool = Arc::new(RayonPool::new(1, "test-pool").expect("rayon pool"));
    let nli_handle = NliServiceHandle::new();
    let inference_config = Arc::new(InferenceConfig::default());
    let confidence_params = Arc::new(ConfidenceParams::default());
    let categories = Arc::new(CategoryAllowlist::new());
    let observation_registry = Arc::new(DomainPackRegistry::with_builtin_claude_code());
    let boosted: HashSet<String> = HashSet::new();

    let result = build_project_server(
        base_path,
        &slug,
        &embed_handle,
        true, // permissive
        None, // instructions
        &rayon_pool,
        &nli_handle,
        20,    // nli_top_k
        false, // nli_enabled
        &inference_config,
        &confidence_params,
        &categories,
        &observation_registry,
        &boosted,
    )
    .await;

    let input = result.expect("torn per-slug dump must NOT hard-abort boot (GH-824 Item 1)");
    assert_eq!(
        input.server.vector_index().point_count(),
        0,
        "torn dump must degrade to an EMPTY per-slug index, not Err"
    );
}
