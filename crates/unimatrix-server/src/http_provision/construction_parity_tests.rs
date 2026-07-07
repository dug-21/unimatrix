//! vnc-046 Wave 2 (ADR-002) construction-parity unit tests for `build_project_server`.
//!
//! Before this wave `build_project_server` set NONE of the per-slug registry/hold/pending
//! or config-snapshot fields, so `UnimatrixServer::new`'s test-defaults were read at runtime
//! (the #930 split-brain + P3 config gap). These tests drive the REAL `build_project_server`
//! and assert the constructed fields are the per-slug instances threaded in — not the
//! constructor defaults — over the binary crate (the integration `tests/` crate hosts the
//! bidirectional behavioral suite in a later wave; this is the additive white-box complement).
//!
//! Any test that builds a `UnimatrixServer` uses `#[tokio::test(flavor = "multi_thread")]`
//! (a per-slug registry construction issues a blocking call that panics on the single-thread
//! runtime — pattern #5637).

use std::collections::HashSet;
use std::sync::Arc;

use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::domain::DomainPackRegistry;
use unimatrix_server::http::{ProjectServerInput, ProjectSlug};
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::config::{InferenceConfig, RetentionConfig, StoreConfig};
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_server::infra::transcript_activity::SignatureScanner;
use unimatrix_store::{PoolConfig, SqlxStore};

use super::{PROJECT_DB_NAME, build_project_server};

/// The distinctive, non-default config threaded into one built slug server, plus the Arcs
/// passed in — so a test can prove each server field is the SAME instance it threaded
/// (`Arc::ptr_eq`), i.e. overwritten from the resolved config, not left at the `new()` default.
struct BuiltSlug {
    input: ProjectServerInput,
    store_config: Arc<StoreConfig>,
    retention_config: Arc<RetentionConfig>,
    signal_class_names: Arc<Vec<String>>,
    inference_config: Arc<InferenceConfig>,
    observation_registry: Arc<DomainPackRegistry>,
    // Held only to keep the slug's on-disk store alive for the server's lifetime.
    _base: tempfile::TempDir,
}

/// Build one real per-slug server with DISTINCTIVE, non-default config-snapshot values.
async fn build_distinctive_slug(slug_name: &str) -> BuiltSlug {
    let base = tempfile::TempDir::new().expect("temp base dir");
    let slug = ProjectSlug::try_from(slug_name).expect("valid slug");

    // The slug's store must already exist — `build_project_server` never creates it.
    let slug_dir = base.path().join(slug.as_str());
    std::fs::create_dir_all(&slug_dir).expect("slug dir");
    let db_path = slug_dir.join(PROJECT_DB_NAME);
    let seed_store = Arc::new(
        SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("open slug store"),
    );
    drop(seed_store);

    let embed_handle = EmbedServiceHandle::new();
    let rayon_pool = Arc::new(RayonPool::new(1, "test-pool").expect("rayon pool"));
    let nli_handle = NliServiceHandle::new();
    let confidence_params = Arc::new(ConfidenceParams::default());
    let categories = Arc::new(CategoryAllowlist::new());
    let boosted: HashSet<String> = HashSet::new();

    // DISTINCTIVE, non-default config-snapshot values (each differs from `*::default()`).
    let store_config = Arc::new(StoreConfig {
        max_content_bytes: 12_345, // != 8_000 default
    });
    let retention_config = Arc::new(RetentionConfig {
        transcript_hold_max_sessions: 7, // != 64 default
        ..RetentionConfig::default()
    });
    let signal_class_names = Arc::new(vec!["alpha_signal".to_string()]); // != empty default
    let inference_config = Arc::new(InferenceConfig {
        nli_top_k: 3, // distinctive; threaded to both ServiceLayer and the server field
        ..InferenceConfig::default()
    });
    let observation_registry = Arc::new(DomainPackRegistry::with_builtin_claude_code());
    let signature_scanner = Arc::new(SignatureScanner::empty());

    let input = build_project_server(
        base.path(),
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
        &store_config,
        &retention_config,
        &signal_class_names,
        &signature_scanner,
    )
    .await
    .expect("build_project_server");

    BuiltSlug {
        input,
        store_config,
        retention_config,
        signal_class_names,
        inference_config,
        observation_registry,
        _base: base,
    }
}

/// Test #1 — the 5 config-snapshot fields are set from the resolved config, NOT the
/// `UnimatrixServer::new` test-default (the #930 silent-default symptom). Proven via
/// `Arc::ptr_eq` to the threaded instance (equals the resolved value AND is not the default
/// allocation) plus a distinctive-value check.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_build_project_server_sets_five_config_snapshot_fields() {
    let built = build_distinctive_slug("alpha").await;
    let server = &built.input.server;

    assert!(
        Arc::ptr_eq(&server.store_config, &built.store_config),
        "store_config must be the threaded per-slug instance, not new()'s default"
    );
    assert_eq!(server.store_config.max_content_bytes, 12_345);

    assert!(
        Arc::ptr_eq(&server.retention_config, &built.retention_config),
        "retention_config must be the threaded per-slug instance"
    );
    assert_eq!(server.retention_config.transcript_hold_max_sessions, 7);

    assert!(
        Arc::ptr_eq(
            &server.transcript_signal_class_names,
            &built.signal_class_names
        ),
        "transcript_signal_class_names must be the threaded per-slug instance"
    );
    assert_eq!(
        server.transcript_signal_class_names.as_slice(),
        &["alpha_signal".to_string()]
    );

    assert!(
        Arc::ptr_eq(&server.inference_config, &built.inference_config),
        "inference_config must be the threaded per-slug instance"
    );
    assert!(
        Arc::ptr_eq(&server.observation_registry, &built.observation_registry),
        "observation_registry must be the threaded per-slug instance"
    );
}

/// Test #2 (R-05 — the load-bearing pairing) — the built registry carries its transcript
/// hold. The hold is constructed as a PAIR with the registry inside `build_project_server`;
/// registry-alone would split the purge gate (F1/SR-03).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_build_project_server_constructs_registry_hold_pair() {
    let built = build_distinctive_slug("alpha").await;
    assert!(
        built.input.server.session_registry.has_transcript_hold(),
        "the per-slug registry must be wired with its paired transcript hold (F1/SR-03)"
    );
}

/// Test #3 — the registry / pending / hold are fresh PER-SLUG instances, not a shared global:
/// two independently built slug servers have pairwise-distinct Arcs for all three.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pending_entries_analysis_constructed_per_slug() {
    let a = build_distinctive_slug("alpha").await;
    let b = build_distinctive_slug("beta").await;

    assert!(
        !Arc::ptr_eq(
            &a.input.server.pending_entries_analysis,
            &b.input.server.pending_entries_analysis
        ),
        "each slug must get its OWN pending accumulator, never a shared global"
    );
    assert!(
        !Arc::ptr_eq(
            &a.input.server.session_registry,
            &b.input.server.session_registry
        ),
        "each slug must get its OWN session registry"
    );
    assert!(
        !Arc::ptr_eq(
            &a.input.server.transcript_hold,
            &b.input.server.transcript_hold
        ),
        "each slug must get its OWN transcript hold"
    );
}
