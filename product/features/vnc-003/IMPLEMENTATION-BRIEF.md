# Implementation Brief: vnc-003 v0.2 Tool Implementations

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-003/SCOPE.md |
| Architecture | product/features/vnc-003/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-003/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-003/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-003/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1: tool-handlers | pseudocode/tool-handlers.md | test-plan/tool-handlers.md |
| C2: validation-extensions | pseudocode/validation-extensions.md | test-plan/validation-extensions.md |
| C3: response-formatters | pseudocode/response-formatters.md | test-plan/response-formatters.md |
| C4: category-extension | pseudocode/category-extension.md | test-plan/category-extension.md |
| C5: vector-index-api | pseudocode/vector-index-api.md | test-plan/vector-index-api.md |
| C6: server-transactions | pseudocode/server-transactions.md | test-plan/server-transactions.md |
| C7: server-state | pseudocode/server-state.md | test-plan/server-state.md |

All pseudocode and test-plan files produced in Stage 3a. See pseudocode/OVERVIEW.md for component interaction summary.

## Goal

Implement the four v0.2 MCP tools (context_correct, context_deprecate, context_status, context_briefing) to enable knowledge lifecycle management, fix the VECTOR_MAP transaction atomicity bug (GH #14), and extend the category allowlist. This completes Milestone 2's tool surface and provides agents with correction chains, deprecation, health monitoring, and orientation briefings.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| VECTOR_MAP atomicity fix approach | Decouple VectorIndex: allocate_data_id() + insert_hnsw_only(). Server writes VECTOR_MAP in combined txn. | GH #14 analysis | architecture/ADR-001-vector-map-transaction-atomicity.md |
| Correction chain transaction scope | Single write txn for deprecate-original + insert-correction + VECTOR_MAP + audit | SCOPE.md design | architecture/ADR-002-correction-chain-atomicity.md |
| Deprecation idempotency behavior | No-op success on already-deprecated. No audit for no-op. | SCOPE.md AC-13 | architecture/ADR-003-deprecation-idempotency.md |
| Status report read consistency | Single begin_read() transaction for all metrics | Architecture design | architecture/ADR-004-status-report-consistent-snapshot.md |
| Briefing embed not ready handling | Graceful degradation: lookup-only briefing. No error. | SCOPE.md AC-28 | architecture/ADR-005-briefing-embed-fallback.md |
| Token budget estimation method | Character-based (~4 chars/token). No tokenizer dependency. | SCOPE.md Non-Goals | architecture/ADR-006-character-based-token-budget.md |
| Feature boost implementation | Score adjustment on returned results, not query modification | SCOPE.md design | architecture/ADR-007-briefing-feature-boost.md |

## Files to Create/Modify

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/tools.rs` | Add 4 param structs (CorrectParams, DeprecateParams, StatusParams, BriefingParams) + 4 tool handlers |
| `crates/unimatrix-server/src/validation.rs` | Add validate_correct_params, validate_deprecate_params, validate_status_params, validate_briefing_params, validated_max_tokens |
| `crates/unimatrix-server/src/response.rs` | Add StatusReport/Briefing structs + format_correct_success, format_deprecate_success, format_status_report, format_briefing |
| `crates/unimatrix-server/src/categories.rs` | Expand INITIAL_CATEGORIES from 6 to 8 (add "duties", "reference") |
| `crates/unimatrix-server/src/server.rs` | Add vector_index field, fix insert_with_audit (GH #14), add correct_with_audit, add deprecate_with_audit, update new() and make_server() |
| `crates/unimatrix-vector/src/index.rs` | Add allocate_data_id() and insert_hnsw_only() methods |

### No New Files

All changes are extensions to existing modules. No new files created.

## Data Structures

### New Parameter Structs (tools.rs)

```rust
pub struct CorrectParams {
    pub original_id: i64,
    pub content: String,
    pub reason: Option<String>,
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub title: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}

pub struct DeprecateParams {
    pub id: i64,
    pub reason: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}

pub struct StatusParams {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}

pub struct BriefingParams {
    pub role: String,
    pub task: String,
    pub feature: Option<String>,
    pub max_tokens: Option<i64>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}
```

### New Response Structs (response.rs)

```rust
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

pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    pub duties: Vec<EntryRecord>,
    pub relevant_context: Vec<(EntryRecord, f32)>,
    pub search_available: bool,
}
```

## Function Signatures

### VectorIndex Extensions (index.rs)

```rust
pub fn allocate_data_id(&self) -> u64;
pub fn insert_hnsw_only(&self, entry_id: u64, data_id: u64, embedding: &[f32]) -> Result<()>;
```

### Server Transaction Methods (server.rs)

```rust
// Fixed (GH #14): VECTOR_MAP write included in combined transaction
pub(crate) async fn insert_with_audit(&self, entry: NewEntry, embedding: Vec<f32>, audit_event: AuditEvent) -> Result<(u64, EntryRecord), ServerError>;

// New: Two-entry atomic correction
pub(crate) async fn correct_with_audit(&self, original_id: u64, entry: NewEntry, embedding: Vec<f32>, reason: Option<String>, audit_event: AuditEvent) -> Result<(EntryRecord, EntryRecord), ServerError>;

// New: Atomic deprecation + audit
pub(crate) async fn deprecate_with_audit(&self, entry_id: u64, reason: Option<String>, audit_event: AuditEvent) -> Result<EntryRecord, ServerError>;
```

### Validation Extensions (validation.rs)

```rust
pub fn validate_correct_params(params: &CorrectParams) -> Result<(), ServerError>;
pub fn validate_deprecate_params(params: &DeprecateParams) -> Result<(), ServerError>;
pub fn validate_status_params(params: &StatusParams) -> Result<(), ServerError>;
pub fn validate_briefing_params(params: &BriefingParams) -> Result<(), ServerError>;
pub fn validated_max_tokens(max_tokens: Option<i64>) -> Result<usize, ServerError>;
```

### Response Formatting Extensions (response.rs)

```rust
pub fn format_correct_success(original: &EntryRecord, correction: &EntryRecord, format: ResponseFormat) -> CallToolResult;
pub fn format_deprecate_success(entry: &EntryRecord, reason: Option<&str>, format: ResponseFormat) -> CallToolResult;
pub fn format_status_report(report: &StatusReport, format: ResponseFormat) -> CallToolResult;
pub fn format_briefing(briefing: &Briefing, format: ResponseFormat) -> CallToolResult;
```

## Constraints

- **rmcp =0.16.0** pinned exactly
- **Rust edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`**
- **No new crate dependencies**
- **redb single-writer**: combined transactions essential for multi-table atomicity
- **EmbedServiceHandle lazy loading**: context_correct fails on not-ready; context_briefing degrades
- **Store auto-computes** content_hash/previous_hash/version on update()
- **update_status()** does NOT change content_hash/previous_hash/version
- **Test infrastructure is cumulative**: 506 existing tests must continue to pass

## Dependencies

| Crate | Role |
|-------|------|
| unimatrix-store | ENTRIES, VECTOR_MAP, indexes, counters, serialization |
| unimatrix-vector | VectorIndex (extended with allocate_data_id, insert_hnsw_only) |
| unimatrix-core | Traits (EntryStore, VectorStore), async wrappers, types |
| unimatrix-embed | EmbedAdapter via EmbedServiceHandle |

## Implementation Order

```
C4 (categories: add "duties" + "reference")
C5 (VectorIndex: allocate_data_id + insert_hnsw_only)
C2 (validation: 4 new validate functions)
C3 (response: 4 new format functions + 2 structs)
  |
  v
C7 (server state: add vector_index field)
C6 (server txns: fix insert_with_audit, add correct_with_audit, deprecate_with_audit)
  |
  v
C1 (tool handlers: 4 new tools wiring everything together)
```

## NOT in Scope

- No v0.3 tools (Resources, Prompts, local embeddings)
- No confidence computation (crt-002)
- No usage tracking (crt-001)
- No "force store" near-duplicate override
- No contradiction detection (crt-003)
- No staleness metrics
- No content_hash re-validation
- No inline duplicate scanning
- No persisted category allowlist
- No HTTP/SSE transport
- No cross-project scope
- No batch operations
- No token counting (character budget only)

## Alignment Status

All 6 alignment checks passed. No variances requiring approval. See ALIGNMENT-REPORT.md for details.
