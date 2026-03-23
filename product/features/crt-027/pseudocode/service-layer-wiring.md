# Service Layer Wiring — Pseudocode
# File: crates/unimatrix-server/src/services/mod.rs

## Purpose

Update `ServiceLayer` and `services/mod.rs` to:
1. Replace `briefing: BriefingService` field with `briefing: IndexBriefingService`.
2. Replace `BriefingService::new()` construction with `IndexBriefingService::new()`.
3. Remove `parse_semantic_k()` call from `with_rate_config`.
4. Remove `pub(crate) use briefing::BriefingService` re-export; add `pub(crate) use briefing::IndexBriefingService`.
5. Add `pub(crate) mod index_briefing` module declaration.
6. Remove `pub(crate) mod briefing` declaration (since briefing.rs is deleted).

---

## Module Declarations (at top of mod.rs, ~lines 27-42)

**Before**:
```rust
pub(crate) mod briefing;
pub(crate) mod confidence;
// ...
```

**After**:
```rust
// briefing module removed — replaced by index_briefing (crt-027)
pub(crate) mod index_briefing;     // NEW: IndexBriefingService, derive_briefing_query
pub(crate) mod confidence;
// ...
```

---

## Re-exports (~lines 41-53)

**Before**:
```rust
pub(crate) use briefing::BriefingService;
```

**After**:
```rust
// DEPRECATED (crt-027): UNIMATRIX_BRIEFING_K env var is no longer read.
// IndexBriefingService uses k=20 hardcoded. Use max_tokens parameter to control budget.
pub(crate) use index_briefing::IndexBriefingService;
```

The deprecation comment goes at the re-export location, not on the type definition itself.
This is the "comment at the removal point" required by ADR-003 and C-08.

---

## ServiceLayer Struct Field (line ~238)

**Before**:
```rust
pub struct ServiceLayer {
    pub(crate) search: SearchService,
    pub(crate) store_ops: StoreService,
    pub(crate) confidence: ConfidenceService,
    pub(crate) briefing: BriefingService,     // ← old type
    pub(crate) status: StatusService,
    // ...
}
```

**After**:
```rust
pub struct ServiceLayer {
    pub(crate) search: SearchService,
    pub(crate) store_ops: StoreService,
    pub(crate) confidence: ConfidenceService,
    pub(crate) briefing: IndexBriefingService,    // crt-027: replaces BriefingService
    pub(crate) status: StatusService,
    // ...
}
```

No other fields change. The doc comment on `briefing` should be updated from
"crt-018b (ADR-004): effectiveness classification handle" to reference crt-027.

---

## `with_rate_config` Construction Block (~lines 422-429)

**Before**:
```rust
let semantic_k = briefing::parse_semantic_k();
let briefing = BriefingService::new(
    Arc::clone(&entry_store),
    search.clone(),
    Arc::clone(&gateway),
    semantic_k,
    Arc::clone(&effectiveness_state),  // crt-018b (ADR-004): required, non-optional
);
```

**After**:
```rust
// crt-027: UNIMATRIX_BRIEFING_K deprecated — IndexBriefingService uses k=20 hardcoded.
// parse_semantic_k() removed. See ADR-003 crt-027.
let briefing = IndexBriefingService::new(
    Arc::clone(&entry_store),
    search.clone(),
    Arc::clone(&gateway),
    Arc::clone(&effectiveness_state),  // required, non-optional (ADR-004 crt-018b pattern)
);
```

The deprecation comment is placed EXACTLY at the site where `parse_semantic_k()` was called.
This satisfies C-08 and FR-13.

---

## `ServiceLayer` Initializer (line ~450-461)

**Before**:
```rust
ServiceLayer {
    search,
    store_ops,
    confidence,
    briefing,     // BriefingService instance
    status,
    usage,
    effectiveness_state,
    // ...
}
```

**After**:
```rust
ServiceLayer {
    search,
    store_ops,
    confidence,
    briefing,     // IndexBriefingService instance (same field name, new type)
    status,
    usage,
    effectiveness_state,
    // ...
}
```

No change to the field name — only the type changes.

---

## Accessors (no change)

The following public accessors on `ServiceLayer` are unchanged:
- `confidence_state_handle()` — no change
- `effectiveness_state_handle()` — no change (doc comment may be updated to remove BriefingService reference)
- `typed_graph_handle()` — no change
- `contradiction_cache_handle()` — no change

Update the `effectiveness_state_handle()` doc comment: remove the reference to
`BriefingService` in the comment "SearchService, BriefingService, and the background tick".
Replace with "SearchService, IndexBriefingService, and the background tick".

---

## Error Handling

This component is pure wiring — no new error paths. If `IndexBriefingService::new()` panics,
the server startup fails (same behavior as before). The constructor does not panic.

---

## Key Test Scenarios

These are compile-time and integration verifications, not unit tests in mod.rs itself.

**T-SL-01** `cargo_build_release_no_type_errors` (IR-03, AC-13):
- `cargo build --release` passes with no type errors or dead-code warnings.
- `BriefingService` reference causes compile error → confirms all callers migrated.

**T-SL-02** `grep_no_briefingservice_references` (AC-13):
- `grep -r "BriefingService" crates/` returns no results (excluding this spec file).
- `grep -r "parse_semantic_k" crates/` returns no results.
- These are gate reviewer checks, not automated tests.

**T-SL-03** `grep_no_unimatrix_briefing_k_in_production_code` (R-09, AC-07):
- `grep -r "UNIMATRIX_BRIEFING_K" crates/unimatrix-server/src/` returns zero results
  in production code paths (only in comments and tests).
- Gate reviewer check.

**T-SL-04** `servicelayer_with_rate_config_constructs_successfully` (IR-03):
- Existing ServiceLayer construction integration test still passes.
- `IndexBriefingService::new()` call site compiles and runs without error.

**T-SL-05** `effectiveness_state_passed_to_index_briefing_service` (R-02):
- Code inspection: `with_rate_config()` passes `Arc::clone(&effectiveness_state)` (not a new
  fresh handle) to `IndexBriefingService::new()`.
- Verified by reading the construction block — no `EffectivenessState::new_handle()` call
  adjacent to the IndexBriefingService construction (that would create a disconnected handle).
