# Architecture: vnc-003 v0.2 Tool Implementations

## System Overview

vnc-003 adds four v0.2 MCP tools (`context_correct`, `context_deprecate`, `context_status`, `context_briefing`) to the existing `unimatrix-server` crate, fixes the VECTOR_MAP transaction atomicity bug (GH #14), extends the category allowlist, and adds new response formatters. All changes are contained within `crates/unimatrix-server/` and `crates/unimatrix-vector/` -- no new crates are introduced.

vnc-003 builds directly on vnc-002's established patterns: the same execution order (identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit), the same combined transaction pattern for mutations, and the same response formatting pipeline. The new tools extend existing modules rather than introducing new ones.

```
Claude Code / MCP Client
        |
        | stdio (JSON-RPC 2.0)
        v
unimatrix-server (binary)
  |-- ServerHandler (rmcp)          -- unchanged
  |-- ToolRouter                    -- EXTENDED: 4 new tool handlers (vnc-003)
  |     |-- tools.rs                -- EXTENDED: +4 tools, +4 param structs
  |     |-- validation.rs           -- EXTENDED: +4 validate functions
  |     |-- scanning.rs             -- unchanged (reused by context_correct)
  |     |-- response.rs             -- EXTENDED: +4 format functions
  |     '-- categories.rs           -- EXTENDED: +2 initial categories
  |
  |-- UnimatrixServer               -- EXTENDED: +correct_with_audit(), +deprecate_with_audit()
  |-- VectorIndex                   -- EXTENDED: +allocate_data_id(), +insert_hnsw_only()
  |-- AsyncEntryStore               -- unchanged
  |-- AsyncVectorStore              -- unchanged
  |-- EmbedServiceHandle            -- consumed (get_adapter().await)
  |-- AgentRegistry                 -- consumed (require_capability())
  |-- AuditLog                      -- consumed (log_event + write_in_txn)
  |
  v
unimatrix-core (traits + async wrappers)
  |         |          |
  v         v          v
store    vector      embed
```

## Component Breakdown

### C1: Tool Parameter Structs and Handlers (`crates/unimatrix-server/src/tools.rs`) -- EXTENDED

Four new `#[tool]` handlers added to the existing `#[rmcp::tool_router]` impl block, plus four new parameter structs.

**New param structs:**

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CorrectParams {
    /// ID of the entry to correct.
    pub original_id: i64,
    /// Corrected content.
    pub content: String,
    /// Reason for the correction.
    pub reason: Option<String>,
    /// Override topic (inherits from original if omitted).
    pub topic: Option<String>,
    /// Override category (inherits from original if omitted).
    pub category: Option<String>,
    /// Override tags (inherits from original if omitted).
    pub tags: Option<Vec<String>>,
    /// Title for the corrected entry.
    pub title: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeprecateParams {
    /// ID of the entry to deprecate.
    pub id: i64,
    /// Reason for deprecation.
    pub reason: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusParams {
    /// Filter by topic.
    pub topic: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BriefingParams {
    /// Role to build the briefing for (freeform, used as topic filter).
    pub role: String,
    /// Task description for semantic search component.
    pub task: String,
    /// Optional feature ID to boost matching entries.
    pub feature: Option<String>,
    /// Max tokens for the briefing (default: 3000). Character-based estimate (~4 chars/token).
    pub max_tokens: Option<i64>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}
```

**Tool execution flows (see Component Interactions section for details).**

### C2: Input Validation Extensions (`crates/unimatrix-server/src/validation.rs`) -- EXTENDED

Four new validation functions following the established pattern.

**New functions:**
```rust
pub fn validate_correct_params(params: &CorrectParams) -> Result<(), ServerError>;
pub fn validate_deprecate_params(params: &DeprecateParams) -> Result<(), ServerError>;
pub fn validate_status_params(params: &StatusParams) -> Result<(), ServerError>;
pub fn validate_briefing_params(params: &BriefingParams) -> Result<(), ServerError>;
pub fn validated_max_tokens(max_tokens: Option<i64>) -> Result<usize, ServerError>;
```

**Validation rules:**
- `validate_correct_params`: validates `original_id` (non-negative), `content` (max 50000, control chars), optional `title`/`topic`/`category`/`tags`/`reason`
- `validate_deprecate_params`: validates `id` (non-negative), optional `reason` (max 1000, control chars)
- `validate_status_params`: validates optional `topic`/`category` (length + control chars)
- `validate_briefing_params`: validates `role` (max 100, control chars), `task` (max 1000, control chars), optional `feature` (max 100)
- `validated_max_tokens`: default 3000, min 500, max 10000

### C3: Response Formatting Extensions (`crates/unimatrix-server/src/response.rs`) -- EXTENDED

Four new format functions following the established pattern.

**New functions:**
```rust
pub fn format_correct_success(
    original: &EntryRecord,
    correction: &EntryRecord,
    format: ResponseFormat,
) -> CallToolResult;

pub fn format_deprecate_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult;

pub fn format_status_report(
    report: &StatusReport,
    format: ResponseFormat,
) -> CallToolResult;

pub fn format_briefing(
    briefing: &Briefing,
    format: ResponseFormat,
) -> CallToolResult;
```

**New data structures for formatting:**
```rust
/// Aggregated health metrics for format_status_report.
pub struct StatusReport {
    pub total_active: u64,
    pub total_deprecated: u64,
    pub total_proposed: u64,
    pub category_distribution: Vec<(String, u64)>,
    pub topic_distribution: Vec<(String, u64)>,
    pub entries_with_supersedes: u64,
    pub entries_with_superseded_by: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: Vec<(String, u64)>,
    pub entries_without_attribution: u64,
}

/// Assembled briefing for format_briefing.
pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    pub duties: Vec<EntryRecord>,
    pub relevant_context: Vec<(EntryRecord, f32)>,
}
```

**Format details:**

`format_correct_success`:
- Summary: `Corrected #42 -> #43 | {title} | {category}`
- Markdown: Shows deprecated original + new correction with chain links
- JSON: `{ "corrected": true, "original": {..., "status": "deprecated"}, "correction": {...} }`

`format_deprecate_success`:
- Summary: `Deprecated #{id} | {title}`
- Markdown: Confirmation with entry summary and reason
- JSON: `{ "deprecated": true, "entry": {...}, "reason": "..." }`

`format_status_report`:
- Summary: `Active: N | Deprecated: N | Proposed: N | Corrections: N`
- Markdown: Structured sections with tables for distributions
- JSON: Full structured object matching `StatusReport` fields

`format_briefing`:
- Summary: Compact bullet points per section
- Markdown: `## Conventions` / `## Duties` / `## Relevant Context` sections
- JSON: `{ "role": "...", "conventions": [...], "duties": [...], "relevant_context": [...] }`

### C4: Category Allowlist Extension (`crates/unimatrix-server/src/categories.rs`) -- EXTENDED

Add "duties" and "reference" to the initial category set.

**Change:**
```rust
const INITIAL_CATEGORIES: [&str; 8] = [
    "outcome",
    "lesson-learned",
    "decision",
    "convention",
    "pattern",
    "procedure",
    "duties",       // NEW: used by context_briefing role duties lookup
    "reference",    // NEW: general reference material
];
```

This is a compile-time change. The `CategoryAllowlist::new()` constructor initializes with all 8 categories. Existing tests that check `valid_categories.len() == 6` must be updated to `8`.

### C5: VectorIndex API Extension (`crates/unimatrix-vector/src/index.rs`) -- EXTENDED

Two new methods to support the VECTOR_MAP transaction atomicity fix (GH #14).

**New methods:**
```rust
impl VectorIndex {
    /// Allocate the next hnsw data ID without performing any insertion.
    ///
    /// Used by server's combined write transaction to write VECTOR_MAP
    /// in the same transaction as entry insert + audit (GH #14 fix).
    pub fn allocate_data_id(&self) -> u64 {
        self.next_data_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Insert into HNSW index and update IdMap only.
    ///
    /// Skips the VECTOR_MAP write (caller already wrote it in a combined
    /// transaction). The data_id must have been allocated via allocate_data_id().
    pub fn insert_hnsw_only(
        &self,
        entry_id: u64,
        data_id: u64,
        embedding: &[f32],
    ) -> Result<()> {
        self.validate_dimension(embedding)?;
        self.validate_embedding(embedding)?;

        // Insert into hnsw_rs
        {
            let hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner());
            let data_vec = embedding.to_vec();
            hnsw.insert_slice((&data_vec, data_id as usize));
        }

        // Update IdMap (no VECTOR_MAP write)
        {
            let mut id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner());
            if let Some(old_data_id) = id_map.entry_to_data.insert(entry_id, data_id) {
                id_map.data_to_entry.remove(&old_data_id);
            }
            id_map.data_to_entry.insert(data_id, entry_id);
        }

        Ok(())
    }
}
```

The existing `VectorIndex::insert()` method remains unchanged for backward compatibility. `insert_hnsw_only` is the counterpart that expects the VECTOR_MAP write to have happened in an external transaction.

### C6: Server Combined Transaction Methods (`crates/unimatrix-server/src/server.rs`) -- EXTENDED

Three changes to `UnimatrixServer`:

1. **Fix `insert_with_audit`** to include VECTOR_MAP write in the combined transaction (GH #14 fix).
2. **Add `correct_with_audit`** for the two-entry atomic operation.
3. **Add `deprecate_with_audit`** for atomic deprecation + audit.

**Fixed `insert_with_audit`:**
```rust
impl UnimatrixServer {
    pub(crate) async fn insert_with_audit(
        &self,
        entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(u64, EntryRecord), ServerError> {
        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);
        let vector_store = Arc::clone(&self.vector_store);

        // Allocate data_id BEFORE the transaction (atomic counter)
        let data_id = self.vector_index.allocate_data_id();

        // Step 1: Combined write transaction
        let (entry_id, record) = tokio::task::spawn_blocking(move || {
            let txn = store.begin_write()?;
            // ... existing entry + index writes ...
            // NEW: Write VECTOR_MAP in the same transaction
            {
                let mut table = txn.open_table(VECTOR_MAP)?;
                table.insert(id, data_id)?;
            }
            // Write audit event
            audit_log.write_in_txn(&txn, audit_event)?;
            txn.commit()?;
            Ok((id, record))
        }).await??;

        // Step 2: HNSW insert only (VECTOR_MAP already written)
        self.vector_index.insert_hnsw_only(entry_id, data_id, &embedding)?;

        Ok((entry_id, record))
    }
}
```

**New `correct_with_audit`:**
```rust
impl UnimatrixServer {
    /// Correct an entry: deprecate original + insert correction + audit, all atomic.
    pub(crate) async fn correct_with_audit(
        &self,
        original_id: u64,
        correction_entry: NewEntry,
        embedding: Vec<f32>,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<(EntryRecord, EntryRecord), ServerError> {
        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);
        let data_id = self.vector_index.allocate_data_id();

        let (deprecated_original, new_correction) = tokio::task::spawn_blocking(move || {
            let txn = store.begin_write()?;

            // 1. Read and deprecate original
            //    - Read existing record from ENTRIES
            //    - Set status = Deprecated, superseded_by = new_id
            //    - Increment correction_count
            //    - Update STATUS_INDEX (remove old, insert new)
            //    - Update status counters
            //    - Serialize and overwrite in ENTRIES

            // 2. Insert correction entry
            //    - Generate new entry ID
            //    - Set supersedes = original_id
            //    - Compute content_hash
            //    - Write ENTRIES + all indexes + VECTOR_MAP
            //    - Increment status counter

            // 3. Write audit event with both IDs in target_ids
            audit_log.write_in_txn(&txn, audit_event)?;

            txn.commit()?;
            Ok((deprecated_original, new_correction))
        }).await??;

        // HNSW insert for the correction (after commit)
        self.vector_index.insert_hnsw_only(
            new_correction.id, data_id, &embedding
        )?;

        Ok((deprecated_original, new_correction))
    }
}
```

**New `deprecate_with_audit`:**
```rust
impl UnimatrixServer {
    /// Deprecate an entry + audit in a single write transaction.
    pub(crate) async fn deprecate_with_audit(
        &self,
        entry_id: u64,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<EntryRecord, ServerError> {
        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);

        let record = tokio::task::spawn_blocking(move || {
            let txn = store.begin_write()?;

            // 1. Read existing record
            // 2. If already deprecated, return as-is (idempotent)
            // 3. Update status to Deprecated
            // 4. Update STATUS_INDEX + counters
            // 5. Write audit event
            txn.commit()?;
            Ok(record)
        }).await??;

        Ok(record)
    }
}
```

### C7: Server State Extension (`crates/unimatrix-server/src/server.rs`) -- EXTENDED

The `UnimatrixServer` struct gains one new field to support the GH #14 fix:

```rust
pub struct UnimatrixServer {
    // ... existing fields ...
    /// Raw VectorIndex for allocate_data_id() and insert_hnsw_only().
    pub(crate) vector_index: Arc<VectorIndex>,
}
```

The server needs direct access to `VectorIndex` (not just `AsyncVectorStore`) because `allocate_data_id()` is synchronous and must happen before the write transaction, while `insert_hnsw_only()` must happen after. The `VectorIndex` reference is extracted from the `VectorAdapter` during server construction.

This requires changes to `UnimatrixServer::new()` to accept the additional parameter, and to `make_server()` in tests.

## Component Interactions

### Data Flow: context_correct

```
MCP Client -> tools/call "context_correct"
  |
  v
1. resolve_agent(&params.agent_id) -> ResolvedIdentity
2. registry.require_capability(agent_id, Write)
3. validate_correct_params(&params)
4. let format = parse_format(&params.format)
5. validated_id(params.original_id) -> original_id: u64
6. entry_store.get(original_id).await -> original entry
7. Verify original.status != Deprecated (reject if so)
8. Category validation: if params.category provided, validate it
   Otherwise, inherit original.category (skip validation)
9. Content scanning: scan(&params.content), scan_title if title provided
10. Embed title+content -> Vec<f32>
    (title = params.title or original.title)
11. Build NewEntry with supersedes = original_id,
    inheriting topic/category/tags from original where not overridden
12. correct_with_audit(original_id, new_entry, embedding, reason, audit_event)
    |-> spawn_blocking:
        a. Read original from ENTRIES
        b. Set original.status = Deprecated, original.superseded_by = new_id
        c. Increment original.correction_count
        d. Update STATUS_INDEX for original (Active -> Deprecated)
        e. Decrement total_active, increment total_deprecated
        f. Serialize and overwrite original in ENTRIES
        g. Generate new entry ID, build correction EntryRecord
        h. Write correction to ENTRIES + all 5 indexes + VECTOR_MAP
        i. Increment total_active
        j. Write audit event (target_ids = [original_id, new_id])
        k. Commit
    |-> vector_index.insert_hnsw_only(new_id, data_id, embedding)
13. format_correct_success(deprecated_original, new_correction, format)
  |
  v
CallToolResult -> MCP Client
```

### Data Flow: context_deprecate

```
MCP Client -> tools/call "context_deprecate"
  |
  v
1. resolve_agent(&params.agent_id) -> ResolvedIdentity
2. registry.require_capability(agent_id, Write)
3. validate_deprecate_params(&params)
4. let format = parse_format(&params.format)
5. validated_id(params.id) -> entry_id: u64
6. entry_store.get(entry_id).await -> entry
7. If entry.status == Deprecated, return idempotent success (AC-13)
8. deprecate_with_audit(entry_id, reason, audit_event)
    |-> spawn_blocking:
        a. Read entry from ENTRIES
        b. Set status = Deprecated
        c. Update STATUS_INDEX (old status -> Deprecated)
        d. Update status counters (decrement old, increment deprecated)
        e. Serialize and overwrite in ENTRIES
        f. Write audit event (target_ids = [entry_id])
        g. Commit
9. format_deprecate_success(entry, reason, format)
  |
  v
CallToolResult -> MCP Client
```

### Data Flow: context_status

```
MCP Client -> tools/call "context_status"
  |
  v
1. resolve_agent(&params.agent_id) -> ResolvedIdentity
2. registry.require_capability(agent_id, Admin)
3. validate_status_params(&params)
4. let format = parse_format(&params.format)
5. Read counters: total_active, total_deprecated, total_proposed
6. Scan CATEGORY_INDEX for category distribution
   (filtered by params.category if provided)
7. Scan TOPIC_INDEX for topic distribution
   (filtered by params.topic if provided)
8. Scan ENTRIES for correction chain metrics:
   - Count entries with supersedes != None
   - Count entries with superseded_by != None
   - Sum correction_count across all entries
9. Scan ENTRIES for security metrics:
   - Group by trust_source
   - Count entries with empty created_by
10. Build StatusReport struct
11. Audit (standalone, best-effort)
12. format_status_report(report, format)
  |
  v
CallToolResult -> MCP Client
```

Note: Steps 5-9 all happen inside a single `spawn_blocking` closure using `store.begin_read()` for a consistent snapshot.

### Data Flow: context_briefing

```
MCP Client -> tools/call "context_briefing"
  |
  v
1. resolve_agent(&params.agent_id) -> ResolvedIdentity
2. registry.require_capability(agent_id, Read)
3. validate_briefing_params(&params)
4. let format = parse_format(&params.format)
5. validated_max_tokens(params.max_tokens) -> budget: usize
6. Lookup conventions:
   entry_store.query(QueryFilter { topic: role, category: "convention", status: Active })
7. Lookup duties:
   entry_store.query(QueryFilter { topic: role, category: "duties", status: Active })
8. Semantic search (if embed ready):
   a. embed_service.get_adapter().await
   b. embed task description -> Vec<f32>
   c. vector_store.search(embedding, 3, ef_search) -> results
   d. Fetch full entries for results
   e. If params.feature provided, reorder: entries tagged with feature first
   [If embed not ready: skip search, continue with lookup-only (AC-28)]
9. Assemble briefing within token budget:
   a. Conventions first (highest priority)
   b. Duties second
   c. Relevant context third (truncate from here if over budget)
   d. Character budget = max_tokens * 4
10. Audit (standalone, best-effort)
11. format_briefing(briefing, format)
  |
  v
CallToolResult -> MCP Client
```

### Data Flow: insert_with_audit (Fixed, GH #14)

```
caller (context_store / context_correct)
  |
  v
1. vector_index.allocate_data_id() -> data_id  [atomic, before txn]
2. spawn_blocking:
   a. store.begin_write()
   b. Insert entry into ENTRIES + 5 indexes
   c. Write VECTOR_MAP: entry_id -> data_id     [NEW: in same txn]
   d. Write audit event via audit.write_in_txn()
   e. txn.commit()
3. vector_index.insert_hnsw_only(entry_id, data_id, embedding)
  |
  v
(entry_id, EntryRecord)
```

## Technology Decisions

| Decision | Choice | Rationale | ADR |
|----------|--------|-----------|-----|
| VECTOR_MAP atomicity fix | Decouple VectorIndex: allocate_data_id() + insert_hnsw_only() | Server writes VECTOR_MAP in combined txn; HNSW insert after commit. Minimal API change to VectorIndex. | ADR-001 |
| Correction chain atomicity | Single write txn for deprecate-original + insert-correction | Both entries must be consistent. Reuses insert_with_audit pattern extended for two-entry operations. | ADR-002 |
| Deprecation idempotency | Return success on already-deprecated entries | Idempotent operations prevent retry errors. No state change means no audit event needed for no-op. | ADR-003 |
| Status report data access | Single read transaction for consistent snapshot | All metrics computed in one begin_read() to avoid torn reads. Full scan is acceptable at Unimatrix scale. | ADR-004 |
| Briefing graceful degradation | Lookup-only fallback when embed not ready | Briefing is useful even without semantic search. Fail-open for read-only composite operation. | ADR-005 |
| Token budget estimation | Character-based (~4 chars/token) | Avoids tokenizer dependency. Adequate for budget enforcement. Actual LLM token counting is not justified yet. | ADR-006 |
| Feature boost in briefing | Score adjustment on returned results, not query modification | Simple, deterministic, no extra queries. Tag matching on returned entries is O(k). | ADR-007 |

## Integration Points

### Consumed (existing, unchanged)

| Component | Interface | Used By |
|-----------|----------|---------|
| AsyncEntryStore | `get()`, `query()` | C1 (tools) |
| AsyncVectorStore | `search()` | C1 (context_briefing search) |
| EmbedServiceHandle | `get_adapter().await` | C1 (context_correct, context_briefing) |
| EmbedAdapter | `embed_entry(title, content)` via spawn_blocking | C1 (tools) |
| AgentRegistry | `require_capability(agent_id, cap)` | C1 (tools) |
| AuditLog | `log_event(event)`, `write_in_txn(txn, event)` | C1, C6 |
| ContentScanner | `global().scan()`, `global().scan_title()` | C1 (context_correct) |
| Store | `begin_write()`, `begin_read()` | C6 (combined transactions) |
| existing validation fns | `validated_id()`, `parse_format()` | C1 |

### Extended

| Component | Change | Purpose |
|-----------|--------|---------|
| VectorIndex | `+allocate_data_id()` | Atomic data_id allocation for external txn use |
| VectorIndex | `+insert_hnsw_only()` | HNSW insert without VECTOR_MAP write |
| UnimatrixServer | fix `insert_with_audit()` | Include VECTOR_MAP write in combined txn |
| UnimatrixServer | `+correct_with_audit()` | Two-entry atomic correction |
| UnimatrixServer | `+deprecate_with_audit()` | Atomic deprecation + audit |
| UnimatrixServer | `+vector_index` field | Direct VectorIndex access for GH #14 |
| validation.rs | +4 validate functions | v0.2 tool param validation |
| response.rs | +4 format functions + 2 structs | v0.2 tool response formatting |
| categories.rs | +2 initial categories | "duties" and "reference" |
| tools.rs | +4 tool handlers + 4 param structs | v0.2 tool implementations |

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `VectorIndex::allocate_data_id()` | `pub fn allocate_data_id(&self) -> u64` | index.rs (EXTENDED) |
| `VectorIndex::insert_hnsw_only(u64, u64, &[f32])` | `pub fn insert_hnsw_only(&self, entry_id: u64, data_id: u64, embedding: &[f32]) -> Result<()>` | index.rs (EXTENDED) |
| `UnimatrixServer::correct_with_audit(...)` | `pub(crate) async fn correct_with_audit(&self, original_id: u64, entry: NewEntry, embedding: Vec<f32>, reason: Option<String>, audit: AuditEvent) -> Result<(EntryRecord, EntryRecord), ServerError>` | server.rs (NEW) |
| `UnimatrixServer::deprecate_with_audit(...)` | `pub(crate) async fn deprecate_with_audit(&self, entry_id: u64, reason: Option<String>, audit: AuditEvent) -> Result<EntryRecord, ServerError>` | server.rs (NEW) |
| `validate_correct_params(&CorrectParams)` | `pub fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validate_deprecate_params(&DeprecateParams)` | `pub fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validate_status_params(&StatusParams)` | `pub fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validate_briefing_params(&BriefingParams)` | `pub fn -> Result<(), ServerError>` | validation.rs (NEW) |
| `validated_max_tokens(Option<i64>)` | `pub fn -> Result<usize, ServerError>` | validation.rs (NEW) |
| `format_correct_success(&EntryRecord, &EntryRecord, ResponseFormat)` | `pub fn -> CallToolResult` | response.rs (NEW) |
| `format_deprecate_success(&EntryRecord, Option<&str>, ResponseFormat)` | `pub fn -> CallToolResult` | response.rs (NEW) |
| `format_status_report(&StatusReport, ResponseFormat)` | `pub fn -> CallToolResult` | response.rs (NEW) |
| `format_briefing(&Briefing, ResponseFormat)` | `pub fn -> CallToolResult` | response.rs (NEW) |
| `StatusReport` | `pub struct` | response.rs (NEW) |
| `Briefing` | `pub struct` | response.rs (NEW) |

## Implementation Order

```
C4 (categories extension)        -- trivial constant change
C5 (VectorIndex API extension)   -- no deps, needed by C6
C2 (validation extensions)       -- no deps, needed by C1
C3 (response format extensions)  -- no deps, needed by C1
  |
  v
C7 (server state extension)     -- depends on C5 (VectorIndex type)
C6 (server combined txn methods) -- depends on C5, C7
  |
  v
C1 (tool implementations)       -- depends on ALL above
```

C2, C3, C4, C5 are independent and can be implemented in parallel. C6 and C7 depend on C5. C1 (tools) is the integration point that wires everything together.
