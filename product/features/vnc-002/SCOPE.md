# vnc-002: v0.1 Tool Implementations

## Problem Statement

The MCP server (vnc-001) is running but every tool call returns "not yet implemented." Four tool stubs exist with parameter schemas, identity resolution, and audit logging wired — but zero business logic. Agents cannot search, store, retrieve, or look up knowledge. Until these stubs become real implementations, Unimatrix provides no value to its consumers.

vnc-001 also established 9 enforcement point slots across the 4 tools for capability checks, input validation, and content scanning. These must be activated simultaneously with the tool logic — shipping writable tools without security enforcement would allow unrestricted writes from any agent, bypassing the trust hierarchy.

Additionally, GH issue #11 identified that the current per-request audit log write transaction pattern (each `AuditLog::log_event()` opens its own redb write transaction) will serialize against tool data writes. For mutating tools like `context_store`, this means two write transactions per call — one for the entry and one for the audit event. This must be addressed before high-throughput use.

## Goals

1. **Implement `context_search`** — embed the query via EmbedService, perform vector similarity search via VectorStore, fetch matching EntryRecords, return results ranked by similarity with scores. Support optional metadata pre-filtering (topic, category, tags) using `search_filtered` when filters are provided. Handle the embedding-not-ready case gracefully (return error suggesting `context_lookup`).

2. **Implement `context_lookup`** — build a `QueryFilter` from the deterministic parameters (topic, category, tags, status), execute against EntryStore's `query()` method, return matching entries. When `id` is provided, delegate to `get()` directly. Apply result limit. No embeddings needed.

3. **Implement `context_store`** — validate and scan input, embed title+content, check for near-duplicates at 0.92 similarity threshold (return existing entry instead of creating duplicate), insert via EntryStore with security fields populated (`created_by` from identity, `trust_source` = "agent"), index the embedding in VectorStore, return the created entry with its assigned ID.

4. **Implement `context_get`** — fetch a single entry by ID via EntryStore's `get()`, return the full record. Handle not-found with actionable error.

5. **Activate security enforcement at all 9 enforcement points:**
   - **Capability checks**: `Search` for context_search, `Read` for context_lookup and context_get, `Write` for context_store.
   - **Input validation**: Max lengths on all string params (title: 200 chars, content: 50,000 chars, topic: 100 chars, category: 50 chars, tags: 20 per entry, 50 chars each, query: 1,000 chars, source: 200 chars). Pattern matching — no control characters (U+0000–U+001F except newline/tab in content). Reject negative IDs and zero-value required fields.
   - **Content scanning** on `context_store` writes: ~50 regex patterns for prompt injection detection (instruction override attempts, role impersonation, system prompt extraction) plus PII patterns (emails, phone numbers, SSNs, API keys). Native Rust `regex` crate — compiled once, reused. Reject or flag on match.
   - **Category allowlist**: initial set `{outcome, lesson-learned, decision, convention, pattern, procedure}`. Reject unknown categories on `context_store`. Store allowlist in a runtime-extensible structure (not hardcoded enum) so vnc-003 can add categories.

6. **Implement output framing** on markdown format: wrap entry content with `[KNOWLEDGE DATA]`/`[/KNOWLEDGE DATA]` markers that distinguish stored knowledge from agent instructions, preventing knowledge entries from being interpreted as directives.

7. **Implement format-selectable responses** via an optional `format` parameter on all four tools with three modes:
   - **`summary`** (default): compact markdown — one line per entry with ID, title, category, tags, similarity score (if search). Minimal context window consumption. Agents call `context_get` for full content on entries they care about.
   - **`markdown`**: full entry content in markdown with output framing and metadata header. For when the agent needs complete entries inline.
   - **`json`**: structured JSON object (single-result) or array (multi-result). For programmatic consumers.

   `context_get` returns full content in all three formats (it's a single-entry fetch — no summary needed). `context_store` responses use the requested format for the created/duplicate entry.

8. **Implement near-duplicate detection** for `context_store`: before inserting, embed the new entry and search for existing entries above 0.92 similarity. If a near-duplicate is found, return the existing entry's ID and content instead of creating a new one. Include similarity score in the response so the agent can decide whether to proceed with a forced store or adjust.

9. **Optimize audit log writes** (GH #11): for mutating tools (`context_store`), combine the audit event write into the same redb write transaction as the data mutation. For read-only tools (`context_search`, `context_lookup`, `context_get`), keep the standalone audit transaction. This eliminates write transaction serialization on the critical path for mutations. Audit events must remain durable — no in-memory buffering or batching.

## Non-Goals

- **No v0.2 tools.** `context_correct`, `context_deprecate`, `context_status`, and `context_briefing` are vnc-003. This feature implements only the four v0.1 tools.
- **No confidence computation.** The `confidence` field exists on EntryRecord but the formula (usage × freshness × correction × helpfulness) is crt-002. Store entries with the default confidence value.
- **No usage tracking.** Access counting (`access_count`, `last_accessed_at`) infrastructure exists on EntryRecord but the USAGE_LOG table and tracking logic are crt-001.
- **No HTTP/SSE transport.** Stdio only, per vnc-001.
- **No batch operations.** Each tool call operates on a single entry or query. Batch store/search is a future optimization.
- **No content summarization or truncation.** Return full entry content. Token budget management for briefings is vnc-003's `context_briefing` tool.
- **No cross-project search.** Single project per server instance, per vnc-001.
- **No async audit channel.** GH #11 proposed three solutions — this feature implements the simplest (combine with tool transaction). Async channels with batching are reserved for if throughput measurements justify the complexity.

## Background Research

### Prior Feature (vnc-001) — What Exists

The `unimatrix-server` crate (72 tests, ~2,200 LOC) provides:

**Tool infrastructure** (`tools.rs`): 4 tool stubs registered via `#[tool_router]`/`#[tool_handler]` macros with full parameter schemas (`SearchParams`, `LookupParams`, `StoreParams`, `GetParams`). Each stub resolves agent identity, logs an audit event with `Outcome::NotImplemented`, and returns a stub message. 9 enforcement point comments mark where capability checks, input validation, and content scanning go.

**Security infrastructure** (`registry.rs`, `audit.rs`, `identity.rs`): `AgentRegistry` with 4 trust levels and 4 capabilities, `resolve_or_enroll()` auto-enrollment, `require_capability()` for enforcement. `AuditLog` with monotonic IDs and `Outcome` enum. `ResolvedIdentity` struct threaded through handlers.

**State** (`server.rs`): `UnimatrixServer` holds `Arc<AsyncEntryStore>`, `Arc<AsyncVectorStore>`, `Arc<EmbedServiceHandle>`, `Arc<AgentRegistry>`, `Arc<AuditLog>`. All fields are `pub(crate)` for tool handler access.

**Lazy embedding** (`embed_handle.rs`): `EmbedServiceHandle` with Loading→Ready|Failed state machine. Non-blocking MCP startup.

**Error mapping** (`error.rs`): `ServerError` with 8 variants → `rmcp::ErrorData` with 6 custom MCP error codes. Actionable messages.

### Foundation Layer API Surface

**EntryStore**: `insert(NewEntry) → u64`, `get(u64) → EntryRecord`, `query(QueryFilter) → Vec<EntryRecord>`, `update(EntryRecord)`, `update_status(u64, Status)`, `put_vector_mapping(entry_id, hnsw_id)`, `get_vector_mapping(entry_id) → Option<hnsw_id>`.

**VectorStore**: `insert(entry_id, &[f32])`, `search(query, top_k, ef_search) → Vec<SearchResult>`, `search_filtered(query, top_k, ef_search, allowed_ids) → Vec<SearchResult>`.

**EmbedService**: `embed_entry(title, content) → Vec<f32>`, `dimension() → usize`.

**Key types**: `NewEntry` (insert params with `created_by`, `feature_cycle`, `trust_source`), `EntryRecord` (25 fields including security fields), `QueryFilter` (topic/category/tags/status/time_range intersection), `SearchResult` (entry_id + similarity), `Status` (Active/Deprecated/Proposed).

### GH Issue #11 Context

Each `AuditLog::log_event()` opens its own redb write transaction. redb serializes all write transactions. For `context_store`, the current pattern would require two serial write transactions: one for the entry insert + vector mapping, one for the audit event. The recommended fix is to expose a way to write the audit event within an existing write transaction. This requires either:
- Extending `AuditLog` with a method that accepts an existing `WriteTransaction`
- Or having the tool handler manage the transaction directly and call both store and audit operations within it

The first approach is cleaner — it keeps transaction management in the audit module while allowing callers to provide an existing transaction. However, the `AsyncEntryStore` wrapper manages its own transactions internally (via `spawn_blocking`). The optimization may require the mutating tool handler to drop down to direct `Store` access (bypassing the async wrapper) for the combined transaction, or extending the async wrapper to support transaction-passing.

### Security Research

**Content scanning patterns** (from `product/research/mcp-security/`): The MCP security analysis identified ~50 prompt injection patterns including instruction override ("ignore previous instructions"), role impersonation ("you are now"), system prompt extraction ("repeat your system prompt"), delimiter injection, and encoding-based evasion. PII patterns cover email addresses, phone numbers, SSN formats, and API key patterns (Bearer tokens, AWS keys, GitHub tokens).

**Output framing**: Wrapping returned content prevents knowledge entries that happen to contain instruction-like text from being interpreted as directives by the consuming agent. Standard approach: prefix/suffix markers like `[KNOWLEDGE DATA]...[/KNOWLEDGE DATA]` around entry content.

**Category allowlist rationale**: Restricting categories prevents namespace pollution and ensures entries are discoverable via known categories. The initial set covers the core knowledge types from the product vision. Runtime extensibility means vnc-003 can add categories without code changes.

### Technical Constraints

- **EmbedServiceHandle may not be ready** when `context_search` is first called. Must check state and return `ServerError::EmbedNotReady` (MCP error code -32004) with guidance to use `context_lookup`.
- **Parameter type mismatch**: Tool params use `i64` (JSON number), store uses `u64`. Must validate non-negative before casting.
- **Status string parsing**: `LookupParams.status` is `Option<String>` — must parse "active"/"deprecated"/"proposed" to `Status` enum.
- **QueryFilter default**: All-None filter returns all **active** entries (implicit status filter). Explicit status param overrides this.
- **Near-duplicate detection timing**: Must embed first, then search, then decide — this is the critical path for `context_store` latency.
- **redb write serialization**: Only one write transaction at a time. Combined audit+data writes eliminate one transaction per mutating call.
- **Regex compilation cost**: Content scanning regexes should be compiled once at server startup and reused via `lazy_static` or `OnceLock`, not per-request.

## Proposed Approach

### Module Structure

Add new modules to `crates/unimatrix-server/src/`:

```
crates/unimatrix-server/src/
  tools.rs          -- Replace stubs with real implementations
  validation.rs     -- Input validation functions (max lengths, patterns, control chars)
  scanning.rs       -- Content scanning (injection patterns, PII detection)
  response.rs       -- Format-selectable responses (summary/markdown/json), output framing
  categories.rs     -- Category allowlist management
```

### Tool Implementation Patterns

**context_search flow:**
1. Resolve agent identity (existing)
2. Capability check: `require_capability(Search)`
3. Validate params (query length, optional filter values)
4. Get embedding: `embed_service.embed_entry("", &params.query)`
5. If metadata filters provided: query store for matching IDs → `search_filtered(embedding, k, ef, allowed_ids)`
6. Else: `search(embedding, k, ef)`
7. Fetch full `EntryRecord` for each result
8. Format response with output framing
9. Audit log (standalone read transaction)

**context_lookup flow:**
1. Resolve agent identity
2. Capability check: `require_capability(Read)`
3. Validate params
4. If `id` provided: `get(id)`, return single entry
5. Else: build `QueryFilter`, call `query(filter)`, apply limit
6. Format response with output framing
7. Audit log (standalone read transaction)

**context_store flow:**
1. Resolve agent identity
2. Capability check: `require_capability(Write)`
3. Validate params (lengths, patterns)
4. Category allowlist check
5. Content scan (injection + PII patterns)
6. Embed title+content
7. Near-duplicate search at 0.92 threshold
8. If duplicate found: return existing entry (no insert)
9. Build `NewEntry` with security fields from identity
10. Insert entry + put vector mapping + audit event (combined write transaction)
11. Format response

**context_get flow:**
1. Resolve agent identity
2. Capability check: `require_capability(Read)`
3. Validate params (positive ID)
4. `get(id)`
5. Format response with output framing
6. Audit log (standalone read transaction)

### Audit Transaction Optimization

For `context_store`, bypass the `AsyncEntryStore` wrapper and use the `Store` directly (via `spawn_blocking`) to manage a single write transaction that includes:
- Entry insert
- Vector mapping put
- Audit event append

This requires the server to hold a reference to the underlying `Store` (or `StoreAdapter`) in addition to the `AsyncEntryStore` wrapper — or extending the adapter/wrapper to support combined operations. The cleanest approach is adding a `store_with_audit` method on `UnimatrixServer` that coordinates both within one `spawn_blocking` call.

For read tools, the standalone audit transaction remains (one read txn for the query, one write txn for the audit event — reads don't serialize with writes in redb).

### Response Format

All tools accept an optional `format` parameter: `"summary"` (default), `"markdown"`, or `"json"`.

**Summary format** (default — minimal context footprint):
```
#42 | Convention: Use conventional commits | convention | [git, workflow] | 0.94
#17 | Decision: redb over SQLite | decision | [storage] | 0.87
#31 | Pattern: Error propagation chain | pattern | [rust, errors] | 0.82
```

One line per entry: ID, title, category, tags, similarity (if search). Agents call `context_get` for full content on entries they want.

**Markdown format** (full content with framing):
```
## Context: {title}
**Topic:** {topic} | **Category:** {category} | **Tags:** {tags}
**Confidence:** {confidence} | **Status:** {status}

[KNOWLEDGE DATA]
{content}
[/KNOWLEDGE DATA]

*Entry #{id} | Created {created_at} | Updated {updated_at}*
```

**JSON format** (structured):
```json
{
  "id": 42,
  "title": "...",
  "content": "...",
  "topic": "...",
  "category": "...",
  "tags": ["..."],
  "status": "active",
  "confidence": 0.85,
  "similarity": 0.94,
  "created_at": 1700000000,
  "created_by": "architect"
}
```

Multi-result tools (search, lookup): summary/markdown repeat per entry, JSON is an array.

## Acceptance Criteria

### Tool Implementations

- **AC-01**: `context_search` accepts a natural language query, embeds it, performs vector similarity search, and returns up to `k` (default 5) matching entries ranked by descending similarity score.
- **AC-02**: `context_search` supports optional metadata pre-filtering — when `topic`, `category`, or `tags` are provided, only entries matching those filters appear in results.
- **AC-03**: `context_search` returns `ServerError::EmbedNotReady` (MCP error -32004) with guidance to use `context_lookup` when the embedding model is still loading.
- **AC-04**: `context_lookup` returns entries matching the provided deterministic filters (topic, category, tags, status) using intersection semantics.
- **AC-05**: `context_lookup` with `id` parameter delegates to direct entry retrieval, ignoring other filter params.
- **AC-06**: `context_lookup` respects the `limit` parameter (default 10) and the `status` parameter (parsed from string to `Status` enum).
- **AC-07**: `context_store` inserts a new entry with all security fields populated: `created_by` from resolved agent identity, `trust_source` = "agent", `content_hash` auto-computed by the store engine.
- **AC-08**: `context_store` embeds the entry's title+content and indexes the embedding in the VectorStore.
- **AC-09**: `context_get` returns the full `EntryRecord` for a given ID, or `ServerError::Core(StoreError::EntryNotFound)` (MCP error -32001) with the entry ID in the message.

### Security Enforcement

- **AC-10**: Capability checks are active on all 4 tools: `Search` for context_search, `Read` for context_lookup and context_get, `Write` for context_store. Agents lacking the required capability receive MCP error -32003 with their agent ID and the missing capability named.
- **AC-11**: Input validation rejects strings exceeding max lengths (title: 200, content: 50,000, topic: 100, category: 50, individual tag: 50, tags count: 20, query: 1,000, source: 200), negative IDs, and strings containing control characters (U+0000–U+001F except U+000A newline and U+0009 tab in content fields).
- **AC-12**: Content scanning on `context_store` detects prompt injection patterns (~50 regex patterns covering instruction override, role impersonation, system prompt extraction, delimiter injection) and flags PII patterns (email, phone, SSN, API key formats). Scanning regexes are compiled once at startup.
- **AC-13**: Category allowlist on `context_store` rejects categories not in the runtime-extensible set `{outcome, lesson-learned, decision, convention, pattern, procedure}`. Error message lists valid categories.
- **AC-14**: Output framing wraps entry content with `[KNOWLEDGE DATA]`/`[/KNOWLEDGE DATA]` markers in `markdown` format responses. Summary format does not include full content, so framing is not applicable. JSON format includes raw content without framing markers.

### Response Format

- **AC-15**: All tools accept an optional `format` parameter with values `"summary"` (default), `"markdown"`, or `"json"`. Invalid values return a validation error listing valid options.
- **AC-16**: Summary format returns one compact line per entry (ID, title, category, tags, similarity if search) — optimized for minimal context window consumption. Agents use `context_get` for full content.
- **AC-17**: Markdown format returns full entry content with metadata header and output framing. JSON format returns structured objects (single) or arrays (multi-result). All formats include similarity scores for search results.

### Near-Duplicate Detection

- **AC-18**: `context_store` performs near-duplicate detection before insertion: embeds the new entry, searches existing entries at 0.92 similarity threshold, and returns the existing entry (with similarity score) instead of creating a duplicate when a match is found.
- **AC-19**: The near-duplicate response includes sufficient information (existing entry ID, content preview, similarity score) for the agent to decide whether to proceed differently.

### Audit Optimization

- **AC-20**: For `context_store`, the audit event is written in the same redb write transaction as the entry insert and vector mapping, eliminating the second write transaction per mutation.
- **AC-21**: For read-only tools, audit events are written in standalone write transactions (unchanged from vnc-001 pattern).
- **AC-22**: Audit event monotonic IDs and cross-session continuity are preserved after the optimization.

### Integration

- **AC-23**: All existing vnc-001 tests continue to pass (no regressions in server infrastructure, registry, audit, identity, error mapping).
- **AC-24**: All server code follows workspace conventions: `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89.

## Constraints

- **rmcp =0.16.0** pinned exactly, per vnc-001.
- **Rust edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`** per workspace.
- **No new crate dependencies** beyond `regex` (for content scanning) and potentially `once_cell` or std `OnceLock` (for compiled regex caching). Prefer std where available.
- **redb write serialization** — only one write transaction at a time across the entire database. Combined audit+data writes are essential for mutation throughput.
- **EmbedServiceHandle lazy loading** — context_search must handle the not-ready state without panicking.
- **Store engine auto-computes** `content_hash`, `previous_hash`, `version` on insert/update. Tool handlers provide `created_by`, `trust_source`, `feature_cycle` only.
- **Test infrastructure is cumulative** — build on vnc-001's 72 existing tests, fixtures, and patterns.
- **No hardcoded agent roles** — categories are data, trust levels are data. The category allowlist is a runtime data structure, not a Rust enum.

## Resolved Open Questions

1. **Content scanning severity levels**: RESOLVED — Hard-reject. `context_store` returns an error when injection patterns or PII are detected. Simpler, safer, no review workflow needed. Agents get an actionable error message identifying the scan failure category.

2. **Near-duplicate response as error vs success**: RESOLVED — Success with duplicate indicator. `context_store` returns a success response containing the existing entry's ID, similarity score, and a duplicate indicator in the requested format. The agent can then decide to adjust and retry or accept the existing entry. More ergonomic than forcing error handling for a non-error condition.

3. **Audit optimization scope**: RESOLVED — Extend the async wrapper API generally. Add a `insert_with_audit` (or similar combined-operation) method to `AsyncEntryStore` that accepts both entry data and an audit event, executing them in a single `spawn_blocking` call with one redb write transaction. This keeps the abstraction clean and is reusable for future mutating tools (vnc-003's `context_correct`).

## Tracking

- GH Issue: https://github.com/dug-21/unimatrix/issues/12
