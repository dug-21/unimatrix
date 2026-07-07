//! End-to-end `hook SubagentStart` injection — drives the REAL `unimatrix` binary
//! over its process boundary against a fake UDS server (bugfix-918, PD2 / AC-SR02).
//!
//! The former in-module test `write_stdout_subagent_inject_valid_json_envelope`
//! (deleted) was tautological: it rebuilt the `hookSpecificOutput` envelope with its
//! own `serde_json::json!` and asserted on that literal, invoking ZERO production code.
//! Delete the writer body and it still passed — as PD2 / AC-SR02 evidence it was a
//! false capability claim.
//!
//! THIS file proves the two clauses that are only observable across the process
//! boundary — the writer emits on the process's REAL stdout, and the routing is
//! synthesized by the hook itself (no agent, no `context_*` tool call):
//!
//! - **Routing (the "without the agent asking" clause)** — with NO `prompt_snippet` in
//!   the stdin payload, `build_request` yields `RecordEvent`, forcing run() step-5b to
//!   derive a query from the transcript tail and rewrite the request to
//!   `ContextSearch{source:Some("SubagentStart"), role:Some("developer"), query:non-empty}`.
//!   The fake server asserts it received EXACTLY that one request via an mpsc channel.
//! - **Writer (AC-SR02)** — the real `write_stdout_subagent_inject_response` →
//!   `write_stdout_subagent_inject` path emits a `hookSpecificOutput` envelope with a
//!   NON-EMPTY `additionalContext` on the child's real stdout, captured here.
//!
//! Both asserts are load-bearing: the hook exits 0 on ANY internal failure (graceful
//! degradation, pattern #3324), so `status.success()` proves nothing. Asserting the
//! received request AND the captured stdout separates "hook did not route" (no request)
//! from "writer did not emit" (request received, stdout empty) — clean attribution.
//!
//! Flake decoupling: the child's transport applies a per-read/write socket timeout
//! (default 40ms). This test sets `UNIMATRIX_HOOK_TIMEOUT_MS` to a generous value so a
//! correctness/routing test is never also a 40ms latency test (bugfix-918 seam,
//! precedent `UNIMATRIX_PARITY_DIR`).

#![cfg(unix)]

use std::io::Write as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use unimatrix_engine::project::{compute_project_hash, detect_project_root};
use unimatrix_engine::wire::{
    EntryPayload, HookRequest, HookResponse, MAX_PAYLOAD_SIZE, deserialize_request, read_frame,
    serialize_response, write_frame,
};

/// Distinctive marker text carried in the fake server's response entry — asserted to
/// appear verbatim in the child's captured `additionalContext`.
const ENTRY_TITLE: &str = "bugfix-918 retry backoff pattern";
const ENTRY_CONTENT: &str = "Use exponential backoff in the storage flush retry loop.";

/// A git-rooted project dir (so `detect_project_root` accepts it without walking up).
fn make_project(tmp: &Path) -> PathBuf {
    let root = tmp.join("proj");
    std::fs::create_dir_all(root.join(".git")).expect("create .git");
    root
}

/// The socket path the child will resolve for `--project-dir <proj>` under `$HOME=home`,
/// mirroring run() exactly: detect_project_root(Some(proj)) → compute_project_hash.
fn resolved_socket(home: &Path, project_root: &Path) -> PathBuf {
    let root = detect_project_root(Some(project_root)).expect("detect project root");
    let hash = compute_project_hash(&root);
    home.join(".unimatrix").join(hash).join("unimatrix.sock")
}

/// Small JSONL transcript whose tail yields a non-empty derived query in step-5b.
/// Shape mirrors `parity_corpus_transcripts.rs` (Claude Code UX `message.content`).
fn write_transcript(path: &Path) {
    let user = serde_json::json!({
        "type": "user",
        "message": { "content": [
            { "type": "text",
              "text": "Implement the retry backoff for the storage engine flush loop" }
        ] }
    });
    let assistant = serde_json::json!({
        "type": "assistant",
        "message": { "content": [
            { "type": "text",
              "text": "I will add exponential backoff to the flush retry path in storage_engine.rs" }
        ] }
    });
    let jsonl = format!("{user}\n{assistant}\n");
    std::fs::write(path, jsonl).expect("write transcript fixture");
}

/// Pre-serialized fake response: one non-empty `Entries` result carrying the markers.
fn response_frame() -> Vec<u8> {
    let response = HookResponse::Entries {
        items: vec![EntryPayload {
            id: 918,
            title: ENTRY_TITLE.to_string(),
            content: ENTRY_CONTENT.to_string(),
            confidence: 0.9,
            similarity: 0.88,
            category: "pattern".to_string(),
        }],
        total_tokens: 42,
    };
    serialize_response(&response).expect("serialize fake response")
}

/// Fake-server accept loop. The child makes TWO connections to this socket. The first
/// is run() step-7 connect + replay (queue dir absent → Ok(0), writes nothing) +
/// disconnect, so the server sees an EMPTY connection (EOF on the header read). The
/// second is request() auto-reconnecting and sending the real ContextSearch frame.
/// Tolerate the empty first connection, answer the real request, send it back over
/// `tx`, then return. Bounded to a handful of connections so a routing regression that
/// never sends a framed request cannot hang the test.
fn serve_one_request(listener: UnixListener, tx: mpsc::Sender<HookRequest>) {
    let reply = response_frame();
    for _ in 0..8 {
        let mut stream: UnixStream = match listener.accept() {
            Ok((s, _)) => s,
            Err(_) => return,
        };
        match read_frame(&mut stream, MAX_PAYLOAD_SIZE) {
            Ok(bytes) => {
                let req = deserialize_request(&bytes).expect("deserialize request frame");
                // Best-effort reply; the child may have disconnected on a fire-and-forget
                // path (a routing regression), which is fine — the mpsc assert catches it.
                let _ = write_frame(&mut stream, &reply);
                let _ = stream.flush();
                let _ = tx.send(req);
                return;
            }
            // Empty/EOF connection (the replay connect+disconnect) — keep accepting.
            Err(_) => continue,
        }
    }
}

#[test]
fn test_e2e_subagent_start_routes_and_writes_injection_envelope() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path();
    let project = make_project(home);

    // Bind the fake server socket BEFORE spawning the child (connect() fast-fails on a
    // missing socket file).
    let socket_path = resolved_socket(home, &project);
    std::fs::create_dir_all(socket_path.parent().unwrap()).expect("create socket dir");
    let listener = UnixListener::bind(&socket_path).expect("bind fake server socket");

    let (tx, rx) = mpsc::channel::<HookRequest>();
    let server = thread::spawn(move || serve_one_request(listener, tx));

    // Transcript fixture — its tail derives the step-5b query.
    let transcript = home.join("transcript.jsonl");
    write_transcript(&transcript);

    // stdin: SubagentStart payload with NO prompt_snippet (forces RecordEvent → step-5b)
    // and agent role "developer".
    let stdin_json = serde_json::json!({
        "session_id": "sess-918",
        "agent_type": "developer",
        "transcript_path": transcript.to_string_lossy(),
    })
    .to_string();

    let exe = env!("CARGO_BIN_EXE_unimatrix");
    let mut child = Command::new(exe)
        .arg("--project-dir")
        .arg(&project)
        .arg("hook")
        .arg("SubagentStart")
        .env("HOME", home)
        // Generous timeout so this correctness test never doubles as a 40ms latency test.
        .env("UNIMATRIX_HOOK_TIMEOUT_MS", "5000")
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn unimatrix hook SubagentStart");

    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin_json.as_bytes())
        .expect("write child stdin");

    let out = child.wait_with_output().expect("wait for child");
    server.join().expect("join fake server thread");

    // --- Assert (routing): the fake server received EXACTLY one request, and it is the
    // hook-synthesized ContextSearch{source:"SubagentStart"} — no agent, no context_* call.
    let received = rx
        .try_recv()
        .expect("fake server must have received exactly one request (hook did not route)");
    assert!(
        rx.try_recv().is_err(),
        "fake server must receive EXACTLY one request"
    );
    match received {
        HookRequest::ContextSearch {
            source,
            role,
            query,
            ..
        } => {
            assert_eq!(
                source.as_deref(),
                Some("SubagentStart"),
                "step-5b must route to ContextSearch with source=SubagentStart"
            );
            assert_eq!(
                role.as_deref(),
                Some("developer"),
                "role must carry agent_type from the payload"
            );
            assert!(
                !query.trim().is_empty(),
                "query must be derived (non-empty) from the transcript tail"
            );
        }
        other => panic!("expected ContextSearch{{source:SubagentStart}}, got {other:?}"),
    }

    // --- Assert (writer / AC-SR02): captured child stdout parses to a hookSpecificOutput
    // envelope with a NON-EMPTY additionalContext carrying the fake entry (proves the real
    // writer emitted through the process boundary). Do NOT rely on exit code — the hook
    // exits 0 on internal failure by design (pattern #3324).
    let stdout = String::from_utf8(out.stdout).expect("stdout utf8");
    let envelope: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("stdout must be a JSON envelope; parse error {e}; stdout={stdout:?}")
    });
    assert_eq!(
        envelope["hookSpecificOutput"]["hookEventName"], "SubagentStart",
        "envelope hookEventName must be SubagentStart"
    );
    let additional = envelope["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext must be a string");
    assert!(
        !additional.is_empty(),
        "additionalContext must be non-empty (writer emitted)"
    );
    assert!(
        additional.contains(ENTRY_TITLE) && additional.contains(ENTRY_CONTENT),
        "additionalContext must carry the fake entry's title and content; got: {additional:?}"
    );
}
