# Architecture: vnc-002 v0.1 Tool Implementations

## System Overview

vnc-002 replaces the four tool stubs in `unimatrix-server` with real implementations and activates the security enforcement points established by vnc-001. It adds four new modules to the crate (`validation.rs`, `scanning.rs`, `response.rs`, `categories.rs`), rewrites `tools.rs` from stubs to working handlers, extends `error.rs` with new error variants, and extends the async wrapper layer to support combined audit+data write transactions.

vnc-002 does not add new crates or change the server's external interface (same 4 tools, same parameter schemas, same MCP transport). It is entirely contained within `crates/unimatrix-server/` plus a targeted extension to `crates/unimatrix-core/src/async_wrappers.rs` for the combined transaction method.

```
Claude Code / MCP Client
        |
        | stdio (JSON-RPC 2.0)
        v
unimatrix-server (binary)
  |-- ServerHandler (rmcp)          -- unchanged from vnc-001
  |-- ToolRouter                    -- REAL implementations (vnc-002)
  |     |-- validation.rs           -- NEW: input validation functions
  |     |-- scanning.rs             -- NEW: content scanning (injection + PII)
  |     |-- response.rs             -- NEW: format-selectable responses + output framing
  |     |-- categories.rs           -- NEW: category allowlist
  |     '-- tools.rs                -- REWRITTEN: stub -> real handlers
  |
  |-- AsyncEntryStore               -- EXTENDED: insert_with_audit() method
  |-- AsyncVectorStore              -- unchanged
  |-- EmbedServiceHandle            -- consumed (get_adapter().await)
  |-- AgentRegistry                 -- consumed (require_capability())
  |-- AuditLog                      -- consumed + extended (write_in_txn())
  |
  v
unimatrix-core (traits + async wrappers)
  |         |          |
  v         v          v
store    vector      embed
```

## Component Breakdown

### C1: Input Validation (`crates/unimatrix-server/src/validation.rs`) -- NEW

Validates all tool parameters before any storage or search operation.

**Responsibilities:**
- Max length enforcement for string params (title: 200, content: 50000, topic: 100, category: 50, tag: 50, query: 1000, source: 200)
- Max count enforcement (tags: 20 per entry)
- Control character rejection (U+0000-U+001F except U+000A and U+0009 in content fields)
- Non-negative ID validation (i64 -> u64 conversion with rejection of negative values)
- Status string parsing ("active"/"deprecated"/"proposed" -> Status enum)
- Validation of k/limit defaults (k default 5, limit default 10)

**Interface:**
```rust
pub fn validate_search_params(params: &SearchParams) -> Result<(), ServerError>;
pub fn validate_lookup_params(params: &LookupParams) -> Result<(), ServerError>;
pub fn validate_store_params(params: &StoreParams) -> Result<(), ServerError>;
pub fn validate_get_params(params: &GetParams) -> Result<(), ServerError>;
pub fn parse_status(s: &str) -> Result<Status, ServerError>;
pub fn validated_id(id: i64) -> Result<u64, ServerError>;
pub fn validated_k(k: Option<i64>) -> Result<usize, ServerError>;      // default 5
pub fn validated_limit(limit: Option<i64>) -> Result<usize, ServerError>; // default 10
```

**Design notes:**
- Each `validate_*_params` function returns `ServerError::InvalidInput` on failure
- Validation functions are pure -- no I/O, no state. They take param references and return Result.
- Control character check uses a single pass over each string, checking each char's Unicode scalar value
- The i64-to-u64 conversion is needed because JSON numbers arrive as i64 via serde

### C2: Content Scanning (`crates/unimatrix-server/src/scanning.rs`) -- NEW

Detects prompt injection and PII patterns in content destined for storage.

**Responsibilities:**
- Compile ~50 regex patterns at startup via `std::sync::OnceLock`
- Categorize patterns: injection (instruction override, role impersonation, system prompt extraction, delimiter injection) and PII (email, phone, SSN, API key)
- Return scan results with the category and matched pattern for actionable error messages
- Hard-reject on any match (no flagging/review workflow)

**Interface:**
```rust
pub struct ContentScanner {
    injection_patterns: Vec<CompiledPattern>,
    pii_patterns: Vec<CompiledPattern>,
}

pub struct CompiledPattern {
    pub category: PatternCategory,
    pub description: &'static str,
    pub regex: Regex,
}

pub enum PatternCategory {
    InstructionOverride,
    RoleImpersonation,
    SystemPromptExtraction,
    DelimiterInjection,
    EncodingEvasion,
    EmailAddress,
    PhoneNumber,
    SocialSecurityNumber,
    ApiKey,
}

pub struct ScanResult {
    pub category: PatternCategory,
    pub description: &'static str,
    pub matched_text: String,
}

impl ContentScanner {
    pub fn global() -> &'static ContentScanner;  // OnceLock singleton
    pub fn scan(&self, content: &str) -> Result<(), ScanResult>;
    pub fn scan_title(&self, title: &str) -> Result<(), ScanResult>;
}
```

**Design notes:**
- `ContentScanner::global()` uses `std::sync::OnceLock` to compile patterns exactly once
- Patterns are applied to content and title separately (title gets injection patterns only, content gets all)
- The `regex` crate is the only new dependency
- Pattern set is defined as static data in the module, compiled into `Regex` objects on first access
- Scan checks injection patterns first (higher priority), then PII patterns
- On match, returns `ScanResult` which tools.rs converts to `ServerError::ContentScanRejected`

### C3: Response Formatting (`crates/unimatrix-server/src/response.rs`) -- NEW

Produces format-selectable responses (summary/markdown/json) with output framing on markdown.

**Responsibilities:**
- Accept a `ResponseFormat` enum (Summary, Markdown, Json) parsed from the optional `format` tool parameter
- Summary format: one compact line per entry (ID, title, category, tags, similarity if search) — minimal context footprint
- Markdown format: full entry content with metadata header and `[KNOWLEDGE DATA]` output framing
- Json format: structured JSON object (single) or array (multi-result)
- Include similarity scores in search results (all formats)
- Include duplicate indicator in store responses (all formats)
- Produce `CallToolResult` with a single `Content::text()` block in the requested format

**Interface:**
```rust
pub enum ResponseFormat { Summary, Markdown, Json }

pub fn parse_format(format: &Option<String>) -> Result<ResponseFormat, ServerError>;
pub fn format_single_entry(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult;
pub fn format_search_results(results: &[(EntryRecord, f32)], format: ResponseFormat) -> CallToolResult;
pub fn format_lookup_results(entries: &[EntryRecord], format: ResponseFormat) -> CallToolResult;
pub fn format_store_success(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult;
pub fn format_duplicate_found(
    existing: &EntryRecord,
    similarity: f32,
    format: ResponseFormat,
) -> CallToolResult;
pub fn format_empty_results(tool: &str, format: ResponseFormat) -> CallToolResult;
```

**Markdown format (per entry, with output framing):**
```
## Context: {title}
**Topic:** {topic} | **Category:** {category} | **Tags:** {tags}
**Confidence:** {confidence} | **Status:** {status}

[KNOWLEDGE DATA]
{content}
[/KNOWLEDGE DATA]

*Entry #{id} | Created {created_at} | Updated {updated_at}*
```

**JSON format (per entry):**
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
  "created_at": 1700000000,
  "created_by": "architect"
}
```

Search results add similarity scores in all formats. Store duplicate responses add duplicate indicator in all formats.

**Design notes:**
- `CallToolResult::success(vec![Content::text(formatted_output)])` produces a single content block in the requested format
- Output framing markers `[KNOWLEDGE DATA]`/`[/KNOWLEDGE DATA]` wrap only the content field in markdown format, not in summary or json
- Summary format: `#42 | Convention: Use conventional commits | convention | [git, workflow] | 0.94` — one line per entry
- Timestamps are formatted as ISO 8601 strings in markdown, unix seconds in JSON
- Empty results return a helpful message suggesting broader filters

### C4: Category Allowlist (`crates/unimatrix-server/src/categories.rs`) -- NEW

Runtime-extensible category validation.

**Responsibilities:**
- Maintain the set of allowed categories
- Validate category strings against the set
- Provide actionable error listing valid categories
- Support runtime extension (vnc-003 can add categories)

**Interface:**
```rust
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,
}

impl CategoryAllowlist {
    pub fn new() -> Self;                              // initial set
    pub fn validate(&self, category: &str) -> Result<(), ServerError>;
    pub fn add_category(&self, category: String);      // runtime extension
    pub fn list_categories(&self) -> Vec<String>;      // for error messages
}
```

**Initial category set:** `{outcome, lesson-learned, decision, convention, pattern, procedure}`

**Design notes:**
- `RwLock<HashSet<String>>` allows concurrent reads (validation) with rare writes (extension)
- The allowlist is stored on `UnimatrixServer` as `Arc<CategoryAllowlist>`
- Validation is case-sensitive (categories are lowercase by convention)
- Error message includes the full list of valid categories for agent guidance
- Not backed by redb -- runtime-only. vnc-003 may persist to database.

### C5: Tool Implementations (`crates/unimatrix-server/src/tools.rs`) -- REWRITTEN

Replaces all four stubs with real implementations.

**context_search flow:**
1. Resolve agent identity (existing pattern)
2. `registry.require_capability(agent_id, Capability::Search)`
3. `validate_search_params(&params)`
4. `let format = parse_format(&params.format)`
5. `embed_service.get_adapter().await` -> handle EmbedNotReady/EmbedFailed
6. `adapter.embed_entry("", &params.query)` via spawn_blocking
7. If metadata filters provided (topic/category/tags):
   a. Build QueryFilter, query entry_store for matching IDs
   b. `vector_store.search_filtered(embedding, k, ef_search, allowed_ids)`
8. Else: `vector_store.search(embedding, k, ef_search)`
9. Fetch full EntryRecord for each result via `entry_store.get(id)`
10. `response::format_search_results(results_with_scores, format)`
11. Audit log (standalone write transaction, Outcome::Success)

**context_lookup flow:**
1. Resolve agent identity
2. `registry.require_capability(agent_id, Capability::Read)`
3. `validate_lookup_params(&params)`
4. `let format = parse_format(&params.format)`
5. If `id` provided: `validated_id(id)` -> `entry_store.get(id)` -> `format_single_entry(entry, format)`
6. Else: build QueryFilter from params, `entry_store.query(filter)`, apply limit, `format_lookup_results(entries, format)`
7. Audit log (standalone write transaction)

**context_store flow:**
1. Resolve agent identity
2. `registry.require_capability(agent_id, Capability::Write)`
3. `validate_store_params(&params)`
4. `let format = parse_format(&params.format)`
5. `categories.validate(&params.category)`
6. `ContentScanner::global().scan(&params.content)` + `scan_title` if title provided
7. `embed_service.get_adapter().await` -> embed title+content
8. Near-duplicate search: `vector_store.search(embedding, 1, ef_search)` -> check if similarity >= 0.92
9. If duplicate found: fetch existing entry, `format_duplicate_found(existing, similarity, format)` -> return success
10. Build NewEntry with security fields (created_by from identity, trust_source = "agent")
11. Combined transaction: `insert_with_audit(entry, embedding, audit_event)` (single write txn)
12. `format_store_success(stored_entry, format)`

**context_get flow:**
1. Resolve agent identity
2. `registry.require_capability(agent_id, Capability::Read)`
3. `validate_get_params(&params)` -> `validated_id(params.id)`
4. `let format = parse_format(&params.format)`
5. `entry_store.get(id)`
6. `format_single_entry(entry, format)`
7. Audit log (standalone write transaction)

**Error handling:**
- Capability denied -> ServerError::CapabilityDenied -> MCP error -32003
- Validation failure -> ServerError::InvalidInput -> MCP error -32602
- Content scan hit -> ServerError::ContentScanRejected -> MCP error -32006
- Category invalid -> ServerError::InvalidCategory -> MCP error -32007
- Embed not ready -> ServerError::EmbedNotReady -> MCP error -32004
- Entry not found -> ServerError::Core(StoreError::EntryNotFound) -> MCP error -32001

### C6: Audit Transaction Optimization (`crates/unimatrix-core/src/async_wrappers.rs` + `crates/unimatrix-server/src/audit.rs`) -- EXTENDED

Enables combined data+audit writes in a single redb write transaction.

**What changes:**

1. **AuditLog** gets a new method `write_in_txn` that writes an audit event into an existing redb write transaction instead of opening its own:

```rust
impl AuditLog {
    // Existing -- opens its own write transaction
    pub fn log_event(&self, event: AuditEvent) -> Result<(), ServerError>;

    // NEW -- writes into caller's transaction
    pub fn write_in_txn(
        &self,
        txn: &redb::WriteTransaction,
        event: AuditEvent,
    ) -> Result<u64, ServerError>;
}
```

2. **AsyncEntryStore** gets a new method `insert_with_audit` that performs entry insert + vector mapping + audit event in one spawn_blocking call with one write transaction:

```rust
impl<T: EntryStore + 'static> AsyncEntryStore<T> {
    // NEW -- combined insert + audit in single transaction
    pub async fn insert_with_audit(
        &self,
        entry: NewEntry,
        audit_event: AuditEvent,
        store: Arc<Store>,
        audit_log: Arc<AuditLog>,
    ) -> Result<(u64, EntryRecord), CoreError>;
}
```

**Design notes:**
- `write_in_txn` does NOT commit the transaction -- the caller commits after all writes
- `write_in_txn` still uses the COUNTERS table for monotonic ID generation within the transaction
- The `insert_with_audit` method needs access to the raw `Store` (for begin_write) and `AuditLog` (for write_in_txn). It bypasses the normal `EntryStore::insert` trait method and calls `Store` directly.
- This means `AsyncEntryStore` needs to hold a reference to `Arc<Store>` in addition to the trait object. Alternatively, the combined operation lives on a new struct or on `UnimatrixServer` directly.
- **Chosen approach (ADR-001):** Add `insert_with_audit` as a method on `UnimatrixServer` itself, since it coordinates store, audit, and vector store. This avoids polluting the generic `AsyncEntryStore` with server-specific concerns.

**Revised interface (on UnimatrixServer):**

```rust
impl UnimatrixServer {
    /// Combined insert + vector mapping + audit in single write transaction.
    /// Used by context_store for optimized mutation path.
    async fn insert_with_audit(
        &self,
        entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(u64, EntryRecord), ServerError>;
}
```

This method:
1. Calls `spawn_blocking` with clones of `Arc<Store>` and `Arc<AuditLog>`
2. Inside the blocking closure: `store.begin_write()`, insert entry, put vector mapping, write audit event, commit
3. Then (outside the write txn): `vector_store.insert(entry_id, embedding)` for the HNSW index (separate from redb)
4. Returns the new entry ID and full EntryRecord

### C7: Server Error Extensions (`crates/unimatrix-server/src/error.rs`) -- EXTENDED

New `ServerError` variants for vnc-002's enforcement points.

**New variants:**
```rust
pub enum ServerError {
    // ... existing variants ...

    /// Input validation failure.
    InvalidInput {
        field: String,
        reason: String,
    },

    /// Content scan detected prohibited pattern.
    ContentScanRejected {
        category: String,
        description: String,
    },

    /// Category not in allowlist.
    InvalidCategory {
        category: String,
        valid_categories: Vec<String>,
    },
}
```

**New MCP error code constants:**
```rust
pub const ERROR_CONTENT_SCAN_REJECTED: ErrorCode = ErrorCode(-32006);
pub const ERROR_INVALID_CATEGORY: ErrorCode = ErrorCode(-32007);
```

**Error mapping:**

| ServerError variant | MCP code | Message format |
|---|---|---|
| InvalidInput { field, reason } | -32602 | "Invalid parameter '{field}': {reason}" |
| ContentScanRejected { category, description } | -32006 | "Content rejected: {description} ({category} detected). Remove the flagged content and retry." |
| InvalidCategory { category, valid_categories } | -32007 | "Unknown category '{category}'. Valid categories: {list}." |

### C8: UnimatrixServer State Extension

The server struct gains one new field:

```rust
pub struct UnimatrixServer {
    // ... existing fields ...
    pub(crate) categories: Arc<CategoryAllowlist>,
    pub(crate) store: Arc<Store>,  // raw Store for combined transactions
}
```

The raw `Store` reference is needed for `insert_with_audit` to open its own write transaction that spans both entry insert and audit event.

## Component Interactions

### Data Flow: context_search

```
MCP Client -> tools/call "context_search"
  |
  v
1. resolve_agent(&params.agent_id) -> ResolvedIdentity
2. registry.require_capability(agent_id, Search)
3. validate_search_params(&params)
4. let format = parse_format(&params.format)
5. embed_service.get_adapter().await -> Arc<EmbedAdapter>
6. adapter.embed_entry("", query).await -> Vec<f32>
7. [IF filters provided]
     a. Build QueryFilter { topic, category, tags, status: Some(Active) }
     b. entry_store.query(filter).await -> Vec<EntryRecord>
     c. allowed_ids = records.iter().map(|r| r.id).collect()
     d. vector_store.search_filtered(embedding, k, ef, allowed_ids).await
   [ELSE]
     d. vector_store.search(embedding, k, ef).await
8. For each SearchResult: entry_store.get(entry_id).await
9. response::format_search_results(entries_with_scores, format)
10. audit.log_event(Success, target_ids=[...])
  |
  v
CallToolResult -> MCP Client
```

### Data Flow: context_store (with near-duplicate + combined transaction)

```
MCP Client -> tools/call "context_store"
  |
  v
1. resolve_agent(&params.agent_id) -> ResolvedIdentity
2. registry.require_capability(agent_id, Write)
3. validate_store_params(&params)
4. let format = parse_format(&params.format)
5. categories.validate(&params.category)
6. ContentScanner::global().scan(&params.content)
7. embed_service.get_adapter().await -> embed title+content -> Vec<f32>
8. vector_store.search(embedding, 1, ef).await -> check similarity
  |
  [IF similarity >= 0.92]
     a. entry_store.get(duplicate_entry_id).await
     b. audit.log_event(Success, detail: "near-duplicate detected")
     c. response::format_duplicate_found(existing, similarity, format)
     d. RETURN
  |
  [ELSE: no duplicate]
9. Build NewEntry { created_by: identity.agent_id, trust_source: "agent", ... }
10. self.insert_with_audit(entry, embedding, audit_event).await
     |-> spawn_blocking:
         a. store.begin_write()
         b. Insert entry into ENTRIES + secondary indexes
         c. Put vector mapping (entry_id -> hnsw_data_id)
         d. Write audit event via audit.write_in_txn(txn, event)
         e. txn.commit()
     |-> vector_store.insert(entry_id, embedding).await  [HNSW index, separate]
11. entry_store.get(new_id).await -> full EntryRecord
12. response::format_store_success(entry, format)
  |
  v
CallToolResult -> MCP Client
```

### Data Flow: context_lookup

```
MCP Client -> tools/call "context_lookup"
  |
  v
1. resolve_agent -> require_capability(Read) -> validate_lookup_params
2. let format = parse_format(&params.format)
3. [IF id provided]
     a. validated_id(id) -> u64
     b. entry_store.get(id).await
     c. response::format_single_entry(entry, format)
   [ELSE]
     a. Build QueryFilter from topic/category/tags/status
     b. entry_store.query(filter).await
     c. Apply limit (truncate results)
     d. response::format_lookup_results(entries, format)
4. audit.log_event(Success)
  |
  v
CallToolResult -> MCP Client
```

### Data Flow: context_get

```
MCP Client -> tools/call "context_get"
  |
  v
1. resolve_agent -> require_capability(Read) -> validate_get_params
2. let format = parse_format(&params.format)
3. validated_id(params.id) -> u64
4. entry_store.get(id).await
5. response::format_single_entry(entry, format)
6. audit.log_event(Success, target_ids=[id])
  |
  v
CallToolResult -> MCP Client
```

## Technology Decisions

| Decision | Choice | Rationale | ADR |
|----------|--------|-----------|-----|
| Audit optimization | Combined write txn on UnimatrixServer | Eliminates 2nd write txn per mutation; keeps AsyncEntryStore generic | ADR-001 |
| Content scanning | OnceLock + regex crate, ~50 patterns | Compiled once, zero per-request allocation; regex is battle-tested | ADR-002 |
| Category allowlist | RwLock<HashSet<String>> at runtime | Extensible without code changes; no enum dispatch overhead | ADR-003 |
| Response format | Format-selectable: summary (default), markdown, json — single Content block | Summary minimizes context window; markdown for full content; json for programmatic use | ADR-004 |
| Output framing | `[KNOWLEDGE DATA]`/`[/KNOWLEDGE DATA]` markers | Prevents stored content from being interpreted as instructions | ADR-005 |
| Near-duplicate detection | 0.92 cosine similarity threshold | Balances duplicate catch rate vs false positives at embedding granularity | ADR-006 |
| New error variants | Three new ServerError variants + two MCP codes | Keeps error taxonomy clean; actionable messages for each failure mode | ADR-007 |

## Integration Points

### Consumed (existing, unchanged)

| Component | Interface | Used By |
|-----------|----------|---------|
| AsyncEntryStore | `insert()`, `get()`, `query()`, `put_vector_mapping()` | C5 (tools) |
| AsyncVectorStore | `insert()`, `search()`, `search_filtered()` | C5 (tools) |
| EmbedServiceHandle | `get_adapter().await` -> `Arc<EmbedAdapter>` | C5 (tools) |
| EmbedAdapter | `embed_entry(title, content)` via spawn_blocking | C5 (tools) |
| AgentRegistry | `require_capability(agent_id, cap)` | C5 (tools) |
| AuditLog | `log_event(event)` | C5 (tools, read-only paths) |
| identity::resolve_identity | `resolve_identity(registry, agent_id)` | C5 (tools) |
| Store::begin_write | `fn begin_write() -> WriteTransaction` | C6 (audit optimization) |
| Store::begin_read | `fn begin_read() -> ReadTransaction` | C6 (audit optimization) |

### Extended

| Component | New Method | Purpose |
|-----------|-----------|---------|
| AuditLog | `write_in_txn(&self, txn, event) -> Result<u64>` | Write audit event in caller's transaction |
| UnimatrixServer | `insert_with_audit(&self, entry, embedding, event)` | Combined mutation + audit |

### New Modules

| Module | Purpose | Dependencies |
|--------|---------|-------------|
| `validation.rs` | Input validation | ServerError, Status, tools param types |
| `scanning.rs` | Content scanning | regex, OnceLock, ServerError |
| `response.rs` | Response formatting | EntryRecord, CallToolResult, Content, serde_json |
| `categories.rs` | Category allowlist | RwLock, HashSet, ServerError |

### New Crate Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| regex | latest stable | Content scanning pattern matching |
| serde_json | (already transitive dep) | JSON response format serialization |

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `validate_search_params(&SearchParams)` | `fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validate_lookup_params(&LookupParams)` | `fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validate_store_params(&StoreParams)` | `fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validate_get_params(&GetParams)` | `fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validated_id(i64) -> u64` | `fn -> Result<u64, ServerError>` | validation.rs (NEW) |
| `validated_k(Option<i64>) -> usize` | `fn -> Result<usize, ServerError>` | validation.rs (NEW) |
| `validated_limit(Option<i64>) -> usize` | `fn -> Result<usize, ServerError>` | validation.rs (NEW) |
| `parse_status(&str) -> Status` | `fn -> Result<Status, ServerError>` | validation.rs (NEW) |
| `ContentScanner::global()` | `fn -> &'static ContentScanner` | scanning.rs (NEW) |
| `ContentScanner::scan(&str)` | `fn -> Result<(), ScanResult>` | scanning.rs (NEW) |
| `ContentScanner::scan_title(&str)` | `fn -> Result<(), ScanResult>` | scanning.rs (NEW) |
| `parse_format(&Option<String>)` | `fn -> Result<ResponseFormat, ServerError>` | response.rs (NEW) |
| `format_single_entry(&EntryRecord, ResponseFormat)` | `fn -> CallToolResult` | response.rs (NEW) |
| `format_search_results(&[(EntryRecord, f32)], ResponseFormat)` | `fn -> CallToolResult` | response.rs (NEW) |
| `format_lookup_results(&[EntryRecord], ResponseFormat)` | `fn -> CallToolResult` | response.rs (NEW) |
| `format_store_success(&EntryRecord, ResponseFormat)` | `fn -> CallToolResult` | response.rs (NEW) |
| `format_duplicate_found(&EntryRecord, f32, ResponseFormat)` | `fn -> CallToolResult` | response.rs (NEW) |
| `format_empty_results(&str, ResponseFormat)` | `fn -> CallToolResult` | response.rs (NEW) |
| `CategoryAllowlist::new()` | `fn -> CategoryAllowlist` | categories.rs (NEW) |
| `CategoryAllowlist::validate(&str)` | `fn -> Result<(), ServerError>` | categories.rs (NEW) |
| `CategoryAllowlist::add_category(String)` | `fn` | categories.rs (NEW) |
| `AuditLog::write_in_txn(&WriteTransaction, AuditEvent)` | `fn -> Result<u64, ServerError>` | audit.rs (EXTENDED) |
| `UnimatrixServer::insert_with_audit(NewEntry, Vec<f32>, AuditEvent)` | `async fn -> Result<(u64, EntryRecord), ServerError>` | server.rs (EXTENDED) |
| `ServerError::InvalidInput { field, reason }` | enum variant | error.rs (EXTENDED) |
| `ServerError::ContentScanRejected { category, description }` | enum variant | error.rs (EXTENDED) |
| `ServerError::InvalidCategory { category, valid_categories }` | enum variant | error.rs (EXTENDED) |
| `ERROR_CONTENT_SCAN_REJECTED` (-32006) | ErrorCode constant | error.rs (EXTENDED) |
| `ERROR_INVALID_CATEGORY` (-32007) | ErrorCode constant | error.rs (EXTENDED) |

## Implementation Order

```
C7 (error extensions)          -- no deps, needed by all new modules
C1 (validation)                -- depends on C7
C4 (categories)                -- depends on C7
C2 (scanning)                  -- depends on C7, regex crate
C3 (response)                  -- depends on C7
  |
  v
C6 (audit optimization)       -- depends on C7, AuditLog, Store
C8 (server state extension)   -- depends on C4, C6
  |
  v
C5 (tool implementations)     -- depends on ALL above
```

Components C1, C2, C3, C4 are independent of each other and can be implemented in parallel. C5 (tools) is the integration point that depends on everything else.
