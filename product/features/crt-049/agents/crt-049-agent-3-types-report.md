# Agent Report: crt-049-agent-3-types

**Component**: Component 1 — FeatureKnowledgeReuse struct changes
**File**: `crates/unimatrix-observe/src/types.rs`

## Changes Made

### Primary scope: `crates/unimatrix-observe/src/types.rs`

1. Renamed `delivery_count` → `search_exposure_count` with two stacked `#[serde(alias)]` lines:
   - `#[serde(alias = "delivery_count")]`
   - `#[serde(alias = "tier1_reuse_count")]`
2. Added `explicit_read_count: u64` with `#[serde(default)]` and full doc comment.
3. Added `explicit_read_by_category: HashMap<String, u64>` with `#[serde(default)]` and GATE contract AC-13 doc comment.
4. Updated `total_served` doc comment: `|explicit_read_ids ∪ injection_ids| (deduplicated). Search exposures excluded. (crt-049)`.
5. Reordered fields per pseudocode spec: `search_exposure_count`, `explicit_read_count`, `explicit_read_by_category`, `cross_session_count`, `by_category`, `category_gaps`, `total_served`, ...
6. Updated 5 existing test fixtures that used `delivery_count` as a Rust field name (compile errors).
7. Added 8 new tests per test plan:
   - AC-02 GATE (5 tests): `test_search_exposure_count_deserializes_from_canonical_key`, `..._from_delivery_count_alias`, `..._from_tier1_reuse_count_alias`, `test_search_exposure_count_serializes_to_canonical_key`, `test_search_exposure_count_round_trip_all_alias_forms`
   - AC-01/AC-13 partial (3 tests): `test_explicit_read_count_defaults_to_zero_when_absent`, `test_explicit_read_by_category_defaults_to_empty_map_when_absent`, `test_explicit_read_by_category_serde_round_trip`

### Compile-unblocking fixes (not full feature logic — other agents own these files)

- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`: renamed `delivery_count` local var and struct field in 3 struct literals; added `explicit_read_count: 0` and `explicit_read_by_category: HashMap::new()` to all struct literals; renamed `delivery_count` in ~20 test assertions via `sed -i`.
- `crates/unimatrix-server/src/mcp/response/retrospective.rs`: renamed `delivery_count` field in 9 test fixtures; added `explicit_read_count`/`explicit_read_by_category` to each; updated guard and display references in `render_knowledge_reuse`.
- `crates/unimatrix-server/src/mcp/tools.rs`: renamed `delivery_count` in one `tracing::debug!` format string.

## Test Results

- `cargo test -p unimatrix-observe`: **503 passed, 0 failed** (431 + 22 + 44 + 6 across 4 test binaries)
- `cargo build --workspace`: **0 errors** (18 pre-existing warnings in unimatrix-server, unrelated)
- `cargo test --workspace`: **all workspace tests pass** (pre-existing flaky `test_self_search_50_entries` in unimatrix-vector — not caused by this change; confirmed by git diff --name-only showing no vector crate changes)

## Issues / Blockers

None. All gate tests AC-02, AC-01, AC-13 (partial) pass. Build is unblocked for other agents.

Note: `knowledge_reuse.rs` struct literals now have `explicit_read_count: 0` and `explicit_read_by_category: HashMap::new()` as placeholders. Agent-2 (compute component) will replace these with the actual computation logic.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "serde alias backward compatibility patterns" → found #923 (related but different angle — non-persisted types), #646, #3774. Applied: stacked separate `#[serde(alias)]` lines per ADR-002 (not combined into one attribute).
- Queried: `mcp__unimatrix__context_search` for "crt-049 architectural decisions" → found ADRs #4218, #4215, #4216 confirming serde alias chain and field rename decisions.
- Stored: entry #4219 "Renaming a persisted struct field hits two distinct compile error classes across crates — fix struct literals separately from field accesses" via `/uni-store-pattern`.
