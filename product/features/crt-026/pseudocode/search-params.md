# crt-026: Component — `ServiceSearchParams` new fields (`services/search.rs`)

File: `crates/unimatrix-server/src/services/search.rs`
Wave: 1

---

## Purpose

Add two new data-carrier fields to `ServiceSearchParams`. No logic belongs in this
struct. These fields carry the pre-resolved session context from handlers into the
`SearchService` scoring loop.

---

## Current State

`ServiceSearchParams` is currently defined at lines 202-217 of `search.rs`:

```rust
pub(crate) struct ServiceSearchParams {
    pub query: String,
    pub k: usize,
    pub filters: Option<QueryFilter>,
    pub similarity_floor: Option<f64>,
    pub confidence_floor: Option<f64>,
    #[allow(dead_code)]
    pub feature_tag: Option<String>,
    #[allow(dead_code)]
    pub co_access_anchors: Option<Vec<u64>>,
    #[allow(dead_code)]
    pub caller_agent_id: Option<String>,
    pub retrieval_mode: RetrievalMode,
}
```

---

## Modifications to `ServiceSearchParams`

Add two fields at the end of the struct, after `retrieval_mode`:

```
/// crt-026: Session identifier for logging and tracing (WA-2).
/// Populated from ctx.audit_ctx.session_id (MCP path) or
/// HookRequest::ContextSearch.session_id (UDS path).
/// Not used in scoring logic; carried for observability.
pub session_id: Option<String>,

/// crt-026: Pre-resolved category histogram clone (WA-2, ADR-002).
///
/// Set to None when:
///   - session_id is None
///   - session is not registered in SessionRegistry
///   - get_category_histogram() returned an empty map (is_empty() → None)
///
/// When Some, the histogram is used in the scoring loop to compute
/// phase_histogram_norm = p(entry.category) per candidate.
///
/// Cold-start invariant: None → phase_histogram_norm = 0.0 for all candidates
/// → compute_fused_score output bit-for-bit identical to pre-crt-026 (NFR-02).
pub category_histogram: Option<HashMap<String, u32>>,
```

The `HashMap<String, u32>` type is already imported at the top of the file via
`use std::collections::HashMap`. No new imports needed.

---

## Impact on Existing Construction Sites

All `ServiceSearchParams { ... }` struct literals in the codebase must be updated to
include the two new fields. Known construction sites:

1. `mcp/tools.rs` — `context_search` handler (step 4): set both fields (see `search-handler.md`)
2. `uds/listener.rs` — `handle_context_search` function: set both fields (see `uds.md`)
3. Any test helpers in `services/search.rs` that construct `ServiceSearchParams` directly:
   set `session_id: None, category_histogram: None` to preserve cold-start behavior

For all test-only construction sites where no session context is needed, use:
```
session_id: None,
category_histogram: None,
```

This is equivalent to cold-start and produces identical behavior to pre-crt-026.

---

## Error Handling

No error handling belongs in `ServiceSearchParams`. It is a plain data struct. All
validation and resolution logic is the responsibility of the caller (handler).

---

## Key Test Scenarios

See `test-plan/search-params.md` for the full test plan.

1. **R-12 (compilation gate)**: All `ServiceSearchParams { ... }` literal sites compile
   after adding both new fields. No silent default omission.

2. **AC-04**: `ServiceSearchParams` struct has `session_id: Option<String>` field.

3. **AC-05**: Handler test — `ServiceSearchParams` constructed with the correct
   `session_id` and `category_histogram` values from the session registry.

4. **Cold-start**: `ServiceSearchParams { session_id: None, category_histogram: None, ... }`
   produces identical scoring output to a pre-crt-026 params struct.
