//! vnc-025 (#670) dispatch-wiring component tests, part 2: PreCompact tail
//! block (§3), content-free logging (§4), HTTP convergence (§5).
//! Test plan: product/features/vnc-025/test-plan/dispatch-wiring.md.
//! Split from `transcript.rs` per the 500-line rule; shares its helpers.

use super::transcript::{
    Deps, buffer_contents, capture_tracing, dispatch_compact, dispatch_delta, jsonl_line,
};
use super::*;
use crate::uds::transcript_block::extract_transcript_block;

// =========================================================================
// §3 PreCompact block build — R-09, AC-11 (empty-buffer byte-identity hard
// gate lives in the parent module's Wave 0 baseline test)
// =========================================================================

/// AC-11 end-to-end golden: BriefingContent for a streamed session starts with
/// the block `extract_transcript_block(path)` produces on the same fixture —
/// expectation computed at test time, never hand-written.
#[tokio::test]
async fn test_compact_payload_nonempty_buffer_prepends_tail_block() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-golden", None, None);

    let mut lines = Vec::new();
    for i in 0..8 {
        lines.push(jsonl_line(
            "user",
            &format!("user message {i} for the golden run"),
        ));
        lines.push(jsonl_line(
            "assistant",
            &format!("assistant response {i} for the golden run"),
        ));
    }
    let full = lines.join("\n");
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("golden.jsonl");
    std::fs::write(&path, &full).unwrap();
    let expected = extract_transcript_block(path.to_str().unwrap())
        .expect("fixture must yield a block via the path variant");

    // Stream the same bytes as shuffled deltas: evens ascending, odds descending.
    const CHUNK: usize = 101;
    let chunks: Vec<(u64, &[u8])> = full
        .as_bytes()
        .chunks(CHUNK)
        .enumerate()
        .map(|(i, c)| ((i * CHUNK) as u64, c))
        .collect();
    for (off, c) in chunks.iter().filter(|(o, _)| (o / CHUNK as u64) % 2 == 0) {
        let resp = dispatch_delta(
            &deps,
            &registry,
            "sess-golden",
            *off,
            std::str::from_utf8(c).unwrap(),
        )
        .await;
        assert!(matches!(resp, HookResponse::Ack));
    }
    for (off, c) in chunks
        .iter()
        .rev()
        .filter(|(o, _)| (o / CHUNK as u64) % 2 == 1)
    {
        let resp = dispatch_delta(
            &deps,
            &registry,
            "sess-golden",
            *off,
            std::str::from_utf8(c).unwrap(),
        )
        .await;
        assert!(matches!(resp, HookResponse::Ack));
    }

    match dispatch_compact(&deps, &registry, "sess-golden").await {
        HookResponse::BriefingContent { content, .. } => {
            assert!(
                content.starts_with(&expected),
                "BriefingContent must start with the path-variant block"
            );
        }
        other => panic!("expected BriefingContent, got {other:?}"),
    }
}

/// R-09.5: token_count is computed AFTER the prepend — it reflects the block.
#[tokio::test]
async fn test_compact_payload_token_count_includes_prepended_block() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-tokens", None, None);

    let line = jsonl_line("user", "token count must include this block");
    dispatch_delta(&deps, &registry, "sess-tokens", 0, &line).await;

    match dispatch_compact(&deps, &registry, "sess-tokens").await {
        HookResponse::BriefingContent {
            content,
            token_count,
        } => {
            assert!(!content.is_empty(), "block must be present");
            assert!(token_count > 0, "token_count must reflect the block");
            assert_eq!(
                token_count,
                (content.len() / 4) as u32,
                "token_count must be computed on the post-prepend content"
            );
        }
        other => panic!("expected BriefingContent, got {other:?}"),
    }
}

/// FR-18/FR-19: a tail that yields no block (post-hole fragment with no
/// complete JSONL line) produces a response identical to the empty-buffer path.
#[tokio::test]
async fn test_compact_payload_contiguous_tail_none_identical_to_empty() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("sess-holey", None, None);
    registry.register_session("sess-empty-control", None, None);

    // Valid content, then a hole, then a fragment with no parseable line:
    // contiguous_tail never crosses the hole, so only the fragment is read.
    let line = jsonl_line("user", "this side of the hole is unreachable");
    dispatch_delta(&deps, &registry, "sess-holey", 0, &line).await;
    dispatch_delta(
        &deps,
        &registry,
        "sess-holey",
        9000,
        "fragment-with-no-complete-jsonl-line",
    )
    .await;

    let holey = dispatch_compact(&deps, &registry, "sess-holey").await;
    let control = dispatch_compact(&deps, &registry, "sess-empty-control").await;
    assert_eq!(
        serde_json::to_string(&holey).unwrap(),
        serde_json::to_string(&control).unwrap(),
        "None-block response must be identical to the empty-buffer path"
    );
}

/// R-09.6: deltas streaming concurrently with compact reads — every response
/// is either the empty path or a well-formed wrapped block (no torn read).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_compact_read_concurrent_with_deltas_point_in_time() {
    let deps = Deps::new().await;
    let registry = Arc::new(make_registry());
    registry.register_session("sess-conc", None, None);

    let stream: Vec<u8> = (0..200)
        .map(|i| jsonl_line("user", &format!("concurrent message {i}")))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let reg_writer = Arc::clone(&registry);
    let bytes = stream.clone();
    let writer = std::thread::spawn(move || {
        const CHUNK: usize = 64;
        for (i, c) in bytes.chunks(CHUNK).enumerate() {
            reg_writer.apply_transcript_delta("sess-conc", (i * CHUNK) as u64, c);
        }
    });

    for _ in 0..5 {
        match dispatch_compact(&deps, &registry, "sess-conc").await {
            HookResponse::BriefingContent { content, .. } => {
                if !content.is_empty() {
                    assert!(
                        content.starts_with("=== Recent conversation"),
                        "point-in-time block must be well-formed"
                    );
                    assert!(content.ends_with("=== End recent conversation ==="));
                }
            }
            other => panic!("expected BriefingContent, got {other:?}"),
        }
    }
    writer.join().unwrap();
}

// =========================================================================
// §4 Content-free logging — R-05.2, AC-04
// =========================================================================

/// R-05.2 sentinel gate: malformed AND well-formed deltas carrying a sentinel
/// through single arm, batch tee, merge, overflow (small-cap), compact, and
/// purge — the sentinel never appears in captured tracing output.
#[tokio::test]
async fn test_delta_paths_never_log_sentinel() {
    const SENTINEL: &str = "SENTINEL-vnc025-cafebabe-NEVER-LOG";
    let (writer, _guard) = capture_tracing();

    let deps = Deps::new().await;
    let registry = SessionRegistry::with_transcript_cap(64); // overflow path
    registry.register_session("sess-sentinel", None, None);

    // Well-formed, over-cap (merge + ring-tail elision).
    let payload = format!("{SENTINEL}-{}", "f".repeat(100));
    let resp = dispatch_delta(&deps, &registry, "sess-sentinel", 0, &payload).await;
    assert!(matches!(resp, HookResponse::Ack));

    // Malformed: sentinel as the wrong-typed `offset` value — serde_json embeds
    // string values in invalid-type error Display (the leak this gate exists for).
    let resp = deps
        .dispatch(
            HookRequest::RecordEvent {
                event: make_delta_event(
                    "sess-sentinel",
                    serde_json::json!({"offset": SENTINEL, "bytes": "x"}),
                ),
            },
            &registry,
        )
        .await;
    assert!(
        matches!(resp, HookResponse::Ack),
        "AC-04: malformed must Ack"
    );

    // Batch tee: one malformed + one well-formed, both carrying the sentinel.
    let resp = deps
        .dispatch(
            HookRequest::RecordEvents {
                events: vec![
                    make_delta_event(
                        "sess-sentinel",
                        serde_json::json!({"offset": SENTINEL, "bytes": "y"}),
                    ),
                    make_delta_event(
                        "sess-sentinel",
                        serde_json::json!({"offset": 200u64, "bytes": SENTINEL}),
                    ),
                ],
            },
            &registry,
        )
        .await;
    assert!(matches!(resp, HookResponse::Ack));

    // Compact (block build over sentinel-bearing buffer).
    dispatch_compact(&deps, &registry, "sess-sentinel").await;

    // Purge via session close (audit emission logging included).
    deps.dispatch(
        HookRequest::SessionClose {
            session_id: "sess-sentinel".to_string(),
            outcome: Some("success".to_string()),
            duration_secs: 1,
        },
        &registry,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let captured = String::from_utf8_lossy(&writer.0.lock().unwrap()).to_string();
    assert!(
        !captured.is_empty(),
        "capture layer must have seen tracing output"
    );
    assert!(
        !captured.contains(SENTINEL),
        "transcript sentinel leaked into tracing output"
    );
}

// =========================================================================
// §5 HTTP convergence — R-12, AC-06 (pattern #4725: pre-dispatch transform
// proven in http/router tests; merge assertions once-only via §1)
// =========================================================================

/// AC-06: an HTTP-shaped delta (post-`prefix_session_id`) lands in the
/// `http-{id}` buffer, never a bare-`{id}` buffer.
#[tokio::test]
async fn test_http_delta_lands_in_http_prefixed_buffer() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("http-sess-h1", None, None);

    let resp = dispatch_delta(&deps, &registry, "http-sess-h1", 0, "http delta bytes").await;
    assert!(matches!(resp, HookResponse::Ack));
    assert_eq!(
        buffer_contents(&registry, "http-sess-h1"),
        b"http delta bytes".to_vec()
    );
    assert!(
        registry.get_state("sess-h1").is_none(),
        "no bare-id session slot may exist"
    );
}

/// R-12: a delta without `SessionWrite` is rejected before dispatch reaches
/// the merge — no buffer mutation.
#[tokio::test]
async fn test_http_delta_without_session_write_rejected_before_dispatch() {
    let deps = Deps::new().await;
    let registry = make_registry();
    registry.register_session("http-sess-h2", None, None);

    let resp = deps
        .dispatch_with_caps(
            HookRequest::RecordEvent {
                event: make_delta_event(
                    "http-sess-h2",
                    serde_json::json!({"offset": 0, "bytes": "must not merge"}),
                ),
            },
            &registry,
            &[Capability::Search],
        )
        .await;
    match resp {
        HookResponse::Error { code, .. } => assert_eq!(code, -32003),
        other => panic!("expected capability Error, got {other:?}"),
    }
    assert!(
        buffer_contents(&registry, "http-sess-h2").is_empty(),
        "rejected delta must not have merged"
    );
}
