# Pseudocode Overview: col-007 Automatic Context Injection

## Components

| Component | Location | Role |
|-----------|----------|------|
| hook-handler | `crates/unimatrix-server/src/hook.rs` | Client-side: build ContextSearch request, handle Entries response |
| injection-format | `crates/unimatrix-server/src/hook.rs` | Client-side: format EntryPayload vec as bounded plain text |
| uds-dispatch | `crates/unimatrix-server/src/uds_listener.rs` | Server-side: async dispatch, ContextSearch pipeline, CoAccessDedup |
| session-warming | `crates/unimatrix-server/src/uds_listener.rs` | Server-side: ONNX pre-warm on SessionRegister |

Wire protocol changes in `crates/unimatrix-engine/src/wire.rs` are shared across components.

## Data Flow

```
stdin JSON -> parse_hook_input() -> build_request("UserPromptSubmit", input)
  -> HookRequest::ContextSearch { query: input.prompt }
  -> LocalTransport.request() -> UDS -> dispatch_request()
  -> embed query -> adapt -> L2 normalize -> HNSW search -> fetch entries
  -> re-rank (0.85*sim + 0.15*conf) -> co-access boost -> filter floors
  -> HookResponse::Entries { items, total_tokens }
  -> format_injection(items, MAX_INJECTION_BYTES) -> Option<String>
  -> print to stdout (or silent skip if None)
```

## Wire Protocol Changes (shared)

Remove `#[allow(dead_code)]` from: `ContextSearch`, `Entries`, `EntryPayload`.

Add to `HookInput`:
```
pub prompt: Option<String>,  // #[serde(default)]
```

Add to `parse_hook_input()` fallback:
```
prompt: None,
```

## Shared Constants

| Constant | Value | Location |
|----------|-------|----------|
| `MAX_INJECTION_BYTES` | 1400 | hook.rs |
| `SIMILARITY_FLOOR` | 0.5 | uds_listener.rs |
| `CONFIDENCE_FLOOR` | 0.3 | uds_listener.rs |
| `INJECTION_K` | 5 | uds_listener.rs |
| `EF_SEARCH` | 32 | uds_listener.rs |

## Build Order

1. Wire protocol changes (no runtime dependencies)
2. hook-handler + injection-format (client-side, can parallelize with 3)
3. uds-dispatch + session-warming (server-side)
4. main.rs integration (pass Arcs to start_uds_listener)
