# Implementation Brief: crt-002 Confidence Evolution

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-002/SCOPE.md |
| Scope Risk Assessment | product/features/crt-002/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-002/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-002/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-002/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-002/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| confidence-module | pseudocode/confidence-module.md | test-plan/confidence-module.md |
| store-confidence | pseudocode/store-confidence.md | test-plan/store-confidence.md |
| server-retrieval-integration | pseudocode/server-retrieval-integration.md | test-plan/server-retrieval-integration.md |
| server-mutation-integration | pseudocode/server-mutation-integration.md | test-plan/server-mutation-integration.md |
| search-reranking | pseudocode/search-reranking.md | test-plan/search-reranking.md |

## Goal

Compute a meaningful confidence score for every knowledge entry from six independent, gaming-resistant signals and write it to the existing `confidence: f32` field on EntryRecord. Confidence evolves continuously via inline computation on retrieval, insert, and mutation paths. Search results are re-ranked using a similarity-confidence blend.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Formula structure | Additive weighted composite (not multiplicative) | Research spike + SCOPE.md Decision 1 | architecture/ADR-001-inline-confidence-in-usage-write.md |
| Confidence in usage transaction | Merged into existing record_usage write transaction via function pointer | SR-03, SR-08 | architecture/ADR-001-inline-confidence-in-usage-write.md |
| Arithmetic precision | f64 intermediates, f32 final result | SR-01 | architecture/ADR-002-f64-intermediate-computation.md |
| Confidence floor | No explicit floor (emergent minimum from formula) | SCOPE.md Decision 8 | architecture/ADR-003-no-confidence-floor.md |
| Confidence display lag | One-retrieval lag (confidence shown is from previous computation) | Fire-and-forget pattern | architecture/ADR-004-one-retrieval-confidence-lag.md |
| Re-ranking scope | context_search only; deterministic paths unchanged | Human clarification + SR-04 | architecture/ADR-005-search-reranking-scope.md |
| Helpfulness minimum sample | 5 votes required before Wilson score deviates from neutral 0.5 | SCOPE.md Goal 6 | — |
| base_score for deprecated | 0.2 (vs 0.5 for active entries) | SCOPE.md AC-18 | — |

## Files to Create/Modify

### New Files
| File | Description |
|------|-------------|
| `crates/unimatrix-server/src/confidence.rs` | Confidence formula, 6 component functions, Wilson score, re-rank blend, all constants |

### Modified Files
| File | Description |
|------|-------------|
| `crates/unimatrix-server/src/lib.rs` | Add `mod confidence;` declaration |
| `crates/unimatrix-server/src/server.rs` | Extend `record_usage_for_entries()` to pass confidence_fn; add confidence on insert/correct/deprecate |
| `crates/unimatrix-server/src/tools.rs` | Add re-ranking step in context_search after fetching entries |
| `crates/unimatrix-store/src/write.rs` | Add `record_usage_with_confidence()` and `update_confidence()` methods |

### Test Files
| File | Description |
|------|-------------|
| `crates/unimatrix-server/src/confidence.rs` (inline tests) | Unit tests for all component functions, Wilson score, composite, re-rank |
| `crates/unimatrix-store/src/write.rs` (inline tests) | Unit tests for update_confidence and record_usage_with_confidence |
| Integration tests in existing test modules | End-to-end: retrieval -> confidence update, insert -> seed, correct -> recompute |

## Data Structures

### Constants (all in `confidence.rs`)

```rust
pub const W_BASE: f32 = 0.20;
pub const W_USAGE: f32 = 0.15;
pub const W_FRESH: f32 = 0.20;
pub const W_HELP: f32 = 0.15;
pub const W_CORR: f32 = 0.15;
pub const W_TRUST: f32 = 0.15;

pub const MAX_MEANINGFUL_ACCESS: f64 = 50.0;
pub const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0;
pub const MINIMUM_SAMPLE_SIZE: u32 = 5;
pub const WILSON_Z: f64 = 1.96;
pub const SEARCH_SIMILARITY_WEIGHT: f32 = 0.85;
```

### No New Data Structures

crt-002 does not introduce new structs, enums, or tables. It writes to the existing `confidence: f32` field on `EntryRecord` and uses the existing `Status` enum for `base_score` dispatch.

## Function Signatures

### Confidence Module (`crates/unimatrix-server/src/confidence.rs`)

```rust
pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f32
pub fn rerank_score(similarity: f32, confidence: f32) -> f32
pub fn base_score(status: Status) -> f64
pub fn usage_score(access_count: u32) -> f64
pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f64
pub fn helpfulness_score(helpful_count: u32, unhelpful_count: u32) -> f64
pub fn correction_score(correction_count: u32) -> f64
pub fn trust_score(trust_source: &str) -> f64
fn wilson_lower_bound(positive: f64, total: f64) -> f64  // private
```

### Store Extensions (`crates/unimatrix-store/src/write.rs`)

```rust
pub fn record_usage_with_confidence(
    &self,
    all_ids: &[u64],
    access_ids: &[u64],
    helpful_ids: &[u64],
    unhelpful_ids: &[u64],
    decrement_helpful_ids: &[u64],
    decrement_unhelpful_ids: &[u64],
    confidence_fn: Option<&dyn Fn(&EntryRecord, u64) -> f32>,
) -> Result<()>

pub fn update_confidence(&self, entry_id: u64, confidence: f32) -> Result<()>
```

## Constraints

- No schema changes -- writes to existing `confidence: f32` field only
- Fire-and-forget -- confidence updates on retrieval path must not block responses
- Synchronous store, async server -- store methods called via `spawn_blocking`
- bincode positional encoding -- read-modify-write full EntryRecord even for confidence-only updates
- No background tasks -- confidence computed inline only
- Weight sum invariant -- six weights must sum to exactly 1.0
- f64 intermediates -- all statistical math uses f64, final result cast to f32
- Object-safe EntryStore trait -- no trait changes in crt-002

## Dependencies

### Internal (no new crate dependencies)
- `unimatrix-store`: EntryRecord, Status, serialize_entry, deserialize_entry, ENTRIES
- `unimatrix-core`: re-exports EntryRecord and Status
- `std::f64` for ln(), exp(), sqrt()

### Feature Dependencies
- crt-001 (merged): `record_usage()`, `helpful_count`/`unhelpful_count` fields, `UsageDedup`
- vnc-001/002/003 (merged): tool handlers, identity resolution
- nxs-001 (merged): `confidence: f32` on EntryRecord

## NOT in Scope

- Batch recomputation of all entries
- `min_confidence` filter parameter on tools
- Confidence history/time series
- Implicit outcome correlation (AUDIT_LOG mining)
- Agent diversity signal
- Anomaly detection integration
- UI for confidence weight tuning
- Background confidence decay process
- Schema migration (no new fields)
- New external crate dependencies

## Alignment Status

**4 PASS, 2 WARN, 0 VARIANCE, 0 FAIL**

- **W1 (Additive formula)**: Additive weighted composite replaces product vision's multiplicative formula. Backed by gaming resistance research. Vision note already recommends additive. Accept.
- **W2 (No confidence floor)**: No explicit 0.1 floor. Emergent minimum from formula (~0.19 for active entries) exceeds the vision's 0.1. Accept.

No variances requiring human approval.

## Implementation Order

Components should be implemented in this order (each builds on the previous):

1. **C1: confidence-module** -- Pure computation, no dependencies beyond EntryRecord. All component functions + Wilson score + composite + rerank. Fully testable in isolation.
2. **C2: store-confidence** -- `update_confidence()` and `record_usage_with_confidence()`. Depends on C1 only at call site (function pointer).
3. **C3: server-retrieval-integration** -- Modify `record_usage_for_entries()` to pass confidence_fn. Depends on C1 and C2.
4. **C4: server-mutation-integration** -- Confidence on insert/correct/deprecate. Depends on C1 and C2.
5. **C5: search-reranking** -- Re-rank context_search results. Depends on C1 only.

C3 and C4 can be implemented in parallel after C2. C5 can be implemented in parallel with C3/C4.
