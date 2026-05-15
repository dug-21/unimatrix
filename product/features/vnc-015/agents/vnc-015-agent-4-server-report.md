# Agent Report: vnc-015-agent-4-server

**Agent ID:** vnc-015-agent-4-server
**Task:** Components 1 and 2 — create edge_write.rs helper module and extend tools.rs with EdgeInput/params/pipeline

---

## Files Created/Modified

- `crates/unimatrix-server/src/mcp/edge_write.rs` — CREATED (491 lines)
- `crates/unimatrix-server/src/mcp/mod.rs` — MODIFIED (added `pub(crate) mod edge_write;`)
- `crates/unimatrix-server/src/mcp/tools.rs` — MODIFIED
- `crates/unimatrix-server/src/infra/validation.rs` — MODIFIED (test struct literals updated)

---

## Where EdgeInput Is Defined

`EdgeInput` is defined in `tools.rs` (inline, above `StoreParams`), per the architecture spec (ARCHITECTURE.md Component 1). It is imported into `edge_write.rs` via `use crate::mcp::tools::EdgeInput;`.

---

## default_rules() Caller — Before and After

**Before:**
```rust
let rules = unimatrix_observe::default_rules(history_slice);
```

**After:**
```rust
// vnc-015: default_rules gains stale_edges parameter (DependencyOnDeprecatedRule).
// The stale-edge pre-query for context_cycle_review belongs to Component 8
// (context_edge handler). For now, pass vec![] as placeholder.
let rules = unimatrix_observe::default_rules(history_slice, vec![]);
```

---

## Implementation Summary

### edge_write.rs
- `EDGE_SOURCE_AGENT: &str = "agent"` constant (ADR-008)
- `EdgeValidationError` enum with 4 variants + `Display` impl
- `EdgeDeleteError` enum (StoreError wrapper) + `Display` impl
- `EdgeRedirectError` enum (TargetNotFound, TargetQuarantined, TransactionError) + `Display` impl
- `validate_target()` private helper — checks existence + quarantine via `store.get()`
- `validate_and_write_edges()` — type resolution + self-ref + target validation in single pass, then write loop with Contradicts bidirectional writes (AC-06)
- `delete_graph_edge()` — idempotent DELETE, bidirectional for Contradicts
- `redirect_graph_edge()` — RAII `pool.begin().await?` transaction (lesson #2269), 2-row non-Contradicts, 4-row Contradicts atomically

### tools.rs additions
- `EdgeInput` struct with `#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]`
- `StoreParams.edges: Option<Vec<EdgeInput>>` with `#[serde(default)]` (AC-01 backward compat)
- `CorrectParams.edges: Option<Vec<EdgeInput>>` with `#[serde(default)]` (AC-02 backward compat)
- Phase A pre-insert validation inline in `context_store` and `context_correct` handlers
- Phase B post-insert `validate_and_write_edges` call after duplicate guard in both handlers
- 5 new unit tests for EdgeInput deserialization and edges field defaults

---

## Tests

- **Before:** 2974 passing
- **After:** 2979 passing (+5 new unit tests)
- **Failed:** 0
- **New tests added:**
  - `test_edge_input_deserializes_valid_json`
  - `test_store_params_edges_field_defaults_to_none`
  - `test_correct_params_edges_field_defaults_to_none`
  - `test_store_params_accepts_edges_vec`
  - `test_store_params_accepts_empty_edges_vec`
  - Plus 8 inline unit tests in `edge_write.rs` (constant, distinctness, error variants, Display impls)

---

## Issues / Blockers

None. All ADR constraints satisfied:
- ADR-001: Phase A validates types and targets pre-insert; self-ref check post-insert in `validate_and_write_edges`
- ADR-003: Infrastructure edge write failures logged inside `write_graph_edge`, not rolled back
- ADR-005: edge_write.rs is 491 lines (under 500-line limit)
- ADR-008: `EDGE_SOURCE_AGENT = "agent"` constant used at all `write_graph_edge` call sites
- Lesson #2269: `redirect_graph_edge` uses `pool.begin().await?` RAII transaction, not raw SQL strings

`delete_graph_edge`, `redirect_graph_edge`, `EdgeDeleteError`, and `EdgeRedirectError` are annotated `#[allow(dead_code)]` pending Component 8 (`context_edge` handler) implementation.

---

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_briefing` — returned 13 relevant entries including ADR-005, ADR-008, pattern #4417, lesson #2269. Applied write_graph_edge three-case contract (pattern #4041), EDGE_SOURCE_AGENT convention (ADR-008), RAII transaction pattern (lesson #2269).
- **Stored:** entry #4435 "Phase A/B edge validation split: pre-insert inline loop + post-insert validate_and_write_edges" via `/uni-store-pattern`
