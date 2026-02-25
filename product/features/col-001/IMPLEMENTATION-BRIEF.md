# Implementation Brief: col-001 Outcome Tracking

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-001/SCOPE.md |
| Scope Risk Assessment | product/features/col-001/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-001/architecture/ARCHITECTURE.md |
| ADR-001 Tag Validation Boundary | product/features/col-001/architecture/ADR-001-tag-validation-boundary.md |
| ADR-002 Outcome Index Write Location | product/features/col-001/architecture/ADR-002-outcome-index-write-location.md |
| ADR-003 Extensible Category Validation | product/features/col-001/architecture/ADR-003-extensible-category-validation.md |
| Specification | product/features/col-001/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-001/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-001/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| outcome-index | pseudocode/outcome-index.md | test-plan/outcome-index.md |
| outcome-tags | pseudocode/outcome-tags.md | test-plan/outcome-tags.md |
| store-pipeline | pseudocode/store-pipeline.md | test-plan/store-pipeline.md |
| status-extension | pseudocode/status-extension.md | test-plan/status-extension.md |

## Goal

Add structured outcome tracking to Unimatrix by introducing OUTCOME_INDEX (a secondary index linking feature cycles to outcome entries), structured tag validation for outcome entries, a `feature_cycle` parameter on `context_store`, and outcome statistics in `context_status`. This provides the queryable data layer that col-002, col-003, and col-004 depend on.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Tag validation lives in server crate, not store crate | Store remains domain-agnostic; server does semantic validation | ADR-001 | architecture/ADR-001-tag-validation-boundary.md |
| OUTCOME_INDEX populated in insert_with_audit, not Store::insert | Server crate manages multi-table writes for domain concerns | ADR-002 | architecture/ADR-002-outcome-index-write-location.md |
| Per-category validation uses simple conditional, not dispatch registry | One category has rules today; refactor when second appears | ADR-003 | architecture/ADR-003-extensible-category-validation.md |
| feature_cycle reused without renaming | Avoids schema migration and bincode deserialization risk | SCOPE.md Resolved Decisions #1 | — |
| Workflow type via required `type` tag, not field | Composes with existing TAG_INDEX; no schema addition | SCOPE.md Resolved Decisions #2 | — |
| `gate` tag values are open strings | Avoids coupling to protocol-specific identifiers | SCOPE.md Resolved Decisions #3 | — |

## Files to Create/Modify

### New Files

| File | Purpose |
|------|---------|
| `crates/unimatrix-server/src/outcome_tags.rs` | Structured tag parsing and validation module |

### Modified Files

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/schema.rs` | Add OUTCOME_INDEX table definition constant |
| `crates/unimatrix-store/src/db.rs` | Open OUTCOME_INDEX in Store::open (13th table) |
| `crates/unimatrix-store/src/lib.rs` | Export OUTCOME_INDEX |
| `crates/unimatrix-server/src/lib.rs` | Declare outcome_tags module |
| `crates/unimatrix-server/src/tools.rs` | Add feature_cycle to StoreParams; add outcome tag validation call in context_store |
| `crates/unimatrix-server/src/server.rs` | Add OUTCOME_INDEX insert in insert_with_audit when category is outcome |
| `crates/unimatrix-server/src/response.rs` | Add 4 outcome fields to StatusReport; extend format_status_report |
| `crates/unimatrix-server/src/validation.rs` | Add feature_cycle input validation (max length, no control chars) |

## Data Structures

### OUTCOME_INDEX Table (store crate — schema.rs)

```rust
/// Outcome-to-feature-cycle index: (feature_cycle, entry_id) -> ().
/// Populated when context_store creates an outcome entry with non-empty feature_cycle.
pub const OUTCOME_INDEX: TableDefinition<(&str, u64), ()> =
    TableDefinition::new("outcome_index");
```

### Structured Tag Types (server crate — outcome_tags.rs)

```rust
/// Recognized structured tag keys for outcome entries.
pub enum OutcomeTagKey {
    Type,
    Gate,
    Phase,
    Result,
    Agent,
    Wave,
}

/// Workflow type values for the required `type` tag.
pub enum WorkflowType {
    Feature,
    Bugfix,
    Incident,
    Process,
}

/// Outcome result values for the `result` tag.
pub enum OutcomeResult {
    Pass,
    Fail,
    Rework,
    Skip,
}

/// Phase values for the `phase` tag.
pub enum PhaseValue {
    Research,
    Design,
    Implementation,
    Testing,
    Validation,
}
```

### StoreParams Extension (server crate — tools.rs)

```rust
pub struct StoreParams {
    // ... existing fields ...
    /// Feature cycle or workflow identifier. Associates this entry with a workflow.
    pub feature_cycle: Option<String>,
}
```

### StatusReport Extension (server crate — response.rs)

```rust
pub struct StatusReport {
    // ... existing fields ...
    /// Total outcome entries.
    pub total_outcomes: u64,
    /// Outcome count by workflow type (from type: tag).
    pub outcomes_by_type: Vec<(String, u64)>,
    /// Outcome count by result (from result: tag).
    pub outcomes_by_result: Vec<(String, u64)>,
    /// Top feature cycles by outcome count.
    pub outcomes_by_feature_cycle: Vec<(String, u64)>,
}
```

## Function Signatures

### outcome_tags.rs

```rust
/// Validate all tags for an outcome entry.
/// Checks: recognized keys, required `type` tag, enum values.
/// Tags without `:` pass through as plain tags.
pub fn validate_outcome_tags(tags: &[String]) -> Result<(), ServerError>

/// Parse a single structured tag. Returns None for plain tags.
fn parse_structured_tag(tag: &str) -> Option<(&str, &str)>

/// Validate a structured tag key-value pair.
fn validate_tag_key_value(key: &str, value: &str) -> Result<(), ServerError>
```

### insert_with_audit extension (server.rs)

The existing `insert_with_audit` method gains a conditional OUTCOME_INDEX insert after the ENTRIES write:

```rust
// In insert_with_audit, after existing index writes, before commit:
if record.category == "outcome" && !record.feature_cycle.is_empty() {
    let mut outcome_table = txn.open_table(OUTCOME_INDEX)
        .map_err(...)?;
    outcome_table.insert((record.feature_cycle.as_str(), id), ())
        .map_err(...)?;
}
```

### context_status outcome stats (tools.rs)

Within the existing spawn_blocking read transaction:

```rust
// After existing stats computation:
// Scan CATEGORY_INDEX for outcome count
// Scan OUTCOME_INDEX for feature_cycle distribution
// For each outcome entry: extract type: and result: tags from EntryRecord.tags
```

## Constraints

- `#![forbid(unsafe_code)]` — all crates
- Edition 2024, MSRV 1.89
- No new crate dependencies
- Schema version remains 2
- All new StoreParams fields are `Option<T>` for backward compatibility
- Object-safe traits maintained
- Store crate stays domain-agnostic
- Test infrastructure is cumulative — build on existing fixtures

## Dependencies

| Crate | Role | Existing? |
|-------|------|-----------|
| unimatrix-store | OUTCOME_INDEX definition, Store::open | Yes |
| unimatrix-server | outcome_tags module, tool handlers, StatusReport | Yes |
| redb | TableDefinition, write transactions | Yes |
| serde, schemars | StoreParams derivation | Yes |

## NOT in Scope

- No new MCP tool
- No outcome analysis (col-002)
- No process proposals (col-003)
- No gate status tracking (col-004)
- No confidence formula changes
- No retroactive indexing
- No outcome templates
- No workflow_type field
- No feature_cycle rename
- No schema migration

## Alignment Status

All checks PASS. No variances requiring approval. See ALIGNMENT-REPORT.md for details.
