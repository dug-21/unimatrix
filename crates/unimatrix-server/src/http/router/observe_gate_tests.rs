//! vnc-050 C6 observe gate — real HTTP boundary (harness in parent).

use super::*;

fn frames(sid: &str) -> Vec<String> {
    let ev =
        format!(r#""event_type":"tool_use","session_id":"{sid}","timestamp":1,"payload":{{}}"#);
    vec![
        format!(
            r#"{{"type":"SessionRegister","session_id":"{sid}","cwd":"/w","agent_role":null,"feature":null}}"#
        ),
        format!(r#"{{"type":"RecordEvent",{ev}}}"#),
        format!(r#"{{"type":"RecordEvents","events":[{{{ev}}}]}}"#),
        format!(
            r#"{{"type":"SessionClose","session_id":"{sid}","outcome":null,"duration_secs":1}}"#
        ),
        format!(
            r#"{{"type":"ContextSearch","query":"q","session_id":"{sid}","role":null,"task":null,"feature":null,"k":null,"max_tokens":null}}"#
        ),
    ]
}

const TABLES: [&str; 9] = [
    "observations",
    "sessions",
    "signal_queue",
    "cycle_events",
    "cycle_tags",
    "co_access",
    "injection_log",
    "query_log",
    "entries",
];

async fn table_counts(store: &Store) -> Vec<i64> {
    let mut v = Vec::new();
    for t in TABLES {
        v.push(count(store, &format!("SELECT COUNT(*) FROM {t}")).await);
    }
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_token_observe_403_pre_dispatch_no_table_change() {
    let h = boot(Variant::Normal).await;
    let before = table_counts(&h.alpha).await;
    for f in frames("sg-read-sess") {
        let (st, b) = send(h.addr, "POST", "/v1/alpha/observe", &READ_A, &f).await;
        assert_eq!(st, 403, "{f}: {b}");
        assert_eq!(b.trim(), r#"{"error":"token lacks SessionWrite"}"#);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        table_counts(&h.alpha).await,
        before,
        "read token wrote session state"
    );
    // The 403 is still audited in the bound slug.
    let rows = access_rows(&h.alpha, L_READ, 5).await;
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r.1.contains("\"route\":\"observe\"")));
}

fn alpha_dispatches() -> usize {
    let spy = DISPATCH_SPY.lock().unwrap_or_else(|p| p.into_inner());
    spy.iter().filter(|s| s.as_str() == "alpha").count()
}

/// Read 403 never reaches `dispatch_request`; rw companion does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_token_observe_never_reaches_dispatcher() {
    let h = boot(Variant::Normal).await;
    let before = alpha_dispatches();
    let (st, _) = send(h.addr, "POST", "/v1/alpha/observe", &READ_A, PING).await;
    assert_eq!(st, 403);
    let after_read = alpha_dispatches();
    let (st, _) = send(h.addr, "POST", "/v1/alpha/observe", &RW_A, PING).await;
    assert_eq!(st, 200);
    assert!(alpha_dispatches() > after_read && after_read >= before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rw_and_legacy_observe_lands() {
    let h = boot(Variant::Normal).await;
    for (tok, sid) in [(&RW_A, "sg-rw-sess"), (&LEGACY, "sg-legacy-sess")] {
        let f = &frames(sid)[1];
        let (st, b) = send(h.addr, "POST", "/v1/alpha/observe", tok, f).await;
        assert!((200..300).contains(&st), "{st} {b}");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let n = count(
                &h.alpha,
                &format!("SELECT COUNT(*) FROM observations WHERE session_id LIKE '%{sid}%'"),
            )
            .await;
            if n >= 1 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{sid}: no observation row"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_token_wrong_slug_observe_slug_403_wins() {
    let h = boot(Variant::Normal).await;
    let (st, b) = send(h.addr, "POST", "/v1/beta/observe", &READ_A, PING).await;
    assert!(
        denied(st, &b),
        "slug guard precedes SessionWrite gate: {st} {b}"
    );
}
