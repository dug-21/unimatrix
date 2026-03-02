# Pseudocode: hook-subcommand

## Purpose

Add a `hook` subcommand to the `unimatrix-server` binary that reads Claude Code hook JSON from stdin, connects to the running server via UDS, and dispatches events. Uses synchronous std I/O only (no tokio) per ADR-002. Lives in `unimatrix-server/src/hook.rs` with integration into `main.rs`.

## File: crates/unimatrix-server/src/main.rs (modifications)

### CLI Extension

```
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "unimatrix-server", about = "Unimatrix MCP knowledge server")]
struct Cli {
    #[arg(long)]
    project_dir: Option<PathBuf>,

    #[arg(long, short)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Handle a Claude Code lifecycle hook event
    Hook {
        /// The hook event name (e.g., SessionStart, Stop, Ping)
        event: String,
    },
}
```

### Early Branch in main()

```
fn main() -> Result<(), Box<dyn std::error::Error>>:
    let cli = Cli::parse()

    match cli.command:
        Some(Command::Hook { event }) =>
            // Sync path: NO tokio, NO tracing init, NO database open
            // Minimal startup for <50ms budget
            hook::run(event, cli.project_dir)

        None =>
            // Async path: full server with tokio runtime
            tokio_main(cli)

#[tokio::main]
async fn tokio_main(cli: Cli) -> Result<(), Box<dyn std::error::Error>>:
    // ... existing server startup code (moved from current main) ...
```

**Critical**: The `fn main()` is no longer `#[tokio::main]`. The tokio runtime is only initialized for the server path. The hook path runs pure synchronous code. This saves ~1-3ms (R-18, ADR-002).

## File: crates/unimatrix-server/src/hook.rs

### Entry Point

```
fn run(event: String, project_dir: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>>:
    // Step 1: Read stdin
    let stdin_content = read_stdin()

    // Step 2: Parse hook input (defensive -- ADR-006)
    let hook_input = parse_hook_input(&stdin_content)

    // Step 3: Determine working directory and compute project hash
    let cwd = resolve_cwd(&hook_input, project_dir.as_deref())
    let project_hash = compute_project_hash(&cwd)

    // Step 4: Compute socket path
    let home = dirs::home_dir()
        .ok_or("home directory not found")?
    let socket_path = home
        .join(".unimatrix")
        .join(&project_hash)
        .join("unimatrix.sock")

    // Step 5: Construct request from event + input
    let request = build_request(&event, &hook_input)

    // Step 6: Determine if fire-and-forget or synchronous
    let is_fire_and_forget = matches!(
        request,
        HookRequest::SessionRegister { .. }
        | HookRequest::SessionClose { .. }
        | HookRequest::RecordEvent(_)
        | HookRequest::RecordEvents(_)
    )

    // Step 7: Connect and send
    let timeout = Duration::from_millis(40)  // 40ms, 10ms margin for startup
    let mut transport = LocalTransport::new(socket_path.clone(), timeout)

    match transport.connect():
        Ok(()) =>
            // Connected to server
            // Try to replay any queued events first (best-effort)
            let queue = EventQueue::new(queue_dir(&home, &project_hash))
            let _ = queue.replay(&mut transport)

            if is_fire_and_forget:
                transport.fire_and_forget(&request)?
            else:
                let response = transport.request(&request, timeout)?
                // Write response to stdout for synchronous hooks
                write_stdout(&response)?

        Err(TransportError::Unavailable(_)) =>
            // Server not running -- graceful degradation
            if is_fire_and_forget:
                // Queue the event for later replay
                let queue = EventQueue::new(queue_dir(&home, &project_hash))
                queue.enqueue(&request)?
                // Log to stderr for diagnostics
                eprintln!("unimatrix: server unavailable, event queued")
            // else: synchronous query with no server -> produce no output, exit 0

        Err(e) =>
            // Other transport errors -- log and exit 0
            eprintln!("unimatrix: transport error: {e}")

    // Always exit 0 (FR-03.7)
    Ok(())
```

### Read Stdin

```
fn read_stdin() -> String:
    let mut input = String::new()
    // Read all of stdin. If nothing is piped, this returns immediately with empty string.
    let _ = io::stdin().read_to_string(&mut input)
    input
```

### Parse Hook Input (ADR-006)

```
fn parse_hook_input(raw: &str) -> HookInput:
    match serde_json::from_str::<HookInput>(raw):
        Ok(input) => input
        Err(e) =>
            eprintln!("unimatrix: stdin parse error: {e}")
            // Return default with empty fields -- graceful degradation
            HookInput {
                hook_event_name: String::new(),
                session_id: None,
                cwd: None,
                transcript_path: None,
                extra: serde_json::Value::Null,
            }
```

### Resolve CWD

```
fn resolve_cwd(input: &HookInput, project_dir: Option<&Path>) -> PathBuf:
    // Priority: --project-dir flag > cwd from stdin > process cwd
    if let Some(dir) = project_dir:
        return dir.to_path_buf()

    if let Some(cwd) = &input.cwd:
        return PathBuf::from(cwd)

    // Fallback to process working directory
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
```

### Build Request

```
fn build_request(event: &str, input: &HookInput) -> HookRequest:
    // Resolve session_id with fallback to parent PID
    let session_id = input.session_id
        .clone()
        .unwrap_or_else(|| format!("ppid-{}", std::os::unix::process::parent_id()))

    let cwd = input.cwd
        .clone()
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        })

    match event:
        "SessionStart" =>
            HookRequest::SessionRegister {
                session_id,
                cwd,
                agent_role: None,  // Not available from stdin in col-006
                feature: None,     // Not available from stdin in col-006
            }

        "Stop" =>
            HookRequest::SessionClose {
                session_id,
                outcome: None,
                duration_secs: 0,  // Not tracked in col-006
            }

        "Ping" =>
            HookRequest::Ping

        _ =>
            // Unknown event -- record as generic event
            HookRequest::RecordEvent(ImplantEvent {
                event_type: event.to_string(),
                session_id,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                payload: input.extra.clone(),
            })
```

### Write Stdout

```
fn write_stdout(response: &HookResponse) -> Result<(), Box<dyn std::error::Error>>:
    let json = serde_json::to_string(response)?
    println!("{json}")
    Ok(())
```

### Queue Directory Helper

```
fn queue_dir(home: &Path, project_hash: &str) -> PathBuf:
    home.join(".unimatrix")
        .join(project_hash)
        .join("event-queue")
```

## Bootstrap: cortical-implant Agent

In `crates/unimatrix-server/src/registry.rs`, extend `bootstrap_defaults()`:

```
// After existing "system" and "human" agent bootstraps:
self.resolve_or_enroll(
    "cortical-implant",
    TrustLevel::Internal,
    &[Capability::Read, Capability::Search],
)?;
```

The `resolve_or_enroll` already handles idempotency -- if the agent exists, it is not modified (R-20).

## Design Notes

1. **No tokio in hook path**: The `main()` function branches before any tokio initialization. The hook path uses only synchronous std I/O. This is the critical performance decision (ADR-002, R-18).

2. **Exit code always 0**: Per FR-03.7, the hook never exits non-zero for expected failures (server down, timeout, parse failure). Only truly unexpected errors (e.g., panic) would exit non-zero.

3. **Session identity fallback**: When Claude Code does not provide `session_id`, the hook uses `ppid-{parent_pid}` as a proxy. This groups events by the spawning Claude Code process.

4. **Event routing**: The `event` CLI argument determines which `HookRequest` variant to construct. Known events (SessionStart, Stop, Ping) map to specific variants. Unknown events become `RecordEvent` for generic telemetry.

5. **Queue replay on connect**: When the hook successfully connects to the server, it first attempts to replay any queued events from previous disconnected sessions. This is best-effort and does not block the current request.

6. **No tracing init**: The hook path does not call `tracing_subscriber::init()`. Diagnostic output goes directly to stderr via `eprintln!`. This saves a few ms of subscriber setup.

## Error Handling

- stdin read failure -> empty string, treated as parse failure
- stdin parse failure -> default HookInput, graceful degradation
- Socket not found -> queue fire-and-forget events, skip sync queries
- Transport error -> log to stderr, exit 0
- Response write failure -> log to stderr, exit 0
- All errors exit 0 (never block the user's workflow)

## Key Test Scenarios

1. `hook SessionStart` with valid stdin JSON -> connects to server, sends SessionRegister
2. `hook Stop` with valid stdin -> sends SessionClose
3. `hook Ping` -> sends Ping, receives Pong, writes to stdout
4. `hook SessionStart` with no running server -> queues event, exits 0
5. `hook Ping` with no running server -> no output, exits 0 (sync query skipped)
6. Empty stdin -> parse fails gracefully, exits 0
7. Invalid JSON stdin -> parse fails gracefully, exits 0
8. Stdin with unknown fields -> parsed without error (serde(flatten))
9. Stdin with missing session_id -> falls back to ppid-{parent_pid}
10. Unknown event name -> RecordEvent variant
11. No tokio symbols imported in hook.rs (static analysis check for R-18)
12. Latency benchmark: Ping round-trip < 50ms (p95 over 10 iterations)
