# Component: wire-accept-text (`crates/unimatrix-engine/src/wire.rs`)

ADR-001 §1,§3,§6. FR-17, FR-18, AC-11. Risks R-07, R-08.
Additive-only against the frozen F1 contract. Merge step 2.

## Purpose

Add the additive wire surface that lets the UDS listener return server-side
preformatted sync responses: an optional `accept` field on the two
injection-bearing requests, and a new `HookResponse::Text { body }` variant. No
`format_injection` JS port is needed. Serialized bytes of every existing frame stay
identical (proven by AC-11 byte-unchanged fixtures + ts-rs binding drift check).

## Modified: `HookRequest::ContextSearch` (wire.rs ~line 139)

Add ONE field at the end (after the existing `source` field). Exact attribute is
load-bearing — it is what keeps existing frames byte-identical:

```
ContextSearch {
    query: String,
    #[serde(default)] session_id: Option<String>,
    role: Option<String>, task: Option<String>, feature: Option<String>,
    k: Option<u32>, max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")] source: Option<String>,
    // NEW (ADR-001 §1): HTTP-Accept mirror. Set ONLY by transport-uds.js at
    // serialization time, value Some("text/plain"). hook.rs construction sites
    // pass None (mechanical edit). Absent on the wire when None.
    #[serde(default, skip_serializing_if = "Option::is_none")] accept: Option<String>,
}
```

## Modified: `HookRequest::CompactPayload` (wire.rs ~line 165)

Add the same field at the end (after `transcript_excerpt`):

```
CompactPayload {
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>, feature: Option<String>, token_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")] transcript_excerpt: Option<String>,
    // NEW (ADR-001 §1)
    #[serde(default, skip_serializing_if = "Option::is_none")] accept: Option<String>,
}
```

`accept` is NOT added to `Ping` or `Briefing` (ADR-001 §1): Pong stays JSON because
the handshake parses `server_version`.

## New: `HookResponse::Text { body: String }` (wire.rs ~line 193)

Add a new variant to the `#[serde(tag = "type")]` enum. Additive: existing variants
unchanged, no `deny_unknown_fields`, so old serialized responses still deserialize.

```
pub enum HookResponse {
    Pong { server_version: String },
    Ack,
    Error { code: i32, message: String },
    Entries { items: Vec<EntryPayload>, total_tokens: u32 },
    BriefingContent { content: String, token_count: u32 },
    // NEW (ADR-001 §3): preformatted injection text for UDS sync callers that sent
    // accept. body = exact HTTP text/plain bytes:
    //   Entries  → format_injection(items, MAX_INJECTION_BYTES) output, INCLUDING
    //              the load-bearing "--- Unimatrix Context ---\n" header.
    //   BriefingContent → content verbatim (no header).
    // Returned ONLY to callers that sent accept (ADR-001 §6 coupling, R-08).
    Text { body: String },
}
```

`#[cfg_attr(test, derive(ts_rs::TS))]` already on the enum → the TS binding
regenerates additively (drift check confirms — AC-11).

## Framing (UNCHANGED — documented as the byte authority)

`write_frame(writer, payload)` and `read_frame(reader, max_size)` (wire.rs:349,372)
are NOT modified. They remain the parity oracle for transport-uds.js:
- `MAX_PAYLOAD_SIZE = 1_048_576` (wire.rs:16).
- write rejects `payload.len() > MAX_PAYLOAD_SIZE`.
- read rejects declared length `0` and `> MAX_PAYLOAD_SIZE` before allocating.
transport-uds.js mirrors these byte-for-byte (see transport-uds.md).

## Data flow

- Request: client → `accept: Some("text/plain")` for sync injection frames only.
- Response: listener → `Text { body }` only when the request carried `accept` AND
  the dispatch result is `Entries`/`BriefingContent` (see listener-preformatted.md).

## Error handling

No new runtime errors. Deserialization safety is structural: `skip_serializing_if`
means a `None`/absent `accept` is wire-identical to the pre-feature frame, so a
frozen Rust hook's frames are unchanged and a frozen hook never receives `Text`.

## Constraints honored

- **Additive only** (FR-18, AC-11): only `skip_serializing_if` optionals added, one
  new enum variant; no renames, removals, reorders of existing fields, no
  `deny_unknown_fields`.
- **Text↔accept coupling** is enforced at the listener (ADR-001 §6); wire.rs only
  provides the types.

## Key test scenarios (hints for tester)

1. AC-11: the entire pre-existing Rust parity fixture suite + ts-rs binding drift
   check run unmodified and pass byte-unchanged after these additions — R-07 s1.
2. A `ContextSearch`/`CompactPayload` serialized with `accept = None` produces bytes
   identical to the pre-feature frame (no `accept` key present) — R-07.
3. A `ContextSearch` deserialized from a frame WITH `accept:"text/plain"` round-trips
   the field; one WITHOUT it deserializes to `None` (serde default).
4. `HookResponse::Text { body }` serializes to `{"type":"Text","body":"..."}` and
   round-trips; older HookResponse JSON (no Text) still deserializes.
5. ts-rs export includes the new optional fields and the `Text` variant additively.
