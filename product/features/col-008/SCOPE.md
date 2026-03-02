# col-008: Compaction Resilience — PreCompact Knowledge Preservation

## Problem Statement

When Claude Code's conversation history grows too large, it fires a `PreCompact` hook and then compresses earlier messages. Knowledge that was injected by col-007's `UserPromptSubmit` hook in those earlier messages is lost during compaction. The agent continues working but has forgotten critical context — active architectural decisions, coding conventions, and feature-specific knowledge that was previously injected.

This is a known and researched problem. ASS-014 (Cortical Implant Architecture) designed the PreCompact strategy in detail: the server maintains in-memory session state tracking which entries were injected, and when PreCompact fires, the most important entries are re-injected into the compacted window via stdout. The PreCompact hook is synchronous — Claude Code waits for its output before proceeding — so the content is guaranteed to survive into the post-compaction context.

col-007 establishes the injection pipeline (UserPromptSubmit -> ContextSearch -> Entries -> stdout). col-008 closes the compaction gap: when compaction happens, previously-injected knowledge is preserved rather than silently lost.

## Goals

1. Implement a `PreCompact` hook handler in the hook process that sends a `CompactPayload` request to the running Unimatrix server via UDS and prints the response to stdout
2. Implement server-side `CompactPayload` dispatch in the UDS listener that constructs a prioritized knowledge payload from in-memory session state
3. Extend server-side session state (introduced by col-007's CoAccessDedup) to track injection history — which entry IDs were injected during each session, ordered by confidence
4. Update session state on every `ContextSearch` dispatch — after returning entries to the hook process, record the injected entry IDs in the session's injection tracker
5. Implement priority-based token budget allocation within a 2000-token budget (~8000 bytes at 4 bytes/token heuristic): active decisions first, then high-confidence previously-injected entries, then relevant conventions
6. Implement a briefing-based fallback path when no injection history is available (first compaction, server restart mid-session) using the existing `context_briefing` infrastructure internally
7. Provide graceful degradation when the server is unavailable — exit 0 with no stdout (same pattern as col-007)

## Non-Goals

- **Session lifecycle persistence** — col-010 implements the SESSIONS table, INJECTION_LOG table, and schema v4 migration. col-008 uses in-memory session state only.
- **Disk-based compaction cache** — ASS-014 designed a sidecar file fallback (`~/.unimatrix/{hash}/compaction-cache.json`). This adds write I/O on every injection for a marginal benefit (server-unavailable fallback). Deferred to a future enhancement if the briefing fallback proves insufficient.
- **Confidence feedback from compaction** — col-009 implements implicit helpfulness signals. col-008 does not signal which entries survived compaction.
- **Injection recording** — col-010 implements INJECTION_LOG with typed fields. col-008 does not write injection events to redb.
- **Adaptive injection volume** — ASS-014 proposed reducing col-007's injection volume on repeated compaction (compaction_count > 3). This is a tuning optimization, not a core requirement.
- **New redb tables or schema changes** — col-008 operates entirely with in-memory state and existing redb reads.
- **Prompt classification for compaction** — The compaction payload is constructed from injection history and entry metadata, not from analyzing the current prompt.
- **Correction chain tracking in session** — ASS-014 proposed tracking entries corrected during a session (~200 token budget). This requires observing `context_correct` calls during the session, which needs col-010's session-level event tracking. Deferred.

## Background Research

### Claude Code PreCompact Hook Format

The `PreCompact` hook receives the standard Claude Code hook JSON on stdin:

```json
{
  "session_id": "abc123",
  "transcript_path": "/Users/.../.claude/projects/.../transcript.jsonl",
  "cwd": "/Users/my-project",
  "permission_mode": "default",
  "hook_event_name": "PreCompact"
}
```

Key difference from `UserPromptSubmit`: there is no `prompt` field. The hook's job is to output content to stdout that will be preserved in the compacted window.

For output, the same mechanisms apply as col-007: plain text stdout (exit 0) is added as context that Claude can see. col-008 uses plain text stdout, consistent with col-007.

### ASS-014 Compaction Defense Architecture (Validated)

The ASS-014 research spike (product/research/ass-014/) thoroughly designed the PreCompact architecture:

- **D4 (Server-Side Session State)**: The server maintains an in-memory map of `session_id -> SessionState` containing injection history. The hook process is ephemeral and stateless.
- **D8 (Pre-Computed Compaction Payload)**: The server maintains a rolling compaction payload updated after every injection. When PreCompact fires, the payload is served from memory.
- **Strategy 2+1 Hybrid**: Primary strategy is injection history replay (ID-based fetch). Fallback is briefing-based query when no injection history exists.
- **Token Budget**: 2000 tokens total. Priority allocation: active decisions (~400), session context (~200), high-confidence injections (~600), conventions (~400), buffer (~200).
- **Latency**: ~10-15ms total (hook start ~3ms, socket ~0.5ms, server ~5-10ms for ID fetch + sort + format). Well within 50ms budget.
- **No embedding needed**: ID-based fetch, not semantic search. No ONNX runtime dependency at PreCompact time.

### Wire Protocol Stubs (Already Defined in col-006)

`wire.rs` already has the `CompactPayload` and `BriefingContent` stubs:

```rust
// Request
CompactPayload {
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
}

// Response
BriefingContent {
    content: String,
    token_count: u32,
}
```

The `injected_entry_ids` field on `CompactPayload` was designed for a scenario where the hook process tracks IDs. Since the hook process is ephemeral and does not track state, the server must maintain the injection history. The `injected_entry_ids` field becomes a hint — the server uses its own tracked history if available, falling back to any IDs provided by the hook (which will be empty in the current architecture).

### col-007 Server-Side State (Current State)

col-007 introduces `CoAccessDedup` — an in-memory `HashMap<String, HashSet<Vec<u64>>>` keyed by session_id. This tracks which entry-set combinations have been recorded for co-access, preventing duplicate writes. It does NOT track the full ordered injection history.

col-007's `ContextSearch` handler:
1. Embeds query, searches HNSW, re-ranks, applies co-access boost
2. Returns `HookResponse::Entries` with matched entries
3. Records co-access pairs (session-scoped dedup)

col-008 needs to extend this: after step 2 (before returning), capture the returned entry IDs into a session-scoped injection history tracker. This tracker is separate from CoAccessDedup — it records all injected entry IDs in order, not just unique sets.

### Existing Briefing Infrastructure

The `context_briefing` MCP tool (vnc-003) performs:
1. Lookup conventions by role/topic
2. Lookup duties by role/topic
3. Semantic search (embed task, HNSW search, confidence re-rank, co-access boost)
4. Feature boost (entries tagged with feature)
5. Format and truncate to token budget

The briefing logic is embedded in the MCP tool handler (`tools.rs`). For the fallback path, col-008 needs to either:
- Call into the briefing logic directly from the UDS dispatcher (requires extracting briefing logic into a shared function), or
- Construct a simpler fallback using ID-based entry fetches (lookup by category/topic)

Following col-007's ADR-001 pattern (no shared extraction), the fallback should duplicate the essential briefing logic in the UDS dispatcher (~30-40 lines for lookup + format).

### Session State Architecture Decision

col-008 introduces `SessionState` — the in-memory struct that tracks a session's injection history for compaction defense. This is the first concrete implementation of ASS-014's D4 (Server-Side Session State). Key design choice:

The `SessionState` struct should be separate from `CoAccessDedup`. CoAccessDedup serves a narrow purpose (dedup tracking) with its own data structure (HashSet of sorted ID vectors). SessionState serves a broader purpose (injection history, session metadata) and will be extended by col-009 (confidence signals) and col-010 (session persistence). Keeping them separate preserves single-responsibility.

However, both are keyed by session_id and both are populated from the ContextSearch handler. They should share a parent container (`SessionRegistry` or similar) that manages both, keyed by session_id, with cleanup on SessionClose.

## Proposed Approach

### 4 Build Components

**1. Session State Tracker (server-side)**

Introduce a `SessionState` struct in the UDS listener module that tracks:
- `session_id`: String
- `injection_history`: `Vec<InjectionRecord>` — ordered list of (entry_id, confidence, timestamp) for each injected entry
- `role`: `Option<String>` — from SessionRegister
- `feature`: `Option<String>` — from SessionRegister
- `compaction_count`: `u32` — how many times this session has been compacted

And a `SessionRegistry` that wraps `Mutex<HashMap<String, SessionState>>`, replacing or encompassing the existing `CoAccessDedup`:
- `register_session(session_id, role, feature)` — called on SessionRegister
- `record_injection(session_id, entry_ids_with_confidence)` — called after ContextSearch returns
- `get_compaction_state(session_id)` — called on CompactPayload
- `clear_session(session_id)` — called on SessionClose

**2. PreCompact Hook Handler (hook process)**

Add a `"PreCompact"` arm to `build_request()` in `hook.rs`. Construct `HookRequest::CompactPayload` from the session_id. The `injected_entry_ids` field is left empty (server has the state). This is a synchronous request (waits for response), not fire-and-forget.

After receiving the `HookResponse::BriefingContent` response, print the content to stdout if non-empty.

**3. Server-Side CompactPayload Endpoint (UDS listener)**

Implement the `HookRequest::CompactPayload` dispatch handler:
- Look up `SessionState` for the session_id
- If injection history available (normal path):
  1. Collect unique entry IDs from injection history, sorted by confidence descending
  2. Fetch full entries by ID from the entry store
  3. Partition into categories: decisions, conventions, other
  4. Allocate token budget by priority (decisions first, then high-confidence injections, then conventions)
  5. Format as structured plain text
  6. Return `HookResponse::BriefingContent`
- If no injection history (fallback path):
  1. Use the role and feature from SessionState (or from the request) to construct a briefing query
  2. Lookup decisions and conventions by category/topic from the entry store
  3. If feature is known, fetch entries tagged with that feature
  4. Format and truncate to token budget
  5. Return `HookResponse::BriefingContent`
- Increment compaction_count in SessionState

**4. Integration with col-007's ContextSearch Handler**

After the ContextSearch handler returns entries to the hook process, record the injected entry IDs and their confidence scores in the SessionState. This is the "update on every injection" step from ASS-014's D8.

This requires the ContextSearch handler to have access to the SessionRegistry. The session_id comes from either:
- The `ContextSearch` request itself (needs a session_id field added), or
- The `HookInput.session_id` passed through as part of the ContextSearch wire message

The current `ContextSearch` wire stub does not have a `session_id` field. It needs one for col-008 to track injection history per-session.

## Acceptance Criteria

- AC-01: The `PreCompact` hook handler constructs a `CompactPayload` request with the session_id and sends it to the running Unimatrix server via UDS
- AC-02: The UDS listener dispatches `CompactPayload` requests and returns `HookResponse::BriefingContent` with a formatted knowledge payload
- AC-03: The server maintains per-session injection history (entry IDs + confidence scores) in memory, updated after every `ContextSearch` dispatch
- AC-04: The compaction payload includes previously-injected entries sorted by confidence, with active decisions prioritized
- AC-05: The compaction payload respects a configurable token budget (initially 8000 bytes, ~2000 tokens at 4 bytes/token heuristic), with priority-based allocation across entry categories
- AC-06: When no injection history exists for a session (server restart, first compaction before any injection), a briefing-based fallback constructs a reasonable payload using role/feature context and entry lookups by category
- AC-07: When the server is unavailable (no socket), the `PreCompact` hook exits 0 with no stdout output (graceful degradation, same as col-007)
- AC-08: SessionState is created on `SessionRegister` and cleaned up on `SessionClose`
- AC-09: The `ContextSearch` wire message includes a `session_id` field (with `#[serde(default)]`) to enable injection tracking per session
- AC-10: The compaction payload output is formatted as structured plain text suitable for Claude's context (titles, categories, confidence scores, content)
- AC-11: All existing MCP tool integration tests pass without modification after session state changes (zero behavioral regression)
- AC-12: Compaction payload construction from in-memory state completes within 15ms server-side (ID fetch + sort + format), keeping the total hook execution under 50ms

## Constraints

### Hard Constraints

- **redb exclusive file lock**: Hook processes cannot open the database. All data access through IPC to the running MCP server. (Inherited from col-006.)
- **50ms latency budget**: End-to-end hook execution (process start to exit) under 50ms. The hot-path estimate is ~10-15ms (no embedding needed).
- **Zero regression**: All existing MCP tools and hook handlers must continue to work identically. Existing integration tests (174+) must pass without modification.
- **Single binary**: Hook subcommand is part of `unimatrix-server`. No separate binary.
- **No new redb tables**: col-008 operates with in-memory session state. No schema changes.
- **Edition 2024, MSRV 1.89**: Workspace Rust edition and version constraints.

### Soft Constraints

- **Linux + macOS only**: UDS transport inherited from col-006.
- **Token budget is heuristic**: 4-bytes-per-token is an approximation. Acceptable for v1.
- **In-memory state is volatile**: Server restart mid-session loses injection history. Briefing fallback mitigates.
- **No embedding at PreCompact time**: ID-based fetch only. No ONNX runtime dependency.
- **2000-token budget (8000 bytes)**: From PRODUCT-VISION.md. Larger than col-007's 350-token injection budget because compaction payloads must reconstruct fuller context.

### Dependencies

- **col-006** (hard, COMPLETE): UDS transport, hook subcommand, wire protocol types, LocalTransport, graceful degradation
- **col-007** (hard, IN IMPLEMENTATION): ContextSearch handler, CoAccessDedup, server-side session awareness, injection formatting patterns
- **vnc-003** (existing): `context_briefing` logic informs the fallback path design
- **Existing entry store**: `AsyncEntryStore::get()` for ID-based entry fetching, `AsyncEntryStore::query()` for category/topic lookups

### Downstream Dependents

| Feature | What It Needs from col-008 |
|---------|---------------------------|
| col-009 | Session injection history (same SessionState struct) for confidence signal generation |
| col-010 | SessionState as the in-memory backing for persistent session records; injection history for INJECTION_LOG |

## Open Questions

1. **ContextSearch session_id field**: The current `ContextSearch` wire stub does not include `session_id`. col-008 needs it to track which session an injection belongs to. Should `session_id` be added to the `ContextSearch` request, or should the server correlate by connection/timing? (Recommendation: add `session_id: Option<String>` with `#[serde(default)]` to `ContextSearch`, consistent with the defensive parsing pattern.)

2. **CoAccessDedup integration**: Should `SessionState` absorb `CoAccessDedup` (unified session container), or should they remain separate with a shared `SessionRegistry` wrapper? (Recommendation: Shared `SessionRegistry` that owns both. CoAccessDedup stays focused on dedup; SessionState handles injection history. Both keyed by session_id, both cleaned on SessionClose.)

3. **Fallback path complexity**: The briefing fallback needs to lookup entries by category/topic without using the MCP tool handler directly. How much of the briefing logic should be duplicated? (Recommendation: Minimal — just `entry_store.query()` with `category: "decision"` and `category: "convention"`, sorted by confidence, truncated to budget. No embedding, no semantic search in the fallback.)

4. **Token budget allocation tuning**: The initial allocation (decisions: 400t, context: 200t, injections: 600t, conventions: 400t, buffer: 200t) from ASS-014 is theoretical. Should the budget be configurable as constants or hardcoded? (Recommendation: Named constants, same pattern as col-007's `MAX_INJECTION_BYTES`. Tune empirically after delivery.)

5. **CompactPayload wire stub fields**: The existing `CompactPayload` stub has `injected_entry_ids: Vec<u64>`. Since the server tracks injection history, should this field be kept (as a hint/override) or removed? (Recommendation: Keep it. The server prefers its own tracked history but falls back to the provided IDs if its state is missing. This allows future hook processes to pass IDs if they acquire state.)

## Tracking

- GH Issue: https://github.com/dug-21/unimatrix/issues/69
