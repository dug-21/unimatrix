# Unimatrix Rust Codebase Refactoring Analysis

**Date**: 2026-03-03
**Scope**: All 8 crates, 86 Rust source files, 45,424 total lines
**Objective**: Identify structural improvements without modifying code

---

## Executive Summary

The codebase has grown organically across ~15 feature cycles. The foundation crates (`unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`) are well-structured with clean module boundaries. The server crate (`unimatrix-server`) bears the most technical debt, concentrating 3 files over 2,000 lines each with significant internal duplication.

**Top concerns by impact**:
1. `tools.rs` (3,061 lines) -- monolithic tool implementations with heavy boilerplate
2. `server.rs` (2,105 lines) -- duplicated index-writing logic that already exists in `write.rs`
3. `response.rs` (2,550 lines) -- repetitive format-dispatch pattern across 14 formatters
4. `uds_listener.rs` (2,271 lines) -- second implementation of search/ranking logic
5. Stringly-typed category and status APIs throughout the server layer

---

## Module Size Heat Map

| File | Lines | Functions >50 LOC | Complexity Notes |
|------|------:|:-----------------:|------------------|
| `unimatrix-server/src/tools.rs` | 3,061 | 10 | 12 MCP tool handlers, all with identical 13-step ceremony |
| `unimatrix-server/src/response.rs` | 2,550 | 2 | 14 format functions, each with Summary/Markdown/Json branches |
| `unimatrix-server/src/uds_listener.rs` | 2,271 | 6 | UDS IPC + duplicated search logic from tools.rs |
| `unimatrix-server/src/server.rs` | 2,105 | 4 | Combined write transactions duplicate store/write.rs index code |
| `unimatrix-store/src/write.rs` | 1,939 | 1 | 14 write methods; clean but large |
| `unimatrix-store/src/migration.rs` | 1,421 | 0 | 3 legacy struct definitions; acceptable for migration code |
| `unimatrix-vector/src/index.rs` | 1,383 | 0 | Well-structured; includes tests |
| `unimatrix-server/src/hook.rs` | 1,280 | 1 | Hook script generation |
| `unimatrix-server/src/validation.rs` | 1,209 | 0 | Validation functions; well-scoped |
| `unimatrix-engine/src/wire.rs` | 1,093 | 0 | Wire protocol types |
| `unimatrix-server/src/session.rs` | 1,006 | 0 | Session registry logic |
| `unimatrix-observe/src/metrics.rs` | 999 | 1 | `compute_universal`: 267 lines |
| `unimatrix-server/src/registry.rs` | 933 | 2 | Agent registry |
| `unimatrix-store/src/read.rs` | 924 | 0 | Read operations; clean |
| `unimatrix-observe/src/detection/agent.rs` | 909 | 0 | Detection rules |
| `unimatrix-server/src/contradiction.rs` | 820 | 1 | Contradiction scanning |
| `unimatrix-engine/src/confidence.rs` | 736 | 0 | Confidence computation |
| `unimatrix-store/src/sessions.rs` | 674 | 1 | Session GC |
| `unimatrix-store/src/schema.rs` | 656 | 0 | Table definitions, EntryRecord |

### Largest Functions (>100 lines)

| File | Line | Function | Lines |
|------|-----:|----------|------:|
| `server/src/tools.rs` | 1050 | `context_status` | 628 |
| `server/src/response.rs` | 621 | `format_status_report` | 409 |
| `observe/src/metrics.rs` | 27 | `compute_universal` | 267 |
| `server/src/tools.rs` | 2137 | `context_retrospective` | 231 |
| `server/src/uds_listener.rs` | 586 | `handle_context_search` | 228 |
| `server/src/tools.rs` | 1683 | `context_briefing` | 223 |
| `server/src/uds_listener.rs` | 368 | `dispatch_request` | 213 |
| `server/src/server.rs` | 354 | `correct_with_audit` | 197 |
| `server/src/tools.rs` | 526 | `context_store` | 186 |
| `server/src/tools.rs` | 778 | `context_correct` | 179 |
| `server/src/tools.rs` | 255 | `context_search` | 178 |
| `server/src/main.rs` | 86 | `tokio_main` | 167 |
| `server/src/tools.rs` | 2404 | `write_lesson_learned` | 165 |
| `server/src/server.rs` | 193 | `insert_with_audit` | 156 |
| `server/src/server.rs` | 555 | `record_usage_for_entries` | 150 |
| `server/src/tools.rs` | 1911 | `context_quarantine` | 146 |
| `server/src/hook.rs` | 159 | `build_request` | 116 |
| `server/src/error.rs` | 171 | `From<ServerError> for ErrorData` | 111 |

---

## Top 10 Refactoring Opportunities

### 1. Extract Index-Writing Transaction Logic (Impact: HIGH)

**Problem**: `server.rs` lines 193-348 (`insert_with_audit`) and lines 354-570 (`correct_with_audit`) manually write to ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, and OUTCOME_INDEX -- the exact same index-writing logic that exists in `store/src/write.rs` lines 27-112 (`insert`) and lines 119-190 (`update`).

**Evidence**: 34 `open_table` calls in `server.rs` vs 42 in `write.rs`. The server duplicates the store's index management because it needs to combine entry writes with audit logging in a single transaction.

**Recommendation**: Add a `Store::insert_in_txn(&WriteTransaction, NewEntry)` and `Store::update_in_txn(&WriteTransaction, EntryRecord)` method that accept an external transaction. The server can then call these within its combined audit transaction, eliminating ~200 lines of duplicated index logic.

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/server.rs` lines 193-570
- `/workspaces/unimatrix/crates/unimatrix-store/src/write.rs` lines 27-190

### 2. Extract Shared Tool Handler Ceremony (Impact: HIGH)

**Problem**: All 12 MCP tool handlers in `tools.rs` follow an identical 13-step pattern:
1. Identity resolution
2. Capability check
3. Validation
4. Format parsing
5-12. Business logic
13. Audit logging
14. Usage recording

Each handler repeats the `.map_err(rmcp::ErrorData::from)?` chain verbatim. There are **79 occurrences** of `.map_err(rmcp::ErrorData::from)` and **18 occurrences** of `.map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(` in `tools.rs` alone.

**Recommendation**: Extract a `ToolContext` struct that encapsulates identity resolution, capability checking, format parsing, and audit logging. Each tool handler would receive a pre-validated `ToolContext` and only implement business logic. This could reduce each handler by 30-50 lines.

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` -- every tool handler

### 3. Split `context_status` (628 lines) into Sub-Functions (Impact: HIGH)

**Problem**: `context_status` at `tools.rs:1050` is a 628-line function with 12 labeled sections (5a through 5l). It reads counters, scans distributions, runs contradiction detection, computes coherence, refreshes confidence, compacts the graph, sweeps sessions, runs GC, and formats the report -- all in one function.

**Recommendation**: Extract into composable functions:
- `build_base_report(store, topic_filter, category_filter) -> (StatusReport, Vec<EntryRecord>)` (already a spawn_blocking closure)
- `run_contradiction_scan(embed_service, store, vector_index) -> ContradictionResult`
- `compute_coherence_dimensions(report, entries, now) -> CoherenceDimensions`
- `run_maintenance(store, entries, ...) -> MaintenanceResult`

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` lines 1050-1677

### 4. Eliminate Duplicated Search/Ranking Logic (Impact: HIGH)

**Problem**: `uds_listener.rs:handle_context_search` (line 586, 228 lines) reimplements the search, reranking, co-access boost, and provenance boost logic that already exists in `tools.rs:context_search` (line 255, 178 lines). Both embed queries the same way, apply the same `rerank_score`, use the same `CO_ACCESS_STALENESS_SECONDS`, and follow the same co-access anchor pattern.

**Recommendation**: Extract a shared `SearchPipeline` that both the MCP tool and UDS handler call. The pipeline would accept an embedding and return ranked results, encapsulating the reranking, co-access boosting, and provenance boosting logic.

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds_listener.rs` lines 586-813
- `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` lines 255-432

### 5. Deduplicate Confidence Recompute Pattern (Impact: MEDIUM)

**Problem**: The "fire-and-forget confidence recompute" pattern appears 8 times across `tools.rs` (lines 682-701, 916-948, 1017-1036, 1980-1997, 2030-2047) and `tools.rs:context_status` (lines 1451-1503). Each instance clones `Arc<Store>`, spawns a `spawn_blocking`, reads the entry, calls `compute_confidence`, and calls `update_confidence`.

**Recommendation**: Extract `recompute_confidence_fire_and_forget(store: &Arc<Store>, entry_id: u64)` as a utility function. Could further batch multiple IDs.

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` -- 6 identical blocks
- Each block is ~20 lines

### 6. Reduce Format-Dispatch Boilerplate in `response.rs` (Impact: MEDIUM)

**Problem**: `response.rs` contains 14 public format functions. Functions `format_deprecate_success`, `format_quarantine_success`, and `format_restore_success` (lines 516-618) are nearly identical -- each has the same Summary/Markdown/Json branches with only the action verb changed ("Deprecated", "Quarantined", "Restored").

**Evidence**: Lines 516-548 vs 551-583 vs 586-618 are structurally identical with only string literal differences.

**Recommendation**: Create a generic `format_status_change_success(entry, verb, reason, format)` that parameterizes the action verb. Reduces 3 functions to 1.

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/response.rs` lines 516-618

### 7. Introduce Category Enum to Replace String Literals (Impact: MEDIUM)

**Problem**: Category names are stringly-typed throughout the codebase. The string `"lesson-learned"` appears 15 times across 3 server files, `"outcome"` appears 17 times across 7 files, and `"convention"` / `"duties"` / `"decision"` etc. appear 156 times total. The `CategoryAllowlist` validates at runtime using `HashSet<String>`, but there is no compile-time safety.

**Evidence**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/categories.rs` defines 8 string constants
- `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` line 552: `if params.category == "outcome"`
- `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` line 355-356: `if entry_a.category == "lesson-learned"`
- `/workspaces/unimatrix/crates/unimatrix-engine/src/confidence.rs` uses `"lesson-learned"` for PROVENANCE_BOOST

**Recommendation**: Define a `Category` enum with `Display`/`FromStr` implementations. The allowlist can remain for extensibility, but known categories get compile-time safety.

### 8. Consolidate Timestamp Utilities (Impact: LOW)

**Problem**: The "get current unix timestamp" pattern is implemented independently in at least 3 locations with different function names:
- `unimatrix-store/src/write.rs:15` -- `current_unix_timestamp_secs()`
- `unimatrix-server/src/uds_listener.rs:39` -- `unix_now_secs()`
- 37 total inline `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()` occurrences across 12 files

**Recommendation**: Add `pub fn unix_now() -> u64` to `unimatrix-core` and use it everywhere. Minor but improves consistency.

**Files**: 12 files with 37 inline occurrences

### 9. Convert `StatusReport` to Derive Serializable (Impact: LOW)

**Problem**: The `StatusReport` struct in `response.rs` (lines 361-440) has 34 fields. It is manually serialized to JSON in `format_status_report` (409 lines) with hand-built `serde_json::json!` macros. Any field addition requires updating 3 branches (Summary, Markdown, JSON).

**Recommendation**: Derive `Serialize` on `StatusReport` and use `serde_json::to_value(&report)` for the JSON branch. The Summary and Markdown branches can use a trait or template approach.

**Files**:
- `/workspaces/unimatrix/crates/unimatrix-server/src/response.rs` lines 361-440 (struct), 621-1030 (formatter)

### 10. Extract `EntryRecord` Construction into Builder (Impact: LOW)

**Problem**: `EntryRecord` has 26 fields and is manually constructed in 6 locations:
- `store/src/write.rs:38-65` (insert)
- `server/src/server.rs:225-252` (insert_with_audit)
- `server/src/server.rs:444-471` (correct_with_audit)
- `store/src/migration.rs:176-203` (v0->v1)
- `store/src/migration.rs:280-310` (v1->v2)
- Various test helpers

Each construction must list all 26 fields manually. Missing a `#[serde(default)]` field is a silent deserialization bug.

**Recommendation**: Add `EntryRecord::new(id, new_entry, now) -> EntryRecord` constructor that initializes all fields from a `NewEntry`, setting computed fields to their defaults.

**Files**: 6 construction sites across `write.rs`, `server.rs`, `migration.rs`

---

## Duplication Analysis

### Critical Duplications

| Pattern | Occurrences | Files | Est. Duplicate Lines |
|---------|:-----------:|-------|---------------------:|
| Index writing (ENTRIES + 5 indexes + VECTOR_MAP) | 3 | `write.rs`, `server.rs` (insert_with_audit, correct_with_audit) | ~200 |
| Search + rerank + co-access boost | 2 | `tools.rs`, `uds_listener.rs` | ~180 |
| Confidence recompute fire-and-forget block | 8 | `tools.rs` (6), `tools.rs:context_status` (2) | ~160 |
| Format status-change response (deprecate/quarantine/restore) | 3 | `response.rs` | ~100 |
| `.map_err(rmcp::ErrorData::from)?` chains | 79 | `tools.rs` | boilerplate |
| AuditEvent construction with placeholder fields | 14 | `tools.rs` (14) | ~140 |
| Embedding pipeline (get_adapter + spawn_blocking + adapt + normalize) | 5 | `tools.rs` (search, store, correct, briefing), `uds_listener.rs` | ~100 |

### Moderate Duplications

| Pattern | Occurrences | Notes |
|---------|:-----------:|-------|
| `SystemTime::now().duration_since(UNIX_EPOCH)` | 37 | Across 12 files |
| `EntryRecord { id, title, ...26 fields }` construction | 6 | No builder pattern |
| `unwrap_or_else(\|e\| e.into_inner())` poison recovery | 36 | Consistent but verbose |
| `entry_to_json` / `entry_to_json_with_similarity` | 2 | Nearly identical |
| `format_entry_markdown_section` called with `None`/`Some(sim)` | 6 | Could unify parameter |

---

## Error Handling Review

### Error Type Hierarchy

The codebase uses a clean 4-layer error hierarchy:

```
ServerError (server/)
  -> CoreError (core/)
       -> StoreError (store/)
       -> VectorError (vector/)
       -> EmbedError (embed/)
  -> Registry, Audit, Capability errors
ObserveError (observe/)  -- separate, not wrapped by CoreError
```

### Consistency Assessment

**Good practices**:
- All error types implement `std::error::Error` with proper `source()` chains
- `EmbedError` uses `thiserror` derive (the only crate that does)
- `StoreError`, `VectorError`, `CoreError` use manual `Display` + `Error` impls -- consistent within themselves
- Error codes are well-defined constants in `server/src/error.rs`

**Issues found**:

1. **Mixed error derive strategy**: `EmbedError` uses `thiserror`, all others are manual. This is functional but inconsistent. Lines: `embed/src/error.rs:8` vs `store/src/error.rs:37`, `vector/src/error.rs:28`, `core/src/error.rs:21`.

2. **Silent error swallowing in search**: At `tools.rs:349`, entry fetch failures during search result enumeration are silently skipped with `Err(_) => continue`. While documented as "FR-01g", this could hide real errors (e.g., corruption) behind seemingly successful but incomplete search results.

   ```rust
   // tools.rs:349
   Err(_) => continue, // silently skip deleted entries (FR-01g)
   ```

3. **Empty match arms swallow errors**: At `tools.rs:1993`, `server.rs` quarantine/restore confidence recompute:
   ```rust
   // tools.rs:1993
   Err(_) => {}
   ```
   No logging, no tracing. Compare with `tools.rs:693-697` which does log warnings.

4. **Inconsistent `unwrap()` usage**: 1,782 `.unwrap()` calls across 47 files. Most are in test code (acceptable), but several are in production paths:
   - `uds_listener.rs` has 30 `.unwrap()` calls (some in tests, but some in production `format_compaction_payload`)
   - `server.rs` has 89 `.unwrap()` calls (many are timestamp unwrap_or_default, but the spawn_blocking `.await.unwrap()` calls at lines 1581, 1601, 1616, 2179, 2285, 2300 would panic on task panic)

5. **`ObserveError` not integrated into `CoreError`**: The observe pipeline has its own error type not wrapped by `CoreError`. When the server encounters observe errors, they're wrapped as `ServerError::ObservationError(String)`, losing the typed error chain. File: `server/src/error.rs:102`.

### Error Propagation Pattern

The `.map_err(rmcp::ErrorData::from)?` chain in `tools.rs` is used 79 times. This is mechanically correct but adds visual noise. A `trait MapRmcpError` extension trait could reduce each call to `.rmcp_err()?`.

---

## Type System Recommendations

### Stringly-Typed APIs

| API | Current Type | Recommended Type | Occurrences |
|-----|-------------|-----------------|:-----------:|
| Category validation | `&str` | `enum Category` | 156 uses across 19 files |
| Status parsing | `&str` -> `Status` | Already an enum, but parsed from strings in validation | 18 uses in `validation.rs`/`response.rs` |
| Trust level | `&str` | `enum TrustLevel` (exists in `registry.rs` but parsed from `String`) | Used in `tools.rs`, `validation.rs` |
| Capability | `Vec<String>` | `Vec<Capability>` (exists but parsed from `Vec<String>`) | Used in `tools.rs`, `validation.rs` |
| Operation name in AuditEvent | `String` | `enum Operation` | 14 construction sites |
| `format` parameter | `Option<String>` | `Option<ResponseFormat>` (parsed after receipt) | Every tool param struct |
| `trust_source` in EntryRecord | `String` | `enum TrustSource` | Part of EntryRecord (26 fields) |
| `feature_cycle` | `String` | Newtype `FeatureCycle(String)` | Scattered across store/server |

### Missing Validation at Type Boundaries

1. **Embedding dimension not enforced at type level**: Embeddings are `Vec<f32>` throughout. The `VectorIndex` validates dimension at runtime (`validate_dimension`), but nothing prevents constructing an entry with a mismatched embedding. A `newtype Embedding(Vec<f32>)` with dimension validation on construction would provide compile-time safety.

2. **Entry ID is bare `u64`**: Entry IDs are raw `u64` throughout the codebase. A `newtype EntryId(u64)` would prevent accidental confusion with `data_id` (also `u64`), `hnsw_data_id` (also `u64`), and timestamps (also `u64`). The `VectorIndex.IdMap` maps between these, and a type mix-up would be a subtle bug.

3. **`i64` to `u64` conversion at API boundary**: Tool parameter structs use `i64` for IDs (because JSON-RPC uses signed integers), and the server converts via `validated_id` / `as u64` casts. The conversion is validated but scattered across 5+ call sites.

### Primitive Obsession

- **`StatusReport` has 34 fields**: This struct is a flat bag of scalars. Grouping into sub-structs (`CoherenceMetrics`, `CorrectionChainMetrics`, `CoAccessMetrics`, `OutcomeMetrics`, `ObservationMetrics`) would improve readability and make it possible to test/format each section independently.

- **`AuditEvent` uses placeholder fields**: Every construction site sets `event_id: 0`, `timestamp: 0`, `session_id: String::new()` because these are filled later by `AuditLog::write_in_txn`. The struct should use `Option` or a builder pattern to avoid misleading placeholder values at 14 construction sites.

---

## Module Dependency Graph

```
unimatrix-server (binary + lib)
  |-- unimatrix-core (traits, adapters, async_wrappers)
  |     |-- unimatrix-store (redb storage engine)
  |     |-- unimatrix-vector (HNSW index)
  |     |     |-- unimatrix-store (VECTOR_MAP persistence)
  |     |-- unimatrix-embed (ONNX embedding pipeline)
  |-- unimatrix-engine (confidence, auth, coaccess, wire protocol)
  |     |-- unimatrix-store
  |-- unimatrix-adapt (MicroLoRA, prototypes)
  |     |-- unimatrix-embed
  |-- unimatrix-observe (detection, metrics, attribution)
  |-- unimatrix-store (direct, for combined transactions)
  |-- unimatrix-vector (direct, for graph operations)
  |-- unimatrix-embed (direct, for normalization)
```

**Observation**: `unimatrix-server` depends on ALL 7 other crates. It also reaches past `unimatrix-core`'s trait abstractions to use `unimatrix-store::Store` directly (for combined write transactions), `unimatrix-vector::VectorIndex` directly (for `allocate_data_id`, `compact`), and `unimatrix-embed` directly (for `l2_normalized`). This bypasses the adapter/trait layer established by `unimatrix-core`.

The direct dependencies on foundation crates exist for valid reasons (combined transactions, graph operations), but they make the server a "god module" that knows about every crate's internals.

---

## Crate-Specific Findings

### unimatrix-store (4,499 lines)
- Clean separation: `read.rs`, `write.rs`, `schema.rs`, `query.rs`, `migration.rs`
- `write.rs` at 1,939 lines is the largest but contains 14 well-separated write methods
- No major issues; the main refactoring leverage is exposing transaction-level methods

### unimatrix-vector (2,279 lines)
- Well-structured with `index.rs`, `persistence.rs`, `filter.rs`, `config.rs`
- Good error handling with typed `VectorError`
- `index.rs` at 1,383 lines includes substantial test code

### unimatrix-embed (1,294 lines)
- Smallest crate; cleanest module organization
- Only crate using `thiserror` for error derives
- Good separation of concerns (`onnx.rs`, `text.rs`, `pooling.rs`, `normalize.rs`)

### unimatrix-core (823 lines)
- Clean trait definitions, but traits are partially bypassed by server
- `async_wrappers.rs` at 312 lines wraps 27 `spawn_blocking` calls -- mechanical but working

### unimatrix-engine (3,139 lines)
- Newer crate; well-organized
- `wire.rs` (1,093 lines) defines the hook IPC protocol types
- `confidence.rs` (736 lines) is self-contained and well-tested

### unimatrix-observe (4,297 lines)
- Modular detection system with `detection/{mod,agent,session,scope,friction}.rs`
- `metrics.rs:compute_universal` at 267 lines is the longest function; could be split into per-dimension calculators

### unimatrix-adapt (2,392 lines)
- Relatively new (crt-006); well-structured
- `training.rs:execute_training_step` at 96 lines is at the edge but manageable

### unimatrix-server (16,500 lines)
- Contains 37% of the entire codebase
- 4 files over 2,000 lines
- Primary target for refactoring
- Concentrates business logic, response formatting, IPC handling, and transaction management

---

## Appendix: File Inventory by Crate

### unimatrix-store (31 lines lib.rs)
| File | Lines |
|------|------:|
| write.rs | 1,939 |
| migration.rs | 1,421 |
| read.rs | 924 |
| sessions.rs | 674 |
| schema.rs | 656 |
| db.rs | 532 |
| query.rs | 318 |
| test_helpers.rs | 257 |
| injection_log.rs | 253 |
| error.rs | 171 |
| signal.rs | 142 |
| hash.rs | 78 |
| counter.rs | 56 |

### unimatrix-server (34 lines lib.rs)
| File | Lines |
|------|------:|
| tools.rs | 3,061 |
| response.rs | 2,550 |
| uds_listener.rs | 2,271 |
| server.rs | 2,105 |
| hook.rs | 1,280 |
| validation.rs | 1,209 |
| session.rs | 1,006 |
| registry.rs | 933 |
| contradiction.rs | 820 |
| audit.rs | 599 |
| error.rs | 567 |
| coherence.rs | 581 |
| pidfile.rs | 472 |
| outcome_tags.rs | 435 |
| scanning.rs | 423 |
| usage_dedup.rs | 320 |
| main.rs | 284 |
| categories.rs | 242 |
| embed_handle.rs | 161 |
| identity.rs | 140 |
| shutdown.rs | 179 |
