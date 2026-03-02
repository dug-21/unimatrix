# col-006 Pseudocode Overview

## Components and Build Order

| Wave | Component | Crate | Dependencies |
|------|-----------|-------|-------------|
| 1 | engine-extraction | unimatrix-engine | None (moves existing code) |
| 1 | wire-protocol | unimatrix-engine | None (new types) |
| 2 | transport | unimatrix-engine | wire-protocol |
| 2 | authentication | unimatrix-engine | None |
| 2 | event-queue | unimatrix-engine | wire-protocol |
| 3 | uds-listener | unimatrix-server | wire-protocol, transport, authentication |
| 3 | hook-subcommand | unimatrix-server | wire-protocol, transport, event-queue |

## Data Flow

```
Claude Code
  |-- spawns hook process
  |-- pipes JSON to stdin
  v
hook-subcommand (sync, no tokio)
  |-- reads stdin -> HookInput (defensive serde)
  |-- computes project hash -> socket_path
  |-- LocalTransport::connect(socket_path)
  |     |-- on failure: EventQueue::enqueue() for fire-and-forget, skip for sync
  |-- LocalTransport::request(HookRequest) or fire_and_forget(HookRequest)
  |     |-- write_frame(4-byte BE u32 + JSON)
  |     |-- read_frame() -> HookResponse
  |-- stdout: response payload (sync) or nothing (fire-and-forget)
  |-- exit 0

uds-listener (async, tokio task per connection)
  |-- UnixListener::accept()
  |-- authenticate_connection(stream) -> PeerCredentials
  |     |-- Layer 1: socket mode 0o600 (OS enforced)
  |     |-- Layer 2: extract_peer_credentials(stream) -> UID match check
  |     |-- Layer 3: is_unimatrix_process(pid) on Linux (advisory)
  |-- read_frame() -> HookRequest
  |-- dispatch request:
  |     |-- Ping -> Pong { server_version }
  |     |-- SessionRegister -> log, Ack
  |     |-- SessionClose -> log, Ack
  |     |-- RecordEvent -> log, Ack
  |-- write_frame(HookResponse)
  |-- close connection
```

## Shared Types (defined in wire-protocol component)

- `HookInput` -- Claude Code stdin JSON, defensive serde
- `HookRequest` -- tagged enum: Ping, SessionRegister, SessionClose, RecordEvent, RecordEvents, + stubs
- `HookResponse` -- tagged enum: Pong, Ack, Error, + stubs
- `ImplantEvent` -- telemetry event payload
- `TransportError` -- 5-variant error enum
- `PeerCredentials` -- platform-abstracted peer identity
- `SocketGuard` -- RAII socket file cleanup
- `EntryPayload` -- stub for future search results

## Cross-Component Interactions

1. **engine-extraction** moves project.rs, confidence.rs, coaccess.rs to unimatrix-engine. Server re-exports via `pub use`. ProjectPaths gains `socket_path` field.

2. **wire-protocol** defines types consumed by transport (serialization), uds-listener (deserialization/dispatch), hook-subcommand (construction), and event-queue (JSONL serialization).

3. **transport** uses wire-protocol types for request/response framing. Used by hook-subcommand for UDS communication and by event-queue for replay.

4. **authentication** is called by uds-listener on each accepted connection. Reuses `is_unimatrix_process` pattern from pidfile.rs.

5. **event-queue** serializes HookRequest to JSONL. Used by hook-subcommand when server is unavailable.

6. **uds-listener** integrates into server startup/shutdown via LifecycleHandles extension. Shares Arc<Store> and other resources with MCP handlers.

7. **hook-subcommand** extends main.rs with early branch before tokio init. Uses project.rs for instance discovery.

## Sequencing Constraints

- engine-extraction MUST complete first (all tests must pass after extraction)
- wire-protocol has no runtime dependencies, can parallel with extraction
- transport, authentication, event-queue depend on wire types
- uds-listener and hook-subcommand depend on all Wave 1-2 components
- main.rs modifications are the final integration point
