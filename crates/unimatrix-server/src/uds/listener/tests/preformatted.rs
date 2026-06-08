//! vnc-027 (#680) listener-preformatted component tests: the UDS server-side
//! preformatted-response contract (ADR-001 §5,§6).
//!
//! Covers the two seams of `handle_connection` as pure functions:
//! - `request_wants_text` (seam 1, pre-dispatch `accept` extraction)
//! - `negotiate_text_response` (seam 2, post-dispatch `Text` conversion via the
//!   shared injection-text core)
//!
//! plus HTTP-vs-UDS body byte-equivalence (R-09 s3) and the hard allowlist /
//! frozen-hook safety coupling (R-08). Split per the 500-line rule; shares the
//! parent module's `use super::*`.
//! Test plan: product/features/vnc-027/test-plan/listener-preformatted.md.

use super::*;
use crate::uds::hook::{MAX_INJECTION_BYTES, format_injection};
use http_body_util::BodyExt;
use unimatrix_engine::wire::EntryPayload;

const INJECTION_HEADER: &str = "--- Unimatrix Context ---\n";

// -- Fixtures --------------------------------------------------------------

fn sample_entry(id: u64) -> EntryPayload {
    EntryPayload {
        id,
        title: format!("Entry {id}"),
        content: format!("Body content for entry {id}"),
        confidence: 0.9,
        similarity: 0.42,
        category: "pattern".to_string(),
    }
}

fn entries_resp(n: u64) -> HookResponse {
    HookResponse::Entries {
        items: (0..n).map(sample_entry).collect(),
        total_tokens: 100,
    }
}

fn empty_entries_resp() -> HookResponse {
    HookResponse::Entries {
        items: vec![],
        total_tokens: 0,
    }
}

fn briefing_resp() -> HookResponse {
    HookResponse::BriefingContent {
        content: "Use context_get to retrieve full entries.\n\nVerbatim briefing body.".to_string(),
        token_count: 12,
    }
}

fn context_search_req(accept: Option<&str>) -> HookRequest {
    HookRequest::ContextSearch {
        query: "q".to_string(),
        session_id: None,
        role: None,
        task: None,
        feature: None,
        k: None,
        max_tokens: None,
        source: None,
        accept: accept.map(|s| s.to_string()),
    }
}

fn compact_payload_req(accept: Option<&str>) -> HookRequest {
    HookRequest::CompactPayload {
        session_id: "s1".to_string(),
        injected_entry_ids: vec![],
        role: None,
        feature: None,
        token_limit: None,
        transcript_excerpt: None,
        accept: accept.map(|s| s.to_string()),
    }
}

// -- wants_text extraction (seam 1) ---------------------------------------

#[test]
fn test_wants_text_extracted_pre_dispatch() {
    // Extraction is by reference: the request is NOT consumed and remains usable
    // (it still moves into `dispatch_request` unchanged downstream).
    let req = context_search_req(Some("text/plain"));
    assert!(request_wants_text(&req));
    // Request still owned and matchable after extraction — dispatch shape unchanged.
    assert!(matches!(req, HookRequest::ContextSearch { .. }));

    let compact = compact_payload_req(Some("text/plain"));
    assert!(request_wants_text(&compact));
    assert!(matches!(compact, HookRequest::CompactPayload { .. }));
}

#[test]
fn test_unknown_accept_value_treated_as_absent() {
    // Any non-`text/plain` value (or absence) is inert — no `Text` can be coaxed.
    assert!(!request_wants_text(&context_search_req(Some(
        "application/xml"
    ))));
    assert!(!request_wants_text(&context_search_req(Some("text/html"))));
    assert!(!request_wants_text(&context_search_req(Some(""))));
    assert!(!request_wants_text(&context_search_req(None)));
    assert!(!request_wants_text(&compact_payload_req(Some(
        "application/json"
    ))));
}

#[test]
fn test_ping_briefing_never_want_text() {
    // No other request variant carries `accept`.
    assert!(!request_wants_text(&HookRequest::Ping));
    assert!(!request_wants_text(&HookRequest::Briefing {
        role: "dev".to_string(),
        task: "t".to_string(),
        feature: None,
        max_tokens: None,
    }));
}

// -- negotiation / allowlist / coupling (seam 2) --------------------------

#[test]
fn test_no_accept_yields_typed_json_never_text() {
    // R-07 s2 / ADR-001 §6 coupling: wants_text == false leaves the typed frame
    // untouched — never `Text`. This is the frozen-hook safety contract.
    let out = negotiate_text_response(entries_resp(2), false);
    assert!(matches!(out, HookResponse::Entries { .. }));

    let out = negotiate_text_response(briefing_resp(), false);
    assert!(matches!(out, HookResponse::BriefingContent { .. }));

    let out = negotiate_text_response(HookResponse::Ack, false);
    assert!(matches!(out, HookResponse::Ack));
}

#[test]
fn test_accept_text_plain_entries_yields_text() {
    let out = negotiate_text_response(entries_resp(2), true);
    assert!(matches!(out, HookResponse::Text { .. }));
}

#[test]
fn test_accept_text_plain_briefing_yields_text() {
    let HookResponse::BriefingContent { content, .. } = briefing_resp() else {
        unreachable!()
    };
    let out = negotiate_text_response(briefing_resp(), true);
    match out {
        HookResponse::Text { body } => {
            assert_eq!(body, content, "BriefingContent body is verbatim")
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn test_allowlist_ack_error_pong_always_json() {
    // Hard allowlist (R-08 s3): even WITH accept, Ack/Error/Pong stay typed JSON.
    let ack = negotiate_text_response(HookResponse::Ack, true);
    assert!(matches!(ack, HookResponse::Ack));

    let err = negotiate_text_response(
        HookResponse::Error {
            code: 400,
            message: "bad".to_string(),
        },
        true,
    );
    assert!(matches!(err, HookResponse::Error { .. }));

    // A Pong carrying server_version must remain parseable JSON (vnc-024 OQ-06).
    let pong = negotiate_text_response(
        HookResponse::Pong {
            server_version: "1.2.3".to_string(),
        },
        true,
    );
    match pong {
        HookResponse::Pong { server_version } => assert_eq!(server_version, "1.2.3"),
        other => panic!("expected Pong, got {other:?}"),
    }
}

#[test]
fn test_empty_injection_yields_ack_not_text() {
    // ADR-001 §4: empty Entries -> format_injection None -> Ack (204-equiv),
    // never a headerless Text.
    let out = negotiate_text_response(empty_entries_resp(), true);
    assert!(
        matches!(out, HookResponse::Ack),
        "empty injection with accept must be Ack, got {out:?}"
    );
}

// -- shared-core / injection-header (R-09) --------------------------------

#[test]
fn test_entries_text_body_starts_with_injection_header() {
    let out = negotiate_text_response(entries_resp(2), true);
    match out {
        HookResponse::Text { body } => assert!(
            body.starts_with(INJECTION_HEADER),
            "Entries Text body must start with the load-bearing injection header"
        ),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn test_briefing_text_body_has_no_injection_header() {
    let out = negotiate_text_response(briefing_resp(), true);
    match out {
        HookResponse::Text { body } => assert!(
            !body.starts_with(INJECTION_HEADER),
            "BriefingContent Text body is verbatim content, no injection header"
        ),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn test_shared_core_single_implementation() {
    // The shared core is THE formatting truth: response_injection_text(Entries)
    // is exactly format_injection(items, MAX_INJECTION_BYTES). No duplicated logic.
    let resp = entries_resp(3);
    let HookResponse::Entries { items, .. } = &resp else {
        unreachable!()
    };
    let direct = format_injection(items, MAX_INJECTION_BYTES);
    let via_core = crate::http::router::observe::response_injection_text(&resp);
    assert_eq!(via_core, direct);

    // Allowlist: non-injection variants always None.
    assert_eq!(
        crate::http::router::observe::response_injection_text(&HookResponse::Ack),
        None
    );
    assert_eq!(
        crate::http::router::observe::response_injection_text(&HookResponse::Pong {
            server_version: "x".to_string()
        }),
        None
    );
}

#[tokio::test]
async fn test_http_text_plain_and_uds_text_body_byte_identical() {
    // R-09 s3 — the parity backbone: for the SAME response, the HTTP text/plain
    // body bytes and the UDS Text body bytes are byte-identical by construction.
    let resp = entries_resp(2);

    // HTTP path: observe_response_to_http(resp, wants_text=true) -> 200 text/plain.
    let http_resp = crate::http::router::observe::observe_response_to_http(resp.clone(), true);
    assert_eq!(http_resp.status(), http::StatusCode::OK);
    let http_body = http_resp
        .into_body()
        .collect()
        .await
        .expect("collect http body")
        .to_bytes();

    // UDS path: negotiate_text_response(resp, wants_text=true) -> Text { body }.
    let uds_body = match negotiate_text_response(resp, true) {
        HookResponse::Text { body } => body,
        other => panic!("expected Text, got {other:?}"),
    };

    assert_eq!(
        http_body.as_ref(),
        uds_body.as_bytes(),
        "HTTP text/plain body and UDS Text body must be byte-identical"
    );
}
