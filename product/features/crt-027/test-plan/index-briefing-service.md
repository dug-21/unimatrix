# Test Plan: IndexBriefingService (services/index_briefing.rs)

## Component

`crates/unimatrix-server/src/services/index_briefing.rs` (new file)

Also touches: `crates/unimatrix-server/src/services/briefing.rs` (deleted)

Introduces: `IndexBriefingService`, `IndexBriefingParams`, `derive_briefing_query`

## Risks Covered

R-02 (EffectivenessStateHandle wiring), R-06 (query derivation divergence), R-08 (feature flag),
R-09 (UNIMATRIX_BRIEFING_K env var), R-10 (cold-state topic fallback), IR-02 (status filter),
IR-04 (session state access on UDS path), EC-03 (k=0), EC-06 (fewer than 3 topic_signals)

## ACs Covered

AC-07, AC-08 (service layer), AC-09, AC-10, AC-11 (service portion), AC-13, AC-15, AC-24

---

## Unit Test Expectations

Tests in `crates/unimatrix-server/src/services/index_briefing.rs` `#[cfg(test)]` block.
Use an in-memory `Store` (same pattern as existing `briefing.rs` tests, which are deleted
as part of this feature).

### Query Derivation Tests (R-06)

#### `derive_briefing_query_task_param_takes_priority` (AC-09 step 1, R-06 scenario 1)
**Arrange**: `task = Some("implement spec writer")`, `session_state = None`, `topic = "crt-027"`
**Act**: `derive_briefing_query(Some("implement spec writer"), None, "crt-027")`
**Assert**: Returns `"implement spec writer"` (step 1)

#### `derive_briefing_query_empty_task_falls_through` (R-06 scenario 5)
**Arrange**: `task = Some("")`, `session_state = Some(state)`, `topic = "crt-027"`
**Act**: `derive_briefing_query(Some(""), Some(&state), "crt-027")`
**Assert**: Does NOT return `""` — falls through to step 2 or step 3

#### `derive_briefing_query_session_signals_step_2` (AC-09 step 2, R-06 scenario 2)
**Arrange**: `task = None`, `session_state = Some(state)` where:
- `state.feature_cycle = Some("crt-027/spec")`
- `state.topic_signals = [("briefing", 5), ("hook", 3), ("compaction", 2), ("extra", 1)]`
**Act**: `derive_briefing_query(None, Some(&state), "crt-027")`
**Assert**: Returns `"crt-027/spec briefing hook compaction"` (feature_cycle + top 3 by vote)

#### `derive_briefing_query_fewer_than_three_signals` (EC-06)
**Arrange**: `session_state` with `topic_signals = [("briefing", 5)]` (only 1 signal)
**Act**: `derive_briefing_query(None, Some(&state), "crt-027")`
**Assert**: Returns `"crt-027/spec briefing"` — no trailing spaces, well-formed

#### `derive_briefing_query_empty_signals_fallback_to_topic` (R-06 scenario 3)
**Arrange**: `task = None`, `session_state = Some(state)` where `topic_signals = []`
**Act**: `derive_briefing_query(None, Some(&state), "crt-027")`
**Assert**: Returns `"crt-027"` (step 3)

#### `derive_briefing_query_no_session_fallback_to_topic` (AC-09 step 3, R-06 scenario 4)
**Arrange**: `task = None`, `session_state = None`, `topic = "crt-027"`
**Act**: `derive_briefing_query(None, None, "crt-027")`
**Assert**: Returns `"crt-027"` (step 3)

### Service Behavior Tests

#### `index_briefing_service_default_k_is_20` (AC-07, R-09 scenario 1)
**Arrange**: Construct `IndexBriefingService` with 25 active entries in store.
Set `UNIMATRIX_BRIEFING_K=3` in test environment via `std::env::set_var`.
**Act**: Call `service.index(IndexBriefingParams { query: "test", k: 20, session_id: None, max_tokens: None })`
**Assert**: Result contains up to 20 entries (NOT capped at 3)

#### `index_briefing_service_k_override` (AC-07 — k param)
**Arrange**: Store 25 active entries
**Act**: `service.index(IndexBriefingParams { k: 5, ... })`
**Assert**: `result.len() <= 5`

#### `index_briefing_service_active_entries_only` (AC-06, IR-02)
**Arrange**: Store 2 entries with same topic: one `Status::Active`, one `Status::Deprecated`
**Act**: `service.index(...)` with a query that matches both
**Assert**: Result contains the Active entry; does NOT contain the Deprecated entry's ID

#### `index_briefing_service_returns_sorted_by_fused_score` (AC-19 — service layer)
**Arrange**: Store entries with varying confidence; query that matches all
**Act**: `service.index(...)`
**Assert**: `result[0].confidence >= result[1].confidence >= result[2].confidence`

#### `index_briefing_service_empty_result_on_no_match` (R-10 scenario 1)
**Arrange**: Empty store (or store with no matching entries for "nonexistent-feature-id-xyz")
**Act**: `service.index(IndexBriefingParams { query: "nonexistent-feature-id-xyz", k: 20, ... })`
**Assert**: Returns `Ok(vec![])` — no panic, no `Err`

#### `index_briefing_service_snippet_chars_limit` (AC-17 — service layer)
**Arrange**: Store one entry with `content = "a".repeat(300)` (300 chars)
**Act**: `service.index(...)`
**Assert**: `result[0].snippet.chars().count() <= 150`

### EffectivenessStateHandle Tests (R-02)

#### `index_briefing_service_requires_effectiveness_handle` (R-02 scenario 3)
**Verification**: Compile-time only. The `IndexBriefingService::new()` constructor requires
`effectiveness_state: EffectivenessStateHandle` as a non-optional parameter. A test that
attempts to construct without it will fail to compile — this is the designed guarantee.
**Action at Gate 3c**: Confirm the constructor signature in `services/index_briefing.rs`
has no `Option<EffectivenessStateHandle>` parameter and no default.

#### `index_briefing_service_effectiveness_influences_ranking` (R-02 scenario 1)
**Arrange**: Two entries that differ only in helpfulness_count (effectiveness signal).
Mock or advance the effectiveness tick to distinguish them.
**Act**: `service.index(...)` after effectiveness tick
**Assert**: The higher-helpfulness entry has a higher fused score (appears first in result)
**Note**: This test may require a test helper that triggers an effectiveness generation tick.
If the effectiveness subsystem is hard to mock, accept a weaker assertion: result is non-empty
and no panic occurs. Document this as a partial coverage gap for R-02.

### Feature Flag Tests (R-08)

#### `handle_compact_payload_compiles_without_mcp_briefing_flag` (AC-24)
**Verification**: Run `cargo test --workspace` (without `--features mcp-briefing`).
All `handle_compact_payload` tests pass. This is a CI gate, not a source-level test.
**Note**: `IndexBriefingService` itself is NOT gated by `#[cfg(feature = "mcp-briefing")]`.
Compile-time confirmation: `IndexBriefingService::new()` compiles unconditionally.

### env var deprecation (R-09)

#### `parse_semantic_k_function_does_not_exist` (R-09 scenario 3)
**Verification**: `grep -r "parse_semantic_k" crates/unimatrix-server/src/` returns no results.
Static gate, not a source test.

#### `unimatrix_briefing_k_not_referenced_in_service` (R-09 scenario 2)
**Verification**: `grep -r "UNIMATRIX_BRIEFING_K" crates/unimatrix-server/src/services/` returns
only the deprecation comment (not a functional read call). Static gate.

---

## Integration Test Expectations

`IndexBriefingService` is exercised via MCP through the `context_briefing` tool. See
`context-briefing-handler.md` for infra-001 integration scenarios.

The UDS path is exercised via `handle_compact_payload` unit tests in `listener.rs`
(see `listener-dispatch.md`).

---

## Deleted Content Verification (AC-13)

The following must NOT be present after `services/briefing.rs` is replaced:
- `BriefingService` struct
- `BriefingParams` struct
- `BriefingResult` struct
- `InjectionSections` struct
- `InjectionEntry` struct
- `parse_semantic_k()` function
- All `#[test]` functions previously in `briefing.rs`

**Gate check**: `grep -r "BriefingService" crates/` returns no results (excluding docs/specs/this plan).

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|----------|
| k = 0 | `index_briefing_service_k_zero_safe` | Returns empty vec or clamps to k=1, no panic |
| k > total active entries | (covered by `index_briefing_service_default_k_is_20` with <20 entries) | Returns all active entries |
| 150-char CJK snippet at multi-byte boundary | `index_briefing_service_snippet_chars_limit` | Chars <= 150, valid UTF-8 |
| Fewer than 3 topic_signals | `derive_briefing_query_fewer_than_three_signals` | Well-formed query, no trailing spaces |
| Session state held directly on UDS path (IR-04) | `derive_briefing_query_*` tests use `session_state` directly | No registry lookup in shared helper |
