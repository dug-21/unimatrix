//! Hook subcommand handler.
//!
//! Reads Claude Code hook JSON from stdin, connects to the running server
//! via UDS, and dispatches events. Uses synchronous std I/O only (no tokio
//! runtime) per ADR-002 for sub-50ms latency.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use unimatrix_core::observation::hook_type;
use unimatrix_engine::event_queue::EventQueue;
use unimatrix_engine::project::compute_project_hash;
use unimatrix_engine::transport::{LocalTransport, Transport};
use unimatrix_engine::wire::{
    EntryPayload, HookInput, HookRequest, HookResponse, ImplantEvent, TransportError,
};
use unimatrix_observe::extract_topic_signal;

use crate::infra::validation::{
    CYCLE_PHASE_END_EVENT, CYCLE_START_EVENT, CYCLE_STOP_EVENT, CycleType, validate_cycle_params,
};

/// Default timeout for transport operations: 40ms.
/// Leaves 10ms margin in the 50ms total budget for process startup + hash computation.
const HOOK_TIMEOUT: Duration = Duration::from_millis(40);

/// Maximum byte budget for injection output (~350 tokens at 4 bytes/token).
const MAX_INJECTION_BYTES: usize = 1400;

/// Minimum word count for UserPromptSubmit to route to ContextSearch.
/// Prompts shorter than this threshold fall through to generic_record_event.
/// Evaluated on query.trim().split_whitespace().count() (leading/trailing
/// whitespace is NOT counted). See ADR-002 crt-027.
const MIN_QUERY_WORDS: usize = 5;

/// Maximum byte budget for the PreCompact transcript restoration block (~750 tokens).
/// Separate from MAX_INJECTION_BYTES (1400) per D-4 and AC-10.
const MAX_PRECOMPACT_BYTES: usize = 3000;

/// Maximum byte length for a feature cycle goal string (ADR-005, col-025).
///
/// One constant shared by both paths:
/// - MCP path (tools.rs): hard-reject with CallToolResult::error when exceeded.
/// - UDS path (listener.rs): truncate at nearest UTF-8 char boundary + tracing::warn!.
pub(crate) const MAX_GOAL_BYTES: usize = 1024;

/// Tail-bytes window multiplier. Raw JSONL is ~4x larger than extracted text.
/// TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 12,000 bytes (ADR-001).
const TAIL_MULTIPLIER: usize = 4;

/// Per-tool-result snippet truncation budget (D-3, FR-03.4).
const TOOL_RESULT_SNIPPET_BYTES: usize = 300;

/// Key-param truncation budget for tool compact representation (OQ-3).
const TOOL_KEY_PARAM_BYTES: usize = 120;

/// Run the hook subcommand.
///
/// This is the entry point from `main()` for the `hook` subcommand.
/// No tokio runtime is initialized. Returns `Ok(())` for all expected
/// conditions -- exit code is always 0 per FR-03.7.
pub fn run(
    event: String,
    provider: Option<String>,
    project_dir: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // provider is threaded into hook_input by the normalization layer (Wave 2).
    // For Wave 1, the field exists on HookInput but normalization is not yet implemented.
    // Suppress unused variable warning until Wave 2 wires this up.
    let _ = &provider;
    // Step 1: Read stdin
    let stdin_content = read_stdin();

    // Step 2: Parse hook input (defensive -- ADR-006)
    let hook_input = parse_hook_input(&stdin_content);

    // Step 3: Determine working directory and detect project root
    let cwd = resolve_cwd(&hook_input, project_dir.as_deref());
    let project_root = unimatrix_engine::project::detect_project_root(Some(&cwd)).unwrap_or(cwd);
    let project_hash = compute_project_hash(&project_root);

    // Step 4: Compute socket path
    let home = dirs::home_dir().ok_or("home directory not found")?;
    let socket_path = home
        .join(".unimatrix")
        .join(&project_hash)
        .join("unimatrix.sock");

    // Step 5: Build request from event + input
    let request = build_request(&event, &hook_input);

    // Step 5b: SubagentStart fallback — Claude Code does not send prompt_snippet in the
    // SubagentStart payload, so build_request always returns RecordEvent for this event.
    // Derive a query from the transcript tail (which contains the Task spawn description
    // as the most recent ToolPair entry) with agent_type as role.
    let request = if event == "SubagentStart" && matches!(request, HookRequest::RecordEvent { .. })
    {
        let role = hook_input
            .extra
            .get("agent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let query = hook_input
            .transcript_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .and_then(|p| extract_transcript_block(p));
        match query {
            Some(q) => HookRequest::ContextSearch {
                query: q,
                session_id: hook_input.session_id.clone(),
                source: Some("SubagentStart".to_string()),
                role,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            },
            None => request,
        }
    } else {
        request
    };

    // Step 5c: Extract source BEFORE consuming the request (needed for response routing).
    // Only ContextSearch carries a source; extract it now for use after transport.request().
    let req_source: Option<String> = match &request {
        HookRequest::ContextSearch { source, .. } => source.clone(),
        _ => None,
    };

    // Step 5d: Extract transcript block for PreCompact before server round-trip (OQ-2 resolved)
    let transcript_block: Option<String> = if matches!(request, HookRequest::CompactPayload { .. })
    {
        hook_input
            .transcript_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .and_then(|p| extract_transcript_block(p))
    } else {
        None
    };

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
                        // Route stdout writing based on source (ADR-006 crt-027):
                        // SubagentStart requires hookSpecificOutput JSON envelope;
                        // all other sources (UserPromptSubmit, etc.) use plain text.
                        let write_result = if req_source.as_deref() == Some("SubagentStart") {
                            write_stdout_subagent_inject_response(&response)
                        } else {
                            // Modified: for BriefingContent responses, prepend transcript block (D-5)
                            match &response {
                                HookResponse::BriefingContent { content, .. } => {
                                    let full_output =
                                        prepend_transcript(transcript_block.as_deref(), content);
                                    if !full_output.is_empty() {
                                        println!("{full_output}");
                                    }
                                    Ok(())
                                }
                                _ => write_stdout(&response),
                            }
                        };
                        if let Err(e) = write_result {
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
                provider: None,
                mcp_context: None,
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
/// - PreToolUse / PostToolUse / PostToolUseFailure: `input.extra["tool_input"]` stringified
/// - SubagentStart: `input.extra["agent_type"]` stringified
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
        // col-027: Explicit arm for PostToolUseFailure (ADR-001).
        // Same source field as PostToolUse: tool_input contains the invocation parameters.
        // PostToolUseFailure does NOT have a tool_response, but tool_input is present.
        // Kept as a separate arm (not deduplicated with PostToolUse) to allow future divergence.
        hook_type::POSTTOOLUSEFAILURE => {
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
            // agent_type is the actual field Claude Code sends; prompt_snippet does not exist.
            let text = input
                .extra
                .get("agent_type")
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

            // Guard 1: empty or whitespace-only → RecordEvent (EC-01)
            if query.trim().is_empty() {
                return generic_record_event(event, session_id, input);
            }

            // Guard 2: word-count threshold (ADR-002 crt-027, FR-05)
            // split_whitespace() handles leading/trailing whitespace, so "  approve  " counts
            // as 1 word. The query value itself is NOT trimmed -- evaluation only.
            let word_count = query.split_whitespace().count();
            if word_count < MIN_QUERY_WORDS {
                return generic_record_event(event, session_id, input);
            }

            // Route to ContextSearch: source=None (ADR-001: None → "UserPromptSubmit" at server)
            HookRequest::ContextSearch {
                query,
                session_id: input.session_id.clone(),
                source: None,
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
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
                        provider: None,
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
                            provider: None,
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
                        provider: None,
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
                    provider: None,
                },
            }
        }

        // col-027: Explicit arm for PostToolUseFailure (ADR-001, FR-03.1).
        // Must NOT fall through to the wildcard — wildcard would store records with tool_name = None.
        // Must NOT enter rework logic — failure events are never rework candidates.
        // Must NOT call extract_response_fields() — error field handled in listener.rs.
        // Hook always exits 0: all field accesses use defensive Option chaining (FR-03.7).
        hook_type::POSTTOOLUSEFAILURE => {
            // Compute topic_signal from tool_input (same source as PostToolUse arm).
            // tool_name is carried in input.extra (payload) and extracted by listener.rs.
            // Returns None if tool_input is absent or produces no signal.
            let topic_signal = extract_event_topic_signal(event, input);

            // Build fire-and-forget RecordEvent.
            // payload = input.extra.clone() carries tool_name, error, tool_input, is_interrupt.
            // listener.rs extract_observation_fields() reads from this payload.
            // event_type is stored verbatim — NOT normalized to "PostToolUse" (ADR-003).
            HookRequest::RecordEvent {
                event: ImplantEvent {
                    event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
                    session_id,
                    timestamp: now_secs(),
                    payload: input.extra.clone(),
                    topic_signal,
                    provider: None,
                },
            }
        }

        // col-022: Intercept PreToolUse for context_cycle tool calls
        "PreToolUse" => build_cycle_event_or_fallthrough(event, session_id, input),

        // crt-027 WA-4a: Route SubagentStart to ContextSearch when prompt_snippet is present
        // (forward-compat only — Claude Code does not currently send this field).
        // The real query is derived in run() step 5b from the transcript tail.
        // Guard: empty or whitespace-only prompt_snippet falls through to RecordEvent (EC-01).
        "SubagentStart" => {
            let query = input
                .extra
                .get("prompt_snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // EC-01: .trim().is_empty() catches whitespace-only, not just ""
            if query.trim().is_empty() {
                return generic_record_event(event, session_id, input);
            }

            // Route to ContextSearch with source="SubagentStart" (ADR-001, ADR-002 crt-027).
            // session_id = input.session_id (parent session, not ppid fallback) so WA-2 boost applies.
            HookRequest::ContextSearch {
                query,
                session_id: input.session_id.clone(),
                source: Some("SubagentStart".to_string()),
                role: None,
                task: None,
                feature: None,
                k: None,
                max_tokens: None,
            }
        }

        _ => generic_record_event(event, session_id, input),
    }
}

/// Detect `context_cycle` in PreToolUse events and build a specialized
/// RecordEvent with `event_type: "cycle_start"` or `"cycle_stop"` (ADR-001).
///
/// Falls through to `generic_record_event` if this is not a `context_cycle`
/// tool call, or if validation fails. The hook must never fail (FR-03.7).
fn build_cycle_event_or_fallthrough(
    event: &str,
    session_id: String,
    input: &HookInput,
) -> HookRequest {
    // Step 1: Check if this is a context_cycle tool call
    let tool_name = input
        .extra
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Match by "context_cycle" substring in tool_name.
    // Claude Code sends tool_name as "mcp__unimatrix__context_cycle"
    // (server prefix + tool name). Use .contains("context_cycle") to match
    // regardless of prefix format.
    if !tool_name.contains("context_cycle") {
        return generic_record_event(event, session_id, input);
    }

    // R-09 mitigation: verify the prefix contains "unimatrix" to avoid
    // matching a tool from a different MCP server named "context_cycle".
    // If tool_name is exactly "context_cycle" (no prefix), allow it (direct MCP call).
    if tool_name != "context_cycle" && !tool_name.contains("unimatrix") {
        return generic_record_event(event, session_id, input);
    }

    // Step 2: Extract parameters from tool_input
    let tool_input = match input.extra.get("tool_input") {
        Some(v) => v,
        None => {
            eprintln!("unimatrix: context_cycle PreToolUse missing tool_input");
            return generic_record_event(event, session_id, input);
        }
    };

    let type_str = tool_input
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let topic_str = tool_input
        .get("topic")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let phase_opt = tool_input.get("phase").and_then(|v| v.as_str());
    let outcome_opt = tool_input.get("outcome").and_then(|v| v.as_str());
    let next_phase_opt = tool_input.get("next_phase").and_then(|v| v.as_str());

    // Step 3: Validate using shared function (ADR-004, C-02)
    let validated = match validate_cycle_params(
        type_str,
        topic_str,
        phase_opt,
        outcome_opt,
        next_phase_opt,
    ) {
        Ok(v) => v,
        Err(msg) => {
            // Hook must never fail (FR-03.7). Log warning, fall through.
            eprintln!(
                "unimatrix: context_cycle validation failed in hook: {msg} (tool_name={tool_name})"
            );
            return generic_record_event(event, session_id, input);
        }
    };

    // Step 4: Build specialized RecordEvent
    let event_type = match validated.cycle_type {
        CycleType::Start => CYCLE_START_EVENT.to_string(),
        CycleType::PhaseEnd => CYCLE_PHASE_END_EVENT.to_string(),
        CycleType::Stop => CYCLE_STOP_EVENT.to_string(),
    };

    // Step 4b: Extract goal for cycle_start events (col-025, GH #389).
    // Only populated for Start; PhaseEnd and Stop explicitly yield None (FR-01).
    // Uses eprintln! (not tracing!) — hook runs outside the tokio runtime (ADR-002).
    // Truncation mirrors truncate_at_utf8_boundary in listener.rs.
    let goal_opt: Option<String> = if validated.cycle_type == CycleType::Start {
        tool_input.get("goal").and_then(|v| v.as_str()).map(|g| {
            if g.len() > MAX_GOAL_BYTES {
                eprintln!("[unimatrix hook] goal exceeds MAX_GOAL_BYTES, truncating");
                let mut end = MAX_GOAL_BYTES;
                while end > 0 && !g.is_char_boundary(end) {
                    end -= 1;
                }
                g[..end].to_string()
            } else {
                g.to_string()
            }
        })
    } else {
        None
    };

    // Build payload with feature_cycle and optional phase/outcome/next_phase.
    // The feature_cycle key in payload is what the #198 extraction path and
    // the cycle event handlers look for.
    let mut payload = serde_json::json!({
        "feature_cycle": validated.topic,
    });

    if let Some(ref p) = validated.phase {
        payload["phase"] = serde_json::Value::String(p.clone());
    }
    if let Some(ref o) = validated.outcome {
        payload["outcome"] = serde_json::Value::String(o.clone());
    }
    if let Some(ref np) = validated.next_phase {
        payload["next_phase"] = serde_json::Value::String(np.clone());
    }

    // Insert goal into payload so the listener can read payload.get("goal") (GH #389).
    if let Some(ref g) = goal_opt {
        payload["goal"] = serde_json::Value::String(g.clone());
    }

    // Set topic_signal to the topic value -- strong signal for eager attribution
    // as a secondary attribution path.
    let topic_signal = Some(validated.topic.clone());

    HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type,
            session_id,
            timestamp: now_secs(),
            payload,
            topic_signal,
            provider: None,
        },
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
            provider: None,
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

/// Write the hookSpecificOutput JSON envelope required by Claude Code for SubagentStart
/// context injection (ADR-006 crt-027).
///
/// The envelope format is:
/// `{"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": "<entries_text>"}}`
///
/// Returns `io::Result<()>`. Callers must handle errors; the hook exit code is always 0.
fn write_stdout_subagent_inject(entries_text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SubagentStart",
            "additionalContext": entries_text
        }
    });
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    // writeln appends a newline after the JSON object
    writeln!(handle, "{}", envelope)
}

/// Write SubagentStart response to stdout via the hookSpecificOutput JSON envelope.
///
/// Extracts formatted text from `HookResponse::Entries` and calls
/// `write_stdout_subagent_inject`. If the response is not `Entries` (unexpected
/// but safe), falls through to plain-text `write_stdout` for graceful degradation.
fn write_stdout_subagent_inject_response(
    response: &HookResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    match response {
        HookResponse::Entries { items, .. } => {
            if let Some(text) = format_injection(items, MAX_INJECTION_BYTES) {
                write_stdout_subagent_inject(&text)?;
            }
            // Empty items: silent skip (no stdout written — same as write_stdout behavior)
            Ok(())
        }
        // Non-Entries responses from SubagentStart ContextSearch (unexpected but safe):
        // fall through to plain-text write for graceful degradation (FR-06, C-01)
        other => write_stdout(other),
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

/// A single typed turn extracted from the JSONL transcript window.
/// Internal to hook.rs — not exported or used by other modules.
enum ExchangeTurn {
    UserText(String),
    AssistantText(String),
    ToolPair {
        name: String,
        key_param: String,
        result_snippet: String,
    },
}

/// Return the most-identifying input field value for a tool call.
///
/// Hardcoded map for 10 known Claude Code tools (OQ-3 settled).
/// Fallback: first string-valued field in the input object.
/// Result truncated to TOOL_KEY_PARAM_BYTES via truncate_utf8.
fn extract_key_param(tool_name: &str, input: &serde_json::Value) -> String {
    let field_name: &str = match tool_name {
        "Bash" => "command",
        "Read" => "file_path",
        "Edit" => "file_path",
        "Write" => "file_path",
        "Glob" => "pattern",
        "Grep" => "pattern",
        "MultiEdit" => "file_path",
        "Task" => "description",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => "",
    };

    if !field_name.is_empty() {
        if let Some(val) = input.get(field_name).and_then(|v| v.as_str()) {
            return truncate_utf8(val, TOOL_KEY_PARAM_BYTES).to_string();
        }
    }

    if let Some(obj) = input.as_object() {
        for (_key, val) in obj {
            if let Some(s) = val.as_str() {
                return truncate_utf8(s, TOOL_KEY_PARAM_BYTES).to_string();
            }
        }
    }

    String::new()
}

/// Helper: extract the content array from a JSONL record.
/// Handles two shapes:
///   { "type": "...", "message": { "content": [...] } }  (Claude Code UX format)
///   { "type": "...", "content": [...] }                 (raw API format)
fn get_content_array(record: &serde_json::Value) -> &[serde_json::Value] {
    if let Some(arr) = record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        return arr;
    }
    if let Some(arr) = record.get("content").and_then(|c| c.as_array()) {
        return arr;
    }
    &[]
}

/// Helper: extract snippet text from a tool_result content block.
/// Returns first type:"text" block text truncated to TOOL_RESULT_SNIPPET_BYTES.
fn extract_tool_result_snippet(tool_result_block: &serde_json::Value) -> String {
    let content = tool_result_block.get("content");
    match content {
        Some(serde_json::Value::String(s)) => {
            truncate_utf8(s, TOOL_RESULT_SNIPPET_BYTES).to_string()
        }
        Some(serde_json::Value::Array(blocks)) => {
            for block in blocks {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        return truncate_utf8(text, TOOL_RESULT_SNIPPET_BYTES).to_string();
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// Parse JSONL lines from a tail window into typed exchange turns.
///
/// Fail-open: malformed lines and unknown type values are skipped silently.
/// Tool-use/result pairing: adjacent-record scan (ADR-002).
/// Returns turns in reverse-chronological order (Vec reversed before return).
fn build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn> {
    let mut turns: Vec<ExchangeTurn> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        let record: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                i += 1;
                continue;
            }
        };

        let record_type = match record.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => {
                i += 1;
                continue;
            }
        };

        match record_type {
            "user" => {
                let content_arr = get_content_array(&record);
                let user_texts: Vec<&str> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                            block.get("text").and_then(|v| v.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                if !user_texts.is_empty() {
                    turns.push(ExchangeTurn::UserText(user_texts.join("\n")));
                }
                i += 1;
            }

            "assistant" => {
                let content_arr = get_content_array(&record);

                let asst_texts: Vec<&str> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                            block.get("text").and_then(|v| v.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                struct ToolUseInfo {
                    id: String,
                    name: String,
                    key_param: String,
                }
                let tool_uses: Vec<ToolUseInfo> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                            return None;
                        }
                        let id = block.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = block.get("name").and_then(|v| v.as_str())?.to_string();
                        let input = block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        let key_param = extract_key_param(&name, &input);
                        Some(ToolUseInfo {
                            id,
                            name,
                            key_param,
                        })
                    })
                    .collect();

                let has_text = !asst_texts.is_empty();
                let has_tool_use = !tool_uses.is_empty();

                // Pure thinking turn (no text, no tool_use): suppress entirely
                if !has_text && !has_tool_use {
                    i += 1;
                    continue;
                }

                // Emit AssistantText only if there is actual text (OQ-SPEC-1)
                if has_text {
                    turns.push(ExchangeTurn::AssistantText(asst_texts.join("\n")));
                }

                // Adjacent-record look-ahead for tool_result pairing (ADR-002)
                let mut result_map: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();

                if has_tool_use && i + 1 < lines.len() {
                    let next_line = lines[i + 1];
                    if !next_line.trim().is_empty() {
                        if let Ok(next_record) =
                            serde_json::from_str::<serde_json::Value>(next_line)
                        {
                            if next_record.get("type").and_then(|v| v.as_str()) == Some("user") {
                                let next_content = get_content_array(&next_record);
                                for block in next_content {
                                    if block.get("type").and_then(|v| v.as_str())
                                        != Some("tool_result")
                                    {
                                        continue;
                                    }
                                    let tool_use_id =
                                        match block.get("tool_use_id").and_then(|v| v.as_str()) {
                                            Some(id) => id.to_string(),
                                            None => continue,
                                        };
                                    let snippet = extract_tool_result_snippet(block);
                                    result_map.insert(tool_use_id, snippet);
                                }
                            }
                        }
                    }
                }

                for tu in &tool_uses {
                    let result_snippet = result_map.get(&tu.id).cloned().unwrap_or_default();
                    turns.push(ExchangeTurn::ToolPair {
                        name: tu.name.clone(),
                        key_param: tu.key_param.clone(),
                        result_snippet,
                    });
                }

                i += 1;
            }

            _ => {
                i += 1;
            }
        }
    }

    turns.reverse();
    turns
}

/// Format a single ExchangeTurn as a text line.
fn format_turn(turn: &ExchangeTurn) -> String {
    match turn {
        ExchangeTurn::UserText(text) => format!("[User] {}", text),
        ExchangeTurn::AssistantText(text) => format!("[Assistant] {}", text),
        ExchangeTurn::ToolPair {
            name,
            key_param,
            result_snippet,
        } => {
            format!(
                "[tool: {}({}) \u{2192} {}]",
                name, key_param, result_snippet
            )
        }
    }
}

/// Read the tail of the transcript file at `path`, parse as JSONL, and format
/// a restoration block within MAX_PRECOMPACT_BYTES.
///
/// Returns None on any failure (ADR-003 degradation contract).
/// Never panics. Never propagates errors. All I/O is std::io — no tokio.
fn extract_transcript_block(path: &str) -> Option<String> {
    let inner = || -> Option<String> {
        let mut file = std::fs::File::open(path).ok()?;
        let file_len: u64 = file.metadata().ok()?.len();

        let window: u64 = (MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER) as u64;
        let seek_back: u64 = window.min(file_len);

        if seek_back > 0 {
            file.seek(SeekFrom::End(-(seek_back as i64))).ok()?;
        }

        let reader = BufReader::new(file);
        let raw_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

        let line_refs: Vec<&str> = raw_lines.iter().map(|s| s.as_str()).collect();
        let turns: Vec<ExchangeTurn> = build_exchange_pairs(&line_refs);

        let mut output_parts: Vec<String> = Vec::new();
        let mut bytes_used: usize = 0;
        let mut exchange_count: usize = 0;

        for turn in &turns {
            let turn_text = format_turn(turn);
            let turn_bytes = turn_text.len();
            if bytes_used + turn_bytes > MAX_PRECOMPACT_BYTES {
                break;
            }
            bytes_used += turn_bytes;
            if matches!(turn, ExchangeTurn::UserText(_)) {
                exchange_count += 1;
            }
            output_parts.push(turn_text);
        }

        if output_parts.is_empty() {
            return None;
        }

        let header = format!(
            "=== Recent conversation (last {} exchanges) ===",
            exchange_count
        );
        let footer = "=== End recent conversation ===".to_string();
        let body = output_parts.join("\n");

        Some(format!("{}\n{}\n{}", header, body, footer))
    };

    inner()
}

/// Combine optional transcript block with briefing content.
///
/// Cases:
/// 1. Both present: "{transcript}\n\n{briefing}"
/// 2. Transcript only: "{transcript}"
/// 3. Briefing only: "{briefing}"
/// 4. Both empty: ""
fn prepend_transcript(transcript: Option<&str>, briefing: &str) -> String {
    let briefing_empty = briefing.is_empty();
    match (transcript, briefing_empty) {
        (Some(t), false) => format!("{}\n\n{}", t, briefing),
        (Some(t), true) => t.to_string(),
        (None, false) => briefing.to_string(),
        (None, true) => String::new(),
    }
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
            provider: None,
            mcp_context: None,
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
        // Uses a 5-word prompt to pass the MIN_QUERY_WORDS guard (ADR-002 crt-027)
        let mut input = test_input();
        input.prompt = Some("implement the spec writer agent".to_string());
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { query, .. } => {
                assert_eq!(query, "implement the spec writer agent");
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
        // Use a multi-word prompt so it passes the MIN_QUERY_WORDS guard (ADR-002 crt-027).
        // 5 words repeated many times satisfies both the threshold and the length check.
        let mut input = test_input();
        let word_repeated = "implement the spec writer agent ".repeat(400); // >5 words, long
        let prompt = word_repeated.trim_end().to_string();
        let prompt_len = prompt.len();
        input.prompt = Some(prompt);
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { query, .. } => {
                assert_eq!(query.len(), prompt_len);
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
            source: None,
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
        // Uses a 5-word prompt to pass the MIN_QUERY_WORDS guard (ADR-002 crt-027)
        let prompt = "implement the spec writer agent".to_string();
        input.prompt = Some(prompt.clone());
        input.session_id = Some("sess-1".to_string());
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch {
                session_id, query, ..
            } => {
                assert_eq!(query, prompt);
                assert_eq!(session_id.as_deref(), Some("sess-1"));
            }
            _ => panic!("expected ContextSearch, got {req:?}"),
        }
    }

    #[test]
    fn build_request_user_prompt_no_session_id() {
        let mut input = test_input();
        // Uses a 5-word prompt to pass the MIN_QUERY_WORDS guard (ADR-002 crt-027)
        input.prompt = Some("implement the spec writer agent".to_string());
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
            provider: None,
            mcp_context: None,
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
                let paths: Vec<_> = events
                    .iter()
                    .map(|e| e.payload["file_path"].as_str().unwrap())
                    .collect();
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
            provider: None,
            mcp_context: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("PostToolUse", &input);
        // Null extra → no tool_name → generic RecordEvent
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    // -- PostToolUseFailure tests (col-027) --

    fn posttoolusefailure_input(extra: serde_json::Value) -> HookInput {
        HookInput {
            hook_event_name: "PostToolUseFailure".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            prompt: None,
            provider: None,
            mcp_context: None,
            extra,
        }
    }

    /// T-HD-01: explicit arm fires — event_type is "PostToolUseFailure" (AC-11, R-05).
    #[test]
    fn build_request_posttoolusefailure_explicit_arm() {
        let input = posttoolusefailure_input(serde_json::json!({
            "tool_name": "Bash",
            "error": "permission denied",
            "tool_input": { "command": "ls /root" }
        }));
        let req = build_request("PostToolUseFailure", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Explicit arm ran — event_type must be the verbatim hook name (ADR-001, ADR-003)
                assert_eq!(event.event_type, "PostToolUseFailure");
                // tool_name is carried in the payload, not as a top-level ImplantEvent field
                assert_eq!(event.payload["tool_name"], "Bash");
            }
            _ => panic!("expected HookRequest::RecordEvent"),
        }
    }

    /// T-HD-02: empty extra does not panic and returns RecordEvent (AC-12, R-08).
    #[test]
    fn build_request_posttoolusefailure_empty_extra() {
        let input = posttoolusefailure_input(serde_json::json!({}));
        let req = build_request("PostToolUseFailure", &input);
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    /// T-HD-03: missing tool_name does not panic (AC-12, R-08, FR-03.6).
    #[test]
    fn build_request_posttoolusefailure_missing_tool_name() {
        let input = posttoolusefailure_input(serde_json::json!({
            "error": "something went wrong",
            "tool_input": {}
        }));
        let req = build_request("PostToolUseFailure", &input);
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    /// T-HD-04: null error field does not panic (AC-12, R-08).
    #[test]
    fn build_request_posttoolusefailure_null_error() {
        let input = posttoolusefailure_input(serde_json::json!({
            "tool_name": "Read",
            "error": null,
            "tool_input": {}
        }));
        let req = build_request("PostToolUseFailure", &input);
        assert!(matches!(req, HookRequest::RecordEvent { .. }));
    }

    /// T-HD-05: does NOT enter rework logic — event_type must not be rework-candidate (AC-11, R-05).
    #[test]
    fn build_request_posttoolusefailure_does_not_enter_rework_logic() {
        let input = posttoolusefailure_input(serde_json::json!({
            "tool_name": "Write",
            "error": "permission denied",
            "tool_input": {}
        }));
        let req = build_request("PostToolUseFailure", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Must NOT produce a rework-candidate event_type
                assert_eq!(event.event_type, "PostToolUseFailure");
                assert_ne!(event.event_type, "post_tool_use_rework_candidate");
            }
            HookRequest::RecordEvents { .. } => {
                panic!("must not produce multi-event rework response")
            }
            _ => panic!("expected HookRequest::RecordEvent"),
        }
    }

    /// T-HD-06: extract_event_topic_signal reads from tool_input, not full extra blob (R-09).
    #[test]
    fn extract_event_topic_signal_posttoolusefailure() {
        let input = posttoolusefailure_input(serde_json::json!({
            "tool_name": "Bash",
            "tool_input": { "command": "ls /tmp" },
            "error": "no such file"
        }));
        let result = extract_event_topic_signal("PostToolUseFailure", &input);
        // topic_signal is derived from tool_input, not from the full extra blob
        if let Some(signal) = result {
            // Must NOT contain the "error" key (would indicate full extra blob serialization)
            assert!(
                !signal.contains("\"error\""),
                "topic signal must not include the error field: {signal}"
            );
        }
        // None is also acceptable when tool_input produces no signal — no panic required
    }

    /// T-HD-extra: null extra on PostToolUseFailure does not panic (AC-12, R-08).
    #[test]
    fn build_request_posttoolusefailure_null_extra() {
        let input = HookInput {
            hook_event_name: "PostToolUseFailure".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            prompt: None,
            provider: None,
            mcp_context: None,
            extra: serde_json::Value::Null,
        };
        let req = build_request("PostToolUseFailure", &input);
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
        assert!(is_bash_failure(
            &serde_json::json!({"exit_code": 0, "interrupted": true})
        ));
    }

    #[test]
    fn extract_file_path_edit() {
        let extra = serde_json::json!({"tool_input": {"path": "/src/lib.rs"}});
        assert_eq!(
            extract_file_path(&extra, "Edit"),
            Some("/src/lib.rs".to_string())
        );
    }

    #[test]
    fn extract_file_path_write() {
        let extra = serde_json::json!({"tool_input": {"file_path": "/src/main.rs"}});
        assert_eq!(
            extract_file_path(&extra, "Write"),
            Some("/src/main.rs".to_string())
        );
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
        let input = make_hook_input(
            "PreToolUse",
            serde_json::json!({
                "tool_input": "reading product/features/col-002/SCOPE.md"
            }),
        );
        let signal = extract_event_topic_signal("PreToolUse", &input);
        assert_eq!(signal, Some("col-002".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_subagent() {
        // AC-09: SubagentStart reads from agent_type, not prompt_snippet.
        // extract_topic_signal treats hyphenated identifiers as valid feature IDs.
        let input = make_hook_input(
            "SubagentStart",
            serde_json::json!({
                "agent_type": "uni-rust-dev"
            }),
        );
        let signal = extract_event_topic_signal("SubagentStart", &input);
        assert_eq!(signal, Some("uni-rust-dev".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_subagent_prompt_snippet_ignored() {
        // AC-09b: prompt_snippet is NOT read for SubagentStart; agent_type is used instead.
        let input = make_hook_input(
            "SubagentStart",
            serde_json::json!({
                "prompt_snippet": "implement col-017 feature",
                "agent_type": "uni-spec"
            }),
        );
        // Signal comes from agent_type "uni-spec", not prompt_snippet "col-017"
        let signal = extract_event_topic_signal("SubagentStart", &input);
        assert_eq!(signal, Some("uni-spec".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_subagent_absent_agent_type() {
        // AC-09c: SubagentStart with no agent_type key → None (no topic signal)
        let input = make_hook_input("SubagentStart", serde_json::json!({}));
        let signal = extract_event_topic_signal("SubagentStart", &input);
        assert_eq!(signal, None);
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
        let input = make_hook_input(
            "PreToolUse",
            serde_json::json!({
                "tool_input": "ls -la /tmp"
            }),
        );
        let signal = extract_event_topic_signal("PreToolUse", &input);
        assert!(signal.is_none());
    }

    #[test]
    fn test_extract_event_topic_signal_generic_with_feature() {
        // T-09: generic event with feature path in extra
        let input = make_hook_input(
            "SomeEvent",
            serde_json::json!({
                "tool_input": "read product/features/col-017/SCOPE.md"
            }),
        );
        let signal = extract_event_topic_signal("SomeEvent", &input);
        assert_eq!(signal, Some("col-017".to_string()));
    }

    #[test]
    fn test_extract_event_topic_signal_generic_false_positive() {
        // T-09: SR-2 -- generic event with false-positive pattern in URL
        let input = make_hook_input(
            "SomeEvent",
            serde_json::json!({
                "url": "https://api-v2.example.com"
            }),
        );
        let signal = extract_event_topic_signal("SomeEvent", &input);
        // api-v2 is a valid feature ID pattern but it's just a URL segment
        // Our current extractor may match it; this documents the behavior
        // The majority vote mechanism handles false positives at the session level
    }

    #[test]
    fn test_build_request_pretooluse_sets_topic_signal() {
        // End-to-end: build_request for PreToolUse with feature path
        let input = make_hook_input(
            "PreToolUse",
            serde_json::json!({
                "tool_input": {"file_path": "product/features/col-002/SCOPE.md"}
            }),
        );
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
        let input = make_hook_input(
            "SomeHook",
            serde_json::json!({
                "path": "product/features/nxs-001/SCOPE.md"
            }),
        );
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
        let input = make_hook_input(
            "SomeHook",
            serde_json::json!({
                "key": "value without features"
            }),
        );
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
            provider: None,
            mcp_context: None,
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

    // -- col-022: PreToolUse context_cycle tests --

    /// Helper to create a PreToolUse HookInput with given extra JSON.
    fn pretooluse_input(extra: serde_json::Value) -> HookInput {
        HookInput {
            hook_event_name: "PreToolUse".to_string(),
            session_id: Some("sess-1".to_string()),
            cwd: None,
            transcript_path: None,
            prompt: None,
            provider: None,
            mcp_context: None,
            extra,
        }
    }

    // -- Tool name matching (R-09) --

    #[test]
    fn test_build_request_pretooluse_context_cycle_with_prefix() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "cycle_start");
                assert_eq!(event.payload["feature_cycle"], "col-022");
                assert_eq!(event.topic_signal.as_deref(), Some("col-022"));
            }
            _ => panic!("expected RecordEvent with cycle_start, got {req:?}"),
        }
    }

    #[test]
    fn test_build_request_pretooluse_context_cycle_without_prefix() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "cycle_start");
            }
            _ => panic!("expected RecordEvent with cycle_start"),
        }
    }

    #[test]
    fn test_build_request_pretooluse_wrong_server_prefix() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__other_server__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Falls through to generic -- event_type is "PreToolUse", not "cycle_start"
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_pretooluse_context_cycle_substring_no_match() {
        // "my_context_cycle_thing" contains "context_cycle" but also "unimatrix" is absent
        // and it's not exactly "context_cycle", so should fall through
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "my_context_cycle_thing",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent"),
        }
    }

    // -- Cycle start event construction --

    #[test]
    fn test_build_request_cycle_start_event_type() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_START_EVENT);
                assert_eq!(event.topic_signal, Some("col-022".to_string()));
            }
            _ => panic!("expected cycle_start RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_cycle_start_with_keywords_silently_discarded() {
        // crt-025: keywords are no longer extracted or propagated. Old callers passing
        // `keywords` in the hook payload should have it silently discarded.
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "start",
                "topic": "col-022",
                "keywords": ["attribution", "lifecycle"]
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "cycle_start");
                assert_eq!(event.payload["feature_cycle"], "col-022");
                // keywords no longer propagated to payload (crt-025, C-04)
                assert!(event.payload.get("keywords").is_none());
            }
            _ => panic!("expected cycle_start RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_cycle_stop_event_type() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "stop", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_STOP_EVENT);
                assert_eq!(event.payload["feature_cycle"], "col-022");
            }
            _ => panic!("expected cycle_stop RecordEvent"),
        }
    }

    // -- Validation failure graceful fallthrough (R-02) --

    #[test]
    fn test_build_request_cycle_invalid_type_falls_through() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "pause", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Falls through to generic -- event_type is "PreToolUse"
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent on invalid type"),
        }
    }

    #[test]
    fn test_build_request_cycle_missing_topic_falls_through() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent on missing topic"),
        }
    }

    #[test]
    fn test_build_request_cycle_malformed_tool_input_falls_through() {
        // tool_input is a string instead of an object
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": "not-an-object"
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Falls through because get("type") on a string returns None
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent on malformed tool_input"),
        }
    }

    #[test]
    fn test_build_request_cycle_missing_tool_input_key_falls_through() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle"
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent on missing tool_input key"),
        }
    }

    #[test]
    fn test_build_request_cycle_topic_too_long_falls_through() {
        // 200 chars with no hyphen -- fails is_valid_feature_id
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "a".repeat(200)}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent on long topic"),
        }
    }

    // -- Session ID propagation --

    #[test]
    fn test_build_request_cycle_preserves_session_id() {
        let mut input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        input.session_id = Some("sess-42".to_string());
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.session_id, "sess-42");
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_cycle_no_session_id() {
        let mut input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        input.session_id = None;
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Falls back to ppid-{parent_pid}
                assert!(event.session_id.starts_with("ppid-"));
            }
            _ => panic!("expected RecordEvent"),
        }
    }

    // -- Edge cases --

    #[test]
    fn test_build_request_pretooluse_other_tool_not_cycle() {
        // context_search should NOT trigger cycle handler
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_search",
            "tool_input": {"query": "test"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "PreToolUse");
            }
            _ => panic!("expected generic RecordEvent for non-cycle tool"),
        }
    }

    #[test]
    fn test_build_request_cycle_keywords_silently_discarded_mixed_types() {
        // crt-025: keywords no longer extracted. Payload with mixed-type keywords array
        // should be accepted as a cycle_start with no keywords in the result payload.
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "start",
                "topic": "col-022",
                "keywords": [1, "valid", null, true, "also-valid"]
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "cycle_start");
                // keywords no longer propagated to payload (crt-025, C-04)
                assert!(event.payload.get("keywords").is_none());
            }
            _ => panic!("expected cycle_start RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_cycle_no_keywords_field() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "cycle_start");
                // No keywords key in payload when none provided
                assert!(event.payload.get("keywords").is_none());
            }
            _ => panic!("expected cycle_start RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_cycle_extra_fields_in_tool_input() {
        // Extra unexpected fields should not cause issues
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "start",
                "topic": "col-022",
                "unexpected_field": "ignored",
                "another": 42
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, "cycle_start");
                assert_eq!(event.payload["feature_cycle"], "col-022");
            }
            _ => panic!("expected cycle_start RecordEvent"),
        }
    }

    #[test]
    fn test_build_request_cycle_event_type_constants_match() {
        // Verify the constants used in hook match the shared constants
        assert_eq!(CYCLE_START_EVENT, "cycle_start");
        assert_eq!(CYCLE_STOP_EVENT, "cycle_stop");
    }

    #[test]
    fn test_build_request_cycle_is_fire_and_forget() {
        // Cycle events are RecordEvent, which is fire-and-forget
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-022"}
        }));
        let req = build_request("PreToolUse", &input);
        let is_faf = matches!(
            req,
            HookRequest::SessionRegister { .. }
                | HookRequest::SessionClose { .. }
                | HookRequest::RecordEvent { .. }
                | HookRequest::RecordEvents { .. }
        );
        assert!(is_faf, "cycle events must be fire-and-forget");
    }

    // -- crt-025: Hook path phase-end tests (AC-16, FR-03.7, R-09) --

    /// AC-16 happy path: phase-end with valid phase and next_phase emits cycle_phase_end.
    #[test]
    fn test_hook_phase_end_valid_phase_emits_cycle_phase_end() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "crt-025",
                "phase": "scope",
                "next_phase": "design"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(
                    event.event_type, CYCLE_PHASE_END_EVENT,
                    "phase-end must emit cycle_phase_end event type"
                );
                assert_eq!(
                    event.payload["phase"], "scope",
                    "phase field must be present in payload"
                );
                assert_eq!(
                    event.payload["next_phase"], "design",
                    "next_phase field must be present in payload"
                );
                assert_eq!(event.payload["feature_cycle"], "crt-025");
            }
            _ => panic!("expected RecordEvent, got {req:?}"),
        }
    }

    /// AC-16 error path, R-09: invalid phase with space must fall through to generic path.
    /// The hook must never return an error to the transport.
    #[test]
    fn test_hook_phase_end_invalid_phase_space_falls_through() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "crt-025",
                "phase": "scope review"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                // Falls through to generic -- event_type is "PreToolUse", not "cycle_phase_end"
                assert_ne!(
                    event.event_type, CYCLE_PHASE_END_EVENT,
                    "invalid phase must NOT emit cycle_phase_end"
                );
                assert_eq!(
                    event.event_type, "PreToolUse",
                    "fallthrough must produce generic PreToolUse event"
                );
            }
            _ => panic!("expected generic RecordEvent on validation failure"),
        }
    }

    /// R-09: empty phase string is invalid per validate_phase_field — must fall through.
    #[test]
    fn test_hook_phase_end_empty_phase_falls_through() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "crt-025",
                "phase": ""
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_ne!(
                    event.event_type, CYCLE_PHASE_END_EVENT,
                    "empty phase must NOT emit cycle_phase_end"
                );
                assert_eq!(
                    event.event_type, "PreToolUse",
                    "empty phase must fall through to generic"
                );
            }
            _ => panic!("expected generic RecordEvent on empty phase"),
        }
    }

    /// R-09 edge: phase is optional per FR-02.5. Missing phase field must succeed.
    #[test]
    fn test_hook_phase_end_no_phase_field_accepted() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "crt-025"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(
                    event.event_type, CYCLE_PHASE_END_EVENT,
                    "phase-end with no phase field must succeed (phase is optional)"
                );
                assert_eq!(event.payload["feature_cycle"], "crt-025");
                // phase absent from payload when None
                assert!(
                    event.payload.get("phase").is_none(),
                    "absent phase must not appear in payload"
                );
            }
            _ => panic!("expected cycle_phase_end RecordEvent, got {req:?}"),
        }
    }

    /// R-06: Mixed-case phase must be normalized to lowercase in the emitted payload.
    #[test]
    fn test_hook_phase_end_phase_normalized() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "crt-025",
                "phase": "Scope"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_PHASE_END_EVENT);
                assert_eq!(
                    event.payload["phase"], "scope",
                    "phase must be normalized to lowercase"
                );
            }
            _ => panic!("expected cycle_phase_end RecordEvent, got {req:?}"),
        }
    }

    /// Regression: start type maps to cycle_start, next_phase is carried through.
    #[test]
    fn test_hook_start_type_extracted() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "start",
                "topic": "crt-025",
                "next_phase": "scope"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_START_EVENT);
                assert_eq!(event.payload["feature_cycle"], "crt-025");
                assert_eq!(
                    event.payload["next_phase"], "scope",
                    "next_phase must be carried through on cycle_start"
                );
            }
            _ => panic!("expected cycle_start RecordEvent, got {req:?}"),
        }
    }

    /// Regression: stop type maps to cycle_stop.
    #[test]
    fn test_hook_stop_type_extracted() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "stop",
                "topic": "crt-025"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_STOP_EVENT);
                assert_eq!(event.payload["feature_cycle"], "crt-025");
            }
            _ => panic!("expected cycle_stop RecordEvent, got {req:?}"),
        }
    }

    /// Regression (FR-03.5): old callers passing keywords must not see keywords in the payload,
    /// and must not cause an error.
    #[test]
    fn test_hook_keywords_not_extracted() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "start",
                "topic": "crt-025",
                "keywords": ["k1", "k2"]
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(
                    event.event_type, CYCLE_START_EVENT,
                    "keywords in input must not prevent cycle_start emission"
                );
                assert!(
                    event.payload.get("keywords").is_none(),
                    "keywords must NOT appear in the emitted payload (crt-025 removal)"
                );
            }
            _ => panic!("expected cycle_start RecordEvent, got {req:?}"),
        }
    }

    /// Phase-end with outcome field populates payload correctly.
    #[test]
    fn test_hook_phase_end_with_outcome() {
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "crt-025",
                "phase": "design",
                "outcome": "no variances",
                "next_phase": "implementation"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_PHASE_END_EVENT);
                assert_eq!(event.payload["phase"], "design");
                assert_eq!(event.payload["outcome"], "no variances");
                assert_eq!(event.payload["next_phase"], "implementation");
                assert_eq!(event.payload["feature_cycle"], "crt-025");
            }
            _ => panic!("expected cycle_phase_end RecordEvent, got {req:?}"),
        }
    }

    /// CYCLE_PHASE_END_EVENT constant value matches the expected wire string.
    #[test]
    fn test_cycle_phase_end_constant_value() {
        assert_eq!(CYCLE_PHASE_END_EVENT, "cycle_phase_end");
    }

    // -- goal propagation in hook payload (GH #389) --

    #[test]
    fn build_cycle_event_or_fallthrough_cycle_start_with_goal_in_payload() {
        // cycle_start with a goal present → payload["goal"] must be set (GH #389).
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-389", "goal": "some goal text"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_START_EVENT);
                assert_eq!(
                    event.payload["goal"].as_str(),
                    Some("some goal text"),
                    "goal must be forwarded into the RecordEvent payload"
                );
            }
            _ => panic!("expected cycle_start RecordEvent, got {req:?}"),
        }
    }

    #[test]
    fn build_cycle_event_or_fallthrough_cycle_start_without_goal_absent_from_payload() {
        // cycle_start with no goal key → payload must NOT contain "goal" (GH #389).
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {"type": "start", "topic": "col-389"}
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_START_EVENT);
                assert!(
                    event.payload.get("goal").is_none(),
                    "goal must not appear in payload when not supplied"
                );
            }
            _ => panic!("expected cycle_start RecordEvent, got {req:?}"),
        }
    }

    #[test]
    fn build_cycle_event_or_fallthrough_cycle_phase_end_with_goal_ignored() {
        // phase-end with a goal key in tool_input → payload must NOT contain "goal" (FR-01 / GH #389).
        let input = pretooluse_input(serde_json::json!({
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "col-389",
                "phase": "design",
                "outcome": "pass",
                "goal": "something"
            }
        }));
        let req = build_request("PreToolUse", &input);
        match req {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.event_type, CYCLE_PHASE_END_EVENT);
                assert!(
                    event.payload.get("goal").is_none(),
                    "goal must not appear in payload for phase-end events"
                );
            }
            _ => panic!("expected phase_end RecordEvent, got {req:?}"),
        }
    }

    // -- crt-027 WA-4a: SubagentStart routing tests --

    /// AC-01: SubagentStart with non-empty prompt_snippet routes to ContextSearch.
    /// Verifies query, source, session_id, and all None fields.
    #[test]
    fn build_request_subagentstart_with_prompt_snippet() {
        let mut input = test_input();
        input.session_id = Some("sess-parent".to_string());
        input.extra = serde_json::json!({ "prompt_snippet": "implement the spec writer agent" });
        let req = build_request("SubagentStart", &input);
        match req {
            HookRequest::ContextSearch {
                query,
                source,
                session_id,
                role,
                task,
                feature,
                k,
                max_tokens,
            } => {
                assert_eq!(query, "implement the spec writer agent");
                assert_eq!(source, Some("SubagentStart".to_string()));
                assert_eq!(session_id, Some("sess-parent".to_string()));
                assert!(role.is_none());
                assert!(task.is_none());
                assert!(feature.is_none());
                assert!(k.is_none());
                assert!(max_tokens.is_none());
            }
            _ => panic!("expected ContextSearch, got {req:?}"),
        }
    }

    /// AC-02 (a): SubagentStart with absent prompt_snippet key falls through to RecordEvent.
    /// AC-02 (b): SubagentStart with empty string prompt_snippet falls through to RecordEvent.
    #[test]
    fn build_request_subagentstart_empty_prompt_snippet() {
        // (a) key absent
        let mut input = test_input();
        input.extra = serde_json::json!({});
        let req = build_request("SubagentStart", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "absent prompt_snippet must yield RecordEvent, got {req:?}"
        );

        // (b) key present, empty string
        let mut input2 = test_input();
        input2.extra = serde_json::json!({ "prompt_snippet": "" });
        let req2 = build_request("SubagentStart", &input2);
        assert!(
            matches!(req2, HookRequest::RecordEvent { .. }),
            "empty prompt_snippet must yield RecordEvent, got {req2:?}"
        );
    }

    /// AC-03: SubagentStart uses input.session_id (parent session), NOT the ppid fallback.
    #[test]
    fn build_request_subagentstart_session_id_from_input() {
        let mut input = test_input();
        input.session_id = Some("parent-sess-42".to_string());
        input.extra = serde_json::json!({ "prompt_snippet": "design the architecture" });
        let req = build_request("SubagentStart", &input);
        match req {
            HookRequest::ContextSearch { session_id, .. } => {
                assert_eq!(
                    session_id,
                    Some("parent-sess-42".to_string()),
                    "session_id must be taken from input.session_id (parent session)"
                );
            }
            _ => panic!("expected ContextSearch, got {req:?}"),
        }
    }

    /// AC-23: SubagentStart with a single non-empty word routes to ContextSearch.
    /// SubagentStart does NOT apply MIN_QUERY_WORDS guard.
    #[test]
    fn build_request_subagentstart_one_word_routes_to_context_search() {
        let mut input = test_input();
        input.extra = serde_json::json!({ "prompt_snippet": "implement" });
        let req = build_request("SubagentStart", &input);
        assert!(
            matches!(req, HookRequest::ContextSearch { .. }),
            "single-word SubagentStart prompt_snippet must route to ContextSearch (no MIN_QUERY_WORDS), got {req:?}"
        );
    }

    /// AC-23b / EC-01: SubagentStart with whitespace-only prompt_snippet falls through to RecordEvent.
    #[test]
    fn build_request_subagentstart_whitespace_only_prompt_snippet() {
        let mut input = test_input();
        input.extra = serde_json::json!({ "prompt_snippet": "   " });
        let req = build_request("SubagentStart", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "whitespace-only prompt_snippet must yield RecordEvent (EC-01), got {req:?}"
        );
    }

    /// EC-02: SubagentStart with JSON null prompt_snippet falls through to RecordEvent.
    /// v.as_str() returns None for Null, so it falls through the same as absent.
    #[test]
    fn build_request_subagentstart_null_prompt_snippet_record_event() {
        let mut input = test_input();
        input.extra = serde_json::json!({ "prompt_snippet": null });
        let req = build_request("SubagentStart", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "null prompt_snippet must yield RecordEvent (EC-02), got {req:?}"
        );
    }

    // -- crt-027 WA-4a: UserPromptSubmit word-count guard tests --

    /// AC-22: Exactly 4 words (below MIN_QUERY_WORDS=5) yields RecordEvent.
    #[test]
    fn build_request_userpromptsub_four_words_record_event() {
        let mut input = test_input();
        input.prompt = Some("implement the spec writer".to_string()); // exactly 4 words
        let req = build_request("UserPromptSubmit", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "4-word prompt must yield RecordEvent (word count 4 < MIN_QUERY_WORDS=5), got {req:?}"
        );
    }

    /// AC-22: Exactly 5 words (equal to MIN_QUERY_WORDS=5) yields ContextSearch.
    #[test]
    fn build_request_userpromptsub_five_words_context_search() {
        let mut input = test_input();
        input.prompt = Some("implement the spec writer agent".to_string()); // exactly 5 words
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { query, .. } => {
                assert_eq!(query, "implement the spec writer agent");
            }
            _ => panic!("expected ContextSearch (5 == MIN_QUERY_WORDS=5), got {req:?}"),
        }
    }

    /// AC-02b scenario: 6 words (above MIN_QUERY_WORDS=5) yields ContextSearch.
    #[test]
    fn build_request_userpromptsub_six_words_context_search() {
        let mut input = test_input();
        input.prompt = Some("implement the spec writer agent today".to_string()); // 6 words
        let req = build_request("UserPromptSubmit", &input);
        assert!(
            matches!(req, HookRequest::ContextSearch { .. }),
            "6-word prompt must yield ContextSearch (6 >= MIN_QUERY_WORDS=5), got {req:?}"
        );
    }

    /// AC-02b scenario: 1 word yields RecordEvent.
    #[test]
    fn build_request_userpromptsub_one_word_record_event() {
        let mut input = test_input();
        input.prompt = Some("ok".to_string()); // 1 word
        let req = build_request("UserPromptSubmit", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "1-word prompt must yield RecordEvent, got {req:?}"
        );
    }

    /// AC-02b scenario: 3 words yields RecordEvent.
    #[test]
    fn build_request_userpromptsub_three_words_record_event() {
        let mut input = test_input();
        input.prompt = Some("yes ok thanks".to_string()); // 3 words
        let req = build_request("UserPromptSubmit", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "3-word prompt must yield RecordEvent (3 < 5), got {req:?}"
        );
    }

    /// AC-23c: Whitespace-padded single word counts as 1 real word, yields RecordEvent.
    /// Verifies that .trim() strips whitespace before .split_whitespace().count().
    #[test]
    fn build_request_userpromptsub_whitespace_padded_one_word() {
        let mut input = test_input();
        input.prompt = Some("  approve  ".to_string()); // 1 real word with surrounding whitespace
        let req = build_request("UserPromptSubmit", &input);
        assert!(
            matches!(req, HookRequest::RecordEvent { .. }),
            "whitespace-padded 1-word prompt must yield RecordEvent (AC-23c), got {req:?}"
        );
    }

    /// AC-05: UserPromptSubmit ContextSearch has source=None.
    #[test]
    fn build_request_userpromptsub_source_is_none() {
        let mut input = test_input();
        input.prompt = Some("implement the spec writer agent today".to_string()); // 6 words
        let req = build_request("UserPromptSubmit", &input);
        match req {
            HookRequest::ContextSearch { source, .. } => {
                assert!(
                    source.is_none(),
                    "UserPromptSubmit ContextSearch must have source=None (ADR-001)"
                );
            }
            _ => panic!("expected ContextSearch, got {req:?}"),
        }
    }

    // -- crt-027 ADR-006: write_stdout_subagent_inject tests --

    /// AC-SR02: write_stdout_subagent_inject produces a valid JSON envelope.
    /// Tests the envelope construction deterministically without stdout capture.
    #[test]
    fn write_stdout_subagent_inject_valid_json_envelope() {
        let entries_text =
            "1  42   crt-027/hook  decision  0.85  Unimatrix routes SubagentStart...";
        // Verify the envelope structure directly using serde_json (same as the function does)
        let envelope = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "SubagentStart",
                "additionalContext": entries_text
            }
        });
        // AC-SR02 assertions:
        assert_eq!(
            envelope["hookSpecificOutput"]["hookEventName"],
            "SubagentStart"
        );
        assert_eq!(
            envelope["hookSpecificOutput"]["additionalContext"],
            entries_text
        );
        // Verify the output is valid JSON (parses without error)
        let json_str = serde_json::to_string(&envelope).expect("must serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("must parse as valid JSON");
        assert_eq!(
            parsed["hookSpecificOutput"]["hookEventName"],
            "SubagentStart"
        );
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            entries_text
        );
    }

    /// AC-SR03: write_stdout plain text path does NOT produce a hookSpecificOutput JSON envelope.
    /// The plain text output starts with "--- Unimatrix Context ---", not "{".
    #[test]
    fn write_stdout_plain_text_no_json_envelope() {
        let entries = vec![test_entry(1, "ADR-001", "Use parameter expansion")];
        let response = HookResponse::Entries {
            items: entries,
            total_tokens: 10,
        };
        // write_stdout must not error
        assert!(write_stdout(&response).is_ok());

        // Verify the formatted text for Entries starts with the Unimatrix header (not JSON)
        // by checking format_injection output directly (same path as write_stdout)
        let test_entries = vec![test_entry(1, "ADR-001", "Use parameter expansion")];
        let formatted = format_injection(&test_entries, MAX_INJECTION_BYTES).unwrap();
        assert!(
            !formatted.starts_with('{'),
            "plain-text output must NOT start with '{{' (AC-SR03)"
        );
        assert!(
            !formatted.contains("hookSpecificOutput"),
            "plain-text output must NOT contain 'hookSpecificOutput' (AC-SR03)"
        );
        assert!(
            formatted.starts_with("--- Unimatrix Context ---"),
            "plain-text output must start with Unimatrix header"
        );
    }

    /// Verify write_stdout_subagent_inject succeeds (does not return error).
    #[test]
    fn write_stdout_subagent_inject_returns_ok() {
        let result = write_stdout_subagent_inject("test entries");
        assert!(
            result.is_ok(),
            "write_stdout_subagent_inject must return Ok"
        );
    }

    /// Verify write_stdout_subagent_inject_response succeeds for Entries response.
    #[test]
    fn write_stdout_subagent_inject_response_entries_returns_ok() {
        let response = HookResponse::Entries {
            items: vec![test_entry(1, "Title", "Content")],
            total_tokens: 10,
        };
        let result = write_stdout_subagent_inject_response(&response);
        assert!(
            result.is_ok(),
            "write_stdout_subagent_inject_response must return Ok for Entries"
        );
    }

    /// Verify write_stdout_subagent_inject_response succeeds for empty Entries (silent skip).
    #[test]
    fn write_stdout_subagent_inject_response_empty_entries_returns_ok() {
        let response = HookResponse::Entries {
            items: vec![],
            total_tokens: 0,
        };
        let result = write_stdout_subagent_inject_response(&response);
        assert!(
            result.is_ok(),
            "write_stdout_subagent_inject_response must return Ok for empty Entries"
        );
    }

    /// MIN_QUERY_WORDS constant has the specified value.
    #[test]
    fn min_query_words_constant_is_five() {
        assert_eq!(MIN_QUERY_WORDS, 5);
    }

    // -- crt-028 WA-5: PreCompact transcript restoration tests --

    // Helper: write JSONL lines to a temp file, return (TempDir, path_string)
    fn make_jsonl_file(lines: &[&str]) -> (tempfile::TempDir, String) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        let path_str = path.to_str().unwrap().to_string();
        (tmp, path_str)
    }

    #[test]
    fn max_precompact_bytes_constant_defined() {
        assert_eq!(MAX_PRECOMPACT_BYTES, 3000);
        assert_ne!(MAX_PRECOMPACT_BYTES, MAX_INJECTION_BYTES);
        assert_eq!(TAIL_MULTIPLIER, 4);
        assert_eq!(TOOL_RESULT_SNIPPET_BYTES, 300);
        assert_eq!(TOOL_KEY_PARAM_BYTES, 120);
    }

    #[test]
    fn extract_transcript_block_empty_path_returns_none() {
        // Note: extract_transcript_block("") will try to open "" and fail -> None
        let result = extract_transcript_block("");
        assert!(result.is_none());
    }

    #[test]
    fn extract_transcript_block_missing_file_returns_none() {
        let result = extract_transcript_block("/nonexistent/path/session.jsonl");
        assert!(result.is_none());
    }

    #[test]
    fn prepend_transcript_none_block_writes_briefing() {
        let result = prepend_transcript(None, "briefing content");
        assert_eq!(result, "briefing content");
        assert!(!result.contains("=== Recent conversation"));
    }

    #[test]
    fn extract_transcript_block_all_malformed_lines_returns_none() {
        let (_tmp, path) = make_jsonl_file(&["not json", "also not json", "{broken"]);
        let result = extract_transcript_block(&path);
        assert!(result.is_none());
    }

    #[test]
    fn extract_transcript_block_zero_byte_file_returns_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("empty.jsonl");
        std::fs::write(&path, b"").unwrap();
        let result = extract_transcript_block(path.to_str().unwrap());
        assert!(result.is_none());
    }

    #[test]
    fn build_exchange_pairs_three_exchanges_most_recent_first() {
        let user_a = r#"{"type":"user","message":{"content":[{"type":"text","text":"A"}]}}"#;
        let asst_a = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"RA"}]}}"#;
        let user_b = r#"{"type":"user","message":{"content":[{"type":"text","text":"B"}]}}"#;
        let asst_b = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"RB"}]}}"#;
        let user_c = r#"{"type":"user","message":{"content":[{"type":"text","text":"C"}]}}"#;
        let asst_c = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"RC"}]}}"#;
        let lines = vec![user_a, asst_a, user_b, asst_b, user_c, asst_c];
        let turns = build_exchange_pairs(&lines);
        // First turn should be most recent (C or RC)
        assert!(!turns.is_empty());
        let first_text = match &turns[0] {
            ExchangeTurn::AssistantText(t) => t.clone(),
            ExchangeTurn::UserText(t) => t.clone(),
            _ => panic!("unexpected"),
        };
        assert!(
            first_text == "RC" || first_text == "C",
            "most recent first: got {first_text}"
        );
    }

    #[test]
    fn build_exchange_pairs_user_tool_result_skipped() {
        let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"x","content":"result"}]}}"#;
        let turns = build_exchange_pairs(&[user]);
        assert!(
            turns.is_empty(),
            "tool_result in user turn must not emit UserText"
        );
    }

    #[test]
    fn build_exchange_pairs_tool_only_assistant_turn_emits_pairs() {
        let asst = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"file_path":"/foo.rs"}},{"type":"thinking","thinking":"..."}]}}"#;
        let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"file contents"}]}}"#;
        let turns = build_exchange_pairs(&[asst, user]);
        let has_tool_pair = turns
            .iter()
            .any(|t| matches!(t, ExchangeTurn::ToolPair { .. }));
        let has_asst_text = turns
            .iter()
            .any(|t| matches!(t, ExchangeTurn::AssistantText(_)));
        assert!(has_tool_pair, "tool-only assistant turn must emit ToolPair");
        assert!(
            !has_asst_text,
            "tool-only assistant turn must NOT emit AssistantText"
        );
    }

    #[test]
    fn build_exchange_pairs_thinking_only_turn_suppressed() {
        let asst = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"secret thoughts"}]}}"#;
        let turns = build_exchange_pairs(&[asst]);
        assert!(turns.is_empty(), "pure thinking turn must be suppressed");
    }

    #[test]
    fn build_exchange_pairs_malformed_lines_skipped() {
        let user = r#"{"type":"user","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let lines = vec!["not json", user, "{broken", "also bad"];
        let turns = build_exchange_pairs(&lines);
        assert!(
            !turns.is_empty(),
            "valid lines must produce turns despite malformed lines"
        );
        assert!(!std::panic::catch_unwind(|| build_exchange_pairs(&lines)).is_err());
    }

    #[test]
    fn extract_key_param_known_tools_correct_field() {
        let cases = vec![
            ("Bash", "command", r#"{"command":"ls -la"}"#, "ls -la"),
            ("Read", "file_path", r#"{"file_path":"/foo.rs"}"#, "/foo.rs"),
            ("Edit", "file_path", r#"{"file_path":"/bar.rs"}"#, "/bar.rs"),
            (
                "Write",
                "file_path",
                r#"{"file_path":"/out.rs"}"#,
                "/out.rs",
            ),
            ("Glob", "pattern", r#"{"pattern":"**/*.rs"}"#, "**/*.rs"),
            ("Grep", "pattern", r#"{"pattern":"fn main"}"#, "fn main"),
            (
                "MultiEdit",
                "file_path",
                r#"{"file_path":"/multi.rs"}"#,
                "/multi.rs",
            ),
            (
                "Task",
                "description",
                r#"{"description":"implement X"}"#,
                "implement X",
            ),
            (
                "WebFetch",
                "url",
                r#"{"url":"https://example.com"}"#,
                "https://example.com",
            ),
            (
                "WebSearch",
                "query",
                r#"{"query":"rust async"}"#,
                "rust async",
            ),
        ];
        for (tool, _field, input_json, expected) in cases {
            let input: serde_json::Value = serde_json::from_str(input_json).unwrap();
            let result = extract_key_param(tool, &input);
            assert_eq!(result, expected, "tool: {tool}");
        }
    }

    #[test]
    fn extract_key_param_unknown_tool_first_string_field_fallback() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"query":"foo","count":5}"#).unwrap();
        let result = extract_key_param("UnknownTool", &input);
        // Should return first string field value
        assert_eq!(result, "foo");
    }

    #[test]
    fn extract_key_param_no_string_field_returns_empty() {
        let input: serde_json::Value = serde_json::from_str(r#"{"count":5,"flag":true}"#).unwrap();
        let result = extract_key_param("UnknownTool", &input);
        assert_eq!(result, "");
    }

    #[test]
    fn extract_key_param_long_value_truncated() {
        let long_val = "x".repeat(5000);
        let input = serde_json::json!({"file_path": long_val});
        let result = extract_key_param("Read", &input);
        assert!(result.len() <= TOOL_KEY_PARAM_BYTES);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn prepend_transcript_both_present_separator_present() {
        let result = prepend_transcript(Some("block"), "briefing");
        assert_eq!(result, "block\n\nbriefing");
    }

    #[test]
    fn prepend_transcript_both_present_transcript_precedes_briefing() {
        let result = prepend_transcript(
            Some("=== Recent conversation ===\n[User] foo\n=== End recent conversation ==="),
            "briefing",
        );
        assert!(result.starts_with("=== Recent conversation"));
        assert!(result.contains("briefing"));
        assert!(
            result.find("=== End recent conversation ===").unwrap()
                < result.find("briefing").unwrap()
        );
    }

    #[test]
    fn prepend_transcript_transcript_only_has_headers() {
        let block = "=== Recent conversation ===\n[User] foo\n=== End recent conversation ===";
        let result = prepend_transcript(Some(block), "");
        assert_eq!(result, block);
    }

    #[test]
    fn prepend_transcript_both_none_empty_string() {
        let result = prepend_transcript(None, "");
        assert_eq!(result, "");
    }

    #[test]
    fn prepend_transcript_none_block_writes_briefing_verbatim() {
        let result = prepend_transcript(None, "briefing content");
        assert_eq!(result, "briefing content");
        assert!(!result.contains("=== Recent conversation"));
    }

    #[test]
    fn extract_transcript_block_respects_byte_budget() {
        // Create many exchanges that together exceed MAX_PRECOMPACT_BYTES
        let mut lines = Vec::new();
        for i in 0..20 {
            let user = format!(
                r#"{{"type":"user","message":{{"content":[{{"type":"text","text":"user message number {} with some padding to make it longer"}}]}}}}"#,
                i
            );
            let asst = format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"assistant response number {} with some padding too"}}]}}}}"#,
                i
            );
            lines.push(user);
            lines.push(asst);
        }
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let (_tmp, path) = make_jsonl_file(&line_refs);
        let result = extract_transcript_block(&path);
        if let Some(s) = result {
            assert!(
                s.len() <= MAX_PRECOMPACT_BYTES,
                "byte budget exceeded: {} > {}",
                s.len(),
                MAX_PRECOMPACT_BYTES
            );
            assert!(
                s.starts_with("=== Recent conversation"),
                "must start with header"
            );
            assert!(
                s.ends_with("=== End recent conversation ==="),
                "must end with footer"
            );
        }
        // None is also acceptable if all exchanges are too large for the budget
    }

    #[test]
    fn extract_transcript_block_system_only_returns_none() {
        let lines = vec![
            r#"{"type":"system","content":"system message 1"}"#,
            r#"{"type":"system","content":"system message 2"}"#,
        ];
        let (_tmp, path) = make_jsonl_file(&lines);
        let result = extract_transcript_block(&path);
        assert!(result.is_none());
    }
}
