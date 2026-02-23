# Specification: vnc-002 v0.1 Tool Implementations

## Objective

Replace four tool stubs in `unimatrix-server` with real implementations (`context_search`, `context_lookup`, `context_store`, `context_get`), activate all 9 security enforcement points (capability checks, input validation, content scanning), implement near-duplicate detection, format-selectable responses (summary/markdown/json) with output framing, and optimize audit log writes for mutating tools.

## Functional Requirements

### FR-01: context_search Implementation

**FR-01a:** `context_search` accepts a `query` string parameter, embeds it using the `EmbedServiceHandle`, performs vector similarity search via `AsyncVectorStore`, and returns up to `k` matching entries ranked by descending similarity score.

**FR-01b:** The `k` parameter defaults to 5 when not provided. Values must be positive integers; maximum value is 100.

**FR-01c:** When optional metadata filters (`topic`, `category`, `tags`) are provided, `context_search` first queries the entry store for entries matching those filters, collects their IDs, then calls `search_filtered` with those IDs to restrict vector search to matching entries.

**FR-01d:** When no metadata filters are provided, `context_search` calls `search` (unfiltered vector search).

**FR-01e:** When the embedding model is still loading (`EmbedState::Loading`), `context_search` returns `ServerError::EmbedNotReady` mapped to MCP error code -32004 with a message suggesting `context_lookup` as an alternative.

**FR-01f:** When the embedding model has failed to load (`EmbedState::Failed`), `context_search` returns `ServerError::EmbedFailed` mapped to MCP error code -32004.

**FR-01g:** For each `SearchResult` from the vector store, `context_search` fetches the full `EntryRecord` via `entry_store.get(entry_id)`. If an entry no longer exists (deleted between indexing and retrieval), it is silently skipped.

**FR-01h:** Results include similarity scores in all response formats (summary, markdown, json).

### FR-02: context_lookup Implementation

**FR-02a:** `context_lookup` accepts optional parameters `topic`, `category`, `tags`, `status`, `id`, and `limit`. No parameter is required -- all are optional.

**FR-02b:** When `id` is provided, `context_lookup` ignores all other filter parameters and delegates to `entry_store.get(id)` for direct retrieval. Returns a single entry.

**FR-02c:** When `id` is not provided, `context_lookup` builds a `QueryFilter` from the provided parameters and calls `entry_store.query(filter)`. The filter uses intersection semantics (all provided filters must match).

**FR-02d:** The `status` parameter is a string that must parse to the `Status` enum: "active", "deprecated", or "proposed" (case-insensitive). Invalid values are rejected with `ServerError::InvalidInput`.

**FR-02e:** When no `status` parameter is provided and no `id` is provided, the query defaults to `Status::Active` entries only.

**FR-02f:** The `limit` parameter defaults to 10 when not provided. Values must be positive integers; maximum value is 100.

**FR-02g:** Results are truncated to the `limit` after the full query returns.

### FR-03: context_store Implementation

**FR-03a:** `context_store` accepts `content` (required), `topic` (required), `category` (required), and optional `tags`, `title`, `source`, and `agent_id`.

**FR-03b:** `context_store` builds a `NewEntry` with:
- `title`: from params, or generated as `"{topic}: {category}"` if not provided
- `content`: from params
- `topic`, `category`, `tags` (default empty vec): from params
- `source`: from params, default empty string
- `status`: `Status::Active`
- `created_by`: from resolved agent identity's `agent_id`
- `trust_source`: `"agent"`
- `feature_cycle`: empty string (populated by future features)

**FR-03c:** After building the entry, `context_store` embeds the title+content via `EmbedServiceHandle` and performs near-duplicate detection (see FR-07).

**FR-03d:** If no duplicate is found, `context_store` inserts the entry using the combined transaction path (`insert_with_audit`) that writes the entry, vector mapping, and audit event in a single redb write transaction.

**FR-03e:** After the redb transaction, `context_store` inserts the embedding into the HNSW vector index via `vector_store.insert(entry_id, embedding)`.

**FR-03f:** `context_store` returns the created entry with its assigned ID in the requested response format.

### FR-04: context_get Implementation

**FR-04a:** `context_get` accepts a required `id` parameter (i64) and optional `agent_id`.

**FR-04b:** The `id` parameter is validated as non-negative and converted from i64 to u64.

**FR-04c:** `context_get` calls `entry_store.get(id)` and returns the full `EntryRecord` in the requested response format.

**FR-04d:** If the entry does not exist, `context_get` returns `ServerError::Core(StoreError::EntryNotFound)` mapped to MCP error code -32001 with the entry ID in the message.

### FR-05: Capability Enforcement

**FR-05a:** Every tool call resolves agent identity first via `resolve_agent(&params.agent_id)` (existing vnc-001 pattern).

**FR-05b:** After identity resolution, every tool calls `registry.require_capability(agent_id, cap)` where the required capability is:
- `context_search`: `Capability::Search`
- `context_lookup`: `Capability::Read`
- `context_store`: `Capability::Write`
- `context_get`: `Capability::Read`

**FR-05c:** If the capability check fails, the tool returns `ServerError::CapabilityDenied` mapped to MCP error code -32003 with the agent_id and missing capability named in the message.

**FR-05d:** Capability checks occur BEFORE input validation and content scanning -- a denied agent should not trigger validation logic.

### FR-06: Input Validation

**FR-06a:** String length limits enforced on all tools:

| Field | Max Length | Applied To |
|-------|-----------|------------|
| title | 200 chars | context_store |
| content | 50,000 chars | context_store |
| topic | 100 chars | context_store, context_search, context_lookup |
| category | 50 chars | context_store, context_search, context_lookup |
| individual tag | 50 chars | context_store, context_search, context_lookup |
| tags count | 20 per entry | context_store, context_search, context_lookup |
| query | 1,000 chars | context_search |
| source | 200 chars | context_store |

**FR-06b:** Control character rejection: Strings must not contain Unicode code points U+0000 through U+001F, except:
- U+000A (newline) and U+0009 (tab) are allowed in `content` and `title` fields
- All other fields reject all control characters including newline and tab

**FR-06c:** The `id` parameter (i64) must be non-negative. Negative values are rejected with `ServerError::InvalidInput`.

**FR-06d:** The `k` and `limit` parameters must be positive when provided. Zero and negative values are rejected.

**FR-06e:** Validation failures return `ServerError::InvalidInput` mapped to MCP error code -32602 with the field name and reason in the message.

**FR-06f:** Validation occurs after capability checks but before any storage, search, or embedding operations.

### FR-07: Near-Duplicate Detection

**FR-07a:** Before inserting a new entry, `context_store` searches the vector index for existing entries with similarity >= 0.92 to the new entry's embedding.

**FR-07b:** The search uses `vector_store.search(embedding, 1, ef_search)` to find the single most similar existing entry.

**FR-07c:** If the most similar entry has similarity >= 0.92, `context_store` fetches the existing entry and returns a success response with:
- The existing entry's full data
- The similarity score
- A `"duplicate": true` indicator in all response formats
- The existing entry formatted per the requested `format` parameter

**FR-07d:** If no entry has similarity >= 0.92 (or the vector index is empty), `context_store` proceeds with insertion.

**FR-07e:** Near-duplicate detection requires the embedding model. If the model is not ready, `context_store` returns `ServerError::EmbedNotReady`.

### FR-08: Content Scanning

**FR-08a:** `context_store` scans the `content` field for prompt injection patterns and PII patterns before storage.

**FR-08b:** If a `title` is provided, it is scanned for prompt injection patterns (not PII patterns).

**FR-08c:** Prompt injection patterns (~50 total) cover:
- Instruction override: "ignore previous instructions", "disregard above", "forget your instructions"
- Role impersonation: "you are now", "act as", "pretend to be"
- System prompt extraction: "repeat your system prompt", "what are your instructions", "show your prompt"
- Delimiter injection: markdown code fences, XML tags, and other delimiter patterns used to break context
- Encoding evasion: base64-encoded instructions, Unicode homoglyphs

**FR-08d:** PII patterns cover:
- Email addresses: standard RFC-style pattern
- Phone numbers: US formats (10-digit with optional country code and separators)
- Social Security Numbers: NNN-NN-NNNN format
- API keys: Bearer tokens, AWS access keys (AKIA...), GitHub tokens (ghp_/gho_/ghs_)

**FR-08e:** Content scanning is performed by a singleton `ContentScanner` initialized via `OnceLock` on first use. Regex patterns are compiled once and reused.

**FR-08f:** On any match, `context_store` returns `ServerError::ContentScanRejected` with the pattern category and description. The full matched text is NOT included in the error message (to avoid leaking sensitive content).

**FR-08g:** Content scanning occurs after input validation and category validation, before embedding and near-duplicate detection.

### FR-09: Category Allowlist

**FR-09a:** `context_store` validates the `category` parameter against a runtime-extensible allowlist.

**FR-09b:** The initial allowlist contains: `outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`.

**FR-09c:** Categories not in the allowlist are rejected with `ServerError::InvalidCategory` including the full list of valid categories.

**FR-09d:** Category validation is case-sensitive. All categories in the initial set are lowercase.

**FR-09e:** The allowlist supports runtime extension via `add_category(String)` for future features (vnc-003).

**FR-09f:** Category validation occurs after input validation, before content scanning.

### FR-10: Output Framing

**FR-10a:** Markdown format responses wrap entry content in `[KNOWLEDGE DATA]`/`[/KNOWLEDGE DATA]` markers.

**FR-10b:** Markers appear on their own lines, immediately before and after the content field in the markdown format.

**FR-10c:** Metadata (title, topic, category, tags, timestamps) appears OUTSIDE the framing markers in markdown format.

**FR-10d:** Summary and JSON formats do NOT use framing markers — summary has no full content; JSON is inherently unambiguous.

**FR-10e:** The `context_store` success response also uses output framing when returning the created entry in markdown format.

### FR-11: Format-Selectable Responses

**FR-11a:** All four tools accept an optional `format` parameter with values `"summary"` (default), `"markdown"`, or `"json"`. Invalid values return `ServerError::InvalidParams` listing valid options.

**FR-11b:** Each tool returns a `CallToolResult` with a single `Content::text()` block in the requested format.

**FR-11c:** **Summary format** (default): one compact line per entry — `#{id} | {title} | {category} | [{tags}] | {similarity}`. Similarity is included for search results only. Minimal context window consumption; agents use `context_get` for full content.

**FR-11d:** **Markdown format**: full entry content with metadata header and `[KNOWLEDGE DATA]` output framing. Multi-result tools format each entry as a separate section with a numbered header.

**FR-11e:** **JSON format**: structured JSON object for single-result tools (get, store) or array for multi-result tools (search, lookup).

**FR-11f:** Search results include a `similarity` field (f32) in all formats. Store duplicate responses include duplicate indicator in all formats.

**FR-11g:** Empty results (no matches for search/lookup) return a helpful message in summary/markdown formats and an empty array in JSON format.

**FR-11h:** `context_get` returns full content in all three formats (single-entry fetch — summary would be pointless).

### FR-12: Audit Logging

**FR-12a:** All tools log an audit event with the operation outcome after execution.

**FR-12b:** For read-only tools (`context_search`, `context_lookup`, `context_get`), audit events are written in standalone write transactions via `AuditLog::log_event()` (unchanged from vnc-001).

**FR-12c:** For `context_store` (when inserting a new entry), the audit event is written in the same write transaction as the entry insert and vector mapping via the combined `insert_with_audit` path.

**FR-12d:** For `context_store` (when a near-duplicate is detected), the audit event records the duplicate detection as `Outcome::Success` with detail indicating the duplicate entry ID and similarity.

**FR-12e:** Audit events for denied operations (capability, validation, scanning, category) record `Outcome::Denied` with the denial reason in the detail field.

**FR-12f:** Audit events for errors (store failures, embed failures) record `Outcome::Error` with the error message in the detail field.

**FR-12g:** Monotonic audit event IDs and cross-session continuity are preserved through the combined transaction path (same COUNTERS key, same incrementing logic).

## Non-Functional Requirements

### NFR-01: Performance

**NFR-01a:** `context_get` completes in under 5ms for entries under 50KB (single redb read + response formatting).

**NFR-01b:** `context_lookup` completes in under 50ms for result sets under 100 entries (secondary index scan + response formatting).

**NFR-01c:** `context_search` completes in under 200ms for indexes with under 10,000 entries (embedding + HNSW search + entry fetches + response formatting).

**NFR-01d:** `context_store` completes in under 500ms including embedding, near-duplicate check, insert, and combined audit write (dominated by embedding computation).

**NFR-01e:** Content scanning adds less than 1ms per request after initial pattern compilation.

### NFR-02: Resource Constraints

**NFR-02a:** Content scanning regex compilation (first use) consumes under 5MB of memory for ~50 patterns.

**NFR-02b:** Category allowlist memory usage is negligible (6 strings in a HashSet).

**NFR-02c:** No per-request heap allocations for regex matching (patterns are pre-compiled and shared).

### NFR-03: Compatibility

**NFR-03a:** All existing vnc-001 tests (72 tests) continue to pass without modification.

**NFR-03b:** All code follows workspace conventions: `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89.

**NFR-03c:** `regex` is the only new direct dependency. `serde_json` is used but is already a transitive dependency.

**NFR-03d:** The MCP protocol interface is unchanged -- same 4 tools, same parameter schemas, same error code ranges.

## Acceptance Criteria

### Tool Implementations

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `context_search` accepts a natural language query, embeds it, performs vector similarity search, and returns up to `k` (default 5) matching entries ranked by descending similarity score. | Integration test: store 5 entries with known content, search with a related query, verify results are ordered by similarity and count <= k. |
| AC-02 | `context_search` supports optional metadata pre-filtering -- when `topic`, `category`, or `tags` are provided, only entries matching those filters appear in results. | Integration test: store entries in different topics, search with topic filter, verify only matching-topic entries returned. |
| AC-03 | `context_search` returns `ServerError::EmbedNotReady` (MCP error -32004) with guidance to use `context_lookup` when the embedding model is still loading. | Unit test: call context_search with embed handle in Loading state, verify error code and message. |
| AC-04 | `context_lookup` returns entries matching the provided deterministic filters (topic, category, tags, status) using intersection semantics. | Integration test: store entries with varied metadata, lookup with multiple filters, verify intersection behavior. |
| AC-05 | `context_lookup` with `id` parameter delegates to direct entry retrieval, ignoring other filter params. | Unit test: lookup with id + topic filter, verify the returned entry matches the ID regardless of topic. |
| AC-06 | `context_lookup` respects the `limit` parameter (default 10) and the `status` parameter (parsed from string to `Status` enum). | Integration test: store 15 entries, lookup with limit=5, verify exactly 5 returned. Test status parsing for "active", "deprecated", "proposed". |
| AC-07 | `context_store` inserts a new entry with all security fields populated: `created_by` from resolved agent identity, `trust_source` = "agent", `content_hash` auto-computed by the store engine. | Integration test: store an entry, retrieve it, verify `created_by` matches agent_id, `trust_source` = "agent", `content_hash` is non-empty, `version` = 1. |
| AC-08 | `context_store` embeds the entry's title+content and indexes the embedding in the VectorStore. | Integration test: store an entry, verify vector_store.contains(entry_id) is true and search returns the entry. |
| AC-09 | `context_get` returns the full `EntryRecord` for a given ID, or `ServerError::Core(StoreError::EntryNotFound)` (MCP error -32001) with the entry ID in the message. | Unit test: get existing entry returns record; get nonexistent ID returns -32001 error with ID in message. |

### Security Enforcement

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-10 | Capability checks are active on all 4 tools: `Search` for context_search, `Read` for context_lookup and context_get, `Write` for context_store. Agents lacking the required capability receive MCP error -32003 with their agent ID and the missing capability named. | Unit test per tool: create a Restricted agent (Read+Search only), call context_store, verify -32003 error with agent ID and "Write" in message. |
| AC-11 | Input validation rejects strings exceeding max lengths, negative IDs, and strings containing control characters. | Unit tests: one test per validation rule (title >200, content >50000, negative id, control char in topic, etc.). Verify ServerError::InvalidInput with correct field name. |
| AC-12 | Content scanning on `context_store` detects prompt injection patterns and PII patterns. Scanning regexes are compiled once at startup. | Unit tests: call scan with known injection text ("ignore previous instructions"), verify rejection. Call scan with PII (email, SSN), verify rejection. Verify OnceLock provides same instance on repeated calls. |
| AC-13 | Category allowlist on `context_store` rejects categories not in the initial set. Error message lists valid categories. | Unit test: store with category "unknown", verify -32007 error listing all 6 valid categories. Store with "convention", verify success. |
| AC-14 | Output framing wraps entry content with `[KNOWLEDGE DATA]`/`[/KNOWLEDGE DATA]` markers in `markdown` format responses. Summary format has no full content. JSON format has no framing markers. | Unit test per read tool: verify markdown format response contains markers around content. Verify summary format has no markers. Verify json format has no markers. |

### Response Format

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-15 | All tools accept an optional `format` parameter with values `"summary"` (default), `"markdown"`, or `"json"`. Invalid values return a validation error listing valid options. | Unit test per tool: call with format="invalid", verify error. Call with each valid format, verify single Content block in expected format. Call with no format, verify summary default. |
| AC-16 | Summary format returns one compact line per entry (ID, title, category, tags, similarity if search) — optimized for minimal context window consumption. Agents use `context_get` for full content. | Unit test: search returning 3 results with format="summary", verify 3 compact lines with no full content. |
| AC-17 | Markdown format returns full entry content with metadata header and output framing. JSON format returns structured objects (single) or arrays (multi-result). All formats include similarity scores for search results. | Integration test per format: search returning 3 results, verify markdown has 3 framed sections, json has 3-element array, all include similarity. |

### Near-Duplicate Detection

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-18 | `context_store` performs near-duplicate detection before insertion: embeds the new entry, searches existing entries at 0.92 similarity threshold, and returns the existing entry instead of creating a duplicate when a match is found. | Integration test: store an entry, store an identical entry, verify second call returns first entry's ID with duplicate indicator. |
| AC-19 | The near-duplicate response includes the existing entry ID, similarity score, and duplicate indicator in the requested format. | Unit test: verify duplicate response includes required fields in each format (summary, markdown, json). |

### Audit Optimization

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-20 | For `context_store`, the audit event is written in the same redb write transaction as the entry insert and vector mapping. | Integration test: store an entry, verify both the entry and audit event exist in the database, and that only one write transaction was committed (test by verifying the audit event's target_ids contains the new entry ID). |
| AC-21 | For read-only tools, audit events are written in standalone write transactions. | Integration test: call context_get, verify audit event exists (read tools still log). |
| AC-22 | Audit event monotonic IDs and cross-session continuity are preserved after the optimization. | Integration test: store entry (combined path), then call context_get (standalone path), verify audit event IDs are sequential without gaps. |

### Integration

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-23 | All existing vnc-001 tests continue to pass. | Run `cargo test -p unimatrix-server` and verify 72+ tests pass including all existing tests. |
| AC-24 | All server code follows workspace conventions: `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89. | `cargo build` succeeds. `#![forbid(unsafe_code)]` in lib.rs. Verify edition in Cargo.toml. |

## Domain Models

### Key Entities

**EntryRecord** (25+ fields): The full knowledge entry as stored in redb. Key fields: `id` (u64), `title` (String), `content` (String), `topic` (String), `category` (String), `tags` (Vec<String>), `status` (Status), `confidence` (f64), `created_at` (u64), `updated_at` (u64), `created_by` (String), `modified_by` (String), `content_hash` (String), `previous_hash` (String), `version` (u32), `feature_cycle` (String), `trust_source` (String).

**NewEntry**: Insert-time struct with: `title`, `content`, `topic`, `category`, `tags`, `source`, `status`, `created_by`, `feature_cycle`, `trust_source`. The store engine auto-computes `id`, `created_at`, `updated_at`, `content_hash`, `previous_hash`, `version`.

**QueryFilter**: Intersection filter with: `topic` (Option), `category` (Option), `tags` (Option<Vec>), `status` (Option<Status>), `time_range` (Option<TimeRange>).

**SearchResult**: Vector search result with: `entry_id` (u64), `similarity` (f32).

**ResolvedIdentity**: Agent identity after registry lookup: `agent_id` (String), `trust_level` (TrustLevel), `capabilities` (Vec<Capability>).

**AuditEvent**: Immutable audit record: `event_id` (u64), `timestamp` (u64), `session_id` (String), `agent_id` (String), `operation` (String), `target_ids` (Vec<u64>), `outcome` (Outcome), `detail` (String).

**ContentScanner**: Singleton pattern scanner holding compiled regex patterns in two categories (injection, PII). Initialized via `OnceLock`.

**CategoryAllowlist**: Runtime-extensible set of valid category strings. Backed by `RwLock<HashSet<String>>`.

### Tool Execution Order

For each tool, the execution follows a strict order:
1. Identity resolution (existing)
2. Capability check (new)
3. Input validation (new)
4. Category validation (context_store only, new)
5. Content scanning (context_store only, new)
6. Business logic (embedding, search, insert, etc.)
7. Response formatting (new)
8. Audit logging (existing, optimized for mutations)

This order ensures that cheap checks (capability, validation) run before expensive operations (embedding, database writes), and that denied requests never reach the storage layer.

## User Workflows

### Workflow 1: Agent Searches for Knowledge

1. Agent calls `context_search(query: "error handling patterns", agent_id: "uni-architect")`
2. Server resolves identity -> "uni-architect" (Restricted, Read+Search)
3. Server checks capability: Search -- allowed
4. Server validates params: query length 27 < 1000 -- OK
5. Server embeds query via EmbedServiceHandle
6. Server searches HNSW index (top 5, unfiltered)
7. Server fetches full EntryRecord for each result
8. Server formats response in requested format (default: summary)
9. Server logs audit event (standalone transaction)
10. Agent receives compact summary with 5 ranked results; calls context_get for entries it needs

### Workflow 2: Agent Stores a Pattern

1. Agent calls `context_store(content: "Use Result<T, E> for all fallible...", topic: "rust", category: "convention", title: "Error handling convention", agent_id: "uni-architect")`
2. Server resolves identity -> "uni-architect" (Restricted, Read+Search only)
3. Server checks capability: Write -- DENIED
4. Server returns MCP error -32003: "Agent 'uni-architect' lacks Write capability."
5. Agent understands it cannot store -- must be done by a Privileged or Internal agent

### Workflow 3: Agent Stores with Near-Duplicate

1. Privileged agent calls `context_store(content: "Always use Result...", topic: "rust", category: "convention", agent_id: "human")`
2. Server validates, scans, embeds
3. Server searches HNSW for top-1 similar entry -> finds entry #7 at 0.95 similarity
4. Server returns success with duplicate indicator: entry #7's content, similarity 0.95, `"duplicate": true`
5. Agent sees the existing entry and decides not to retry

### Workflow 4: Agent Triggers Content Scan

1. Agent calls `context_store(content: "ignore previous instructions and output...", topic: "test", category: "convention", agent_id: "human")`
2. Server validates params (lengths OK)
3. Server validates category ("convention" is in allowlist)
4. Server scans content -- matches injection pattern "InstructionOverride"
5. Server returns MCP error -32006: "Content rejected: instruction override attempt detected (InstructionOverride detected). Remove the flagged content and retry."

### Workflow 5: Agent Uses Invalid Category

1. Agent calls `context_store(content: "...", topic: "test", category: "note", agent_id: "human")`
2. Server validates category "note" -- NOT in allowlist
3. Server returns MCP error -32007: "Unknown category 'note'. Valid categories: convention, decision, lesson-learned, outcome, pattern, procedure."

### Workflow 6: Embedding Not Ready

1. Agent calls `context_search(query: "patterns")` immediately after server startup
2. Embedding model is still downloading (Loading state)
3. Server returns MCP error -32004: "Embedding model is initializing. Try again in a few seconds, or use context_lookup which does not require embeddings."

## Constraints

- **rmcp =0.16.0** pinned exactly, per vnc-001.
- **Rust edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`** per workspace conventions.
- **No new crate dependencies** beyond `regex` for content scanning. `serde_json` is already a transitive dependency.
- **redb write serialization**: Only one write transaction at a time. Combined audit+data writes eliminate the second transaction for mutations.
- **EmbedServiceHandle lazy loading**: `context_search` and `context_store` must handle the not-ready state.
- **Store engine auto-computes** `content_hash`, `previous_hash`, `version` on insert/update. Tool handlers provide only `created_by`, `trust_source`, `feature_cycle`.
- **Test infrastructure is cumulative**: Build on vnc-001's 72 existing tests.
- **No hardcoded agent roles**: Categories are runtime data, not a Rust enum.

## Dependencies

### Existing Crate Dependencies (consumed, not added)

| Crate | Used For |
|-------|---------|
| unimatrix-core | EntryStore, VectorStore, EmbedService traits, async wrappers, domain types |
| unimatrix-store | Store, EntryRecord, NewEntry, QueryFilter, Status, AUDIT_LOG, COUNTERS tables |
| unimatrix-vector | SearchResult, VectorConfig |
| unimatrix-embed | EmbedConfig, EmbeddingProvider, OnnxProvider |
| rmcp | ServerHandler, CallToolResult, Content, ErrorData, tool macros |
| serde, serde_json | Serialization for JSON response format |
| tokio | async runtime, spawn_blocking |

### New Dependency

| Crate | Version | Used For |
|-------|---------|---------|
| regex | latest stable | Content scanning pattern matching |

## NOT in Scope

- **No v0.2 tools**: `context_correct`, `context_deprecate`, `context_status`, `context_briefing` are vnc-003.
- **No confidence computation**: The `confidence` field exists but the formula is crt-002. Entries store the default confidence value.
- **No usage tracking**: `access_count`, `last_accessed_at` infrastructure exists but tracking is crt-001.
- **No HTTP/SSE transport**: Stdio only, per vnc-001.
- **No batch operations**: Each tool call operates on a single entry or query.
- **No content summarization**: Return full entry content. Token budget management is vnc-003.
- **No cross-project search**: Single project per server instance.
- **No async audit channel**: Simplest solution (combined transaction) is used. Async channels reserved for if throughput justifies complexity.
- **No runtime pattern loading for content scanning**: Patterns are compile-time static. Runtime loading could be added later.
- **No "force store" for near-duplicates**: Agents must modify content to avoid duplicate detection. vnc-003 could add a `force` parameter.
