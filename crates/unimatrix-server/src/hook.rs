//! Hook subcommand handler.
//!
//! Reads Claude Code hook JSON from stdin, connects to the running server
//! via UDS, and dispatches events. Uses synchronous std I/O only (no tokio
//! runtime) per ADR-002 for sub-50ms latency.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use unimatrix_engine::event_queue::EventQueue;
use unimatrix_engine::project::compute_project_hash;
use unimatrix_engine::transport::{LocalTransport, Transport};
use unimatrix_engine::wire::{HookInput, HookRequest, HookResponse, ImplantEvent, TransportError};

/// Default timeout for transport operations: 40ms.
/// Leaves 10ms margin in the 50ms total budget for process startup + hash computation.
const HOOK_TIMEOUT: Duration = Duration::from_millis(40);

/// Run the hook subcommand.
///
/// This is the entry point from `main()` for the `hook` subcommand.
/// No tokio runtime is initialized. Returns `Ok(())` for all expected
/// conditions -- exit code is always 0 per FR-03.7.
pub fn run(
    event: String,
    project_dir: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Read stdin
    let stdin_content = read_stdin();

    // Step 2: Parse hook input (defensive -- ADR-006)
    let hook_input = parse_hook_input(&stdin_content);

    // Step 3: Determine working directory and detect project root
    let cwd = resolve_cwd(&hook_input, project_dir.as_deref());
    let project_root = unimatrix_engine::project::detect_project_root(Some(&cwd))
        .unwrap_or(cwd);
    let project_hash = compute_project_hash(&project_root);

    // Step 4: Compute socket path
    let home = dirs::home_dir().ok_or("home directory not found")?;
    let socket_path = home
        .join(".unimatrix")
        .join(&project_hash)
        .join("unimatrix.sock");

    // Step 5: Build request from event + input
    let request = build_request(&event, &hook_input);

    // Step 6: Determine if fire-and-forget or synchronous
    let is_fire_and_forget = matches!(
        request,
        HookRequest::SessionRegister { .. }
            | HookRequest::SessionClose { .. }
            | HookRequest::RecordEvent { .. }
            | HookRequest::RecordEvents { .. }
    );

    // Step 7: Connect and send
    let mut transport = LocalTransport::new(socket_path, HOOK_TIMEOUT);

    match transport.connect() {
        Ok(()) => {
            // Connected -- try to replay any queued events first (best-effort)
            let queue = EventQueue::new(queue_dir(&home, &project_hash));
            let _ = queue.replay(&mut transport);

            // Reconnect since replay may have used the connection
            let _ = transport.disconnect();

            if is_fire_and_forget {
                if let Err(e) = transport.fire_and_forget(&request) {
                    eprintln!("unimatrix: fire-and-forget failed: {e}");
                }
            } else {
                match transport.request(&request, HOOK_TIMEOUT) {
                    Ok(response) => {
                        if let Err(e) = write_stdout(&response) {
                            eprintln!("unimatrix: stdout write failed: {e}");
                        }
                    }
                    Err(e) => {
                        eprintln!("unimatrix: request failed: {e}");
                    }
                }
            }
        }
        Err(TransportError::Unavailable(_)) => {
            // Server not running -- graceful degradation
            if is_fire_and_forget {
                let queue = EventQueue::new(queue_dir(&home, &project_hash));
                if let Err(e) = queue.enqueue(&request) {
                    eprintln!("unimatrix: event queue failed: {e}");
                } else {
                    eprintln!("unimatrix: server unavailable, event queued");
                }
            }
            // For sync queries with no server: produce no output, exit 0
        }
        Err(e) => {
            // Other transport errors -- log and exit 0
            eprintln!("unimatrix: transport error: {e}");
        }
    }

    // Always exit 0 (FR-03.7)
    Ok(())
}

/// Read all of stdin. Returns empty string if nothing is piped.
fn read_stdin() -> String {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    input
}

/// Parse hook input with maximum defensive serde (ADR-006).
fn parse_hook_input(raw: &str) -> HookInput {
    match serde_json::from_str::<HookInput>(raw) {
        Ok(input) => input,
        Err(e) => {
            if !raw.is_empty() {
                eprintln!("unimatrix: stdin parse error: {e}");
            }
            HookInput {
                hook_event_name: String::new(),
                session_id: None,
                cwd: None,
                transcript_path: None,
                extra: serde_json::Value::Null,
            }
        }
    }
}

/// Resolve working directory: --project-dir > stdin cwd > process cwd.
fn resolve_cwd(input: &HookInput, project_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = project_dir {
        return dir.to_path_buf();
    }

    if let Some(cwd) = &input.cwd {
        if !cwd.is_empty() {
            return PathBuf::from(cwd);
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Build a `HookRequest` from the event name and parsed input.
fn build_request(event: &str, input: &HookInput) -> HookRequest {
    // Resolve session_id with fallback to parent PID
    let session_id = input
        .session_id
        .clone()
        .unwrap_or_else(|| format!("ppid-{}", std::os::unix::process::parent_id()));

    let cwd = input.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    match event {
        "SessionStart" => HookRequest::SessionRegister {
            session_id,
            cwd,
            agent_role: None,
            feature: None,
        },

        "Stop" => HookRequest::SessionClose {
            session_id,
            outcome: None,
            duration_secs: 0,
        },

        "Ping" => HookRequest::Ping,

        _ => HookRequest::RecordEvent {
            event: ImplantEvent {
                event_type: event.to_string(),
                session_id,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                payload: input.extra.clone(),
            },
        },
    }
}

/// Write a response to stdout as JSON.
fn write_stdout(response: &HookResponse) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(response)?;
    println!("{json}");
    Ok(())
}

/// Compute the event queue directory path.
fn queue_dir(home: &Path, project_hash: &str) -> PathBuf {
    home.join(".unimatrix")
        .join(project_hash)
        .join("event-queue")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_session_start() {
        let input = HookInput {
            hook_event_name: "SessionStart".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: Some("/workspace".to_string()),
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister {
                session_id, cwd, ..
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(cwd, "/workspace");
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn build_request_stop() {
        let input = HookInput {
            hook_event_name: "Stop".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("Stop", &input);
        match req {
            HookRequest::SessionClose { session_id, .. } => {
                assert_eq!(session_id, "sess-1");
            }
            _ => panic!("expected SessionClose"),
        }
    }

    #[test]
    fn build_request_ping() {
        let input = HookInput {
            hook_event_name: String::new(),
            session_id: None,
            cwd: None,
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("Ping", &input);
        assert!(matches!(req, HookRequest::Ping));
    }

    #[test]
    fn build_request_unknown_event() {
        let input = HookInput {
            hook_event_name: "PreToolUse".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            extra: serde_json::json!({"tool": "Bash"}),
        };
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PreToolUse");
                assert_eq!(event.session_id, "sess-1");
                assert_eq!(event.payload["tool"], "Bash");
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn build_request_missing_session_id_falls_back_to_ppid() {
        let input = HookInput {
            hook_event_name: String::new(),
            session_id: None,
            cwd: None,
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister { session_id, .. } => {
                assert!(session_id.starts_with("ppid-"));
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn parse_hook_input_valid_json() {
        let json = r#"{"hook_event_name":"Stop","session_id":"s1"}"#;
        let input = parse_hook_input(json);
        assert_eq!(input.hook_event_name, "Stop");
        assert_eq!(input.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn parse_hook_input_empty_string() {
        let input = parse_hook_input("");
        assert_eq!(input.hook_event_name, "");
        assert!(input.session_id.is_none());
    }

    #[test]
    fn parse_hook_input_invalid_json() {
        let input = parse_hook_input("not json");
        assert_eq!(input.hook_event_name, "");
        assert!(input.session_id.is_none());
    }

    #[test]
    fn parse_hook_input_unknown_fields() {
        let json = r#"{"hook_event_name":"Test","unknown":"value"}"#;
        let input = parse_hook_input(json);
        assert_eq!(input.hook_event_name, "Test");
        assert_eq!(input.extra["unknown"], "value");
    }

    #[test]
    fn resolve_cwd_project_dir_takes_precedence() {
        let input = HookInput {
            hook_event_name: String::new(),
            session_id: None,
            cwd: Some("/stdin-cwd".to_string()),
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let result = resolve_cwd(&input, Some(Path::new("/override")));
        assert_eq!(result, PathBuf::from("/override"));
    }

    #[test]
    fn resolve_cwd_stdin_cwd_second() {
        let input = HookInput {
            hook_event_name: String::new(),
            session_id: None,
            cwd: Some("/stdin-cwd".to_string()),
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let result = resolve_cwd(&input, None);
        assert_eq!(result, PathBuf::from("/stdin-cwd"));
    }

    #[test]
    fn resolve_cwd_fallback_to_process_cwd() {
        let input = HookInput {
            hook_event_name: String::new(),
            session_id: None,
            cwd: None,
            transcript_path: None,
            extra: serde_json::Value::Null,
        };
        let result = resolve_cwd(&input, None);
        // Should be the actual process cwd, not "."
        assert!(result.is_absolute() || result == PathBuf::from("."));
    }

    #[test]
    fn queue_dir_path() {
        let home = PathBuf::from("/home/user");
        let hash = "abc123";
        let result = queue_dir(&home, hash);
        assert_eq!(result, PathBuf::from("/home/user/.unimatrix/abc123/event-queue"));
    }
}
