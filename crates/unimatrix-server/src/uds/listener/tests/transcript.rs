//! vnc-025 (#670) dispatch-wiring component tests, part 1: shared helpers,
//! single-arm merge (§1), batch tee + non-persistence (§2).
//! Test plan: product/features/vnc-025/test-plan/dispatch-wiring.md.
//!
//! Child of `listener.rs::tests` — shares its dispatch helpers. §3–§5 live in
//! the sibling module `compact.rs`; purge-audit tests in `purge_audit.rs`.

use super::*;

// -- Shared helpers (also used by the sibling modules) --

/// Bundled dispatch dependencies so each test reads as intent, not plumbing.
pub(crate) struct Deps {
    pub(crate) store: Arc<Store>,
    embed: Arc<EmbedServiceHandle>,
    vs: Arc<AsyncVectorStore<VectorAdapter>>,
    es: Arc<Store>,
    adapt: Arc<AdaptationService>,
}

impl Deps {
    pub(crate) async fn new() -> Self {
        let store = make_store().await;
        let embed = make_embed_service();
        let (vs, es, adapt) = make_dispatch_deps(&store);
        Deps {
            store,
            embed,
            vs,
            es,
            adapt,
        }
    }

    pub(crate) async fn dispatch(
        &self,
        req: HookRequest,
        registry: &SessionRegistry,
    ) -> HookResponse {
        self.dispatch_with_caps(req, registry, crate::uds::UDS_CAPABILITIES)
            .await
    }

    pub(crate) async fn dispatch_with_caps(
        &self,
        req: HookRequest,
        registry: &SessionRegistry,
        caps: &[Capability],
    ) -> HookResponse {
        dispatch_request(
            req,
            &self.store,
            &self.embed,
            &self.vs,
            &self.es,
            &self.adapt,
            "0.1.0",
            registry,
            &make_pending(),
            &make_services(&self.store, &self.embed, &self.vs, &self.es, &self.adapt),
            caps,
        )
        .await
    }
}

pub(crate) async fn dispatch_delta(
    deps: &Deps,
    registry: &SessionRegistry,
    session_id: &str,
    offset: u64,
    bytes: &str,
) -> HookResponse {
    deps.dispatch(
        HookRequest::RecordEvent {
            event: make_delta_event(
                session_id,
                serde_json::json!({"offset": offset, "bytes": bytes}),
            ),
        },
        registry,
    )
    .await
}

pub(crate) async fn dispatch_compact(
    deps: &Deps,
    registry: &SessionRegistry,
    session_id: &str,
) -> HookResponse {
    deps.dispatch(
        HookRequest::CompactPayload {
            session_id: session_id.to_string(),
            injected_entry_ids: vec![],
            role: None,
            feature: None,
            token_limit: None,
            transcript_excerpt: None,
            accept: None,
        },
        registry,
    )
    .await
}

/// Full readable content of a session's buffer (post-hole contiguous tail with
/// an effectively unbounded window). Empty vec for absent session/empty buffer.
pub(crate) fn buffer_contents(registry: &SessionRegistry, session_id: &str) -> Vec<u8> {
    registry
        .get_state(session_id)
        .and_then(|s| lock_buffer(&s.transcript).contiguous_tail(1 << 20))
        .unwrap_or_default()
}

/// Poison a registered session's buffer mutex (panic while holding the lock).
pub(crate) fn poison_buffer(registry: &SessionRegistry, session_id: &str) {
    let state = registry.get_state(session_id).expect("session registered");
    let arc = Arc::clone(&state.transcript);
    let _ = std::thread::spawn(move || {
        let _guard = arc.lock().unwrap();
        panic!("deliberate poison for test");
    })
    .join();
}

pub(crate) fn jsonl_line(role: &str, text: &str) -> String {
    format!(
        r#"{{"type":"{}","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
        role, text
    )
}

/// Tracing capture: thread-local default subscriber writing to a shared buffer.
/// Current-thread tokio tests run spawned tasks on the same thread, so delta
/// dispatch, merge, and fire-and-forget audit logging are all captured.
#[derive(Clone, Default)]
pub(crate) struct CaptureWriter(pub(crate) Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;
    fn make_writer(&'a self) -> CaptureWriter {
        self.clone()
    }
}

pub(crate) fn capture_tracing() -> (CaptureWriter, tracing::subscriber::DefaultGuard) {
    let writer = CaptureWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::level_filters::LevelFilter::TRACE)
        .with_ansi(false)
        .with_writer(writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    (writer, guard)
}

// =========================================================================
// §1 Single-arm merge — AC-01, AC-03, always-Ack matrix (FR-06)
// =========================================================================

/// AC-01: well-formed delta for a registered session → Ack; buffer content
/// equals streamed bytes.
#[tokio::test]
async fn test_transcript_delta_uds_merges_into_registered_buffer() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-merge-1", None, None);

    let resp = dispatch_delta(&deps, &registry, "sess-merge-1", 0, "hello transcript").await;
    assert!(matches!(resp, HookResponse::Ack), "got {resp:?}");
    assert_eq!(
        buffer_contents(&registry, "sess-merge-1"),
        b"hello transcript".to_vec(),
        "buffer content must equal streamed bytes"
    );

    // Second delta extends contiguously.
    let resp = dispatch_delta(&deps, &registry, "sess-merge-1", 16, " continues").await;
    assert!(matches!(resp, HookResponse::Ack));
    assert_eq!(
        buffer_contents(&registry, "sess-merge-1"),
        b"hello transcript continues".to_vec()
    );
}

/// AC-03: unknown session → Ack, no slot created, other buffers unaffected.
#[tokio::test]
async fn test_transcript_delta_unregistered_acks_no_slot() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-other", None, None);
    dispatch_delta(&deps, &registry, "sess-other", 0, "other-bytes").await;
    assert_eq!(registry.session_count(), 1);

    let resp = dispatch_delta(&deps, &registry, "sess-ghost", 0, "ghost-bytes").await;
    assert!(
        matches!(resp, HookResponse::Ack),
        "unregistered delta must Ack, got {resp:?}"
    );
    assert_eq!(
        registry.session_count(),
        1,
        "unregistered delta must not create a session slot"
    );
    assert_eq!(
        buffer_contents(&registry, "sess-other"),
        b"other-bytes".to_vec(),
        "another session's buffer must be unaffected"
    );
}

/// ADR-008/Constraint 4: a poisoned buffer mutex still Acks (treat-as-empty
/// recovery), and the session keeps working afterwards.
#[tokio::test]
async fn test_transcript_delta_poisoned_buffer_still_acks() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-poison", None, None);
    dispatch_delta(&deps, &registry, "sess-poison", 0, "pre-poison").await;

    poison_buffer(&registry, "sess-poison");

    let resp = dispatch_delta(&deps, &registry, "sess-poison", 100, "post-poison").await;
    assert!(
        matches!(resp, HookResponse::Ack),
        "always-Ack must survive poison recovery, got {resp:?}"
    );
}

/// FR-06 over-cap arm of the always-Ack matrix: a delta far beyond the
/// configured cap (ring-tail elision) still Acks.
#[tokio::test]
async fn test_transcript_delta_over_cap_still_acks() {
    let deps = Deps::new().await;
    let registry = SessionRegistry::with_transcript_cap(64);
    registry.register_session("sess-cap", None, None);

    let big = "x".repeat(500);
    let resp = dispatch_delta(&deps, &registry, "sess-cap", 0, &big).await;
    assert!(matches!(resp, HookResponse::Ack), "got {resp:?}");
    // Ring-tail keeps at most the cap.
    assert!(buffer_contents(&registry, "sess-cap").len() <= 64);
}

// =========================================================================
// §2 Batch tee + non-persistence — R-04 (vnc-024 suite runs unmodified in
// the parent module), AC-05, #3902 signature
// =========================================================================

/// AC-05/R-04.2: mixed batch persists exactly the non-delta events (asserting
/// row CONTENT, not just count) while deltas merge into the buffer — repeated
/// through the HTTP-shaped (post-`prefix_session_id`) path.
#[tokio::test]
async fn test_mixed_batch_persists_non_delta_merges_delta() {
    const DELTA_MARKER: &str = "MIXEDBATCH-DELTA-BYTES-XYZZY";
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-mixed", None, None);
    registry.register_session("http-sess-mixed", None, None);

    // UDS shape.
    let resp = deps
        .dispatch(
            HookRequest::RecordEvents {
                events: vec![
                    make_cycle_event(
                        "PreToolUse",
                        "sess-mixed-normal",
                        serde_json::json!({"tool": "Bash"}),
                        None,
                    ),
                    make_delta_event(
                        "sess-mixed",
                        serde_json::json!({"offset": 0, "bytes": DELTA_MARKER}),
                    ),
                    make_cycle_event(
                        "PostToolUse",
                        "sess-mixed-normal",
                        serde_json::json!({"tool": "Read"}),
                        None,
                    ),
                ],
            },
            &registry,
        )
        .await;
    assert!(matches!(resp, HookResponse::Ack));

    // HTTP shape: same batch post-prefix_session_id (ids carry the http- prefix;
    // event_type preserved — transform proven in http/router tests, #4725).
    let resp = deps
        .dispatch(
            HookRequest::RecordEvents {
                events: vec![
                    make_delta_event(
                        "http-sess-mixed",
                        serde_json::json!({"offset": 0, "bytes": DELTA_MARKER}),
                    ),
                    make_cycle_event(
                        "PreToolUse",
                        "http-sess-mixed-normal",
                        serde_json::json!({"tool": "Bash"}),
                        None,
                    ),
                ],
            },
            &registry,
        )
        .await;
    assert!(matches!(resp, HookResponse::Ack));

    // Exactly the 3 non-delta events persist (fire-and-forget — poll), zero
    // delta-derived rows.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while count_all_observations(&deps.store).await < 3 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for the 3 non-delta rows"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Settle, then assert the count is EXACTLY 3 (no delta-derived row trails in).
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(count_all_observations(&deps.store).await, 3);
    assert!(
        query_observations(&deps.store, "sess-mixed")
            .await
            .is_empty()
            && query_observations(&deps.store, "http-sess-mixed")
                .await
                .is_empty(),
        "delta elements must produce zero rows"
    );

    // Deltas merged into the right buffers.
    assert_eq!(
        buffer_contents(&registry, "sess-mixed"),
        DELTA_MARKER.as_bytes().to_vec()
    );
    assert_eq!(
        buffer_contents(&registry, "http-sess-mixed"),
        DELTA_MARKER.as_bytes().to_vec()
    );

    // Delta bytes absent from EVERY persisted column (row content, not count).
    use sqlx::Row as _;
    let rows = sqlx::query(
        "SELECT session_id || ' ' || hook || ' ' || COALESCE(tool,'') || ' ' || \
         COALESCE(input,'') || ' ' || COALESCE(response_snippet,'') || ' ' || \
         COALESCE(topic_signal,'') || ' ' || COALESCE(phase,'') AS t FROM observations",
    )
    .fetch_all(deps.store.read_pool_test())
    .await
    .expect("query all observation columns");
    for row in &rows {
        let text: String = row.get("t");
        assert!(
            !text.contains(DELTA_MARKER),
            "delta bytes leaked into a persisted column: {text}"
        );
    }
}

/// #3902 regression signature: a normal delta dispatch fires ZERO audit events.
#[tokio::test]
async fn test_delta_dispatch_emits_no_new_audit_events() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-noaudit", None, None);

    let count_audit = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log")
            .fetch_one(deps.store.read_pool_test())
            .await
            .expect("count audit rows")
    };
    let before = count_audit().await;

    let resp = dispatch_delta(&deps, &registry, "sess-noaudit", 0, "normal delta").await;
    assert!(matches!(resp, HookResponse::Ack));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(
        count_audit().await,
        before,
        "a happy-path delta dispatch must not write audit rows"
    );
}
