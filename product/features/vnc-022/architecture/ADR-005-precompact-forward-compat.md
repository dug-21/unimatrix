## ADR-005: PreCompact transcript_excerpt Forward Compatibility

### Context

PreCompact is the compaction defense mechanism — when Claude Code compacts its context window, Unimatrix restores critical knowledge via the hook response. The local UDS path extracts a transcript block from the hook's stdin (the compaction payload) and includes it in the briefing.

For remote deployments, the hook process runs locally but POSTs to a remote server. The transcript extraction logic (`extract_transcript_block()`) runs client-side in the hook process. The extracted transcript needs to reach the server.

Day 1 constraint: No `hook-remote` CLI binary. Claude Code uses `"type": "http"` hooks which POST the raw `HookRequest` JSON. The `CompactPayload` wire type currently has no field for the transcript excerpt.

Options:
- **(A) Add transcript_excerpt to CompactPayload now, populate Day 1**: Requires the HTTP hook handler (Claude Code's `"type": "http"`) to extract the transcript and include it. Claude Code's HTTP hook handler passes the hook JSON verbatim — it does not run custom extraction logic. Not viable without client changes.
- **(B) Add transcript_excerpt field as Optional, leave unpopulated Day 1**: Forward compatibility. When #670 ships (server-side transcript buffer from accumulated observations), the server reconstructs the transcript from its own data — no client field needed. The optional field exists for a future client-side optimization path.
- **(C) No wire type change**: Defer entirely. Risk: if #670 takes long, there's no way for improved clients to send transcript data.

### Decision

Option (B): Add `transcript_excerpt: Option<String>` to `CompactPayload` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

```rust
CompactPayload {
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transcript_excerpt: Option<String>,
}
```

Day 1: Field is always `None` over HTTP. The `handle_compact_payload` function ignores it (returns briefing content only, same as current behavior).

Day 1 degradation: Remote PreCompact returns briefing content only — slightly lower quality than local (which includes transcript restoration). This is documented and acceptable. #670 (server-side transcript buffer) improves both local and remote paths.

### Consequences

- Zero breaking change to existing UDS callers — `serde(default)` means missing field deserializes as `None`.
- Zero breaking change to existing `handle_compact_payload` — it ignores the new field.
- Forward compatible: future clients (or a future `hook-remote` CLI) can populate `transcript_excerpt` and the server can use it.
- Wire format remains stable — the field is additive only.
- Remote PreCompact quality is slightly degraded Day 1. This is an explicit, documented trade-off, not a silent failure.
