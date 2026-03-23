# Test Plan: service-layer-wiring (services/mod.rs)

## Component

`crates/unimatrix-server/src/services/mod.rs`

Changes:
- `ServiceLayer.briefing` field type: `BriefingService` → `IndexBriefingService`
- `with_rate_config()` construction: remove `parse_semantic_k()`, replace `BriefingService::new()` with `IndexBriefingService::new()`
- Re-export update: `BriefingService` removed, `IndexBriefingService` exported
- Deprecation comment added where `parse_semantic_k()` was called

## Risks Covered

R-02 (EffectivenessStateHandle wiring in ServiceLayer), R-08 (mcp-briefing flag),
R-09 (parse_semantic_k deletion), IR-03 (ServiceLayer field rename compile surface)

## ACs Covered

AC-13 (no re-export of BriefingService), AC-24, AC-25 (compile pass)

---

## Unit Test Expectations

`services/mod.rs` has minimal logic; it is primarily wiring. Test strategy is:
1. Compile-time verification (the main guarantee)
2. Integration-level construction test confirming the wiring is correct

### Test: `service_layer_construction_with_index_briefing_service`
**Arrange**: Call `ServiceLayer::with_rate_config(store, search, gateway, effectiveness_state)`
in a test context (can use the existing test helpers for store construction).
**Act**: Access `service_layer.briefing` (or call a method that exercises `IndexBriefingService`)
**Assert**: The field is of type `IndexBriefingService` (inferred from successful compilation
and method dispatch); call `service_layer.briefing.index(params)` succeeds without panic.

**Note**: This test primarily validates that:
- The construction does NOT call `parse_semantic_k()`
- The construction passes `Arc::clone(&effectiveness_state)` to `IndexBriefingService::new()`
- The service layer compiles with the new type

### Test: `briefing_service_re_export_removed`
**Verification** (static, not a unit test):
```bash
grep -r "pub.*use.*BriefingService" crates/unimatrix-server/src/services/mod.rs
```
Returns no matches. The re-export of `BriefingService` is removed.

### Test: `index_briefing_service_re_export_present`
**Verification** (static):
```bash
grep -r "IndexBriefingService" crates/unimatrix-server/src/services/mod.rs
```
Returns the new re-export line.

### Test: `unimatrix_briefing_k_deprecation_comment_present`
**Verification** (static):
```bash
grep "UNIMATRIX_BRIEFING_K" crates/unimatrix-server/src/services/mod.rs
```
Returns the deprecation comment line (not a function call).

---

## Feature Flag Verification (R-08, AC-24)

Two CI runs are required:

#### Run 1: Without `mcp-briefing` flag
```bash
cargo test --workspace 2>&1 | tail -30
```
**Assert**: All tests in `handle_compact_payload` path pass. The `IndexBriefingService`
compiles unconditionally.

#### Run 2: With `mcp-briefing` flag
```bash
cargo test --workspace --features mcp-briefing 2>&1 | tail -30
```
**Assert**: `context_briefing` MCP tool tests pass (AC-06, AC-07, AC-08, AC-09, AC-11).

---

## Compile-Time Gate (AC-25, IR-03)

The field rename `briefing: BriefingService` → `briefing: IndexBriefingService` touches
the `ServiceLayer` construction. Pattern #2938 (ADR-003 in Unimatrix) warns that ServiceLayer
construction has 5+ call sites. Verify:

1. `cargo build --release` with zero type errors related to `briefing` field construction.
2. Any call site that constructs `ServiceLayer` directly (test helpers, integration tests)
   compiles without modification if they use `ServiceLayer::with_rate_config()` (they should).
3. Direct field construction (if any test constructs `ServiceLayer { briefing: BriefingService::new(...), ... }`)
   is updated to use `IndexBriefingService::new(...)`.

---

## Edge Cases

| Scenario | Verification | Expected |
|----------|-------------|----------|
| `with_rate_config()` called without env var set | Runtime test (no `UNIMATRIX_BRIEFING_K` set) | k=20 default |
| `with_rate_config()` called with `UNIMATRIX_BRIEFING_K=3` set | Runtime test with env var | k=20 (env var ignored by IndexBriefingService) |
| `effectiveness_state` not passed to constructor | Compile error | Build fails |
