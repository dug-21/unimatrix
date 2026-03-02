# col-007: Automatic Context Injection

## Problem Statement

Unimatrix has a knowledge engine with 53+ active entries covering architectural decisions, conventions, and patterns. Agents can query this knowledge via MCP tools (`context_search`, `context_briefing`, `context_lookup`), but most agents never call these tools. Knowledge delivery depends entirely on agent cooperation -- agents must know the tools exist, know when to call them, and include the right parameters.

col-006 established the hook transport layer: a Unix domain socket listener in the MCP server, a `hook` subcommand, and a wire protocol with stubs for `ContextSearch` requests. The infrastructure is in place, but no hook actually delivers knowledge yet.

col-007 closes this gap. By implementing the `UserPromptSubmit` hook handler, every prompt submitted to Claude Code gets automatically enriched with relevant knowledge from Unimatrix -- no agent action needed. This is the core value proposition of the cortical implant architecture: passive, automatic knowledge delivery.

## Goals

1. Implement a `UserPromptSubmit` hook handler that extracts the user's prompt text and sends a `ContextSearch` request to the running Unimatrix server via UDS
2. Implement server-side `ContextSearch` dispatch in the UDS listener, routing to the existing search pipeline (embed, HNSW search, confidence re-rank, co-access boost)
3. Extract the search pipeline from `tools.rs` into a reusable function in `unimatrix-engine` so both MCP tools and UDS hooks use the same code path
4. Format matched entries as structured text for stdout injection, with token budget enforcement (configurable constant, initially 350 tokens)
5. Generate co-access pairs from injected entry sets, extending the existing crt-004 infrastructure, with session-scoped dedup to prevent redundant pair writes across prompts
6. Pre-warm the ONNX embedding model on `SessionStart` (blocking with readiness check) to ensure the first `UserPromptSubmit` hits the hot path

## Non-Goals

- **Compaction resilience** -- col-008 implements `PreCompact` knowledge preservation using injection history from col-007
- **Confidence feedback loops** -- col-009 implements implicit helpfulness signals from session outcomes
- **Session lifecycle persistence** -- col-010 implements SESSIONS table, INJECTION_LOG table, and schema v4 migration
- **Agent routing** -- col-011 implements semantic agent matching via UserPromptSubmit
- **Injection recording** -- col-010 implements INJECTION_LOG table with typed fields; col-007 does NOT record injection events (deferred per scope risk SR-05 analysis showing design divergence between unstructured RecordEvent and col-010's typed schema)
- **New redb tables** -- col-007 does not add INJECTION_LOG, SESSIONS, or SIGNAL_QUEUE tables
- **Prompt classification or summarization** -- col-007 uses the raw prompt text as the search query; no NLP preprocessing
- **Injection quality tuning** -- col-007 establishes the pipeline; quality tuning (threshold adjustment, prompt weighting, category filtering) is iterative post-delivery work
- **Token counting via tokenizer** -- col-007 uses byte-length heuristics (4 bytes per token) for budget enforcement; a real tokenizer is a future enhancement
- **Daemon architecture** -- col-006 uses ephemeral hook processes; col-007 follows the same pattern
- **Hook configuration automation** -- `unimatrix init` (alc-003) handles automated `.claude/settings.json` setup

## Background Research

### Claude Code Hook Format (Verified)

The `UserPromptSubmit` hook receives the following JSON on stdin ([Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks)):

```json
{
  "session_id": "abc123",
  "transcript_path": "/Users/.../.claude/projects/.../transcript.jsonl",
  "cwd": "/Users/my-project",
  "permission_mode": "default",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "Write a function to calculate the factorial of a number"
}
```

Key field: `prompt` contains the user's submitted text.

For output, there are two injection mechanisms:
- **Plain text stdout** (exit 0): text is added as context that Claude can see and act on
- **JSON with `additionalContext`** (exit 0): more structured control over injection

col-007 uses plain text stdout for simplicity and transparency (the injected knowledge is visible in the transcript).

### Existing Search Pipeline (tools.rs, lines 251-424)

The MCP `context_search` tool performs these steps:
1. Identity resolution + capability check (MCP-specific, not needed for hooks)
2. Validation + format parsing (MCP-specific)
3. Embed query via `embed_entry("", &query)` using `spawn_blocking`
4. Adapt embedding via MicroLoRA + prototype pull (crt-006) + L2 normalize
5. HNSW search (k results, ef_search=32), optionally with metadata pre-filtering
6. Fetch full entries, exclude quarantined
7. Re-rank by blended score: 0.85*similarity + 0.15*confidence
8. Co-access boost (anchor top 3, boost remaining)
9. Truncate to k results
10. Format response

Steps 3-9 are the core pipeline that must be shared between MCP and UDS. Steps 1-2 and 10 are transport-specific.

### UDS Listener Current State

`dispatch_request()` in `uds_listener.rs` is currently synchronous and receives `Arc<Store>` only. The `ContextSearch` variant hits a catch-all `_ => Error("not implemented")`. To serve ContextSearch, the dispatcher needs:
- Access to `EmbedService`, `VectorStore`, `EntryStore`, `AdaptService` (currently only on `UnimatrixServer`)
- Async execution (embedding uses `spawn_blocking`)

### Wire Protocol Stubs (Already Defined in col-006)

`crates/unimatrix-engine/src/wire.rs` already has:
- `HookRequest::ContextSearch { query, role, task, feature, k, max_tokens }` (stub, `#[allow(dead_code)]`)
- `HookResponse::Entries { items: Vec<EntryPayload>, total_tokens }` (stub)
- `EntryPayload { id, title, content, confidence, similarity, category }` (stub struct)

col-007 activates these stubs -- removes `#[allow(dead_code)]`, implements the dispatch handler.

### Hook Subcommand Current State

`hook.rs` `build_request()` maps event names to `HookRequest` variants. `UserPromptSubmit` currently falls into the generic `RecordEvent` catch-all. col-007 adds a dedicated arm that constructs `HookRequest::ContextSearch` from the `prompt` field.

### Latency Budget Analysis

Total budget: 50ms (Claude Code hook timeout for synchronous hooks).

| Step | Estimated Time | Notes |
|------|---------------|-------|
| Process startup | ~3ms | Existing col-006 measurement |
| Hash computation + socket path | ~1ms | |
| UDS connect | ~1ms | |
| Prompt embedding (hot ONNX) | ~3ms | Server-side, model already loaded |
| Prompt embedding (cold ONNX) | ~200ms | First call after server start |
| HNSW search (k=5, ef=32) | ~1ms | |
| Confidence re-ranking | <1ms | |
| Co-access boost | ~1ms | |
| Response serialization + write | ~1ms | |
| **Total (hot path)** | **~12ms** | Well within 50ms budget |
| **Total (cold path)** | **~210ms** | Blows budget without pre-warming |

The cold ONNX path is mitigated by pre-warming on SessionStart (Goal 7).

### Injection Recording Decision

Injection recording is deferred to col-010. The scope risk assessment (SR-05) identified a design divergence: col-007's RecordEvent uses unstructured `serde_json::Value` payload, while col-010's INJECTION_LOG has typed fields (hook_type, prompt_context, injection_reason). Rather than create technical debt requiring col-010 to deserialize and re-store, injection recording is cleanly owned by col-010 alongside the INJECTION_LOG table and schema v4 migration.

col-007 delivers the injection pipeline (search, format, output). col-010 adds the recording layer.

## Proposed Approach

### 4 Build Components

**1. Server-Side ContextSearch Handler**

Implement the `HookRequest::ContextSearch` dispatch in the UDS listener. This involves: accessing the embed service, vector store, entry store, and adapt service to run the search pipeline; constructing `HookResponse::Entries` from results. The architect decides the approach: shared extraction vs. UDS-local implementation (see SR-02 tradeoff analysis). The dispatcher becomes async (SR-08).

**2. Hook-Side UserPromptSubmit Handler**

Add a `"UserPromptSubmit"` arm to `build_request()` in `hook.rs`. Extract the `prompt` field from `HookInput` (named field with `#[serde(default)]`). Construct `HookRequest::ContextSearch { query: prompt, ... }`. This is a synchronous request (waits for response), not fire-and-forget.

After receiving the `HookResponse::Entries` response, format the entries as structured text and print to stdout. No injection recording (deferred to col-010).

**3. Injection Formatting**

Format matched entries as structured text for stdout. Include:
- Entry title, category, and confidence score
- Entry content (truncated to fit token budget)
- Entry IDs (for downstream tracking by col-008/col-009)

Token budget: configurable constant, initially 350 tokens (estimated via 4-bytes-per-token heuristic = 1400 bytes). Entries are added in rank order until the budget is exhausted. The last entry may be truncated.

**4. ONNX Pre-Warming on SessionStart (Blocking)**

Extend the `SessionStart` handler (currently just logs and returns `Ack`) to trigger an embedding model warm-up on the server side. The warm-up calls `embed_entry("", "warmup")` once, forcing the ONNX runtime to load the model. The server blocks the SessionStart response until warming completes (or a readiness check confirms the model is already warm). The hook process is fire-and-forget for SessionStart, so the server-side blocking does not affect hook latency.

### Key Design Choices

- **Plain text stdout** (not JSON `additionalContext`): simpler, transparent in transcript, sufficient for knowledge injection
- **Token budget as a constant**: initially 350 tokens (estimated via byte-length heuristic at 4 bytes/token = 1400 bytes). Will be tuned based on real-world usage. Defined as a named constant for easy adjustment.
- **No injection recording**: deferred to col-010 which introduces typed INJECTION_LOG table. col-007 delivers the pipeline; col-010 adds the observability layer.
- **Prompt as search query**: the raw prompt text is used directly as the semantic search query. No summarization, no keyword extraction. The HNSW index handles semantic matching.
- **Blocking pre-warm**: SessionStart triggers synchronous model loading on the server to guarantee readiness for the first UserPromptSubmit. Since SessionStart is fire-and-forget from the hook process, the server-side blocking is safe.

## Acceptance Criteria

- AC-01: The `UserPromptSubmit` hook handler extracts the `prompt` field from Claude Code's stdin JSON and sends a `ContextSearch` request to the running Unimatrix server via UDS
- AC-02: The UDS listener dispatches `ContextSearch` requests to the search pipeline and returns `HookResponse::Entries` with matched entries
- AC-03: The search pipeline used by the UDS ContextSearch handler produces results equivalent to the MCP `context_search` tool (same embed, HNSW, re-rank, co-access boost steps)
- AC-04: Matched entries are formatted as structured text and printed to stdout, with each entry showing title, category, confidence, and content
- AC-05: Injection output respects a configurable token budget (initially 1400 bytes, ~350 tokens at 4 bytes/token heuristic); entries are added in rank order until the budget is exhausted
- AC-06: Co-access pairs are generated from injected entry sets on the server side, using the existing `generate_pairs()` + `record_co_access()` infrastructure from crt-004, with session-scoped dedup (max one co-access recording per unique entry set per session)
- AC-07: On `SessionStart`, the server pre-warms the ONNX embedding model by running a no-op embedding (blocking until warm), so that the first `UserPromptSubmit` hits the hot path (~3ms, not ~200ms cold)
- AC-08: When the server is unavailable (no socket), the `UserPromptSubmit` hook exits 0 with no stdout output (graceful degradation, no knowledge injection, no error visible to user)
- AC-09: When the knowledge base has no relevant entries (empty search results or all results below similarity floor), the hook produces no stdout output (silent skip)
- AC-10: The `HookInput` struct is extended with a named `prompt` field (`#[serde(default)]`) to replace the `extra` catch-all for this specific field
- AC-11: All existing MCP tool integration tests pass without modification after any search pipeline changes (zero behavioral regression)
- AC-12: End-to-end round-trip (hook process start to stdout output) completes within 50ms on the hot path (ONNX model already loaded), measured as p95 over 10 iterations

## Constraints

### Hard Constraints

- **redb exclusive file lock**: Hook processes cannot open the database. All data access through IPC to the running MCP server. (Inherited from col-006.)
- **50ms latency budget**: End-to-end hook execution (process start to exit) under 50ms for synchronous hooks. The hot-path estimate is ~12ms, leaving margin.
- **Zero regression**: All existing MCP tools must continue to work identically after search pipeline extraction. Existing integration tests (174+) must pass without modification.
- **Single binary**: Hook subcommand is part of `unimatrix-server`. No separate binary.
- **Edition 2024, MSRV 1.89**: Workspace Rust edition and version constraints.

### Soft Constraints

- **Linux + macOS only**: UDS transport inherited from col-006.
- **Token budget is heuristic**: 4-bytes-per-token is an approximation. Acceptable for v1.
- **No new redb tables**: No injection recording in col-007.
- **Confidence threshold**: Entries below 0.3 confidence are excluded from injection results. This threshold is a constant, tunable post-delivery.
- **Similarity floor**: Entries below 0.5 similarity are excluded from injection results. This threshold is a constant, tunable post-delivery.

### Dependencies

- **col-006** (hard): UDS transport, hook subcommand, wire protocol types, LocalTransport, graceful degradation
- **crt-004** (existing): co-access pair generation and recording
- **crt-006** (existing): MicroLoRA embedding adaptation (used in search pipeline)
- **Existing search pipeline**: embed service, HNSW index, confidence re-ranking, co-access boosting

### Downstream Dependents

| Feature | What It Needs from col-007 |
|---------|---------------------------|
| col-008 | Knowledge of which entries were injected (via col-010's INJECTION_LOG, not directly from col-007) |
| col-009 | Injection entry IDs for confidence signaling (via col-010's INJECTION_LOG) |
| col-010 | The injection pipeline to record against (col-010 adds the recording layer on top of col-007's pipeline) |

## Open Questions

1. **HookInput.prompt field**: Should the `prompt` field be added as a named `Option<String>` on `HookInput` (alongside `session_id`, `cwd`, etc.), or should it remain in the `extra` catch-all and be extracted with `extra["prompt"]`? Named field is cleaner but adds a field that only UserPromptSubmit uses. (Recommendation: named field with `#[serde(default)]`, consistent with ADR-006 defensive parsing. AC-11 assumes this.)

2. **Minimum result threshold**: Should there be a minimum similarity score below which entries are not injected (in addition to the confidence threshold)? If the best match has 0.15 similarity, injecting it may be noise. (Recommendation: add a similarity floor of 0.5, configurable constant.)

3. **Search parameters for injection**: Should `k` and other search parameters be configurable constants or derived from the prompt? (Recommendation: constants for v1. k=5, ef_search=32, confidence_floor=0.3, similarity_floor=0.5.)

## Tracking

{Will be updated with GH Issue link after Session 1}
