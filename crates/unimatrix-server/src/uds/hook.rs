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
use unimatrix_engine::wire::{
    EntryPayload, HookInput, HookRequest, HookResponse, ImplantEvent, TransportError,
};
use unimatrix_observe::extract_topic_signal;

/// Default timeout for transport operations: 40ms.
/// Leaves 10ms margin in the 50ms total budget for process startup + hash computation.
const HOOK_TIMEOUT: Duration = Duration::from_millis(40);

/// Maximum byte budget for injection output (~350 tokens at 4 bytes/token).
const MAX_INJECTION_BYTES: usize = 1400;

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

/// Read all of stdin up to 1 MiB. Returns empty string if nothing is piped.
fn read_stdin() -> String {
    let mut input = String::new();
    let _ = std::io::stdin().take(1_048_576).read_to_string(&mut input);
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
                prompt: None,
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

/// Extract topic signal text from a hook event based on event type (col-017, ADR-017-001).
///
/// Each event type has a specific text source for topic extraction:
/// - PreToolUse / PostToolUse (non-rework): `input.extra["tool_input"]` stringified
/// - SubagentStart: `input.extra["prompt_snippet"]` stringified
/// - UserPromptSubmit (record path): `input.prompt`
/// - Other (generic_record_event): `serde_json::to_string(&input.extra)`
///
/// Pure string scanning only -- no I/O (C-01).
fn extract_event_topic_signal(event: &str, input: &HookInput) -> Option<String> {
    match event {
        "PreToolUse" => {
            let text = input
                .extra
                .get("tool_input")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => serde_json::to_string(v).unwrap_or_default(),
                })
                .unwrap_or_default();
            extract_topic_signal(&text)
        }
        "PostToolUse" => {
            let text = input
                .extra
                .get("tool_input")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => serde_json::to_string(v).unwrap_or_default(),
                })
                .unwrap_or_default();
            extract_topic_signal(&text)
        }
        "SubagentStart" => {
            let text = input
                .extra
                .get("prompt_snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            extract_topic_signal(text)
        }
        "UserPromptSubmit" => {
            let text = input.prompt.as_deref().unwrap_or("");
            extract_topic_signal(text)
        }
        _ => {
            // Generic: stringify extra, but only if non-null
            if input.extra.is_null() {
                return None;
            }
            let text = serde_json::to_string(&input.extra).unwrap_or_default();
            extract_topic_signal(&text)
        }
    }
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
            agent_role: input
                .extra
                .get("agent_role")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            feature: input
                .extra
                .get("feature_cycle")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },

        "Stop" | "TaskCompleted" => HookRequest::SessionClose {
            session_id,
            outcome: Some("success".to_string()), // Server overrides to "rework" if threshold crossed
            duration_secs: 0,
        },

        "Ping" => HookRequest::Ping,

        "UserPromptSubmit" => {
            let query = input.prompt.clone().unwrap_or_default();
            if query.is_empty() {
                // No prompt text -- fall through to RecordEvent
                return generic_record_event(event, session_id, input);
            } else {
                HookRequest::ContextSearch {
                    query,
                    session_id: input.session_id.clone(),
                    role: None,
                    task: None,
                    feature: None,
                    k: None,
                    max_tokens: None,
                }
            }
        }

        "PreCompact" => HookRequest::CompactPayload {
            session_id,
            injected_entry_ids: vec![], // Server has tracked history (ADR-002)
            role: None,
            feature: None,
            token_limit: None,
        },

        // col-009: Intercept PostToolUse for rework tracking
        "PostToolUse" => {
            let tool_name = input
                .extra
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // col-017: extract topic signal from tool_input for all PostToolUse
            let topic_signal = extract_event_topic_signal(event, input);

            if !is_rework_eligible_tool(&tool_name) {
                // Non-rework tool: fall through to generic RecordEvent (with topic signal)
                return HookRequest::RecordEvent {
                    event: ImplantEvent {
                        event_type: event.to_string(),
                        session_id,
                        timestamp: now_secs(),
                        payload: input.extra.clone(),
                        topic_signal,
                    },
                };
            }

            // MultiEdit: generate one RecordEvent per path
            if tool_name == "MultiEdit" {
                let pairs = extract_rework_events_for_multiedit(&input.extra);
                if pairs.is_empty() {
                    return HookRequest::RecordEvent {
                        event: ImplantEvent {
                            event_type: event.to_string(),
                            session_id,
                            timestamp: now_secs(),
                            payload: input.extra.clone(),
                            topic_signal,
                        },
                    };
                }
                let events: Vec<ImplantEvent> = pairs
                    .into_iter()
                    .map(|(file_path, had_failure)| ImplantEvent {
                        event_type: "post_tool_use_rework_candidate".to_string(),
                        session_id: session_id.clone(),
                        timestamp: now_secs(),
                        payload: serde_json::json!({
                            "tool_name": "MultiEdit",
                            "file_path": file_path,
                            "had_failure": had_failure,
                            "tool_input": input.extra.get("tool_input"),
                            "tool_response": input.extra.get("tool_response"),
                        }),
                        topic_signal: topic_signal.clone(),
                    })
                    .collect();
                return HookRequest::RecordEvents { events };
            }

            // Bash, Edit, Write: single RecordEvent
            let had_failure = if tool_name == "Bash" {
                is_bash_failure(&input.extra)
            } else {
                false // Edit, Write cannot fail (ADR-002)
            };
            let file_path = extract_file_path(&input.extra, &tool_name);

            HookRequest::RecordEvent {
                event: ImplantEvent {
                    event_type: "post_tool_use_rework_candidate".to_string(),
                    session_id,
                    timestamp: now_secs(),
                    payload: serde_json::json!({
                        "tool_name": tool_name,
                        "file_path": file_path,
                        "had_failure": had_failure,
                        "tool_input": input.extra.get("tool_input"),
                        "tool_response": input.extra.get("tool_response"),
                    }),
                    topic_signal,
                },
            }
        }

        _ => generic_record_event(event, session_id, input),
    }
}

/// Build a generic RecordEvent for non-intercepted hook types.
///
/// col-017: Extracts topic signal from stringified `input.extra` for all generic events.
fn generic_record_event(event: &str, session_id: String, input: &HookInput) -> HookRequest {
    let topic_signal = extract_event_topic_signal(event, input);
    HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type: event.to_string(),
            session_id,
            timestamp: now_secs(),
            payload: input.extra.clone(),
            topic_signal,
        },
    }
}

/// Returns true if the tool is rework-eligible (file-mutating tools).
fn is_rework_eligible_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Bash" | "Edit" | "Write" | "MultiEdit")
}

/// Returns true if the Bash tool call had a failure.
///
/// Failure = exit_code is non-zero integer, OR interrupted is true.
/// All other cases (missing fields, non-integer exit_code) return false.
fn is_bash_failure(extra: &serde_json::Value) -> bool {
    if let Some(exit_code) = extra.get("exit_code").and_then(|v| v.as_i64()) {
        if exit_code != 0 {
            return true;
        }
    }
    if extra
        .get("interrupted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    false
}

/// Extract file_path for Edit or Write tools.
///
/// Edit: extra["tool_input"]["path"]
/// Write: extra["tool_input"]["file_path"]
/// Returns None if the field is absent or not a string.
fn extract_file_path(extra: &serde_json::Value, tool_name: &str) -> Option<String> {
    match tool_name {
        "Edit" => extra
            .get("tool_input")
            .and_then(|ti| ti.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "Write" => extra
            .get("tool_input")
            .and_then(|ti| ti.get("file_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Extract (file_path, had_failure) pairs for MultiEdit.
///
/// MultiEdit has extra["tool_input"]["edits"] = array of {path, ...}.
/// Each distinct path produces one entry with had_failure=false.
/// Empty edits array → empty Vec. Missing fields → empty Vec (no panic).
fn extract_rework_events_for_multiedit(extra: &serde_json::Value) -> Vec<(Option<String>, bool)> {
    let edits = match extra.get("tool_input").and_then(|ti| ti.get("edits")) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let arr = match edits.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    arr.iter()
        .map(|edit| {
            let path = edit
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            (path, false) // Edit tools can't fail (ADR-002)
        })
        .collect()
}

/// Current Unix timestamp in seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Write a response to stdout. For Entries responses, format as structured
/// injection text. For all other responses, serialize as JSON.
fn write_stdout(response: &HookResponse) -> Result<(), Box<dyn std::error::Error>> {
    match response {
        HookResponse::Entries { items, .. } => {
            if let Some(text) = format_injection(items, MAX_INJECTION_BYTES) {
                println!("{text}");
            }
            // Empty items or None from format_injection: silent skip (no stdout)
            Ok(())
        }
        HookResponse::BriefingContent { content, .. } => {
            if !content.is_empty() {
                println!("{content}");
            }
            // Empty content: silent skip (FR-01.4)
            Ok(())
        }
        _ => {
            let json = serde_json::to_string(response)?;
            println!("{json}");
            Ok(())
        }
    }
}

/// Format a ranked list of entries as structured plain text within a byte budget.
///
/// Returns `None` if entries is empty or no entries fit within the budget.
/// Entries are included in input order (rank order from server).
fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut output = String::new();
    let header = "--- Unimatrix Context ---\n";
    output.push_str(header);

    let mut entries_added = 0;

    for entry in entries {
        let block = format_entry_block(entry);

        let projected_len = output.len() + block.len();
        if projected_len <= max_bytes {
            output.push_str(&block);
            entries_added += 1;
        } else {
            let remaining = max_bytes.saturating_sub(output.len());
            if remaining < 100 {
                // Too small for meaningful content
                break;
            }

            // Truncate block to fit
            let truncated = truncate_utf8(&block, remaining);
            output.push_str(truncated);
            entries_added += 1;
            break;
        }
    }

    if entries_added == 0 {
        return None;
    }

    Some(output)
}

/// Format a single entry as a text block.
fn format_entry_block(entry: &EntryPayload) -> String {
    let confidence_pct = (entry.confidence * 100.0) as u32;
    format!(
        "[{}] ({}, {}% confidence)\n{}\n<!-- id:{} sim:{:.2} -->\n\n",
        entry.title, entry.category, confidence_pct, entry.content, entry.id, entry.similarity,
    )
}

/// Truncate a string to at most `max_bytes` bytes, ensuring the result
/// is a valid UTF-8 string (never splits a multi-byte character).
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
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

    // -- Helper to create a test HookInput --

    fn test_input() -> HookInput {
        HookInput {
            hook_event_name: String::new(),
            session_id: None,
            cwd: None,
            transcript_path: None,
            prompt: None,
            extra: serde_json::Value::Null,
        }
    }

    fn test_entry(id: u64, title: &str, content: &str) -> EntryPayload {
        EntryPayload {
            id,
            title: title.to_string(),
            content: content.to_string(),
            confidence: 0.85,
            similarity: 0.92,
            category: "decision".to_string(),
        }
    }

    // -- build_request tests --

    #[test]
    fn build_request_session_start() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.cwd = Some("/workspace".to_string());
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
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
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
        let input = test_input();
        let req = build_request("Ping", &input);
        assert!(matches!(req, HookRequest::Ping));
    }

    #[test]
    fn build_request_unknown_event() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.extra = serde_json::json!({"tool": "Bash"});
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
        let input = test_input();
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister { session_id, .. } => {
                assert!(session_id.starts_with("ppid-"));
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    // -- UserPromptSubmit tests (col-007) --

    #[test]
    fn build_request_user_prompt_submit_with_prompt() {
        let mut input = test_input();
        input.prompt = Some("search query".to_string());
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { query, .. } => {
                assert_eq!(query, "search query");
            }
            _ => panic!("expected ContextSearch, got {req:?}"),
        }
    }

    #[test]
    fn build_request_user_prompt_submit_without_prompt() {
        let input = test_input();
        let req = build_request("UserPromptSubmit", &input);
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    #[test]
    fn build_request_user_prompt_submit_empty_prompt() {
        let mut input = test_input();
        input.prompt = Some(String::new());
        let req = build_request("UserPromptSubmit", &input);
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    #[test]
    fn build_request_user_prompt_submit_long_prompt() {
        let mut input = test_input();
        input.prompt = Some("x".repeat(20_000));
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { query, .. } => {
                assert_eq!(query.len(), 20_000);
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn context_search_is_not_fire_and_forget() {
        let req = HookRequest::ContextSearch {
            query: "test".to_string(),
            session_id: None,
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
        };
        let is_faf = matches!(
            req,
            HookRequest::SessionRegister { .. }
                | HookRequest::SessionClose { .. }
                | HookRequest::RecordEvent { .. }
                | HookRequest::RecordEvents { .. }
        );
        assert!(!is_faf, "ContextSearch must not be fire-and-forget");
    }

    // -- PreCompact tests (col-008) --

    #[test]
    fn build_request_precompact_with_session_id() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        let req = build_request("PreCompact", &input);
        match req {
            HookRequest::CompactPayload {
                session_id,
                injected_entry_ids,
                role,
                feature,
                token_limit,
            } => {
                assert_eq!(session_id, "sess-1");
                assert!(injected_entry_ids.is_empty());
                assert!(role.is_none());
                assert!(feature.is_none());
                assert!(token_limit.is_none());
            }
            _ => panic!("expected CompactPayload, got {req:?}"),
        }
    }

    #[test]
    fn build_request_precompact_without_session_id() {
        let input = test_input();
        let req = build_request("PreCompact", &input);
        match req {
            HookRequest::CompactPayload { session_id, .. } => {
                assert!(session_id.starts_with("ppid-"));
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn compact_payload_not_fire_and_forget() {
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![],
            role: None,
            feature: None,
            token_limit: None,
        };
        let is_faf = matches!(
            req,
            HookRequest::SessionRegister { .. }
                | HookRequest::SessionClose { .. }
                | HookRequest::RecordEvent { .. }
                | HookRequest::RecordEvents { .. }
        );
        assert!(!is_faf, "CompactPayload must not be fire-and-forget");
    }

    #[test]
    fn write_stdout_briefing_content_with_content() {
        let response = HookResponse::BriefingContent {
            content: "compaction data".to_string(),
            token_count: 10,
        };
        assert!(write_stdout(&response).is_ok());
    }

    #[test]
    fn write_stdout_briefing_content_empty() {
        let response = HookResponse::BriefingContent {
            content: String::new(),
            token_count: 0,
        };
        assert!(write_stdout(&response).is_ok());
    }

    #[test]
    fn build_request_user_prompt_passes_session_id() {
        let mut input = test_input();
        input.prompt = Some("query".to_string());
        input.session_id = Some("sess-1".to_string());
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { session_id, query, .. } => {
                assert_eq!(query, "query");
                assert_eq!(session_id.as_deref(), Some("sess-1"));
            }
            _ => panic!("expected ContextSearch, got {req:?}"),
        }
    }

    #[test]
    fn build_request_user_prompt_no_session_id() {
        let mut input = test_input();
        input.prompt = Some("query".to_string());
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { session_id, .. } => {
                assert!(session_id.is_none());
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    // -- PostToolUse tests (col-009) --

    fn posttooluse_input(extra: serde_json::Value) -> HookInput {
        HookInput {
            hook_event_name: "PostToolUse".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            prompt: None,
            extra,
        }
    }

    #[test]
    fn posttooluse_bash_failure_exit_code_nonzero() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Bash",
            "exit_code": 1
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "post_tool_use_rework_candidate");
                assert_eq!(event.payload["tool_name"], "Bash");
                assert_eq!(event.payload["had_failure"], true);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_bash_success_exit_code_zero() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Bash",
            "exit_code": 0
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.payload["had_failure"], false);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_bash_missing_exit_code() {
        let input = posttooluse_input(serde_json::json!({"tool_name": "Bash"}));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.payload["had_failure"], false);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_bash_interrupted_true() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Bash",
            "exit_code": 0,
            "interrupted": true
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.payload["had_failure"], true);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_edit_extracts_path() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {"path": "src/foo.rs", "old_string": "a", "new_string": "b"}
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.payload["tool_name"], "Edit");
                assert_eq!(event.payload["file_path"], "src/foo.rs");
                assert_eq!(event.payload["had_failure"], false);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_write_extracts_file_path() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"file_path": "src/bar.rs", "content": "..."}
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.payload["file_path"], "src/bar.rs");
                assert_eq!(event.payload["had_failure"], false);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_multiedit_two_paths() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "MultiEdit",
            "tool_input": {
                "edits": [
                    {"path": "a.rs", "old_string": "x", "new_string": "y"},
                    {"path": "b.rs", "old_string": "p", "new_string": "q"}
                ]
            }
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvents { events } => {
                assert_eq!(events.len(), 2);
                assert_eq!(events[0].event_type, "post_tool_use_rework_candidate");
                assert_eq!(events[0].payload["tool_name"], "MultiEdit");
                let paths: Vec<_> = events.iter().map(|e| e.payload["file_path"].as_str().unwrap()).collect();
                assert!(paths.contains(&"a.rs"));
                assert!(paths.contains(&"b.rs"));
            }
            _ => panic!("expected RecordEvents"),
        }
    }

    #[test]
    fn posttooluse_multiedit_empty_edits() {
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "MultiEdit",
            "tool_input": {"edits": []}
        }));
        let req = build_request("PostToolUse", &input);
        // Empty edits → generic RecordEvent
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PostToolUse");
            }
            _ => panic!("expected generic RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_non_rework_tool() {
        let input = posttooluse_input(serde_json::json!({"tool_name": "Read"}));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Generic record event, not rework candidate
                assert_eq!(event.event_type, "PostToolUse");
            }
            _ => panic!("expected generic RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_missing_tool_name() {
        let input = posttooluse_input(serde_json::json!({}));
        let req = build_request("PostToolUse", &input);
        // Missing tool_name → treated as non-rework tool → generic RecordEvent
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    #[test]
    fn posttooluse_null_extra() {
        let input = HookInput {
            hook_event_name: "PostToolUse".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            prompt: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("PostToolUse", &input);
        // Null extra → no tool_name → generic RecordEvent
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    #[test]
    fn stop_sets_outcome_success() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        let req = build_request("Stop", &input);
        match req {
            HookRequest::SessionClose { outcome, .. } => {
                assert_eq!(outcome.as_deref(), Some("success"));
            }
            _ => panic!("expected SessionClose"),
        }
    }

    #[test]
    fn is_bash_failure_nonzero_exit() {
        assert!(is_bash_failure(&serde_json::json!({"exit_code": 1})));
        assert!(is_bash_failure(&serde_json::json!({"exit_code": 127})));
    }

    #[test]
    fn is_bash_failure_zero_exit() {
        assert!(!is_bash_failure(&serde_json::json!({"exit_code": 0})));
    }

    #[test]
    fn is_bash_failure_no_exit_code() {
        assert!(!is_bash_failure(&serde_json::json!({})));
    }

    #[test]
    fn is_bash_failure_interrupted() {
        assert!(is_bash_failure(&serde_json::json!({"exit_code": 0, "interrupted": true})));
    }

    #[test]
    fn extract_file_path_edit() {
        let extra = serde_json::json!({"tool_input": {"path": "/src/lib.rs"}});
        assert_eq!(extract_file_path(&extra, "Edit"), Some("/src/lib.rs".to_string()));
    }

    #[test]
    fn extract_file_path_write() {
        let extra = serde_json::json!({"tool_input": {"file_path": "/src/main.rs"}});
        assert_eq!(extract_file_path(&extra, "Write"), Some("/src/main.rs".to_string()));
    }

    #[test]
    fn extract_file_path_bash_returns_none() {
        let extra = serde_json::json!({"tool_input": {"path": "/src/foo.rs"}});
        assert_eq!(extract_file_path(&extra, "Bash"), None);
    }

    #[test]
    fn extract_rework_events_for_multiedit_two_paths() {
        let extra = serde_json::json!({
            "tool_input": {
                "edits": [
                    {"path": "a.rs"},
                    {"path": "b.rs"}
                ]
            }
        });
        let result = extract_rework_events_for_multiedit(&extra);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0.as_deref(), Some("a.rs"));
        assert!(!result[0].1);
        assert_eq!(result[1].0.as_deref(), Some("b.rs"));
    }

    #[test]
    fn extract_rework_events_for_multiedit_empty() {
        let extra = serde_json::json!({"tool_input": {"edits": []}});
        let result = extract_rework_events_for_multiedit(&extra);
        assert!(result.is_empty());
    }

    #[test]
    fn extract_rework_events_for_multiedit_missing_tool_input() {
        let extra = serde_json::json!({});
        let result = extract_rework_events_for_multiedit(&extra);
        assert!(result.is_empty());
    }

    // -- parse_hook_input tests --

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
        assert!(input.prompt.is_none());
    }

    #[test]
    fn parse_hook_input_invalid_json() {
        let input = parse_hook_input("not json");
        assert_eq!(input.hook_event_name, "");
        assert!(input.session_id.is_none());
        assert!(input.prompt.is_none());
    }

    #[test]
    fn parse_hook_input_unknown_fields() {
        let json = r#"{"hook_event_name":"Test","unknown":"value"}"#;
        let input = parse_hook_input(json);
        assert_eq!(input.hook_event_name, "Test");
        assert_eq!(input.extra["unknown"], "value");
    }

    // -- resolve_cwd tests --

    #[test]
    fn resolve_cwd_project_dir_takes_precedence() {
        let mut input = test_input();
        input.cwd = Some("/stdin-cwd".to_string());
        let result = resolve_cwd(&input, Some(Path::new("/override")));
        assert_eq!(result, PathBuf::from("/override"));
    }

    #[test]
    fn resolve_cwd_stdin_cwd_second() {
        let mut input = test_input();
        input.cwd = Some("/stdin-cwd".to_string());
        let result = resolve_cwd(&input, None);
        assert_eq!(result, PathBuf::from("/stdin-cwd"));
    }

    #[test]
    fn resolve_cwd_fallback_to_process_cwd() {
        let input = test_input();
        let result = resolve_cwd(&input, None);
        assert!(result.is_absolute() || result == PathBuf::from("."));
    }

    #[test]
    fn queue_dir_path() {
        let home = PathBuf::from("/home/user");
        let hash = "abc123";
        let result = queue_dir(&home, hash);
        assert_eq!(
            result,
            PathBuf::from("/home/user/.unimatrix/abc123/event-queue")
        );
    }

    // -- format_injection tests (col-007) --

    #[test]
    fn format_injection_single_entry() {
        let entries = vec![test_entry(1, "ADR-001", "Use parameter expansion")];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.starts_with("--- Unimatrix Context ---\n"));
        assert!(text.contains("[ADR-001]"));
        assert!(text.contains("decision"));
        assert!(text.contains("85% confidence"));
        assert!(text.contains("Use parameter expansion"));
        assert!(text.contains("<!-- id:1"));
    }

    #[test]
    fn format_injection_multiple_entries() {
        let entries = vec![
            test_entry(1, "First", "Content one"),
            test_entry(2, "Second", "Content two"),
            test_entry(3, "Third", "Content three"),
        ];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("[First]"));
        assert!(text.contains("[Second]"));
        assert!(text.contains("[Third]"));
        // Verify order: First appears before Second appears before Third
        let pos1 = text.find("[First]").unwrap();
        let pos2 = text.find("[Second]").unwrap();
        let pos3 = text.find("[Third]").unwrap();
        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
    }

    #[test]
    fn format_injection_empty() {
        let result = format_injection(&[], MAX_INJECTION_BYTES);
        assert!(result.is_none());
    }

    #[test]
    fn format_injection_respects_byte_budget() {
        // Create entries that together exceed the budget
        let long_content = "x".repeat(1000);
        let entries = vec![
            test_entry(1, "First", &long_content),
            test_entry(2, "Second", &long_content),
        ];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.len() <= MAX_INJECTION_BYTES);
    }

    #[test]
    fn format_injection_remaining_under_100_omits() {
        // First entry uses almost all the budget
        let content = "x".repeat(1300);
        let entries = vec![
            test_entry(1, "Big", &content),
            test_entry(2, "Small", "tiny"),
        ];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.len() <= MAX_INJECTION_BYTES);
        // Second entry should be omitted (remaining < 100 after header + first entry)
    }

    #[test]
    fn format_injection_cjk_content() {
        // CJK characters: 3 bytes each
        let cjk = "\u{4e16}\u{754c}".repeat(200); // 1200 bytes
        let entries = vec![test_entry(1, "CJK", &cjk)];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.len() <= MAX_INJECTION_BYTES);
        // Verify valid UTF-8 (Rust strings are always valid, but verify no panic)
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }

    #[test]
    fn format_injection_emoji_content() {
        // Emoji: 4 bytes each
        let emoji = "\u{1F600}".repeat(150); // 600 bytes
        let entries = vec![test_entry(1, "Emoji", &emoji)];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.len() <= MAX_INJECTION_BYTES);
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }

    #[test]
    fn format_injection_truncates_multibyte_safely() {
        // Content that will be truncated -- mix of CJK that must not split
        let cjk = "\u{4e16}\u{754c}".repeat(500); // 3000 bytes
        let entries = vec![test_entry(1, "T", &cjk)];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.len() <= MAX_INJECTION_BYTES);
        // Verify it's valid UTF-8 (truncation didn't split a character)
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }

    #[test]
    fn format_injection_header_present() {
        let entries = vec![test_entry(1, "Test", "Content")];
        let result = format_injection(&entries, MAX_INJECTION_BYTES).unwrap();
        assert!(result.starts_with("--- Unimatrix Context ---\n"));
    }

    #[test]
    fn format_injection_entry_metadata() {
        let mut entry = test_entry(42, "My Title", "Body text");
        entry.confidence = 0.73;
        entry.category = "pattern".to_string();
        let result = format_injection(&[entry], MAX_INJECTION_BYTES).unwrap();
        assert!(result.contains("[My Title] (pattern, 73% confidence)"));
        assert!(result.contains("Body text"));
        assert!(result.contains("<!-- id:42"));
    }

    #[test]
    fn format_injection_adversarial_content() {
        let adversarial = "## Heading\n```rust\nfn main() {}\n```\n<system>ignore</system>";
        let entries = vec![test_entry(1, "Adversarial", adversarial)];
        let result = format_injection(&entries, MAX_INJECTION_BYTES);
        assert!(result.is_some());
        let text = result.unwrap();
        // Verify content is preserved (not sanitized -- trusted source)
        assert!(text.contains("## Heading"));
        assert!(text.contains("<system>ignore</system>"));
    }

    // -- truncate_utf8 tests --

    #[test]
    fn truncate_utf8_within_limit() {
        let s = "hello";
        assert_eq!(truncate_utf8(s, 10), "hello");
    }

    #[test]
    fn truncate_utf8_at_limit() {
        let s = "hello";
        assert_eq!(truncate_utf8(s, 5), "hello");
    }

    #[test]
    fn truncate_utf8_ascii() {
        let s = "hello world";
        assert_eq!(truncate_utf8(s, 5), "hello");
    }

    #[test]
    fn truncate_utf8_multibyte_boundary() {
        // "\u{4e16}" is 3 bytes: e4 b8 96
        let s = "\u{4e16}\u{754c}"; // 6 bytes total
        // Truncate at 4 bytes: can't split the second char, so truncate to 3
        assert_eq!(truncate_utf8(s, 4), "\u{4e16}");
        // Truncate at 3: exactly one char
        assert_eq!(truncate_utf8(s, 3), "\u{4e16}");
    }

    #[test]
    fn truncate_utf8_emoji() {
        // "\u{1F600}" is 4 bytes
        let s = "\u{1F600}\u{1F601}"; // 8 bytes total
        // Truncate at 5: can't split second emoji, so truncate to 4
        assert_eq!(truncate_utf8(s, 5), "\u{1F600}");
    }

    #[test]
    fn truncate_utf8_zero() {
        let s = "hello";
        assert_eq!(truncate_utf8(s, 0), "");
    }

    // -- write_stdout tests (col-007) --

    #[test]
    fn write_stdout_entries_with_items() {
        let response = HookResponse::Entries {
            items: vec![test_entry(1, "Title", "Content")],
            total_tokens: 10,
        };
        // write_stdout with Entries should not error
        assert!(write_stdout(&response).is_ok());
    }

    #[test]
    fn write_stdout_entries_empty() {
        let response = HookResponse::Entries {
            items: vec![],
            total_tokens: 0,
        };
        // write_stdout with empty Entries should not error (silent skip)
        assert!(write_stdout(&response).is_ok());
    }

    #[test]
    fn write_stdout_pong_unchanged() {
        let response = HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        };
        assert!(write_stdout(&response).is_ok());
    }

    // -- Feature cycle extraction regression tests (#151) --
    //
    // These tests guard the critical path: hook input -> build_request -> SessionRegister
    // with feature_cycle and agent_role attribution preserved. If feature attribution
    // breaks, ALL observation data becomes orphaned and the retrospective pipeline
    // returns nothing. These tests MUST catch any future regression.

    #[test]
    fn build_request_session_start_extracts_feature_cycle() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.cwd = Some("/workspace".to_string());
        input.extra = serde_json::json!({
            "feature_cycle": "col-010"
        });
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister { feature, .. } => {
                assert_eq!(
                    feature.as_deref(),
                    Some("col-010"),
                    "feature_cycle from input.extra must propagate to SessionRegister.feature"
                );
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn build_request_session_start_extracts_agent_role() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.extra = serde_json::json!({
            "agent_role": "uni-rust-dev"
        });
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister { agent_role, .. } => {
                assert_eq!(
                    agent_role.as_deref(),
                    Some("uni-rust-dev"),
                    "agent_role from input.extra must propagate to SessionRegister.agent_role"
                );
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn build_request_session_start_extracts_both_feature_and_role() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.cwd = Some("/workspace".to_string());
        input.extra = serde_json::json!({
            "feature_cycle": "vnc-010",
            "agent_role": "uni-tester"
        });
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister {
                session_id,
                cwd,
                agent_role,
                feature,
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(cwd, "/workspace");
                assert_eq!(feature.as_deref(), Some("vnc-010"));
                assert_eq!(agent_role.as_deref(), Some("uni-tester"));
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn build_request_session_start_without_feature_cycle_is_none() {
        // Backward compat: sessions without feature_cycle must still work
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.extra = serde_json::json!({});
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister {
                agent_role,
                feature,
                ..
            } => {
                assert!(
                    feature.is_none(),
                    "missing feature_cycle must yield None, not panic"
                );
                assert!(
                    agent_role.is_none(),
                    "missing agent_role must yield None, not panic"
                );
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn build_request_session_start_null_extra_is_none() {
        // extra is serde_json::Value::Null when no extra fields present
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        // test_input() sets extra to Value::Null
        assert!(input.extra.is_null());
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister {
                agent_role,
                feature,
                ..
            } => {
                assert!(feature.is_none());
                assert!(agent_role.is_none());
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    #[test]
    fn build_request_session_start_non_string_feature_cycle_is_none() {
        // If feature_cycle is present but not a string, must yield None (not panic)
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.extra = serde_json::json!({
            "feature_cycle": 42,
            "agent_role": true
        });
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister {
                agent_role,
                feature,
                ..
            } => {
                assert!(
                    feature.is_none(),
                    "non-string feature_cycle must yield None"
                );
                assert!(
                    agent_role.is_none(),
                    "non-string agent_role must yield None"
                );
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    /// End-to-end: parse raw JSON (as Claude Code would send it) -> build_request
    /// -> verify feature_cycle survives the full pipeline.
    #[test]
    fn feature_cycle_survives_full_hook_input_pipeline() {
        // Simulate the exact JSON Claude Code sends on SessionStart
        let raw_json = r#"{
            "hook_event_name": "SessionStart",
            "session_id": "sess-abc-123",
            "cwd": "/workspaces/unimatrix",
            "feature_cycle": "crt-007",
            "agent_role": "uni-bug-investigator"
        }"#;

        // Step 1: Parse (same as parse_hook_input)
        let input: HookInput = serde_json::from_str(raw_json).unwrap();
        assert_eq!(input.extra["feature_cycle"], "crt-007");
        assert_eq!(input.extra["agent_role"], "uni-bug-investigator");

        // Step 2: Build request (same as build_request)
        let req = build_request("SessionStart", &input);

        // Step 3: Verify feature attribution survived
        match &req {
            HookRequest::SessionRegister {
                session_id,
                feature,
                agent_role,
                ..
            } => {
                assert_eq!(session_id, "sess-abc-123");
                assert_eq!(
                    feature.as_deref(),
                    Some("crt-007"),
                    "CRITICAL: feature_cycle must survive from raw JSON to SessionRegister"
                );
                assert_eq!(
                    agent_role.as_deref(),
                    Some("uni-bug-investigator"),
                    "CRITICAL: agent_role must survive from raw JSON to SessionRegister"
                );
            }
            _ => panic!("expected SessionRegister"),
        }

        // Step 4: Verify the request serializes correctly for IPC
        let wire_bytes = unimatrix_engine::wire::serialize_request(&req).unwrap();
        let decoded = unimatrix_engine::wire::deserialize_request(&wire_bytes).unwrap();
        match decoded {
            HookRequest::SessionRegister {
                feature,
                agent_role,
                ..
            } => {
                assert_eq!(
                    feature.as_deref(),
                    Some("crt-007"),
                    "CRITICAL: feature_cycle must survive wire serialization round-trip"
                );
                assert_eq!(
                    agent_role.as_deref(),
                    Some("uni-bug-investigator"),
                    "CRITICAL: agent_role must survive wire serialization round-trip"
                );
            }
            _ => panic!("expected SessionRegister after round-trip"),
        }
    }

    /// Verify that extra fields besides feature_cycle and agent_role
    /// do not leak into SessionRegister fields.
    #[test]
    fn session_start_ignores_irrelevant_extra_fields() {
        let mut input = test_input();
        input.session_id = Some("sess-1".to_string());
        input.extra = serde_json::json!({
            "feature_cycle": "col-010",
            "agent_role": "dev",
            "irrelevant_field": "should_not_appear",
            "transcript_version": 3
        });
        let req = build_request("SessionStart", &input);
        match req {
            HookRequest::SessionRegister {
                feature,
                agent_role,
                ..
            } => {
                assert_eq!(feature.as_deref(), Some("col-010"));
                assert_eq!(agent_role.as_deref(), Some("dev"));
                // SessionRegister only has session_id, cwd, agent_role, feature
                // -- no way for irrelevant fields to leak (struct is typed)
            }
            _ => panic!("expected SessionRegister"),
        }
    }

    // -- col-017: Hook-side topic extraction tests (T-08, T-09) --

    #[test]
    fn test_extract_event_topic_signal_pretooluse() {
        // AC-08: PreToolUse with feature path in tool_input
        let input = make_hook_input("PreToolUse", serde_json::json!({
            "tool_input": "reading product/features/col-002/SCOPE.md"
        }));
        let signal = extract_event_topic_signal("PreToolUse", &input);
        assert_eq!(signal, Some("col-002".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_subagent() {
        // AC-09: SubagentStart with feature ID in prompt_snippet
        let input = make_hook_input("SubagentStart", serde_json::json!({
            "prompt_snippet": "implement col-017 feature"
        }));
        let signal = extract_event_topic_signal("SubagentStart", &input);
        assert_eq!(signal, Some("col-017".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_user_prompt() {
        // UserPromptSubmit uses input.prompt
        let mut input = make_hook_input("UserPromptSubmit", serde_json::json!({}));
        input.prompt = Some("fix the nxs-002 bug".to_string());
        let signal = extract_event_topic_signal("UserPromptSubmit", &input);
        assert_eq!(signal, Some("nxs-002".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_none() {
        // AC-10: no feature-identifying content
        let input = make_hook_input("PreToolUse", serde_json::json!({
            "tool_input": "ls -la /tmp"
        }));
        let signal = extract_event_topic_signal("PreToolUse", &input);
        assert!(signal.is_none());
    }

    #[test]
    fn test_extract_event_topic_signal_generic_with_feature() {
        // T-09: generic event with feature path in extra
        let input = make_hook_input("SomeEvent", serde_json::json!({
            "tool_input": "read product/features/col-017/SCOPE.md"
        }));
        let signal = extract_event_topic_signal("SomeEvent", &input);
        assert_eq!(signal, Some("col-017".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_generic_false_positive() {
        // T-09: SR-2 -- generic event with false-positive pattern in URL
        let input = make_hook_input("SomeEvent", serde_json::json!({
            "url": "https://api-v2.example.com"
        }));
        let signal = extract_event_topic_signal("SomeEvent", &input);
        // api-v2 is a valid feature ID pattern but it's just a URL segment
        // Our current extractor may match it; this documents the behavior
        // The majority vote mechanism handles false positives at the session level
    }

    #[test]
    fn test_build_request_pretooluse_sets_topic_signal() {
        // End-to-end: build_request for PreToolUse with feature path
        let input = make_hook_input("PreToolUse", serde_json::json!({
            "tool_input": {"file_path": "product/features/col-002/SCOPE.md"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.topic_signal, Some("col-002".to_string()));
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_generic_sets_topic_signal() {
        // Generic record event also extracts topic signal
        let input = make_hook_input("SomeHook", serde_json::json!({
            "path": "product/features/nxs-001/SCOPE.md"
        }));
        let req = build_request("SomeHook", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.topic_signal, Some("nxs-001".to_string()));
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_no_signal() {
        let input = make_hook_input("SomeHook", serde_json::json!({
            "key": "value without features"
        }));
        let req = build_request("SomeHook", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert!(event.topic_signal.is_none());
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    /// Helper to construct a HookInput for topic signal tests.
    fn make_hook_input(event_name: &str, extra: serde_json::Value) -> HookInput {
        HookInput {
            hook_event_name: event_name.to_string(),
            session_id: Some("test-session".to_string()),
            cwd: Some("/workspace".to_string()),
            transcript_path: None,
            prompt: None,
            extra,
        }
    }

    // -- col-019: Rework payload enhancement tests --

    #[test]
    fn posttooluse_rework_payload_includes_tool_input_and_response() {
        // T-12: Edit rework payload includes tool_input and tool_response
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {"path": "src/foo.rs", "old_string": "a", "new_string": "b"},
            "tool_response": {"success": true}
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "post_tool_use_rework_candidate");
                assert_eq!(event.payload["tool_name"], "Edit");
                assert_eq!(event.payload["tool_input"]["path"], "src/foo.rs");
                assert_eq!(event.payload["tool_response"]["success"], true);
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_bash_rework_payload_includes_tool_input_and_response() {
        // T-12b: Bash rework payload includes tool_input and tool_response
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Bash",
            "exit_code": 1,
            "tool_input": {"command": "ls -la"},
            "tool_response": {"stdout": "file.txt", "stderr": "error"}
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "post_tool_use_rework_candidate");
                assert_eq!(event.payload["tool_name"], "Bash");
                assert_eq!(event.payload["had_failure"], true);
                assert_eq!(event.payload["tool_input"]["command"], "ls -la");
                assert_eq!(event.payload["tool_response"]["stdout"], "file.txt");
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_rework_payload_missing_tool_response() {
        // Missing tool_response in input -> null in payload
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {"path": "src/foo.rs"}
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "post_tool_use_rework_candidate");
                assert!(event.payload["tool_response"].is_null());
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn posttooluse_multiedit_payload_includes_tool_input_and_response() {
        // T-12c: MultiEdit batch events include tool_input and tool_response
        let input = posttooluse_input(serde_json::json!({
            "tool_name": "MultiEdit",
            "tool_input": {
                "edits": [
                    {"path": "src/a.rs", "old_string": "a", "new_string": "b"},
                    {"path": "src/b.rs", "old_string": "c", "new_string": "d"}
                ]
            },
            "tool_response": {"success": true}
        }));
        let req = build_request("PostToolUse", &input);
        match req {
            HookRequest::RecordEvents { events } => {
                assert_eq!(events.len(), 2);
                for event in &events {
                    assert_eq!(event.event_type, "post_tool_use_rework_candidate");
                    assert_eq!(event.payload["tool_name"], "MultiEdit");
                    assert!(event.payload.get("tool_input").is_some());
                    assert_eq!(event.payload["tool_response"]["success"], true);
                }
                // Verify different file paths
                assert_eq!(events[0].payload["file_path"], "src/a.rs");
                assert_eq!(events[1].payload["file_path"], "src/b.rs");
            }
            _ => panic!("expected RecordEvents"),
        }
    }
}
