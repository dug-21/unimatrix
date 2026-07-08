//! vnc-046 Wave 4 (ADR-003, FR-13, AC-08) Guard 1 tests: the per-slug isolation
//! boot assertion (`assert_per_slug_isolation`) + production-resolver wiring pins.
//!
//! Guard 2 (the compile-time exhaustive field census, no `..`) lives in the lib
//! crate (`server_field_census.rs`) because it must name the module-private
//! `tool_router`/`server_info` fields; the compile step IS its test — a new
//! `UnimatrixServer` field breaks the build until classified (test-plan #6/#7,
//! OQ-3: no trybuild harness in this repo).
//!
//! Every test that builds a `UnimatrixServer`/`MultiProjectRouter` uses
//! `#[tokio::test(flavor = "multi_thread")]` — a per-slug registry construction
//! issues a blocking call that panics on the single-thread runtime (pattern #5637).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::domain::DomainPackRegistry;
use unimatrix_server::http::{MultiProjectRouter, ProjectKey, ProjectSlug, StoreResolver};
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::config::{InferenceConfig, RetentionConfig, StoreConfig};
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_server::infra::session::SessionRegistry;
use unimatrix_server::infra::transcript_activity::SignatureScanner;
use unimatrix_server::server::PendingEntriesAnalysis;
use unimatrix_store::{PoolConfig, SqlxStore};

use super::{IsolationProbe, assert_per_slug_isolation};

const TEST_MAX_BODY: usize = 1_048_576;
/// Mirrors the private `http_provision::PROJECT_DB_NAME` — the slug store filename.
const DB_NAME: &str = "unimatrix.db";

/// A built single-slug PRODUCTION resolver plus the per-slug handles captured off
/// the server BEFORE it moved into `from_servers` — so tests can build a probe and
/// `Arc::ptr_eq`-pin the resolver's returns to the server's instances.
struct Built {
    router: MultiProjectRouter,
    slug: ProjectSlug,
    session_registry: Arc<SessionRegistry>,
    pending: Arc<Mutex<PendingEntriesAnalysis>>,
    has_hold: bool,
    /// THIS slug's RESOLVED `transcript_hold_max_sessions` — the value the hold was
    /// built from (mirrors the production `IsolationProbe` capture, PR #936 Finding 1).
    transcript_hold_max_sessions: usize,
    signal_class_names: Arc<Vec<String>>,
    store_config: Arc<StoreConfig>,
    inference_config: Arc<InferenceConfig>,
    // Held only to keep the slug's on-disk store alive for the server's lifetime.
    _base: tempfile::TempDir,
}

/// Build ONE real per-slug server via the production `build_project_server`, with
/// DISTINCTIVE non-default store/inference config, then wrap it in the production
/// `MultiProjectRouter`. `signals` controls the per-slug signal class names (an
/// empty vec exercises the P3 hollow-counts guard).
async fn build_single(slug_name: &str, signals: Vec<String>) -> Built {
    let base = tempfile::TempDir::new().expect("temp base dir");
    let slug = ProjectSlug::try_from(slug_name).expect("valid slug");

    // The slug's store must already exist — `build_project_server` never creates it.
    let slug_dir = base.path().join(slug.as_str());
    std::fs::create_dir_all(&slug_dir).expect("slug dir");
    let seed = Arc::new(
        SqlxStore::open(&slug_dir.join(DB_NAME), PoolConfig::default())
            .await
            .expect("open slug store"),
    );
    drop(seed);

    let embed_handle = EmbedServiceHandle::new();
    let rayon_pool = Arc::new(RayonPool::new(1, "test-pool").expect("rayon pool"));
    let nli_handle = NliServiceHandle::new();
    let confidence_params = Arc::new(ConfidenceParams::default());
    let categories = Arc::new(CategoryAllowlist::new());
    let boosted: HashSet<String> = HashSet::new();

    // Distinctive, non-default config-snapshot values (each differs from default).
    let store_config = Arc::new(StoreConfig {
        max_content_bytes: 12_345, // != 8_000 default
    });
    let retention_config = Arc::new(RetentionConfig::default());
    let signal_class_names = Arc::new(signals);
    let inference_config = Arc::new(InferenceConfig {
        nli_top_k: 3, // != default
        ..InferenceConfig::default()
    });
    let observation_registry = Arc::new(DomainPackRegistry::with_builtin_claude_code());
    let signature_scanner = Arc::new(SignatureScanner::empty());

    let input = super::http_provision::build_project_server(
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

    // Capture the per-slug handles BEFORE `input` moves into `from_servers`.
    let session_registry = Arc::clone(&input.server.session_registry);
    let pending = Arc::clone(&input.server.pending_entries_analysis);
    let has_hold = input.server.session_registry.has_transcript_hold();
    let hold_max_sessions = retention_config.transcript_hold_max_sessions;
    let server_signal_names = Arc::clone(&input.server.transcript_signal_class_names);
    let server_store_config = Arc::clone(&input.server.store_config);
    let server_inference_config = Arc::clone(&input.server.inference_config);

    let router = MultiProjectRouter::from_servers(
        vec![input],
        TEST_MAX_BODY,
        vec![],
        // bug #774: non-empty allowed_hosts (empty = rmcp fail-open).
        vec!["localhost".to_string()],
    )
    .expect("build resolver");

    Built {
        router,
        slug,
        session_registry,
        pending,
        has_hold,
        transcript_hold_max_sessions: hold_max_sessions,
        signal_class_names: server_signal_names,
        store_config: server_store_config,
        inference_config: server_inference_config,
        _base: base,
    }
}

/// #4 — a correctly-built slug (all handles + config wired) boot-asserts `Ok(())`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assert_per_slug_isolation_fully_wired_returns_ok() {
    let built = build_single("alpha", vec!["alpha_signal".to_string()]).await;
    assert!(
        built.has_hold,
        "build_project_server must wire the transcript hold (F1/SR-03 pair)"
    );
    let probe = IsolationProbe {
        slug: built.slug.clone(),
        session_registry: Arc::clone(&built.session_registry),
        pending: Arc::clone(&built.pending),
        has_hold: built.has_hold,
        transcript_hold_max_sessions: built.transcript_hold_max_sessions,
        signal_class_names: Arc::clone(&built.signal_class_names),
        declares_signals: true,
    };
    assert_per_slug_isolation(&probe, &built.router)
        .expect("a correctly-wired slug must boot-assert Ok");
}

/// #6 (PR #936 Finding 1, #934) — a slug whose RESOLVED
/// `transcript_hold_max_sessions == 0` returns `Err`, even though the GLOBAL config
/// default is non-zero (the fully-wired test above passes that same default and gets
/// `Ok`). Proves the zero-check reads the per-slug value captured on the probe — the
/// value the hold was actually built from — NOT the global config. Closes the evade
/// path where a slug overlays `0` (project-wins, config.rs:4332) while global is
/// non-zero, which previously slipped past this loud-abort.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assert_per_slug_isolation_per_slug_zero_hold_returns_err() {
    let built = build_single("alpha", vec!["alpha_signal".to_string()]).await;
    // Sanity: the global/default retention is non-zero — so a probe value of 0 can
    // ONLY originate from THIS slug's resolved overlay, which is the evade path.
    assert_ne!(
        RetentionConfig::default().transcript_hold_max_sessions,
        0,
        "guard precondition: the global default must be non-zero for this test to \
         exercise the per-slug (not global) read"
    );
    let probe = IsolationProbe {
        slug: built.slug.clone(),
        session_registry: Arc::clone(&built.session_registry),
        pending: Arc::clone(&built.pending),
        has_hold: true,
        transcript_hold_max_sessions: 0, // THIS slug resolved to 0 (global non-zero)
        signal_class_names: Arc::clone(&built.signal_class_names),
        declares_signals: false,
    };
    let err = assert_per_slug_isolation(&probe, &built.router)
        .expect_err("a per-slug resolved transcript_hold_max_sessions == 0 must fail loud");
    assert!(
        err.to_string()
            .contains("transcript_hold_max_sessions == 0 disables the hold"),
        "message must name the disabled per-slug hold: {err}"
    );
}

/// #1 (R-03 / AC-08 — Critical) — a `session_registry` left as the constructor
/// default (write != read instance) returns `Err`, aborting boot. A REAL `Result`,
/// NOT a `debug_assert` (which is compiled out of release ⇒ zero coverage, NFR-2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assert_per_slug_isolation_unwired_registry_returns_err() {
    let built = build_single("alpha", vec!["alpha_signal".to_string()]).await;
    // A DIFFERENT registry instance than the one the resolver holds — the
    // write-path != read-path split the ptr_eq convergence check exists to catch.
    let stray = Arc::new(SessionRegistry::with_transcript_cap(1024));
    let probe = IsolationProbe {
        slug: built.slug.clone(),
        session_registry: stray,
        pending: Arc::clone(&built.pending),
        has_hold: true,
        transcript_hold_max_sessions: built.transcript_hold_max_sessions,
        signal_class_names: Arc::clone(&built.signal_class_names),
        declares_signals: false,
    };
    let err = assert_per_slug_isolation(&probe, &built.router)
        .expect_err("unwired session_registry must fail loud (Result, not a debug panic)");
    assert!(
        err.to_string().contains("not converged"),
        "message must name the convergence failure: {err}"
    );
}

/// #2 (R-05) — a slug whose registry carries no wired hold returns `Err`; prevents
/// the purge-gate split (registry-alone ⇒ held buffers never purge ⇒ unbounded
/// memory).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assert_per_slug_isolation_unpaired_hold_returns_err() {
    let built = build_single("alpha", vec!["alpha_signal".to_string()]).await;
    let probe = IsolationProbe {
        slug: built.slug.clone(),
        session_registry: Arc::clone(&built.session_registry),
        pending: Arc::clone(&built.pending),
        has_hold: false, // the F1/SR-03 pairing violation
        transcript_hold_max_sessions: built.transcript_hold_max_sessions,
        signal_class_names: Arc::clone(&built.signal_class_names),
        declares_signals: false,
    };
    let err =
        assert_per_slug_isolation(&probe, &built.router).expect_err("unpaired hold must fail loud");
    assert!(
        err.to_string().contains("transcript_hold not wired"),
        "message must name the unpaired hold: {err}"
    );
}

/// #3 (R-04 P3) — a config-snapshot field still at the `new()` default (empty
/// `signal_class_names`) while the slug DECLARED signals returns `Err` — the
/// hollow-counts guard (`signal_class_counts_json == "{}"` symptom, #930).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assert_per_slug_isolation_unset_config_sentinels_return_err() {
    // Server built with EMPTY signal class names (the `new()` default symptom), but
    // the slug's config declared signals → the P3 sentinel fires.
    let built = build_single("alpha", vec![]).await;
    assert!(built.has_hold);
    assert!(
        built.signal_class_names.is_empty(),
        "the built server holds the empty-default class names for this case"
    );
    let probe = IsolationProbe {
        slug: built.slug.clone(),
        session_registry: Arc::clone(&built.session_registry),
        pending: Arc::clone(&built.pending),
        has_hold: true,
        transcript_hold_max_sessions: built.transcript_hold_max_sessions,
        signal_class_names: Arc::clone(&built.signal_class_names),
        declares_signals: true, // config declared [transcript_signals]
    };
    let err = assert_per_slug_isolation(&probe, &built.router)
        .expect_err("declared-but-empty signal names must fail loud");
    assert!(
        err.to_string().contains("empty despite declared"),
        "message must name the hollow-counts sentinel: {err}"
    );
}

/// #5 (R-03 / R-06 wiring-pin) — against the PRODUCTION resolver (not a double):
/// `registry_for`/`pending_for` return the SAME instance the slug's server holds.
/// Also the AC-06 white-box home for `store_config`/`inference_config` (no resolver
/// method ⇒ value-pinned to their distinctive threaded snapshots, not defaults).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_registry_for_ptr_eq_slug_server_registry() {
    let built = build_single("alpha", vec!["alpha_signal".to_string()]).await;
    let key = ProjectKey::Slug(built.slug.clone());

    assert!(
        Arc::ptr_eq(
            &built.router.registry_for(&key).expect("registry_for"),
            &built.session_registry
        ),
        "registry_for must hand back the INSTANCE the server holds (write == read)"
    );
    assert!(
        Arc::ptr_eq(
            &built.router.pending_for(&key).expect("pending_for"),
            &built.pending
        ),
        "pending_for must hand back the INSTANCE the server holds (write == read)"
    );

    // AC-06 white-box exception: store_config / inference_config lack a resolver
    // method — pin them by value (distinctive, so not the new() default).
    assert_eq!(
        built.store_config.max_content_bytes, 12_345,
        "store_config must be the threaded per-slug snapshot, not new()'s default"
    );
    assert_eq!(
        built.inference_config.nli_top_k, 3,
        "inference_config must be the threaded per-slug snapshot, not new()'s default"
    );
}
