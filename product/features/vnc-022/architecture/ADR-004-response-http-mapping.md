## ADR-004: HookResponse to HTTP Status Code Mapping

### Context

`dispatch_request` returns `HookResponse` — an enum with 5 variants. The `/observe` HTTP handler must map these to HTTP status codes and response bodies. The mapping must be unambiguous so clients can reliably determine success/failure without parsing the body.

The existing wire format uses `#[serde(tag = "type")]` JSON discrimination. The HTTP response body should use the same serde serialization for content-bearing responses, preserving wire format compatibility between UDS and HTTP.

Options:
- **(A) Always 200 with body discrimination**: Simple but violates HTTP semantics. Clients cannot use status codes for error handling.
- **(B) Semantic mapping**: Ack->204, content->200, Error->400. Follows REST conventions. Fire-and-forget callers can check `status == 204` without parsing.
- **(C) Fine-grained error codes**: Map `Error.code` to specific HTTP statuses (e.g., -32003->403, -32602->422). Over-engineering — the Error variant already carries a structured code+message.

### Decision

Option (B): Semantic mapping with JSON body for content responses.

| HookResponse | HTTP Status | Body |
|---|---|---|
| `Ack` | 204 No Content | empty |
| `Entries { .. }` | 200 OK | `serde_json::to_vec(&response)` |
| `BriefingContent { .. }` | 200 OK | `serde_json::to_vec(&response)` |
| `Pong { .. }` | 200 OK | `serde_json::to_vec(&response)` |
| `Error { .. }` | 400 Bad Request | `serde_json::to_vec(&response)` |

All non-empty responses set `Content-Type: application/json`.

Additionally, HTTP-layer errors (not from dispatch_request) use standard codes:
- Missing/invalid bearer token: 401 (from `StaticTokenAuth`, unchanged)
- Malformed JSON body: 400 with `{"error":"invalid request JSON: {detail}"}`
- Oversized body: 413 (reuse `payload_too_large_response()` from `McpAdapter`)
- Body read failure: 500 (reuse `internal_error_response()`)

The 400 from `HookResponse::Error` and the 400 from malformed JSON are distinguished by body structure: `HookResponse::Error` has `{"type":"Error","code":N,"message":"..."}` while JSON parse failure has `{"error":"..."}`. Clients that need to distinguish can check for the `type` field.

### Consequences

- Fire-and-forget clients (9 events) check `status == 204` — no body parsing needed.
- Sync clients (4 events) check `status == 200` and parse the JSON body.
- Error handling is simple: any non-2xx status is an error.
- Wire format is identical to UDS (same serde serialization of `HookResponse`), just without the 4-byte length prefix.
- The `observe_response_to_http` function is ~30 lines of straightforward mapping. No conditional logic beyond the match.
