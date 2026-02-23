# Implementation Brief: vnc-002 v0.1 Tool Implementations

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-002/SCOPE.md |
| Architecture | product/features/vnc-002/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-002/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-002/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-002/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| error-extensions | pseudocode/error-extensions.md | test-plan/error-extensions.md |
| validation | pseudocode/validation.md | test-plan/validation.md |
| scanning | pseudocode/scanning.md | test-plan/scanning.md |
| categories | pseudocode/categories.md | test-plan/categories.md |
| response | pseudocode/response.md | test-plan/response.md |
| audit-optimization | pseudocode/audit-optimization.md | test-plan/audit-optimization.md |
| tools | pseudocode/tools.md | test-plan/tools.md |

## Goal

Replace the four tool stubs in `unimatrix-server` with real implementations (context_search, context_lookup, context_store, context_get). Activate all 9 security enforcement points (capability checks, input validation, content scanning). Implement near-duplicate detection, format-selectable responses (summary/markdown/json) with output framing, category allowlist validation, and optimize audit log writes for mutating tools.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Audit optimization | Combined write txn on UnimatrixServer; AuditLog gets write_in_txn method | SCOPE.md Q3, GH #11 | architecture/ADR-001-combined-audit-transaction.md |
| Content scanning | OnceLock singleton with ~50 compiled regex patterns; hard-reject on match | SCOPE.md Q1 | architecture/ADR-002-content-scanning-architecture.md |
| Category allowlist | RwLock<HashSet<String>> runtime-extensible; initial 6 categories | SCOPE.md constraint | architecture/ADR-003-category-allowlist-design.md |
| Response format | Format-selectable: summary (default), markdown, json — single Content block | SCOPE.md | architecture/ADR-004-dual-response-format.md |
| Output framing | [KNOWLEDGE DATA]/[/KNOWLEDGE DATA] bracket markers around content | SCOPE.md | architecture/ADR-005-output-framing-strategy.md |
| Near-duplicate threshold | 0.92 cosine similarity; success response with duplicate indicator | SCOPE.md Q2 | architecture/ADR-006-near-duplicate-detection.md |
| Error extensions | 3 new ServerError variants + 2 new MCP error codes (-32006, -32007) | Architecture C7 | architecture/ADR-007-server-error-extensions.md |

## Files to Create/Modify

### New Files

| Path | Purpose |
|------|---------|
| `crates/unimatrix-server/src/validation.rs` | Input validation for all tool params (lengths, control chars, ID conversion, status parsing) |
| `crates/unimatrix-server/src/scanning.rs` | Content scanning singleton (~50 regex patterns for injection + PII detection) |
| `crates/unimatrix-server/src/response.rs` | Format-selectable responses (summary/markdown/json) with output framing |
| `crates/unimatrix-server/src/categories.rs` | Category allowlist (RwLock<HashSet>, initial 6 categories, runtime extensible) |

### Modified Files

| Path | Change |
|------|--------|
| `crates/unimatrix-server/src/error.rs` | Add 3 new ServerError variants (InvalidInput, ContentScanRejected, InvalidCategory) + 2 MCP error codes + Display/ErrorData mappings |
| `crates/unimatrix-server/src/audit.rs` | Add `write_in_txn(&self, txn, event) -> Result<u64>` method for caller-managed transactions |
| `crates/unimatrix-server/src/server.rs` | Add `categories: Arc<CategoryAllowlist>` and `store: Arc<Store>` fields; add `insert_with_audit` method |
| `crates/unimatrix-server/src/tools.rs` | Replace all 4 stubs with real implementations using validation, scanning, response, categories |
| `crates/unimatrix-server/src/lib.rs` | Add module declarations for validation, scanning, response, categories |
| `crates/unimatrix-server/Cargo.toml` | Add `regex` dependency |

## Data Structures

### New Types

```rust
// validation.rs -- no new types, pure functions

// scanning.rs
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

// response.rs -- ResponseFormat enum, pure functions producing CallToolResult

// categories.rs
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,
}
```

### Extended Types

```rust
// error.rs -- new variants on existing ServerError enum
pub enum ServerError {
    // ... existing 8 variants ...
    InvalidInput { field: String, reason: String },
    ContentScanRejected { category: String, description: String },
    InvalidCategory { category: String, valid_categories: Vec<String> },
}

// New MCP error codes
pub const ERROR_CONTENT_SCAN_REJECTED: ErrorCode = ErrorCode(-32006);
pub const ERROR_INVALID_CATEGORY: ErrorCode = ErrorCode(-32007);

// server.rs -- extended UnimatrixServer
pub struct UnimatrixServer {
    // ... existing 5 fields + tool_router + server_info ...
    pub(crate) categories: Arc<CategoryAllowlist>,  // NEW
    pub(crate) store: Arc<Store>,                    // NEW: raw store for combined txn
}
```

## Implementation Order

```
1. error-extensions     -- new ServerError variants + MCP codes (no deps)
2. validation           -- input validation functions (depends on error-extensions)
   categories           -- category allowlist (depends on error-extensions)
   scanning             -- content scanning (depends on error-extensions + regex)
   response             -- response formatting (depends on error-extensions)
3. audit-optimization   -- write_in_txn + insert_with_audit (depends on error-extensions)
4. tools                -- real implementations (depends on ALL above)
```

Steps 2's four components are independent and can be implemented in parallel.

## Key Constraints

- **Edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`**
- **rmcp =0.16.0** pinned exactly
- **regex** is the only new dependency
- **redb write serialization**: combined audit+data writes for context_store, standalone for read tools
- **EmbedServiceHandle**: context_search and context_store must handle Loading/Failed states
- **Store auto-computes**: content_hash, previous_hash, version -- tool handlers provide created_by, trust_source, feature_cycle only
- **Cumulative testing**: build on vnc-001's 72 existing tests

## Risk Hotspots (Test First)

| Priority | Risk | Component | What to Test First |
|----------|------|-----------|-------------------|
| 1 | R-03: Combined transaction atomicity | audit-optimization | Verify entry + audit + mapping in single commit; rollback scenario |
| 2 | R-05: Capability check bypass | tools | Restricted agent blocked from context_store; order: capability before validation |
| 3 | R-04: Input validation bypass | validation | Every length boundary (max, max+1); control chars; negative IDs |
| 4 | R-01: Scanning false positives | scanning | Negative cases (legitimate content passes); positive cases (injection rejected) |
| 5 | R-12: Audit ID monotonicity | audit-optimization | Interleaved combined + standalone paths produce sequential IDs |
| 6 | R-16: vnc-001 regression | tools | All 72 existing tests pass after changes |

## Vision Alignment Warnings

| Warning | Action |
|---------|--------|
| W-01: Content scanning pattern quality | Include 5+ negative test cases per pattern category during scanning component implementation |
| W-02: Near-duplicate threshold sensitivity | Validate 0.92 threshold with 10+ test cases of varying length during tools component implementation |
